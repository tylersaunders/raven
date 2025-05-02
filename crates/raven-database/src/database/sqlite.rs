use std::fs;

mod query;
use log::{debug, error};
use query::{Query, SqlString};
use raven_common::{
    config::{Config, load_config},
    utils::get_data_dir,
};
use rusqlite::{Connection, DropBehavior, OpenFlags, ToSql, named_params};
use time::OffsetDateTime;

use crate::{HistoryFilters, history::model::History};

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
    pub fn new(config: &Config) -> Self {
        let path = config
            .database
            .database_path
            .clone()
            .unwrap_or(get_data_dir());
        let _ = fs::create_dir_all(&path);

        let file = config
            .database
            .database_file
            .clone()
            .unwrap_or(String::from(DATABASE_FILE));

        let database_path = path.join(file);
        let conn = get_connection(
            database_path
                .to_str()
                .expect("Could not generate database file path."),
        );
        Self { conn }
    }
}

impl Default for Sqlite {
    fn default() -> Self {
        Self::new(&load_config().expect("Failed to load config"))
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
            }
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

    fn search(&self, query: &str, filters: HistoryFilters) -> Result<Vec<History>, DatabaseError> {
        debug!("search with query: {query}");

        let mut params: Vec<(&str, &dyn ToSql)> = Vec::new();

        let mut sql_query = Query::select()
            .column("id")
            .column("command")
            .column("cwd")
            .column("exit_code")
            .column("timestamp")
            .from("history")
            .orderby("timestamp", "DESC")
            .to_owned();

        if let Some(limit) = filters.limit {
            sql_query.limit(limit);
        }

        let exit = filters.exit.as_ref();
        if exit.is_some() {
            sql_query.r#where("exit_code");
            params.push((":exit_code", &exit));
        }

        let cwd = filters.cwd.as_ref();
        if cwd.is_some() {
            sql_query.r#where("cwd");
            params.push((":cwd", &cwd));
        }

        let q = if query.is_empty() {
            None
        } else if filters.suggest {
            // For suggestions, use QUERY as a prefix
            Some(format!("{query}%"))
        } else {
            // All other searches should treat QUERY as a substring
            Some(format!("%{query}%"))
        };

        if q.is_some() {
            sql_query.like("command");
            params.push((":command", &q));
        }

        let mut stmt = self.conn.prepare(sql_query.to_sql().as_str())?;

        match stmt.query_map(&*params, |row| {
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

    fn delete(&self, id: i64) -> Result<(), DatabaseError> {
        debug!("Deleting history entry with id: {}", id);
        let query = Query::delete().table("history").r#where("id").to_owned();

        match self.conn.execute(&query.to_sql(), [id]) {
            Ok(rows_affected) => {
                if rows_affected == 1 {
                    debug!("Successfully deleted history entry with id: {}", id);
                    Ok(())
                } else if rows_affected == 0 {
                    // It's not necessarily an error if the ID didn't exist,
                    // but we can log it or return a specific error if needed.
                    debug!(
                        "Attempted to delete non-existent history entry with id: {}",
                        id
                    );
                    // Return Ok(()) as the state is "entry with id does not exist", which is the goal.
                    // Alternatively, return an error:
                    // Err(DatabaseError { msg: format!("History entry with id {} not found for deletion", id) })
                    Ok(())
                } else {
                    // This shouldn't happen with a primary key constraint
                    error!(
                        "Unexpected number of rows ({}) affected when deleting history entry with id: {}",
                        rows_affected, id
                    );
                    Err(DatabaseError {
                        msg: format!(
                            "Unexpected number of rows ({rows_affected}) affected during deletion",
                        ),
                    })
                }
            }
            Err(e) => {
                error!("Failed to delete history entry with id {}: {}", id, e);
                Err(e.into())
            }
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
            debug!("Opened {path}");

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
                    debug!("Created {path}");
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
