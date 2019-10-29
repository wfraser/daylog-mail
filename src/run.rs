use chrono::{Duration, NaiveTime, Timelike};
use crate::{Config, RunArgs};
use crate::db::Database;
use crate::time::DaylogTime;
use failure::ResultExt;
use nix::fcntl::{fcntl, FcntlArg, OFlag};
use nix::poll::{poll, PollFd, PollFlags};
use nix::sys::socket::{send, MsgFlags};
use std::io::{self, Read};
use std::os::unix::io::{AsRawFd, RawFd};
use std::os::unix::net::UnixStream;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

fn handle_signal(signal: i32, sock: UnixStream, flag: Option<Arc<AtomicBool>>)
    -> Result<(), failure::Error>
{
    let action = move || {
        if let Some(ref flag) = flag {
            (*flag).store(true, Ordering::SeqCst);
        }
        // note: we can't handle errors in a signal handler context
        let _ = send(sock.as_raw_fd(), b"X", MsgFlags::MSG_DONTWAIT);
    };
    unsafe {
        signal_hook::register(signal, action)
    }?;
    Ok(())
}

enum SleepResult {
    Completed,
    FdReadable,
}

fn sleep_until(time: DaylogTime, control: &UnixStream) -> io::Result<SleepResult> {
    let pollfd = PollFd::new(control.as_raw_fd(), PollFlags::POLLIN);
    loop {
        let now = chrono::Utc::now().time();
        debug!("now it is {}", now.format("%H:%M:%S"));
        let sleep_duration = if now.hour() == 23 && now.minute() == 59 {
            // get to midnight first
            (NaiveTime::from_hms(23, 59, 59).signed_duration_since(now)) + Duration::seconds(1)
        } else {
            let d = time.duration_from(now);
            if d < Duration::minutes(1) {
                info!("sleep duration is < 1 minute; returning immediately");
                break;
            }
            d
        };
        let sleep_duration_millis = sleep_duration.num_milliseconds() as i32;
        if sleep_duration_millis < 0 {
            warn!("sleep duration is negative: {:?}", sleep_duration); // means we're not keeping up
            return Ok(SleepResult::Completed);
        }
        debug!("sleeping for {:?}", sleep_duration);

        return match poll(&mut[pollfd], sleep_duration_millis) {
            Ok(0) => {
                debug!("sleep completed");
                Ok(SleepResult::Completed)
            }
            Ok(_) => {
                debug!("sleep ended due to readable control file");
                Ok(SleepResult::FdReadable)
            }
            Err(nix_err) => {
                match nix_err {
                    nix::Error::Sys(errno) if errno == nix::errno::Errno::EINTR => {
                        debug!("got EINTR while sleeping");
                        continue;
                    }
                    nix::Error::Sys(errno) => Err(io::Error::from_raw_os_error(errno as i32)),
                    other_nix_err => Err(io::Error::new(io::ErrorKind::Other, other_nix_err)),
                }
            }
        }
    }
    Ok(SleepResult::Completed)
}

fn read_until_ewouldblock(mut file: impl Read) -> io::Result<()> {
    loop {
        let mut data = [0u8; 1];
        let result = file.read_exact(&mut data);
        debug!("control file read result: {:?} / {:x?}", result, data);
        match result {
            Ok(_) => (),
            Err(e) if e.raw_os_error() == Some(nix::errno::EWOULDBLOCK as i32) => {
                break;
            }
            Err(e) => {
                return Err(e);
            }
        }
    }
    Ok(())
}

fn set_nonblocking(f: RawFd) -> Result<(), failure::Error> {
    let flags_raw = fcntl(f, FcntlArg::F_GETFL)?;
    let mut flags = OFlag::from_bits_truncate(flags_raw);
    flags.insert(OFlag::O_NONBLOCK);
    fcntl(f, FcntlArg::F_SETFL(flags))?;
    Ok(())
}

pub fn run(config: &Config, args: RunArgs) -> Result<(), failure::Error> {
    info!("starting service");

    let (control, control_sigterm) = UnixStream::pair()?;
    let control_sighup = control_sigterm.try_clone()?;

    set_nonblocking(control.as_raw_fd())
        .context("failed to set control socket nonblocking")?;

    let sigterm_flag = Arc::new(AtomicBool::new(false));

    handle_signal(
        signal_hook::SIGTERM,
        control_sigterm,
        Some(Arc::clone(&sigterm_flag)),
    )
        .with_context(|e| format!("failed to install SIGTERM handler: {}", e))?;

    handle_signal(
        signal_hook::SIGHUP,
        control_sighup,
        None,
    )
        .with_context(|e| format!("failed to install SIGHUP handler: {}", e))?;

    let db = Database::open(&config.database_path)?;

    info!("process ID: {}", std::process::id());

    let mut now = DaylogTime::now();

    while !sigterm_flag.load(Ordering::SeqCst) {
        let mut users_query = db.get_users_to_send()?;

        let (next_time, users) = match users_query.next_from_time(now)? {
            Some((time, users)) => {
                info!("sleep until {}", time);
                (time, Some(users))
            }
            None => {
                info!("sleep until midnight and try again");
                (DaylogTime::new(23, 59), None)
            }
        };

        let result = sleep_until(next_time, &control)
            .context("failed to sleep")?;
        match result {
            SleepResult::Completed => (),
            SleepResult::FdReadable => {
                read_until_ewouldblock(&control)
                    .with_context(|e| format!("error draining control file: {}", e))?;
                continue;
            }
        }

        if let Some(users) = users {
            for user in users {
                info!("sending to {:?}", user);
                if !args.dry_run {
                    let tz: chrono_tz::Tz = match std::str::FromStr::from_str(user.timezone.as_str()) {
                        Ok(tz) => tz,
                        Err(e) => {
                            error!("failed to parse {:?} as timezone (for user {:?}): {}",
                                user.timezone, user.username, e);
                            continue;
                        }
                    };
                    let result = crate::send::send(config, crate::SendArgs {
                        username: user.username.clone(),
                        email: user.email.clone(),
                        timezone: tz,
                        date_override: None,
                    });
                    if let Err(e) = result {
                        error!("failed to send to {:?}: {}", user, e);
                    }
                }
            }
        }

        // Don't actually use the current time; in case sending takes longer than 1 minute, we want
        // to only advance to the next minute for checking the database.
        now = next_time.succ();
    }

    Ok(())
}
