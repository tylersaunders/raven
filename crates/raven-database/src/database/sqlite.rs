use std::fs;

use log::{debug, error};
use raven_common::utils::get_data_dir;
use rusqlite::{Connection, DropBehavior, OpenFlags, named_params, params};
use time::OffsetDateTime;

use crate::history::model::History;

use super::{Database, DatabaseError};

const DATABASE_FILE: &str = "raven.db";

/// Sqlite database wrapper using rusqlite
pub struct Sqlite {
    pub conn: Connection,
}

impl Sqlite {
    /// Insert statement for a single row into history table
    const SQL_INSERT_INTO_HISTORY: &str =
        "INSERT INTO history(timestamp, command, cwd, exit_code) VALUES (?1, ?2, ?3, ?4)";

    const SQL_UPDATE_HISTORY_ROW: &str =
        "UPDATE history SET command=?1, cwd=?2, exit_code=?3, timestamp=?4 WHERE id=?5";

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

impl Database for Sqlite {
    fn save(&mut self, history: &History) -> Result<i64, DatabaseError> {
        let stmt = self.conn.prepare(Sqlite::SQL_INSERT_INTO_HISTORY);
        return match stmt.unwrap().insert(params![
            history.timestamp.unix_timestamp(),
            history.command,
            history.cwd,
            history.exit_code,
        ]) {
            Ok(row_id) => Ok(row_id),
            Err(err) => Err(DatabaseError {
                msg: format!("{err}"),
            }),
        };
    }

    fn save_bulk(&mut self, history: &[History]) -> Result<Vec<i64>, DatabaseError> {
        let mut row_ids: Vec<i64> = Vec::new();

        let mut tx = self.conn.transaction().expect("expected transaction");
        tx.set_drop_behavior(DropBehavior::Rollback);

        let mut stmt = tx.prepare(Sqlite::SQL_INSERT_INTO_HISTORY).unwrap();
        for h in history {
            match stmt.insert(params![
                h.timestamp.unix_timestamp(),
                h.command,
                h.cwd,
                h.exit_code,
            ]) {
                Ok(row_id) => row_ids.push(row_id),
                Err(err) => {
                    return Err(DatabaseError {
                        msg: format!("{err}"),
                    });
                }
            };
        }
        drop(stmt);
        let _ = tx.commit();
        Ok(row_ids)
    }

    fn get(&self, id: i64) -> Result<Option<History>, DatabaseError> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, command, cwd, exit_code, timestamp FROM history WHERE id=?1")
            .unwrap();

        let h = stmt.query_row([id], |row| {
            Ok(History::builder()
                .id(row.get(0)?)
                .command(row.get(1)?)
                .cwd(row.get(2)?)
                .exit_code(row.get(3)?)
                .timestamp(OffsetDateTime::from_unix_timestamp(row.get(4)?).unwrap())
                .build())
        });

        match h {
            Ok(h) => Ok(Some(h)),
            Err(err) => Err(DatabaseError {
                msg: format!("{err}"),
            }),
        }
    }

    fn update(&self, history: &History) -> Result<(), DatabaseError> {
        if history.id == -1 {
            return Err(DatabaseError {
                msg: "Cannot update object with -1 ID, try save first.".to_string(),
            });
        }

        let mut stmt = self.conn.prepare(Sqlite::SQL_UPDATE_HISTORY_ROW).unwrap();

        match stmt.execute(params![
            history.command,
            history.cwd,
            history.exit_code,
            history.timestamp.unix_timestamp(),
            history.id,
        ]) {
            Ok(rows) => {
                if rows == 1 {
                    Ok(())
                } else {
                    Err(DatabaseError {
                        msg: String::from("Unexpected row count for single row update."),
                    })
                }
            }
            Err(error) => Err(DatabaseError {
                msg: format!("{error}"),
            }),
        }
    }

    fn search(&self, query: &str, limit: Option<usize>) -> Result<Vec<History>, DatabaseError> {
        debug!("search with query: {query}");
        let query_limit = limit.unwrap_or(20);

        let (sql, params) = if query.is_empty() {
            (
                concat!(
                    "SELECT ",
                    "id, command, cwd, exit_code, timestamp ",
                    "FROM history ",
                    "ORDER BY timestamp DESC ",
                    "LIMIT :limit",
                ),
                named_params! { ":limit": query_limit },
            )
        } else {
            (
                concat!(
                    "SELECT ",
                    "id, command, cwd, exit_code, timestamp ",
                    "FROM history ",
                    "WHERE command LIKE :query ",
                    "ORDER BY timestamp DESC ",
                    "LIMIT :limit",
                ),
                named_params! {
                    ":limit": query_limit,
                    ":query": format!("%{query}%")
                },
            )
        };

        let mut stmt = self.conn.prepare(sql).unwrap();

        match stmt.query_map(params, |row| {
            Ok(History::builder()
                .id(row.get(0)?)
                .command(row.get(1)?)
                .cwd(row.get(2)?)
                .exit_code(row.get(3)?)
                .timestamp(OffsetDateTime::from_unix_timestamp(row.get(4)?).unwrap())
                .build())
        }) {
            Ok(rows) => {
                let mut results: Vec<History> = Vec::new();
                for row in rows {
                    results.push(row.unwrap());
                }
                Ok(results)
            }
            Err(error) => Err(DatabaseError {
                msg: format!("{error}"),
            }),
        }
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
