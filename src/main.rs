mod message_id;
mod mail;

use crate::mail::MailSource;
use failure::Error;
use std::path::PathBuf;

fn main() -> Result<(), Error> {
    let mbox = mail::UnixMbox::from_path(PathBuf::from(std::env::args_os().nth(1).expect("missing mbox path argument")));
    let read = mbox.open_for_read()?;
    for mail_result in read.peek()? {
        let mail = mail_result?;
        println!("{:#?}", mail);
    }

    // FIXME!!
    let crypto_key = [0u8; 32];

    let msgid = message_id::gen_message_id(
        &std::env::var("USERNAME").unwrap(),
        &todays_date(),
        crypto_key,
    ).expect("failed to generate message ID");

    println!("sample message ID: {}", msgid);

    println!("do we recognize it? {:?}", message_id::is_our_message_id(&msgid));

    let (user, date) = message_id::verify_message_id(&msgid, crypto_key)
        .expect("failed to verify message ID");

    println!("and parsed it again: {:?}, {:?}", user, date);

    Ok(())
}

fn todays_date() -> String {
    chrono::Utc::today().format("%Y-%m-%d").to_string()
}