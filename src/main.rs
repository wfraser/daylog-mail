mod config;
mod db;
mod ingest;
mod message_id;
mod mail;
mod maildir;
mod send;

use chrono::NaiveDate;
use crate::config::Config;
use failure::Error;
use structopt::StructOpt;

#[derive(StructOpt, Debug)]
struct Args {
    #[structopt(parse(try_from_os_str = Config::try_from_arg))]
    config: Config,

    #[structopt(subcommand)]
    op: Operation,
}

#[derive(StructOpt, Debug)]
enum Operation {
    /// Process incoming mail
    Ingest(IngestArgs),

    /// Send a user their daily email.
    Send(SendArgs),
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

fn main() -> Result<(), Error> {
    let args = Args::from_args();
    println!("{:#?}", args);

    match args.op {
        Operation::Ingest(op) => ingest::ingest(args.config, op),
        Operation::Send(op) => send::send(args.config, op),
    }
}

pub fn todays_date<Tz>(tz: &Tz) -> NaiveDate
    where Tz: chrono::TimeZone,
          Tz::Offset: std::fmt::Display,
{
    chrono::Utc::now().with_timezone(tz).date().naive_local()
}
