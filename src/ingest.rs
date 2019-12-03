use crate::config::{Config, IncomingMailConfig};
use crate::mail::{MailProcessAction, MailSource};
use crate::maildir::DaylogMaildir;
use crate::message_id::{is_our_message_id, read_secret_key, verify_message_id};
use crate::{IngestArgs, MailTransformArgs};
use failure::ResultExt;
use regex::Regex;

pub fn ingest(config: &Config, args: IngestArgs) -> Result<(), failure::Error> {
    let key_bytes = read_secret_key(&config.secret_key_path)
        .with_context(|e|
            format!("failed to read secret key {:?}: {}", config.secret_key_path, e))?;

    let mut db = crate::db::Database::open(&config.database_path)?;

    let mut source: Box<dyn MailSource> = match config.incoming_mail {
        IncomingMailConfig::Maildir { ref path } => {
            Box::new(DaylogMaildir::open(path))
        }
    };

    let stats = source.read(Box::new(move |mail| {
        let mut msgids = vec![];
        for msgid in mail.reply_to {
            if is_our_message_id(&msgid) {
                msgids.push(msgid);
            }
        }

        if msgids.is_empty() {
            return if args.dry_run {
                MailProcessAction::LeaveUnread
            } else {
                MailProcessAction::Keep
            };
        }

        if args.dry_run {
            println!("Message {:?} is interesting", mail.msgid);
        }

        let body = process_body(&mail.body);

        if args.dry_run {
            println!("body:\n{}", body);
        }

        for msgid in msgids {
            let (username, date) = match verify_message_id(&msgid, key_bytes) {
                Ok((username, date)) => {
                    if args.dry_run {
                        println!("{:?} -> ({:?}, {:?})", msgid, username, date);
                    }
                    (username, date)
                }
                Err(e) => {
                    println!("Error: message {:?} replies to {:?}, but: {}",
                             mail.msgid, msgid, e);
                    return if args.dry_run {
                        MailProcessAction::LeaveUnread
                    } else {
                        MailProcessAction::Keep
                    };
                }
            };

            if !args.dry_run {
                if let Err(e) = db.add_entry(&username, &date, &body) {
                    eprintln!("Error adding to database: {:?}", e);
                    return MailProcessAction::LeaveUnread;
                }
            }
        }

        if args.dry_run {
            MailProcessAction::LeaveUnread
        } else {
            MailProcessAction::Remove
        }
    }))?;

    info!("{:#?}", stats);

    Ok(())
}

pub fn mail_transform(_config: &Config, args: MailTransformArgs, raw: &[u8])
    -> Result<String, failure::Error>
{
    let parsed = mailparse::parse_mail(raw)
        .context("failed to parse mail")?;
    let pre_processed = crate::mail::Mail::parse(parsed)
        .context("failed to parse mail as Daylog reply")?;
    if args.pre_transform {
        Ok(pre_processed.body)
    } else {
        let processed = process_body(&pre_processed.body);
        Ok(processed)
    }
}

fn process_body(input: &str) -> String {
    let quote_begin = Regex::new("\nOn (Mon|Tue|Wed|Thu|Fri|Sat|Sun), (Jan|Feb|Mar|Apr|May|Jun|Jul|Aug|Sep|Oct|Nov|Dec) [^>]+([^\n]>)?( |\r?\n)wrote:\r?\n\r?\n?>").unwrap();
    let signature = Regex::new("(?s)\r?\n-- \r?\n.*$").unwrap();

    signature.replace_all(&quote_begin.replace_all(input, "\n>"), "")
        .lines()
        .filter(|line| !line.starts_with('>'))
        .fold(String::new(), |mut acc, line| {
            acc.push('\n');
            acc += &line;
            acc
        })
        .trim()
        .to_string()
}
