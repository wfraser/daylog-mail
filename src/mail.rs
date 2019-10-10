use failure::{Error, ResultExt};
use fs2::FileExt;
use std::ffi::OsStr;
use std::fs::{self, File, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};
use std::time::Duration;
use mailparse::MailHeaderMap;
use mbox_reader::MboxFile;

pub trait MailSource {
    fn read<'a>(&'a self) -> Result<Box<(dyn Iterator<Item = Result<Mail, Error>> + 'a)>, Error>;
    fn truncate(self);
}

pub struct UnixMbox {
    path: PathBuf,
}

impl UnixMbox {
    pub fn from_path(path: PathBuf) -> Self {
        Self { path }
    }

    pub fn open_for_read(&self) -> Result<OpenedUnixMbox, Error> {
        let dotlock = DotLock::new(&self.path)?;
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(false)
            .open(&self.path)
            .with_context(|e| format!("failed to open mbox file {:?}: {}", self.path, e))?;

        file.lock_exclusive()?;

        // safety: safe because we locked the file above
        let mmapped_file = unsafe { MboxFile::from_file(&file) }
            .with_context(|e| format!("unable to open mailbox file {:?}: {}", self.path, e))?;

        Ok(OpenedUnixMbox {
            file,
            mmapped_file,
            _dotlock: dotlock,
        })
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
            .write(false)
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
    _dotlock: DotLock,
}

impl MailSource for OpenedUnixMbox {
    fn read<'a>(&'a self) -> Result<Box<(dyn Iterator<Item = Result<Mail, Error>> + 'a)>, Error> {
        Ok(Box::new(self.mmapped_file.iter()
            .map(|entry| {
                entry.message()
                    .ok_or_else(|| failure::err_msg("mbox-reader returned empty message, why?"))
                    .and_then(Mail::parse)
            })
        ))
    }

    fn truncate(self) {
        std::mem::drop(self.mmapped_file);
        self.file.set_len(0).expect("failed to truncate mbox file");
        std::mem::drop(self.file);
    }
}

/// An email message plucked from a MailSource.
/// Only contains two pieces of information: the list of replied-to message IDs, and the message
/// body text. Things like 'From' are ignored because they can be spoofed. All we care about are
/// message IDs.
#[derive(Debug)]
pub struct Mail {
    pub msgid: String,
    pub reply_to: Vec<String>, // message IDs in 'References:' header
    pub body: String,
}

impl Mail {
    pub fn parse(raw: &[u8]) -> Result<Self, Error> {
        let parsed = mailparse::parse_mail(raw)
            .context("failed to parse email")?;

        let msgid = parsed.headers.get_first_value("message-id")
            .context("message has invalid Message-ID")?
            .ok_or_else(|| failure::err_msg("message lacks a Message-ID"))?;

        let reply_to = parsed.headers.get_first_value("References")
            .context("failed to parse References header")?
            .unwrap_or_else(String::new)
            .split_ascii_whitespace()
            .map(str::to_owned)
            .collect::<Vec<_>>();

        let body = if parsed.subparts.is_empty() {
            parsed.get_body().context("unable to parse email body text")?
        } else {
            // Find parts with "inline" content disposition and "text/plain" mimetype and
            // concatenate them together.
            let mut body = String::new();
            let mut found_something = false;
            for part in parsed.subparts {
                let disposition = part.get_content_disposition()
                    .context("unable to parse context disposition for a message subpart")?
                    .disposition;
                let mimetype = &part.ctype.mimetype;
                if disposition == mailparse::DispositionType::Inline && mimetype == "text/plain" {
                    let part_body = part.get_body().context("unable to parse email message subpart body")?;
                    body += &part_body;
                    body += "\n\n";
                    found_something = true;
                }
            }
            if !found_something {
                return Err(failure::err_msg("no suitable email message part with plain text found"));
            }
            body
        };

        Ok(Mail {
            msgid,
            reply_to,
            body,
        })
    }
}
