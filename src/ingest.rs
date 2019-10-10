use crate::mail::{self, MailSource};
use crate::message_id::{is_our_message_id, read_secret_key, verify_message_id};
use crate::IngestArgs;
use failure::ResultExt;

pub fn ingest(args: IngestArgs) -> Result<(), failure::Error> {
    let key_bytes = read_secret_key(&args.common_args.key_path)
        .with_context(|e|
            format!("failed to read secret key {:?}: {}", args.common_args.key_path, e))?;

    let mbox = mail::UnixMbox::open(&args.mbox_path)?;
    let mut num_processed = 0;
    let mut num_actioned = 0;
    for mail_result in mbox.read()? {
        num_processed += 1;
        let mail = mail_result?;

        let mut msgids = vec![];
        for msgid in mail.reply_to {
            if is_our_message_id(&msgid) {
                msgids.push(msgid);
            }
        }

        if !msgids.is_empty() {
            if args.dry_run {
                println!("Message {:?} is interesting", mail.msgid);
            }
            // TODO: process the email body
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
                        continue;
                    }
                };

                // TODO: stuff
                num_actioned += 1;
            }
        }
    }

    println!("{} mails read, {} actioned", num_processed, num_actioned);

    if !args.dry_run {
        mbox.truncate();
    }

    Ok(())
}