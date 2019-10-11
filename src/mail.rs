use failure::{Error, ResultExt};
use mailparse::MailHeaderMap;

pub trait MailSource {
    fn read<'a>(&'a self) -> Result<Box<(dyn Iterator<Item = Result<Mail, Error>> + 'a)>, Error>;
    fn truncate(self);
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
            .map(trim_msgid)
            .ok_or_else(|| failure::err_msg("message lacks a Message-ID"))?;

        let reply_to = parsed.headers.get_first_value("References")
            .context("failed to parse References header")?
            .unwrap_or_else(String::new)
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

fn trim_msgid(s: impl AsRef<str>) -> String {
    s.as_ref()
        .trim()
        .trim_start_matches('<')
        .trim_end_matches('>')
        .to_owned()
}
