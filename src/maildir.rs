use crate::mail::{Mail, MailSource};
use failure::{Error, ResultExt};
use maildir::Maildir;
use std::path::Path;

pub struct DaylogMaildir {
    maildir: Maildir,
}

impl DaylogMaildir {
    pub fn open(path: &Path) -> Self {
        Self {
            maildir: Maildir::from(path.to_owned()),
        }
    }
}

impl MailSource for DaylogMaildir {
    fn read<'a>(&'a self) -> Result<Box<(dyn Iterator<Item = Result<Mail, Error>> + 'a)>, Error> {
        Ok(Box::new(self.maildir.list_new()
            .map(move |entry_result| {
                let mut mailentry = entry_result?;
                let id = mailentry.id();
                self.maildir.move_new_to_cur(id)
                    .with_context(|e| format!("failed to move message {} from new to cur: {}", id, e))?;

                let parsed = mailentry.parsed()?;
                Mail::parse(&parsed)
            })))
    }

    fn truncate(self: Box<Self>) {
        // nothing
        // TODO: remove processed emails
    }
}
