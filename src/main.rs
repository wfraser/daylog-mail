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

use chrono::NaiveDate;
use crate::config::Config;
use failure::Error;
use std::path::PathBuf;
use structopt::StructOpt;

#[derive(StructOpt, Debug)]
struct Args {
    #[structopt(parse(try_from_os_str = Config::try_from_arg))]
    config: Config,

    #[structopt(subcommand)]
    op: Operation,

    #[structopt(parse(from_occurrences), short("v"), long)]
    verbose: usize,
}

#[derive(StructOpt, Debug)]
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
    MailTransform,
}

#[derive(StructOpt, Debug)]
pub struct IngestArgs {
    /// show what would be done, but do not make any changes
    #[structopt(long("dry-run"))]
    dry_run: bool,
}

#[derive(StructOpt, Debug)]
pub struct SendArgs {
    /// Username
    #[structopt(long)]
    username: String,

    /// Email address to send to.
    #[structopt(long)]
    email: String,

    /// Timezone the user is in; this is used to determine the correct value for today's date.
    #[structopt(long, default_value = "UTC")]
    timezone: chrono_tz::Tz,

    /// Send email for the given date instead of today.
    #[structopt(long("date"))]
    date_override: Option<String>
}

#[derive(StructOpt, Debug)]
pub struct RunArgs {
    /// log what would be done, but do not make any changes
    #[structopt(long("dry-run"))]
    dry_run: bool,
}

#[derive(StructOpt, Debug)]
pub struct ReloadArgs {
    /// Path to a control file in use by a running daylog instance.
    /// See the `--control` flag to the `run` operation.
    /// See `daylog run --help`
    #[structopt(long("control"))]
    control_path: PathBuf,
}

fn main() -> Result<(), Error> {
    let args = Args::from_args();

    stderrlog::new()
        .module(module_path!())
        .verbosity(args.verbose)
        .init()?;

    debug!("{:#?}", args);

    match args.op {
        Operation::Ingest(op) => ingest::ingest(&args.config, op),
        Operation::Send(op) => send::send(&args.config, op),
        Operation::Run(op) => run::run(&args.config, op),
        Operation::MailTransform => {
            let mut raw_input = vec![];
            std::io::Read::read_to_end(&mut std::io::stdin(), &mut raw_input).unwrap();
            let processed = ingest::mail_transform(&args.config, &raw_input)?;
            println!("{}", processed);
            Ok(())
        }
    }
}

pub fn todays_date<Tz>(tz: &Tz) -> NaiveDate
    where Tz: chrono::TimeZone,
          Tz::Offset: std::fmt::Display,
{
    chrono::Utc::now().with_timezone(tz).date().naive_local()
}
