use crate::time::DaylogTime;
use failure::ResultExt;
use rusqlite::{named_params, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_rusqlite::{columns_from_statement, from_row_with_columns};
use std::str::FromStr;
use std::path::Path;

pub struct Database {
    db: rusqlite::Connection,
}

impl Database {
    pub fn open(path: &Path) -> Result<Self, failure::Error> {
        let db = rusqlite::Connection::open(path)
            .with_context(|e| format!("failed to open SQLite database {:?}: {}", path, e))?;

        // TODO: schema upgrades

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
            email_time_local STRING NOT NULL\
        )", rusqlite::NO_PARAMS)
            .with_context(|e| format!("failed to create 'users' database table: {}", e))?;

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
            info!("updating existing row {}: {}/{}", id, username, date);
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

    /*
    pub fn get_next_send_time(&self, from_time: DaylogTime) -> Result<Option<DaylogTime>, failure::Error> {
        let next_time: Option<String> = self.db.query_row_named(
            "SELECT email_time_utc FROM users WHERE email_time_utc >= :from_time ORDER BY email_time_utc ASC LIMIT 1",
            named_params!{ ":from_time": from_time.to_string() },
            |row| row.get(0),
            )
            .optional()
            .with_context(|e| format!("failed to query next email send time from database: {}", e))?;
        match next_time {
            Some(ref time) => {
                let parsed_time = DaylogTime::parse(time)
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
    */

    pub fn get_all_users(&self) -> Result<Users, failure::Error> {
        let mut users = vec![];
        let mut stmt = self.db.prepare("SELECT * FROM users")?;
        let columns = columns_from_statement(&stmt);
        let mut rows = stmt.query(rusqlite::NO_PARAMS)?;
        while let Some(row) = rows.next()? {
            let user_raw: UserRaw = from_row_with_columns(row, &columns)
                .with_context(|e| format!("failed to deserialize user: {}", e))?;
            let tz = chrono_tz::Tz::from_str(&user_raw.timezone)
                .map_err(|e| {
                    failure::err_msg(format!("failed to parse timezone {:?} for user {}: {}",
                        user_raw.timezone, user_raw.username, e))
                })?;
            let time = DaylogTime::parse(&user_raw.email_time_local)
                .with_context(|e| format!("bogus email time {:?} for user {}: {}",
                    user_raw.email_time_local, user_raw.username, e))?;
            users.push(User {
                id: user_raw.id.expect("missing user ID from database row"),
                username: user_raw.username,
                email: user_raw.email,
                timezone: tz,
                email_time_local: time,
            });
        }
        Ok(Users::new(users))
    }
}

/*
pub struct UsersBySendTimeQuery<'db> {
    db: &'db Database,
    stmt: rusqlite::Statement<'db>,
    columns: Vec<String>,
}

impl<'db> UsersBySendTimeQuery<'db> {
    pub fn for_time(&mut self, time: DaylogTime) -> Result<UsersQueryResult<'_>, failure::Error> {
        let rows = self.stmt.query_named(
            named_params!{ ":time": time.to_string() },
            )?;
        Ok(UsersQueryResult { rows, columns: &self.columns })
    }

    pub fn next_from_time(&mut self, curr_time: DaylogTime) -> Result<Option<(DaylogTime, UsersQueryResult<'_>)>, failure::Error> {
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
*/

#[derive(Deserialize, Serialize, Debug)]
pub struct UserRaw {
    pub id: Option<i64>,
    pub username: String,
    pub email: String,
    pub timezone: String,
    pub email_time_local: String,
}

#[derive(Debug)]
pub struct User {
    pub id: i64,
    pub username: String,
    pub email: String,
    pub timezone: chrono_tz::Tz,
    pub email_time_local: DaylogTime,
}

pub struct Users {
    vec: Vec<User>,
}

impl Users {
    pub fn new(users: Vec<User>) -> Self {
        Self { vec: users }
    }

    pub fn next_from_time(&self, time: DaylogTime) -> Option<(DaylogTime, Vec<User>)> {
        unimplemented!()
    }
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
