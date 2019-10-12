use crate::mail::{Mail, MailSource};
use failure::{Error, ResultExt};
use fs2::FileExt;
use std::ffi::OsStr;
use std::fs::{self, File, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};
use std::time::Duration;
use mbox_reader::MboxFile;

pub struct UnixMbox;

impl UnixMbox {
    pub fn open(path: &Path) -> Result<Option<OpenedUnixMbox>, Error> {
        let dotlock = DotLock::new(path)
            .with_context(|e| format!("failed to create .lock file: {}", e))?;

        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(false)
            .open(path)
            .with_context(|e| format!("failed to open mbox file {:?}: {}", path, e))?;

        file.lock_exclusive()
            .with_context(|e| format!("failed to lock mbox file {:?} for exclusive access: {}", path, e))?;

        if file.metadata()
            .context("unable to get mbox file size")?
            .len() == 0
        {
            return Ok(None);
        }

        // safety: safe because we locked the file above
        let mmapped_file = unsafe { MboxFile::from_file(&file) }
            .with_context(|e| format!("unable to open mailbox file {:?}: {}", path, e))?;

        Ok(Some(OpenedUnixMbox {
            file,
            mmapped_file,
            path: path.to_owned(),
            _dotlock: dotlock,
        }))
    }
}

pub struct DotLock {
    path: PathBuf,
}

impl DotLock {
    pub fn new(base_path: impl AsRef<Path>) -> io::Result<Self> {
        let mut filename = base_path.as_ref().file_name().unwrap_or_default().to_owned();
        filename.push(OsStr::new(".lock"));
        let path = base_path.as_ref().with_file_name(filename);

        let mut options = OpenOptions::new();
        options.read(false)
            .write(true)
            .append(false)
            .create_new(true);

        for _retry in 0 .. 20 {
            match options.open(&path) {
                Ok(_file) => return Ok(Self { path }),
                Err(e) => {
                    if e.kind() == io::ErrorKind::AlreadyExists {
                        // sleep and retry
                        std::thread::sleep(Duration::from_secs(1));
                    } else {
                        return Err(e);
                    }
                }
            }
        }

        Err(io::ErrorKind::AlreadyExists.into())
    }
}

impl Drop for DotLock {
    fn drop(&mut self) {
        fs::remove_file(&self.path).expect("unable to remove mbox lock file");
    }
}

pub struct OpenedUnixMbox {
    file: File,
    mmapped_file: MboxFile,
    path: PathBuf,
    _dotlock: DotLock,
}

impl MailSource for OpenedUnixMbox {
    fn read<'a>(&'a self) -> Result<Box<(dyn Iterator<Item = Result<Mail, Error>> + 'a)>, Error> {
        Ok(Box::new(self.mmapped_file.iter()
            .map(|entry| {
                entry.message()
                    .ok_or_else(|| failure::err_msg("mbox-reader returned empty message, why?"))
                    .and_then(Mail::parse_raw)
            })
        ))
    }

    fn truncate(self: Box<Self>) {
        std::mem::drop(self.mmapped_file);
        {
            // During development, let's save copies of the mailboxes.
            let mut copy = self.path.clone().into_os_string();
            let seconds = std::time::SystemTime::now()
                .duration_since(std::time::SystemTime::UNIX_EPOCH)
                .map(|dur| dur.as_secs())
                .unwrap_or(0);
            copy.push(&format!("_{}.bak", seconds));
            fs::copy(&self.path, &copy).expect("failed to backup mbox file");
        }
        self.file.set_len(0).expect("failed to truncate mbox file");
        std::mem::drop(self.file);
    }
}
