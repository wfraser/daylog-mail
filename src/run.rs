use crate::{Config, RunArgs};
use crate::db::Database;
use crate::named_pipe::NamedPipe;
use crate::time::DaylogTime;
use failure::ResultExt;
use nix::poll::{poll, PollFd, PollFlags};
use std::io::{self, Read};
use std::os::unix::io::AsRawFd;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

fn handle_signal<F>(signal: i32, file: F, flag: Arc<AtomicBool>) -> Result<(), failure::Error>
    where F: AsRawFd + Sync + Send + 'static,
{
    let action = move || {
        flag.store(true, Ordering::SeqCst);
        // note: we can't handle errors in a signal handler context
        let _ = nix::unistd::write(file.as_raw_fd(), b"X");
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

fn sleep_until(time: DaylogTime, fd: &impl AsRawFd) -> io::Result<SleepResult> {
    let pollfd = PollFd::new(fd.as_raw_fd(), PollFlags::POLLIN);
    loop {
        let now = chrono::Utc::now().time();
        debug!("now it is {}", now.format("%H:%M:%S"));
        let sleep_duration = time.duration_until(now);
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

pub fn run(config: &Config, args: RunArgs) -> Result<(), failure::Error> {
    info!("starting service; using {:?} as control file", config.control_path);

    let mut control = NamedPipe::open_or_create(&config.control_path)
        .with_context(|e| format!("failed to create/open control file {:?}: {}", config.control_path, e))?;

    control.set_nonblocking(true)
        .context("failed to set control file to nonblocking mode")?;

    let sigterm_flag = Arc::new(AtomicBool::new(false));

    handle_signal(
        signal_hook::SIGTERM,
        control.try_clone()
            .with_context(|e| format!("failed to duplicate control file handle: {}", e))?,
        Arc::clone(&sigterm_flag),
    )
        .with_context(|e| format!("failed to install SIGTERM handler: {}", e))?;

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
                read_until_ewouldblock(control.as_ref())
                    .with_context(|e| format!("error draining control file: {}", e))?;

                if !sigterm_flag.load(Ordering::SeqCst) {
                    // if this flag isn't set, it means we got a request to reload
                    // write back to the pipe to tell the other side we're done.
                    let _ = nix::unistd::write(control.as_raw_fd(), b"!");
                }

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
