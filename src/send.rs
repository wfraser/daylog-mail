use crate::{SendArgs, todays_date};
use crate::message_id::{self, read_secret_key};
use failure::ResultExt;
use std::io::Write;
use std::process::{Command, Stdio};

pub fn send(args: SendArgs) -> Result<(), failure::Error> {
    let key_bytes = read_secret_key(&args.common_args.key_path)
        .context("failed to read secret key")?;

    let today = todays_date(&args.timezone);

    let msgid = message_id::gen_message_id(&args.username, &today, key_bytes)
        .expect("failed to generate message ID");

    let mut child = Command::new("mail")
        .arg("-C")
        .arg(format!("Message-ID: <{}>", msgid))
        .arg("-r")
        .arg(args.return_addr)
        .arg("-s")
        .arg(format!("Daylog for {}", today))
        .arg("-.")
        .arg(args.email)
        .stdin(Stdio::piped())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .with_context(|e| format!("failed to run 'mail' command: {}", e))?;

    {
        let stdin = child.stdin.as_mut().expect("failed to get 'mail' command stdin");
        write!(stdin, "What'd you do today, {}?\r\n\r\n-- \r\nsent by daylog\r\n", today)
            .with_context(|e| format!("failed to write email: {}", e))?;
    }

    child.wait()
        .with_context(|e| format!("failed to wait for 'mail' command: {}", e))?;

    Ok(())
}
