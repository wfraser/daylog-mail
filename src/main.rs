mod ingest;
mod message_id;
mod mail;
mod send;

use failure::Error;
use std::path::PathBuf;
use structopt::StructOpt;

#[derive(StructOpt, Debug)]
enum Args {
    /// Process incoming mail
    Ingest(IngestArgs),

    /// Send a user their daily email.
    Send(SendArgs),
}

#[derive(StructOpt, Debug)]
pub struct IngestArgs {
    /// path to the Unix mbox file to read emails from
    #[structopt(long)]
    mbox: PathBuf,

    /// path to database file
    #[structopt(long)]
    database: PathBuf,

    /// show what would be done, but do not make any changes
    #[structopt(long = "dry-run")]
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
}

fn main() -> Result<(), Error> {
    let args = Args::from_args();
    println!("{:#?}", args);

    match args {
        Args::Ingest(args) => ingest::ingest(args),
        Args::Send(args) => send::send(args),
    }
}

pub fn todays_date<Tz>(tz: &Tz) -> String
    where Tz: chrono::TimeZone,
          Tz::Offset: std::fmt::Display,
{
    chrono::Utc::now().with_timezone(tz).date().format("%Y-%m-%d").to_string()
}