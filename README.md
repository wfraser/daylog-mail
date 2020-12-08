# daylog

Daylog is a daily journal keeper, operating over email.

Every day at a configured time, it sends you an email prompting what you did
that day. Simply reply to the email, and daylog records it.

Daily emails include fun reminders of what you did at various intervals in the
past.

The recorded events are stored in a SQLite3 database, which you can query with
whatever tools you like. Daylog doesn't have any reporting or recall
capabilities yet, beyond the past events included in daily emails.

Support: I use this for my own personal stuff and don't plan on supporting it
for anyone else, but feel free to ask if you have a question or find a bug.

## Requirements

You need a host that can send and receive mail, with some other MTA software
(daylog is not a MTA).

Your MTA needs to be able to save messages in the Maildir format. Configure it
with some email address for daylog, with the maildir somewhere daylog has
permission to read and write.

You need the SQLite3 library installed.

You need a Cron daemon or some other way of running a periodic task.

## Configuration

Build daylog using Cargo.

See the [example config](config.example.yaml). Fill in the fields as
appropriate and save it somewhere.

See the [systemd unit](daylog.service). Update the paths, install, and enable
the service, which sends emails to users at the configured times.

Set up a crontab entry to run `daylog-email <path to config.yaml> ingest` on a
regular basis (at least once a day).

User configurations are stored in the SQLite3 database. There's no tool
currently to add or change users, so just edit the database:

For example:
```sql
INSERT INTO users (username, email, timezone, email_time_local) VALUES(
    'some_username', 'user@domain.com', 'America/Chicago', '18:00');
```

Then restart the service if it's already running, or wait a day and it should
pick up the new user on its own.

## Gotchas

Email is yucky. The process of reading an email sent by a user, decoding it,
and stripping away the quoted part, is tricky.  It's highly fragile, and likely
to get it wrong (particularly the quote stripping), so daylog currently saves
all the emails it receives, even after processing them, in case you need to
re-process them. It works okay when the sender uses GMail, but other mail
clients haven't been tested much.

The email mangling code is at [`src/ingest.rs`](src/ingest.rs), particularly
the `process_body` function.

To test the mail transformation, daylog has a subcommand `daylog-email
mail-transform` which reads an email from standard input and writes the
transformed version to standard output. Use this to iterate on any changes to
the email mangling code.

Email clients that only send HTML messages, without any plaintext part, are
unsupported. Daylog makes no attempt at interpreting HTML.

