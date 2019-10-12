use failure::ResultExt;
use rusqlite::named_params;
use std::path::Path;

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
            .with_context(|e| format!("failed to create database table: {}", e))?;

        db.execute("CREATE UNIQUE INDEX IF NOT EXISTS idx_username_date ON entries (\
            username, date\
        )", rusqlite::NO_PARAMS)
            .with_context(|e| format!("failed to create database index: {}", e))?;

        Ok(Self {
            db,
        })
    }

    pub fn add_entry(&mut self, username: &str, date: &str, body: &str) -> Result<(), failure::Error> {
        let tx = self.db.transaction()?;
        let insert_result = tx.execute_named("INSERT INTO entries (username, date, body) \
                VALUES (:username, :date, :body)",
            named_params!{
                ":username": username,
                ":date": date,
                ":body": body,
            });

        if let Err(rusqlite::Error::SqliteFailure(rusqlite::ffi::Error {
            code: rusqlite::ErrorCode::ConstraintViolation,
            extended_code: 2067, // SQLITE_CONSTRAINT_UNIQUE
        }, _msg)) = insert_result {
            let (id, mut update_body): (i64, String) = tx.query_row_named(
                "SELECT id, body FROM entries WHERE username = :username AND date = :date",
                rusqlite::named_params!{":username": username, ":date": date},
                |row| Ok((row.get(0)?, row.get(1)?)))?;
            eprintln!("updating existing row {}: {}/{}", id, username, date);
            update_body.push('\n');
            update_body +=  body;
            tx.execute_named(
                "UPDATE entries SET body = :body WHERE id = :id",
                named_params!{":body": update_body, ":id": id})
                .context("failed to update existing entry")?;
        } else {
            insert_result.with_context(|e| format!("failed to insert entry: {}", e))?;
        }

        tx.commit().context("failed to commit db transaction")?;
        Ok(())
    }
}
