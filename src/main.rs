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

    Ok(())
}
