use nix::Error;
use nix::errno::Errno;
use nix::fcntl::{fcntl, FcntlArg, OFlag};
use nix::sys::stat::Mode;
use nix::unistd::mkfifo;
use std::fs::{OpenOptions, File};
use std::path::Path;
use std::io;
use std::os::unix::io::{AsRawFd, RawFd};

pub struct NamedPipe {
    file: File,
}

impl NamedPipe {
    pub fn create(path: impl AsRef<Path>) -> io::Result<()> {
        mkfifo(path.as_ref(), Mode::S_IRUSR | Mode::S_IWUSR)
            .as_io_result_ignoring(Errno::EEXIST)
    }

    pub fn open(path: impl AsRef<Path>) -> io::Result<Self> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(path)?;
        Ok(Self { file })
    }

    pub fn open_or_create(path: impl AsRef<Path>) -> io::Result<Self> {
        Self::create(&path)?;
        Self::open(path)
    }

    pub fn try_clone(&self) -> io::Result<Self> {
        Ok(Self { file: self.file.try_clone()? })
    }

    pub fn set_nonblocking(&mut self, nonblocking: bool) -> io::Result<()> {
        let flags_raw = fcntl(self.file.as_raw_fd(), FcntlArg::F_GETFL).as_io_result()?;
        let mut flags = OFlag::from_bits_truncate(flags_raw);
        if nonblocking {
            flags.insert(OFlag::O_NONBLOCK);
        } else {
            flags.remove(OFlag::O_NONBLOCK);
        }
        fcntl(self.file.as_raw_fd(), FcntlArg::F_SETFL(flags))
            .as_io_result()
            .map(|_| ())
    }
}

impl AsRef<File> for NamedPipe {
    fn as_ref(&self) -> &File {
        &self.file
    }
}

impl AsRawFd for NamedPipe {
    fn as_raw_fd(&self) -> RawFd {
        self.file.as_raw_fd()
    }
}

trait NixResultExt<T> {
    fn as_io_result(self) -> io::Result<T>;
    fn as_io_result_ignoring(self, ignored_errno: Errno) -> io::Result<()>;
}

impl<T> NixResultExt<T> for Result<T, nix::Error> {
    fn as_io_result(self) -> io::Result<T> {
        match self {
            Ok(value) => Ok(value),
            Err(Error::Sys(errno)) => Err(io::Error::from_raw_os_error(errno as i32)),
            Err(e) => Err(io::Error::new(io::ErrorKind::Other, e)),
        }
    }
    fn as_io_result_ignoring(self, ignored_errno: Errno) -> io::Result<()> {
        match self {
            Ok(_) => Ok(()),
            Err(Error::Sys(errno)) if errno == ignored_errno => Ok(()),
            Err(Error::Sys(errno)) => Err(io::Error::from_raw_os_error(errno as i32)),
            Err(e) => Err(io::Error::new(io::ErrorKind::Other, e)),
        }
    }
}
