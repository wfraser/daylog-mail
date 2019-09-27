use crate::SendArgs;
use crate::message_id;
use crate::todays_date;

pub fn send(args: SendArgs) -> Result<(), failure::Error> {
    // FIXME!!
    let crypto_key = [0u8; 32];

    let msgid = message_id::gen_message_id(
        &args.username,
        &todays_date(&args.timezone),
        crypto_key,
    ).expect("failed to generate message ID");

    println!("sample message ID: {}", msgid);

    println!("do we recognize it? {:?}", message_id::is_our_message_id(&msgid));

    let (user, date) = message_id::verify_message_id(&msgid, crypto_key)
        .expect("failed to verify message ID");

    println!("and parsed it again: {:?}, {:?}", user, date);

    Ok(())
}