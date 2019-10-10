use crate::{SendArgs, todays_date};
use crate::message_id::{self, read_secret_key};
use failure::ResultExt;

pub fn send(args: SendArgs) -> Result<(), failure::Error> {
    let key_bytes = read_secret_key(&args.common_args.key_path)
        .context("failed to read secret key")?;

    let msgid = message_id::gen_message_id(
        &args.username,
        &todays_date(&args.timezone),
        key_bytes,
    ).expect("failed to generate message ID");

    println!("sample message ID: {}", msgid);

    println!("do we recognize it? {:?}", message_id::is_our_message_id(&msgid));

    let (user, date) = message_id::verify_message_id(&msgid, key_bytes)
        .expect("failed to verify message ID");

    println!("and parsed it again: {:?}, {:?}", user, date);

    Ok(())
}
