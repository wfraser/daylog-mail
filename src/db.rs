use chrono::NaiveTime;
use failure::ResultExt;
use rusqlite::{named_params, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_rusqlite::{columns_from_statement, from_row_with_columns};
use std::path::Path;

pub(crate) const TIME_FORMAT: &str = "%H:%M";

pub struct Database {
    db: rusqlite::Connection,
}

impl Database {
    pub fn open(path: &Path) -> Result<Self, failure::Error> {
        let db = rusqlite::Connection::open(path)
            .with_context(|e| format!("failed to open SQLite database {:?}: {}", path, e))?;

        db.execute("CREATE TABLE IF NOT EXISTS entries (\
            id INTEGER PRIMARY KEY NOT NULL,\
            username STRING NOT NULL,\
            date STRING NOT NULL,\
            body STRING NOT NULL\
        )", rusqlite::NO_PARAMS)
            .with_context(|e| format!("failed to create 'entries' database table: {}", e))?;

        db.execute("CREATE UNIQUE INDEX IF NOT EXISTS idx_username_date ON entries (\
            username, date\
        )", rusqlite::NO_PARAMS)
            .with_context(|e| format!("failed to create index on 'entries' database table: {}", e))?;

        db.execute("CREATE TABLE IF NOT EXISTS users (\
            id INTEGER PRIMARY KEY NOT NULL,\
            username STRING UNIQUE NOT NULL,\
            email STRING NOT NULL,\
            timezone STRING NOT NULL,\
            email_time_utc STRING NOT NULL\
        )", rusqlite::NO_PARAMS)
            .with_context(|e| format!("failed to create 'users' database table: {}", e))?;

        db.execute("CREATE INDEX IF NOT EXISTS idx_emailtime_userid ON users (\
            email_time_utc, id\
        )", rusqlite::NO_PARAMS)
            .with_context(|e| format!("failed to create index on 'users' database table: {}", e))?;

        Ok(Self {
            db,
        })
    }

    pub fn add_entry(&mut self, username: &str, date: &str, body: &str) -> Result<(), failure::Error> {
        let tx = self.db.transaction()?;

        let insert_result = tx.execute_named(
            "INSERT INTO entries (username, date, body) \
                VALUES (:username, :date, :body)",
            named_params!{
                ":username": username,
                ":date": date,
                ":body": body,
            });

        if insert_result.is_unique_constraint_error() {
            let (id, mut update_body): (i64, String) = tx.query_row_named(
                "SELECT id, body FROM entries WHERE username = :username AND date = :date",
                named_params!{ ":username": username, ":date": date },
                |row| Ok((row.get(0)?, row.get(1)?)),
                )?;
            eprintln!("updating existing row {}: {}/{}", id, username, date);
            update_body.push('\n');
            update_body +=  body;
            tx.execute_named(
                "UPDATE entries SET body = :body WHERE id = :id",
                named_params!{ ":body": update_body, ":id": id },
                )
                .context("failed to update existing entry")?;
        } else {
            insert_result.with_context(|e| format!("failed to insert entry: {}", e))?;
        }

        tx.commit().context("failed to commit db transaction")?;
        Ok(())
    }

    pub fn get_next_send_time(&self, from_time: NaiveTime) -> Result<Option<NaiveTime>, failure::Error> {
        let next_time: Option<String> = self.db.query_row_named(
            "SELECT email_time_utc FROM users WHERE email_time_utc >= :from_time ORDER BY email_time_utc ASC LIMIT 1",
            named_params!{ ":from_time": from_time.format(TIME_FORMAT).to_string() },
            |row| row.get(0),
            )
            .optional()
            .with_context(|e| format!("failed to query next email send time from database: {}", e))?;
        match next_time {
            Some(ref time) => {
                let parsed_time = NaiveTime::parse_from_str(time, TIME_FORMAT)
                    .with_context(|e| format!("bad time from the database: {:?} because {}", time, e))?;
                Ok(Some(parsed_time))
            }
            None => Ok(None)
        }
    }

    pub fn get_users_to_send(&self) -> Result<UsersBySendTimeQuery<'_>, failure::Error> {
        let stmt = self.db.prepare("SELECT * FROM users WHERE email_time_utc = :time")?;
        let columns = columns_from_statement(&stmt);
        Ok(UsersBySendTimeQuery { db: self, stmt, columns })
    }
}

pub struct UsersBySendTimeQuery<'db> {
    db: &'db Database,
    stmt: rusqlite::Statement<'db>,
    columns: Vec<String>,
}

impl<'db> UsersBySendTimeQuery<'db> {
    pub fn for_time(&mut self, time: NaiveTime) -> Result<UsersQueryResult<'_>, failure::Error> {
        let rows = self.stmt.query_named(
            named_params!{ ":time": time.format(TIME_FORMAT).to_string() },
            )?;
        Ok(UsersQueryResult { rows, columns: &self.columns })
    }

    pub fn next_from_time(&mut self, curr_time: NaiveTime) -> Result<Option<(NaiveTime, UsersQueryResult<'_>)>, failure::Error> {
        match self.db.get_next_send_time(curr_time)? {
            Some(time) => {
                Ok(Some((time, self.for_time(time)?)))
            }
            None => Ok(None),
        }
    }
}

pub struct UsersQueryResult<'stmt> {
    rows: rusqlite::Rows<'stmt>,
    columns: &'stmt [String],
}

impl<'stmt> Iterator for UsersQueryResult<'stmt> {
    type Item = User;
    fn next(&mut self) -> Option<Self::Item> {
        // TODO: should this return Result instead of panicking?
        let columns = &self.columns;
        self.rows.next().expect("failed to advance")
            .map(|row| {
                from_row_with_columns::<User>(row, columns)
                    .expect("failed to deserialize database row to User")
            })
    }
}

#[derive(Deserialize, Serialize, Debug)]
pub struct User {
    pub id: Option<i64>,
    pub username: String,
    pub email: String,
    pub timezone: String,
    pub email_time_utc: String,
}

trait RusqliteResultExt {
    fn is_unique_constraint_error(&self) -> bool;
}

impl<T> RusqliteResultExt for Result<T, rusqlite::Error> {
    fn is_unique_constraint_error(&self) -> bool {
        match self {
            Err(rusqlite::Error::SqliteFailure(
                    rusqlite::ffi::Error {
                        code: rusqlite::ErrorCode::ConstraintViolation,
                        extended_code: 2067, // SQLITE_CONSTRAINT_UNIQUE
                    },
                    ..
            )) => true,
            _ => false,
        }
    }
}
