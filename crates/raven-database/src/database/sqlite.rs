use std::fs;

mod query;
use log::{debug, error};
use query::{Query, SqlString};
use raven_common::utils::get_data_dir;
use rusqlite::{Connection, DropBehavior, OpenFlags, ToSql, named_params};
use time::OffsetDateTime;

use crate::history::model::History;

use super::{Database, DatabaseError};

const DATABASE_FILE: &str = "raven.db";

/// Sqlite database wrapper using rusqlite
pub struct Sqlite {
    pub conn: Connection,
}

impl Sqlite {
    /// Creates a new [`Sqlite`].
    ///
    /// # Panics
    ///
    /// Panics if a data directory for the database file cannot be found.
    #[must_use]
    pub fn new() -> Self {
        let _ = fs::create_dir_all(get_data_dir());
        let path = get_data_dir().join(DATABASE_FILE);
        let conn = get_connection(
            path.to_str()
                .expect("Could not generate database file path."),
        );
        Self { conn }
    }
}

impl Default for Sqlite {
    fn default() -> Self {
        Self::new()
    }
}

// From rusqlite errors to Raven's internal DatabaseError type.
impl From<rusqlite::Error> for DatabaseError {
    fn from(value: rusqlite::Error) -> Self {
        Self {
            msg: format!("{value}"),
        }
    }
}

impl Database for Sqlite {
    fn save(&mut self, history: &History) -> Result<i64, DatabaseError> {
        let query = Query::insert()
            .column("timestamp")
            .column("command")
            .column("cwd")
            .column("exit_code")
            .table("history")
            .to_owned();

        let stmt = self.conn.prepare(query.to_sql().as_str());
        let result = stmt?.insert(named_params! {
            ":timestamp": history.timestamp.unix_timestamp(),
            ":command": history.command,
            ":cwd": history.cwd,
            ":exit_code": history.exit_code,
        });
        Ok(result?)
    }

    fn save_bulk(&mut self, history: &[History]) -> Result<Vec<i64>, DatabaseError> {
        let mut row_ids: Vec<i64> = Vec::new();

        let mut tx = self.conn.transaction().expect("expected transaction");
        tx.set_drop_behavior(DropBehavior::Rollback);

        let query = Query::insert()
            .column("timestamp")
            .column("command")
            .column("cwd")
            .column("exit_code")
            .table("history")
            .to_owned();
        let mut stmt = tx.prepare(query.to_sql().as_str()).unwrap();
        for h in history {
            match stmt.insert(named_params! {
                ":timestamp": h.timestamp.unix_timestamp(),
                ":command": h.command,
                ":cwd": h.cwd,
                ":exit_code": h.exit_code,
            }) {
                Ok(row_id) => row_ids.push(row_id),
                Err(err) => {
                    return Err(err.into());
                }
            };
        }
        drop(stmt);
        let _ = tx.commit();
        Ok(row_ids)
    }

    fn get(&self, id: i64) -> Result<Option<History>, DatabaseError> {
        let query = Query::select()
            .column("id")
            .column("command")
            .column("cwd")
            .column("exit_code")
            .column("timestamp")
            .from("history")
            .r#where("id")
            .to_owned();

        let mut stmt = self
            .conn
            // .prepare("SELECT id, command, cwd, exit_code, timestamp FROM history WHERE id=?1")?;
            .prepare(query.to_sql().as_str())?;

        let h = stmt.query_row([id], |row| {
            Ok(History::builder()
                .id(row.get("id")?)
                .command(row.get("command")?)
                .cwd(row.get("cwd")?)
                .exit_code(row.get("exit_code")?)
                .timestamp(
                    OffsetDateTime::from_unix_timestamp(row.get("timestamp")?)
                        .expect("Failed to parse timestamp"),
                )
                .build())
        });

        match h {
            Ok(h) => Ok(Some(h)),
            Err(e) => Err(e.into()),
        }
    }

