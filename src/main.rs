mod db;
mod ingest;
mod message_id;
mod mail;
mod maildir;
mod send;

use chrono::NaiveDate;
use failure::Error;
use std::path::PathBuf;
use structopt::StructOpt;

#[derive(StructOpt, Debug)]
struct Args {
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
pub struct CommonArgs {
    /// Path to the secret key used for generating and reading message IDs.
    /// Must contain 32 bytes of data.
    #[structopt(long("key"))]
    key_path: PathBuf,
}

#[derive(StructOpt, Debug)]
pub struct IngestArgs {
    #[structopt(flatten)]
    common_args: CommonArgs,

    #[structopt(flatten)]
    source: MailSourceLocation,

    /// path to database file
    #[structopt(long("database"))]
    database_path: PathBuf,

    /// show what would be done, but do not make any changes
    #[structopt(long("dry-run"))]
    dry_run: bool,
}

#[derive(StructOpt, Debug)]
pub enum MailSourceLocation {
    /// Maildir path
    Maildir{ path: PathBuf },

    // and maybe other sources in the future?
}

#[derive(StructOpt, Debug)]
pub struct SendArgs {
    #[structopt(flatten)]
    common_args: CommonArgs,

    /// Username
    #[structopt(long)]
    username: String,

    /// Email address to send to.
    #[structopt(long)]
    email: String,

    /// Timezone the user is in; this is used to determine the correct value for today's date.
    #[structopt(long, default_value = "UTC")]
    timezone: chrono_tz::Tz,

    /// The value for the From: address on outgoing mail
    #[structopt(long("return-addr"))]
    return_addr: String,

    /// Send email for the given date instead of today.
    #[structopt(long("date"))]
    date_override: Option<String>
}

fn main() -> Result<(), Error> {
    let args = Args::from_args();
    println!("{:#?}", args);

    match args.op {
        Operation::Ingest(op) => ingest::ingest(op),
        Operation::Send(op) => send::send(op),
    }
}

pub fn todays_date<Tz>(tz: &Tz) -> NaiveDate
    where Tz: chrono::TimeZone,
          Tz::Offset: std::fmt::Display,
{
    chrono::Utc::now().with_timezone(tz).date().naive_local()
}
