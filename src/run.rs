use chrono::Duration;
use crate::{Config, RunArgs};
use crate::db::Database;
use crate::time::{SleepTime, DaylogTime};
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

fn duration_fmt(mut dur: Duration) -> String {
    let mut out = format!("{}h, ", dur.num_hours());
    dur = dur - Duration::hours(dur.num_hours());
    out += &format!("{}m, ", dur.num_minutes());
    dur = dur - Duration::minutes(dur.num_minutes());
    out += &format!("{}s, ", dur.num_seconds());
    dur = dur - Duration::seconds(dur.num_seconds());
    out += &format!("{}ns", dur.num_nanoseconds().unwrap());
    out
}

fn sleep_until(time: SleepTime, control: &UnixStream) -> io::Result<SleepResult> {
    let pollfd = PollFd::new(control.as_raw_fd(), PollFlags::POLLIN);
    loop {
        let now = chrono::Utc::now().time();
        debug!("now it is {}", now.format("%H:%M:%S"));
        let sleep_duration = time.duration_from(now);
        let sleep_duration_millis = sleep_duration.num_milliseconds() as i32;
        if sleep_duration_millis < 0 {
            // this means we're not keeping up
            warn!("sleep duration is negative: {:?}", sleep_duration);
            return Ok(SleepResult::Completed);
        }
        debug!("sleeping for {}", duration_fmt(sleep_duration));

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
        };
    }
}

fn read_until_ewouldblock(mut file: impl Read) -> io::Result<()> {
    loop {
        let mut data = [0u8; 1];
        let result = file.read_exact(&mut data);
        debug!("control file read result: {:?} / {:#x?}", result, data);
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

    let mut users = db.get_all_users()?;
    let mut now = SleepTime::Today(DaylogTime::now());

    while !sigterm_flag.load(Ordering::SeqCst) {

        let (next_time, users) = match now {
            SleepTime::Today(time) => match users.next_from_time(time) {
                Some((next_time, users)) => {
                    info!("sleep until {}", next_time);
                    (SleepTime::Today(next_time), users)
                }
                None => {
                    info!("no more users today; checking tomorrow");
                    now = SleepTime::Tomorrow(DaylogTime::zero());
                    continue;
                }
            }
            SleepTime::Tomorrow(time) => match users.next_from_time(time) {
                Some((next_time, users)) => {
                    info!("sleep until tomorrow, {}", next_time);
                    (SleepTime::Tomorrow(next_time), users)
                }
                None => {
                    error!("no users are configured!");
                    return Ok(());
                }
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

        for user in users {
            info!("sending to {:?}", user);
            if !args.dry_run {
                /*let tz: chrono_tz::Tz = match std::str::FromStr::from_str(user.timezone.as_str()) {
                    Ok(tz) => tz,
                    Err(e) => {
                        error!("failed to parse {:?} as timezone (for user {:?}): {}",
                            user.timezone, user.username, e);
                        continue;
                    }
                };*/
                let result = crate::send::send(config, crate::SendArgs {
                    username: user.username.clone(),
                    email: user.email.clone(),
                    timezone: user.timezone,
                    date_override: None,
                });
                if let Err(e) = result {
                    error!("failed to send to {:?}: {}", user, e);
                }
            }
        }

        // Don't actually use the current time; in case sending takes longer than 1 minute, we want
        // to only advance to the next minute for checking the database.
        now = match next_time {
            SleepTime::Today(time) | SleepTime::Tomorrow(time) => {
                // we already slept until tomorrow, so now it's today no matter what
                SleepTime::Today(time.succ())
            }
        };
    }

    info!("termination requested; exiting");
    Ok(())
}
