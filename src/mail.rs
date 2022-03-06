use failure::{Error, ResultExt};
use mailparse::{MailHeaderMap, ParsedMail};

pub trait MailSource {
    fn read(&mut self, handler: Box<dyn FnMut(Mail) -> MailProcessAction>) -> Result<RunStats, Error>;
}

#[derive(Debug, Default)]
pub struct RunStats {
    pub num_processed: u64,
    pub num_removed: u64,
    pub num_kept: u64,
    pub num_left_unread: u64,
}

pub enum MailProcessAction {
    /// Remove the message.
    Remove,

    /// Keep the message around somewhere, but don't process it next time.
    Keep,

    /// Pretend we never saw the message.
    LeaveUnread,
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
    pub fn parse(parsed: ParsedMail) -> Result<Self, Error> {
        let msgid = parsed.headers.get_first_value("message-id")
            .ok_or_else(|| failure::err_msg("message lacks a Message-ID"))
            .map(trim_msgid)
            .context("message has invalid Message-ID")?;

        let reply_to = parsed.headers.get_first_value("References")
            .unwrap_or_default()
            .split_ascii_whitespace()
            .map(trim_msgid)
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

fn trim_msgid(s: impl AsRef<str>) -> String {
    s.as_ref()
        .trim()
        .trim_start_matches('<')
        .trim_end_matches('>')
        .to_owned()
}
