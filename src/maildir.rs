use crate::mail::{Mail, MailProcessAction, MailSource, RunStats};
use failure::{Error, ResultExt};
use std::fs;
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
    fn read(&mut self, mut handler: Box<dyn FnMut(Mail) -> MailProcessAction>)
        -> Result<RunStats, Error>
    {
        let mut stats = RunStats::default();
        for entry_result in self.maildir.list_new() {
            let mut entry = entry_result.context("failed to iterate maildir entries")?;
            let id = entry.id().to_owned();

            let action = match entry.parsed()
                .map_err(|e| format!("failed to parse mail message {}: {}", id, e))
                .and_then(|unstructured| {
                    Mail::parse(unstructured)
                        .map_err(|e| format!("failed to parse mail message {} (inner): {}", id, e))
                })
            {
                Ok(mail) => {
                    stats.num_processed += 1;
                    handler(mail)
                }
                Err(msg) => {
                    eprintln!("Failed to parse mail message {}: {}", id, msg);
                    MailProcessAction::Keep
                }
            };
            match action {
                MailProcessAction::Remove => {
                    let path = entry.path();
                    fs::remove_file(path)
                        .with_context(|e| format!("failed to remove message {:?}: {}", path, e))?;
                    stats.num_removed += 1;
                }
                MailProcessAction::Keep => {
                    self.maildir.move_new_to_cur(&id)
                        .with_context(|e| format!("failed to move message {} from new to cur: {}", id, e))?;
                    stats.num_kept += 1;
                }
                MailProcessAction::LeaveUnread => {
                    stats.num_left_unread += 1;
                }
            }
        }
        Ok(stats)
    }
}
