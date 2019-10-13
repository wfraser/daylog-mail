use chrono::NaiveDate;
use crate::{SendArgs, todays_date};
use crate::config::Config;
use crate::message_id::{self, read_secret_key};
use failure::ResultExt;
use std::io::{self, Write};
use std::process::{Command, Stdio};

pub fn send(config: Config, args: SendArgs) -> Result<(), failure::Error> {
    let key_bytes = read_secret_key(&config.secret_key_path)
        .with_context(|e|
            format!("failed to read secret key {:?}: {}", config.secret_key_path, e))?;

    let date = match args.date_override {
        Some(ref date) => {
            NaiveDate::parse_from_str(date, "%Y-%m-%d")
                .with_context(|e| format!("Invalid date specified ({:?}): {}", date, e))?
        }
        None => todays_date(&args.timezone),
    };

    let msgid = message_id::gen_message_id(&args.username, date, key_bytes)
        .with_context(|e| format!("failed to generate message ID: {}", e))?;

    let hostname = hostname::get_hostname()
        .ok_or_else(|| failure::err_msg("failed to get hostname"))?;

    let mut child = Command::new("sendmail")
        .arg("-i")
        .arg("-f")
        .arg(&config.return_addr)
        .arg(&args.email)
        .stdin(Stdio::piped())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .with_context(|e| format!("failed to run 'sendmail' command: {}", e))?;

    {
        let stdin = child.stdin.as_mut().expect("failed to get 'sendmail' command stdin");
        write_email(stdin, &config, &args, date, &format!("{}@{}", msgid, hostname))
            .with_context(|e| format!("failed to write email: {}", e))?;
    }

    child.wait()
        .with_context(|e| format!("failed to wait for 'mail' command: {}", e))?;

    Ok(())
}

#[allow(clippy::write_with_newline)]
fn write_email(mut w: impl Write, config: &Config, args: &SendArgs, date: NaiveDate, msgid: &str) -> io::Result<()> {
    write!(w, "Date: {}\r\n", chrono::Utc::now().to_rfc2822())?;
    write!(w, "Subject: Daylog for {}\r\n", date.format("%Y-%m-%d"))?;
    write!(w, "From: Daylog <{}>\r\n", config.return_addr)?;
    write!(w, "To: <{}>\r\n", args.email)?;
    write!(w, "Message-ID: <{}>\r\n", msgid)?;
    write!(w, "\r\n")?;
    write!(w, "What'd you do today, {}?\r\n", date.format("%A, %B %e, %Y"))?; // Sunday, July 8, 2001
    write!(w, "\r\n")?;
    write!(w, "-- \r\n")?;
    write!(w, "sent by daylog\r\n")?;
    Ok(())
}
