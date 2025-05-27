#[macro_use] extern crate log;

mod config;
mod db;
mod ingest;
mod message_id;
mod mail;
mod maildir;
mod run;
mod send;
mod time;
mod user;

use chrono::NaiveDate;
use clap::Parser;
use crate::config::{Config, ConfigParser};

#[derive(Parser, Debug)]
#[clap(version, author, about)]
struct Args {
    #[clap(value_parser = ConfigParser)]
    config: Config,

    #[clap(subcommand)]
    op: Operation,

    #[clap(action = clap::ArgAction::Count, short('v'), long)]
    verbose: u8,
}

#[derive(Parser, Debug)]
enum Operation {
    /// Process incoming mail
    Ingest(IngestArgs),

    /// Send a user their daily email.
    Send(SendArgs),

    /// Run as a service, blocking indefinitely. Send all users their daily mail at the
    /// pre-configured time, and process incoming mail periodically.
    Run(RunArgs),

    /// Read a raw email from standard input, and write to standard output the sanitized version of
    /// it. This does not alter the database.
    MailTransform(MailTransformArgs),
}

#[derive(Parser, Debug)]
pub struct IngestArgs {
    /// show what would be done, but do not make any changes
    #[clap(long)]
    dry_run: bool,
}

#[derive(Parser, Debug)]
pub struct SendArgs {
    /// Username
    #[clap(long)]
    username: String,

    /// Different email address to send to.
    #[clap(long("email"))]
    email_override: Option<String>,

    /// Send email for the given date instead of today.
    #[clap(long("date"))]
    date_override: Option<String>,

    /// Print the email to stdout, but do not send it.
    #[clap(long)]
    dry_run: bool,
}

#[derive(Parser, Debug)]
pub struct RunArgs {
    /// log what would be done, but do not make any changes
    #[clap(long)]
    dry_run: bool,
}

#[derive(Parser, Debug)]
pub struct MailTransformArgs {
    /// Print the plain-text mail body without applying any transformations on it.
    #[clap(long)]
    pre_transform: bool,
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    stderrlog::new()
        .module(module_path!())
        .verbosity(args.verbose as usize)
        .init()?;

    debug!("{:#?}", args);

    match args.op {
        Operation::Ingest(op) => ingest::ingest(&args.config, op),
        Operation::Send(op) => send::send(&args.config, send::Mode::Args(op)),
        Operation::Run(op) => run::run(&args.config, op),
        Operation::MailTransform(op) => {
            let mut raw_input = vec![];
            std::io::Read::read_to_end(&mut std::io::stdin(), &mut raw_input).unwrap();
            let processed = ingest::mail_transform(&args.config, op, &raw_input)?;
            println!("{}", processed);
            Ok(())
        }
    }
}

pub fn todays_date<Tz>(tz: &Tz) -> NaiveDate
    where Tz: chrono::TimeZone,
          Tz::Offset: std::fmt::Display,
{
    chrono::Utc::now().with_timezone(tz).date_naive()
}
