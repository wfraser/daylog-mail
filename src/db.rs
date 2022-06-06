use anyhow::Context;
use crate::user::{User, Users};
use rusqlite::{named_params, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::convert::TryFrom;
use std::path::Path;

pub struct Database {
    db: rusqlite::Connection,
}

impl Database {
    pub fn open(path: &Path) -> anyhow::Result<Self> {
        let db = rusqlite::Connection::open(path)
            .with_context(|| format!("failed to open SQLite database {:?}", path))?;

        // TODO: schema upgrades

        db.execute("CREATE TABLE IF NOT EXISTS entries (\
            id INTEGER PRIMARY KEY NOT NULL,\
            username STRING NOT NULL,\
            date STRING NOT NULL,\
            body STRING NOT NULL\
        )", [])
            .context("failed to create 'entries' database table")?;

        db.execute("CREATE UNIQUE INDEX IF NOT EXISTS idx_username_date ON entries (\
            username, date\
        )", [])
            .context("failed to create index on 'entries' database table")?;

        db.execute("CREATE TABLE IF NOT EXISTS users (\
            id INTEGER PRIMARY KEY NOT NULL,\
            username STRING UNIQUE NOT NULL,\
            email STRING NOT NULL,\
            timezone STRING NOT NULL,\
            email_time_local STRING NOT NULL\
        )", [])
            .context("failed to create 'users' database table")?;

        Ok(Self {
            db,
        })
    }

    pub fn add_entry(&mut self, username: &str, date: &str, body: &str) -> anyhow::Result<()> {
        let tx = self.db.transaction()?;

        let insert_result = tx.execute(
            "INSERT INTO entries (username, date, body) \
                VALUES (:username, :date, :body)",
            named_params!{
                ":username": username,
                ":date": date,
                ":body": body,
            });

        if insert_result.is_unique_constraint_error() {
            let (id, mut update_body): (i64, String) = tx.query_row(
                "SELECT id, body FROM entries WHERE username = :username AND date = :date",
                named_params!{ ":username": username, ":date": date },
                |row| Ok((row.get(0)?, row.get(1)?)),
                )?;
            info!("updating existing row {}: {}/{}", id, username, date);
            update_body.push('\n');
            update_body +=  body;
            tx.execute(
                "UPDATE entries SET body = :body WHERE id = :id",
                named_params!{ ":body": update_body, ":id": id },
                )
                .context("failed to update existing entry")?;
        } else {
            insert_result.context("failed to insert entry")?;
        }

        tx.commit().context("failed to commit db transaction")?;
        Ok(())
    }

    pub fn get_all_users(&self) -> anyhow::Result<Users> {
        serde_rusqlite::from_rows::<UserRaw>(
            self.db.prepare("SELECT * FROM users")?
                .query([])?
        )
        .try_fold(vec![], |mut vec, u| {
            vec.push(User::try_from(u?)?);
            Ok(vec)
        })
        .map(Users::new)
    }

    pub fn get_user(&self, username: &str) -> anyhow::Result<User> {
        serde_rusqlite::from_rows::<UserRaw>(
            self.db.prepare("SELECT * FROM users WHERE username = :username")?
                .query(named_params!{ ":username": username })?
        )
        .next()
        .transpose()?
        .ok_or_else(|| anyhow::anyhow!("no such user {}", username))
        .and_then(User::try_from)
    }

    pub fn get_entry(&self, username: &str, date: &str) -> anyhow::Result<Option<String>> {
        self.db.prepare("SELECT body FROM entries \
                WHERE username = :username \
                AND date = :date")
            .context("failed to prepare entry query")?
            .query_row(
                named_params!{ ":username": username, ":date": date },
                |row| row.get::<_, String>(0)
            )
            .optional()
            .context("failed to query entry")
            .map_err(Into::into)
    }
}

#[derive(Deserialize, Serialize, Debug)]
pub struct UserRaw {
    pub id: Option<i64>,
    pub username: String,
    pub email: String,
    pub timezone: String,
    pub email_time_local: String,
}

trait RusqliteResultExt {
    fn is_unique_constraint_error(&self) -> bool;
}

impl<T> RusqliteResultExt for Result<T, rusqlite::Error> {
    fn is_unique_constraint_error(&self) -> bool {
        matches!(self, Err(rusqlite::Error::SqliteFailure(
            rusqlite::ffi::Error {
                code: rusqlite::ErrorCode::ConstraintViolation,
                extended_code: 2067, // SQLITE_CONSTRAINT_UNIQUE
            },
            ..
        )))
    }
}
