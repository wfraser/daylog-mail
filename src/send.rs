use anyhow::{anyhow, Context};
use chrono::{Datelike, Duration, NaiveDate};
use crate::{SendArgs, todays_date};
use crate::config::Config;
use crate::db::Database;
use crate::message_id::{self, read_secret_key};
use std::io::{self, Write};
use std::process::{Command, Stdio};

// This is used in two ways: from the command line, and internally.
pub enum Mode {
    // Use configuration from the command line and read the user from the database.
    Args(SendArgs),

    // User already loaded from the database.
    User(crate::user::User),
}

pub fn send(config: &Config, mode: Mode) -> anyhow::Result<()> {
    let key_bytes = read_secret_key(&config.secret_key_path)
        .with_context(|| format!("failed to read secret key {:?}", config.secret_key_path))?;

    let db = Database::open(&config.database_path)?;

    let username: String;
    let email: String;
    let date: NaiveDate;
    let dry_run: bool;

    match mode {
        Mode::User(user) => {
            username = user.username;
            email = user.email;
            date = todays_date(&user.timezone);
            dry_run = false;
        }
        Mode::Args(args) => {
            username = args.username;

            let user = db.get_user(&username)?;

            email = args.email_override.unwrap_or(user.email);
            date = match args.date_override {
                Some(ref date) => {
                    NaiveDate::parse_from_str(date, "%Y-%m-%d")
                        .with_context(|| format!("Invalid date specified ({:?})", date))?
                }
                None => todays_date(&user.timezone),
            };
            dry_run = args.dry_run;
        }
    }

    let msgid = message_id::gen_message_id(&username, date, key_bytes)
        .context("failed to generate message ID")?;

    let hostname = hostname::get()
        .context("failed to get hostname")?
        .into_string()
        .map_err(|bad| anyhow!("invalid hostname: {:?}", bad))?;

    if dry_run {
        write_email(io::stdout(), config, &username, &email, &db, date,
                    &format!("{}@{}", msgid, hostname))
            .context("failed to write email")?;
        return Ok(());
    }

    let mut child = Command::new("sendmail")
        .arg("-i")
        .arg("-f")
        .arg(&config.return_addr)
        .arg(&email)
        .stdin(Stdio::piped())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .context("failed to run 'sendmail' command")?;

    {
        let sendmail = child.stdin.as_mut().expect("failed to get 'sendmail' command stdin");
        write_email(sendmail, config, &username, &email, &db, date,
                    &format!("{}@{}", msgid, hostname))
            .context("failed to write email")?;
    }

    child.wait()
        .context("failed to wait for 'mail' command")?;

    Ok(())
}

#[allow(clippy::write_with_newline)]
fn write_email(
    mut w: impl Write,
    config: &Config,
    username: &str,
    email: &str,
    db: &Database,
    date: NaiveDate,
    msgid: &str,
) -> anyhow::Result<()> {
    write!(w, "Date: {}\r\n", chrono::Utc::now().to_rfc2822())?;
    write!(w, "Subject: Daylog for {}\r\n", date.format("%Y-%m-%d"))?;
    write!(w, "From: Daylog <{}>\r\n", config.return_addr)?;
    write!(w, "To: <{}>\r\n", email)?;
    write!(w, "Message-ID: <{}>\r\n", msgid)?;
    write!(w, "\r\n")?;
    write!(w, "What'd you do today, {}?\r\n", date.format("%A, %B %e, %Y"))?; // Sunday, July 8, 2001
    write!(w, "\r\n")?;

    fn months_ago(date: NaiveDate, months: i32) -> Option<NaiveDate> {
        let mut year = date.year();
        let mut month = date.month();
        let day = date.day();

        for _ in 0 .. months {
            month -= 1;
            if month == 0 {
                month = 12;
                year -= 1;
            }
        }

        NaiveDate::from_ymd_opt(year, month, day)
    }

    fn years_ago(date: NaiveDate, years: i32) -> Option<NaiveDate> {
        NaiveDate::from_ymd_opt(
            date.year() - years,
            date.month(),
            date.day()
        )
    }

    let past_times = [
        ("one week ago", Some(date - Duration::weeks(1))),
        ("two weeks ago", Some(date - Duration::weeks(2))),
        ("three weeks ago", Some(date - Duration::weeks(3))),
        ("one month ago", months_ago(date, 1)),
        ("two months ago", months_ago(date, 2)),
        ("three months ago", months_ago(date, 3)),
        ("four months ago", months_ago(date, 4)),
        ("five months ago", months_ago(date, 5)),
        ("six months ago", months_ago(date, 6)),
        ("one year ago", years_ago(date, 1)),
        ("two years ago", years_ago(date, 2)),
        ("three years ago", years_ago(date, 3)),
        ("four years ago", years_ago(date, 4)),
        ("five years ago", years_ago(date, 5)),
        ("six years ago", years_ago(date, 6)),
        ("seven years ago", years_ago(date, 7)),
        ("eight years ago", years_ago(date, 8)),
        ("nine years ago", years_ago(date, 9)),
        ("ten years ago", years_ago(date, 10)),
    ];

    let mut past_events = vec![];
    for (label, past_date) in &past_times {
        let past_date = match past_date {
            Some(d) => d.format("%Y-%m-%d").to_string(),
            None => continue,
        };

        match db.get_entry(username, &past_date) {
            Ok(Some(body)) => {
                past_events.push((label, body));
            },
            Ok(None) => (),
            Err(e) => {
                eprintln!("error querying database for {}/{}: {}", username, past_date, e);
            }
        }
    }

    if !past_events.is_empty() {
        write!(w, "Here's what you were doing\r\n")?;
    }
    for (label, body) in &past_events {
        let lines = body.lines().collect::<Vec<_>>();
        if lines.len() > 1 {
            write!(w, "\t{}:\r\n", label)?;
            for line in &lines {
                write!(w, "\t\t{}\r\n", line)?;
            }
        } else {
            write!(w, "\t{}:\t{}\r\n", label, body)?;
        }
    }
    if !past_events.is_empty() {
        write!(w, "\r\n")?;
    }

    write!(w, "-- \r\n")?;
    write!(w, "sent by daylog\r\n")?;
    Ok(())
}
