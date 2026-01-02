use anyhow::{anyhow, Context};
use chrono::{Datelike, Duration, NaiveDate};
use crate::{SendArgs, todays_date};
use crate::config::Config;
use crate::db::Database;
use crate::message_id::{self, read_secret_key};
use std::borrow::Cow;
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
                        .with_context(|| format!("Invalid date specified ({date:?})"))?
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
        .map_err(|bad| anyhow!("invalid hostname: {bad:?}"))?;

    if dry_run {
        write_email(io::stdout(), config, &username, &email, &db, date,
                    &format!("{msgid}@{hostname}"))
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
                    &format!("{msgid}@{hostname}"))
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
    write!(w, "To: <{email}>\r\n")?;
    write!(w, "Message-ID: <{msgid}>\r\n")?;
    write!(w, "\r\n")?;
    write!(w, "What'd you do today, {}?\r\n", date.format("%A, %B %e, %Y"))?; // Sunday, July 8, 2001
    write!(w, "\r\n")?;

    use Cow::Borrowed as B;
    let mut past_times = vec![
        (B("one week ago"), Some(date - Duration::weeks(1))),
        (B("two weeks ago"), Some(date - Duration::weeks(2))),
        (B("three weeks ago"), Some(date - Duration::weeks(3))),
        (B("one month ago"), months_ago(date, 1)),
        (B("two months ago"), months_ago(date, 2)),
        (B("three months ago"), months_ago(date, 3)),
        (B("four months ago"), months_ago(date, 4)),
        (B("five months ago"), months_ago(date, 5)),
        (B("six months ago"), months_ago(date, 6)),
        (B("one year ago"), years_ago(date, 1)),
    ];


    if let Some(ymd_str) = db.oldest_entry_date(username)? {
        let oldest = NaiveDate::parse_from_str(&ymd_str, "%Y-%m-%d").context("invalid date")?;

        for years in 2.. {
            let Some(ts) = years_ago(date, years) else {
                continue;
            };
            if ts < oldest {
                break;
            }
            past_times.push((Cow::Owned(format!("{} years ago", english(years))), Some(ts)));
        }
    }

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
                eprintln!("error querying database for {username}/{past_date}: {e}");
            }
        }
    }

    if !past_events.is_empty() {
        write!(w, "Here's what you were doing\r\n")?;
    }
    for (label, body) in &past_events {
        let lines = body.lines().collect::<Vec<_>>();
        if lines.len() > 1 {
            write!(w, "\t{label}:\r\n")?;
            for line in &lines {
                write!(w, "\t\t{line}\r\n")?;
            }
        } else {
            write!(w, "\t{label}:\t{body}\r\n")?;
        }
    }
    if !past_events.is_empty() {
        write!(w, "\r\n")?;
    }

    write!(w, "-- \r\n")?;
    write!(w, "sent by daylog\r\n")?;
    Ok(())
}

fn months_ago(date: NaiveDate, months: u32) -> Option<NaiveDate> {
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

fn years_ago(date: NaiveDate, years: u16) -> Option<NaiveDate> {
    NaiveDate::from_ymd_opt(
        date.year() - i32::from(years),
        date.month(),
        date.day()
    )
}

/// Return the english number word(s) for inputs less than 100.
/// For numbers >= 100, formats it as a decimal number string.
fn english(n: u16) -> Cow<'static, str> {
    Cow::Borrowed(match n {
        0 => "zero",
        1 => "one",
        2 => "two",
        3 => "three",
        4 => "four",
        5 => "five",
        6 => "six",
        7 => "seven",
        8 => "eight",
        9 => "nine",
        10 => "ten",
        11 => "eleven",
        12 => "twelve",
        13 => "thirteen",
        14 => "fourteen",
        15 => "fifteen",
        16 => "sixteen",
        17 => "seventeen",
        18 => "eighteen",
        19 => "nineteen",
        20 .. 100 => {
            let tens = match n / 10 {
                2 => "twenty",
                3 => "thirty",
                4 => "forty",
                5 => "fifty",
                6 => "sixty",
                7 => "seventy",
                8 => "eighty",
                9 => "ninety",
                _ => unreachable!(),
            };
            let ones = n % 10;
            if ones == 0 {
                tens
            } else {
                return Cow::Owned(format!("{tens}-{}", english(ones)));
            }
        }
        _ => return Cow::Owned(format!("{n}")),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_english() {
        assert_eq!(english(0), "zero");
        assert_eq!(english(1), "one");
        assert_eq!(english(10), "ten");
        assert_eq!(english(16), "sixteen");
        assert_eq!(english(25), "twenty-five");
        assert_eq!(english(99), "ninety-nine");
        assert_eq!(english(100), "100");
        assert_eq!(english(255), "255");
    }
}
