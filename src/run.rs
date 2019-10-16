use chrono::{Utc, NaiveTime};
use crate::{Config, RunArgs};
use crate::db::Database;
use crate::named_pipe::NamedPipe;
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

fn sleep_until(time: NaiveTime, fd: &impl AsRawFd) -> io::Result<SleepResult> {
    let pollfd = PollFd::new(fd.as_raw_fd(), PollFlags::POLLIN);
    loop {
        let now = Utc::now().time();
        let sleep_duration = now - time;
        let sleep_duration_millis = sleep_duration.num_milliseconds() as i32;

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

pub fn run(config: Config, args: RunArgs) -> Result<(), failure::Error> {
    info!("starting service; using {:?} as control file", args.control_path);

    let mut control = NamedPipe::open_or_create(&args.control_path)
        .with_context(|e| format!("failed to create/open control file {:?}: {}", args.control_path, e))?;

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

    while !sigterm_flag.load(Ordering::SeqCst) {
        let now = Utc::now().time();
        let (next_time, do_send) = match db.get_next_send_time(now)? {
            Some(time) => {
                info!("sleep until {}", time);
                (time, true)
            }
            None => {
                info!("sleep until midnight and try again");
                (NaiveTime::from_hms(23, 59, 59), false)
            }
        };

        let result = sleep_until(next_time, &control)
            .context("failed to sleep")?;
        match result {
            SleepResult::Completed => (),
            SleepResult::FdReadable => {
                let mut data = [0u8; 1];
                control.as_ref().read_exact(&mut data)?;
                println!("data read: {:?}", data);
                continue;
            }
        }

        if !do_send {
            continue;
        }

        // TODO: actually do things
    }

    Ok(())
}