    fn update(&self, history: &History) -> Result<(), DatabaseError> {
        if history.id == -1 {
            return Err(DatabaseError {
                msg: "Cannot update object with -1 ID, try save first.".to_string(),
            });
        }

        let query = Query::update()
            .table("history")
            .column("command")
            .column("cwd")
            .column("exit_code")
            .column("timestamp")
            .r#where("id")
            .to_owned();

        let mut stmt = self.conn.prepare(query.to_sql().as_str())?;

        match stmt.execute(named_params! {
            ":command": history.command,
            ":cwd": history.cwd,
            ":exit_code": history.exit_code,
            ":timestamp": history.timestamp.unix_timestamp(),
            ":w_id": history.id,
        }) {
            Ok(rows) => {
                if rows == 1 {
                    Ok(())
                } else {
                    Err(DatabaseError {
                        msg: String::from("Unexpected row count for single row update."),
                    })
                }
            }
            Err(e) => Err(e.into()),
        }
    }

    fn search(&self, query: &str, limit: Option<usize>) -> Result<Vec<History>, DatabaseError> {
        debug!("search with query: {query}");

        let mut sql_query = Query::select()
            .column("id")
            .column("command")
            .column("cwd")
            .column("exit_code")
            .column("timestamp")
            .from("history")
            .orderby("timestamp", "DESC")
            .to_owned();

        if let Some(limit) = limit {
            sql_query.limit(limit);
        }

        let params: &[(&str, &dyn ToSql)] = if query.is_empty() {
            &[]
        } else {
            sql_query.like("command");
            named_params! {
                ":command": format!("%{query}%")
            }
        };

        let mut stmt = self.conn.prepare(sql_query.to_sql().as_str())?;

        match stmt.query_map(params, |row| {
            Ok(History::builder()
                .id(row.get("id")?)
                .command(row.get("command")?)
                .cwd(row.get("cwd")?)
                .exit_code(row.get("exit_code")?)
                .timestamp(OffsetDateTime::from_unix_timestamp(row.get("timestamp")?).unwrap())
                .build())
        }) {
            Ok(rows) => {
                let mut results: Vec<History> = Vec::new();
                for row in rows {
                    results.push(row.unwrap());
                }
                Ok(results)
            }
            Err(e) => Err(e.into()),
        }
    }

    fn get_history_total(&self) -> Result<i64, DatabaseError> {
        let query = Query::select()
            .count("*", "count")
            .from("history")
            .to_owned();
        let mut stmt = self.conn.prepare(query.to_sql().as_str())?;
        let rows = stmt.query_row([], |row| row.get::<&str, i64>("count"));
        Ok(rows?)
    }
}

/// Attempt to open a connection to `path`.
///
/// Will initially try to open as RW, but if the file does not exist, this method will also
/// take care of creating the new database file and initializing the schema before returning
/// the open connection to the new database.
///
/// * `path`: Full path to the sqlite database file.
fn get_connection(path: &str) -> Connection {
    match Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_WRITE) {
        Ok(connection) => {
            debug!("Opened {DATABASE_FILE}");

            // TODO: verify the schema for the established connection before returning.
            connection
        }
        Err(err) => {
            error!("Could not open: {err}");
            match dbg!(Connection::open_with_flags(
                path,
                OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_CREATE
            )) {
                Ok(connection) => {
                    debug!("Created {DATABASE_FILE}");
                    initialize_database(&connection);
                    connection
                }
                Err(err) => panic!("Error opening database: {err}"),
            }
        }
    }
}

/// Sets up the expected Raven schema on the database.
///
/// * `conn`: Connection to the Raven database.
fn initialize_database(conn: &Connection) {
    debug!("initialize_database: create_history");
    let create_history = include_str!("../sql/create/history.sql");
    match conn.execute(create_history, []) {
        Ok(_) => (),
        Err(err) => panic!("Error in initialize_database: {err}"),
    }
}
