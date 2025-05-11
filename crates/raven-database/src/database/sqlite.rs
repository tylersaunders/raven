use std::fs;

mod query;
use crate::MatchMode;
use log::{debug, error};
use query::{Query, SqlString};
use raven_common::{
    config::{Config, load_config},
    utils::get_data_dir,
};
use rusqlite::{Connection, DropBehavior, OpenFlags, ToSql, named_params, types::ToSqlOutput};
use time::OffsetDateTime;

use crate::{HistoryFilters, history::model::History};

use super::{Database, DatabaseError};

const DATABASE_FILE: &str = "raven.db";
const LATEST_STABLE_SCHEMA: SchemaVersion = SchemaVersion::V3;

const MIGRATION_V0_TO_V1: &str = include_str!("./sqlite/sql/migrate/v0_to_v1.sql");
const MIGRATION_V1_TO_V2: &str = include_str!("./sqlite/sql/migrate/v1_to_v2.sql");
const MIGRATION_V2_TO_V3: &str = include_str!("./sqlite/sql/migrate/v2_to_v3.sql");

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Copy)]
#[repr(u32)]
#[allow(dead_code)] // Allow dead code so all the version enums continue to exist.
/// Represents the different versions of the database schema.
/// Used for migrations and ensuring compatibility.
enum SchemaVersion {
    /// V0: The initial state where no raven-specific schema exists.
    V0 = 0,
    /// V1: Introduced the `history` table for command history.
    V1 = 1,
    /// V2: Introduced the `hist_fts` table for full-text search on history.
    V2 = 2,
    /// V3: Introduced the `hist_fts` table for full-text search on history.
    V3 = 3,
}

impl SchemaVersion {
    /// Converts the `SchemaVersion` enum variant to its underlying `u32` representation.
    fn to_u32(self) -> u32 {
        self as u32
    }
}

/// Sqlite database wrapper using rusqlite
pub struct Sqlite {
    pub conn: Connection,
}

impl Sqlite {
    /// Creates a new [`Sqlite`].
    ///
    /// Uses the configuration provided to determine the database path and file.
    /// If not specified in the config, it defaults to a standard data directory
    /// and a file named "database.sqlite3".
    ///
    /// # Panics
    ///
    /// Panics if:
    /// - A suitable data directory for the database file cannot be found or created.
    /// - The generated database file path is not valid UTF-8.
    /// - The database connection cannot be established.
    #[must_use]
    pub fn new(config: &Config) -> Self {
        let path = config
            .database
            .as_ref()
            .and_then(|config| config.database_path.clone())
            .unwrap_or(get_data_dir());
        let _ = fs::create_dir_all(&path);

        let file = config
            .database
            .as_ref()
            .and_then(|config| config.database_file.clone())
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

/// Generates an FTS5 match parameter string based on the query and mode.
///
/// Args:
///   query: The user-provided search string.
///   mode: The desired FTS5 matching mode (`Fuzzy` or `Prefix`).
///
/// Returns:
///   A string suitable for use as the right-hand operand of an FTS5 `MATCH` operator.
///   Returns an empty string if the input query is empty, signifying no FTS filtering.
#[must_use]
pub fn generate_fts5_match_parameter(query: &str, mode: MatchMode) -> String {
    if query.is_empty() {
        return String::new();
    }

    match mode {
        MatchMode::Fuzzy => {
            let words: Vec<String> = query
                .split_whitespace()
                .map(|word| {
                    let escaped_word = word.replace('"', "\"\"");
                    format!("\"{escaped_word}\"*")
                })
                .collect();
            words.join(" ")
        }
        MatchMode::Prefix => {
            let escaped_query = query.replace('"', "\"\"");
            format!("^\"{escaped_query}\"*")
        }
    }
}

/// Provides a default `Sqlite` instance.
///
/// Loads the application configuration and uses it to initialize the database connection.
///
/// # Panics
///
/// Panics if the configuration cannot be loaded. See [`load_config`] for details.
/// Panics if the `Sqlite::new` method panics. See [`Sqlite::new`] for details.
impl Default for Sqlite {
    fn default() -> Self {
        Self::new(&load_config().expect("Failed to load config"))
    }
}

/// Converts a [`rusqlite::Error`] into a [`DatabaseError`].
///
/// This allows for easy error handling by converting the specific `SQLite` error
/// into a more general database error type used within the application.
impl From<rusqlite::Error> for DatabaseError {
    fn from(value: rusqlite::Error) -> Self {
        Self {
            msg: format!("{value}"),
        }
    }
}

/// Implementation of the `Database` trait for the `Sqlite` backend.
///
/// This implementation uses `rusqlite` to interact with an `SQLite` database file.
impl Database for Sqlite {
    /// Saves a single `History` entry to the database.
    ///
    /// # Arguments
    ///
    /// * `history` - A reference to the `History` entry to save.
    ///
    /// # Returns
    ///
    /// * `Ok(i64)` - The row ID of the newly inserted entry.
    /// * `Err(DatabaseError)` - If there was an error during the database operation.
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

    /// Saves multiple `History` entries to the database efficiently using a transaction.
    ///
    /// # Arguments
    ///
    /// * `history` - A slice of `History` entries to save.
    ///
    /// # Returns
    ///
    /// * `Ok(Vec<i64>)` - A vector containing the row IDs of the newly inserted entries.
    /// * `Err(DatabaseError)` - If there was an error during the database operation.
    ///   The transaction will be rolled back in case of an error.
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
                    // Transaction is automatically rolled back due to DropBehavior::Rollback
                    return Err(err.into());
                }
            }
        }
        drop(stmt); // Explicitly drop statement before committing transaction
        tx.commit()?; // Commit the transaction
        Ok(row_ids)
    }

    /// Retrieves a single `History` entry from the database by its ID.
    ///
    /// # Arguments
    ///
    /// * `id` - The ID of the history entry to retrieve.
    ///
    /// # Returns
    ///
    /// * `Ok(Some(History))` - If an entry with the given ID was found.
    /// * `Ok(None)` - If no entry with the given ID was found.
    /// * `Err(DatabaseError)` - If there was an error during the database operation.
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

        let mut stmt = self.conn.prepare(query.to_sql().as_str())?;

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
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None), // Handle not found case
            Err(e) => Err(e.into()),                               // Handle other errors
        }
    }

    /// Updates an existing `History` entry in the database.
    ///
    /// The `History` entry must have a valid ID (not -1).
    ///
    /// # Arguments
    ///
    /// * `history` - A reference to the `History` entry containing the updated data and the ID.
    ///
    /// # Returns
    ///
    /// * `Ok(())` - If the update was successful.
    /// * `Err(DatabaseError)` - If the history ID is invalid or if there was a database error.
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
            ":w_id": history.id, // Parameter name must match the `where` clause in the SQL
        }) {
            Ok(rows) => {
                if rows == 1 {
                    Ok(())
                } else {
                    // Can happen if the ID doesn't exist
                    Err(DatabaseError {
                        msg: format!(
                            "Update affected {rows} rows, expected 1 for ID {}",
                            history.id
                        ),
                    })
                }
            }
            Err(e) => Err(e.into()),
        }
    }

    /// Searches for `History` entries based on a query string and filters.
    ///
    /// # Arguments
    ///
    /// * `query` - The search string to match against the `command` field.
    /// * `filters` - A `HistoryFilters` struct containing additional filtering criteria (limit, exit code, cwd, suggest mode).
    ///
    /// # Returns
    ///
    /// * `Ok(Vec<History>)` - A vector of matching `History` entries, ordered by timestamp descending.
    /// * `Err(DatabaseError)` - If there was an error during the database operation.
    fn search(&self, query: &str, filters: HistoryFilters) -> Result<Vec<History>, DatabaseError> {
        debug!("search with query: '{}', filters: {:?}", query, filters);

        let mut params_map: std::collections::HashMap<String, Box<dyn ToSql>> =
            std::collections::HashMap::new();

        let mut sql_query = Query::select()
            .column("h.id") // No alias needed
            .column("h.command")
            .column("h.cwd")
            .column("h.exit_code")
            .column("h.timestamp")
            .from("history h")
            // Order by timestamp when not using FTS relevance
            .orderby("timestamp", "DESC")
            .to_owned();

        if !query.is_empty() {
            // Reset the from table to use history_fts and join on history.
            sql_query.from.clear();
            sql_query
                .from("history_fts fts JOIN history h ON h.id = fts.rowid")
                .match_fts("fts.command");

            let fts5_query = generate_fts5_match_parameter(query, filters.mode);
            // Add the search tokens to the query parameters.
            params_map.insert(":fts_command".to_string(), Box::new(fts5_query));
        }

        if let Some(exit) = filters.exit {
            let param_name = ":h_exit_code"; // Need distinct param name
            sql_query.r#where("h.exit_code"); // WHERE h.exit_code = :h_exit_code
            params_map.insert(param_name.to_string(), Box::new(exit));
        }

        if let Some(cwd) = filters.cwd.as_ref() {
            let param_name = ":h_cwd"; // Need distinct param name
            sql_query.r#where("h.cwd"); // WHERE h.cwd = :h_cwd
            params_map.insert(param_name.to_string(), Box::new(cwd.clone()));
        }

        // Apply limit regardless of path
        if let Some(limit) = filters.limit {
            sql_query.limit(limit);
        }

        // Parameter Vec preparation, convert the hashmap into a tuple Vec.
        let named_params_vec: Vec<(&str, &dyn ToSql)> = params_map
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_ref()))
            .collect();

        #[cfg(debug_assertions)]
        {
            // Increased logging for dev binaries.
            let sql_string = sql_query.to_sql();
            debug!("Executing search SQL: {}", sql_string);
            debug!(
                "With parameters: {:?}",
                format_named_params_for_debug(&named_params_vec)
            );
        }

        let mut stmt = self.conn.prepare(&sql_query.to_sql())?;

        match stmt.query_map(&*named_params_vec, |row| {
            Ok(History::builder()
                .id(row.get("id")?)
                .command(row.get("command")?)
                .cwd(row.get("cwd")?)
                .exit_code(row.get("exit_code")?)
                .timestamp(OffsetDateTime::from_unix_timestamp(row.get("timestamp")?).unwrap())
                .build())
        }) {
            Ok(rows) => {
                // Collect results, handling potential errors during row processing
                rows.collect::<Result<Vec<History>, rusqlite::Error>>()
                    .map_err(DatabaseError::from)
            }
            Err(e) => {
                debug!(
                    "Search query failed: Query='{}', Params={:?}, Error={}",
                    sql_query.to_sql(),
                    format_named_params_for_debug(&named_params_vec),
                    e
                );
                Err(e.into())
            }
        }
    }

    /// Deletes a `History` entry from the database by its ID.
    ///
    /// # Arguments
    ///
    /// * `id` - The ID of the history entry to delete.
    ///
    /// # Returns
    ///
    /// * `Ok(())` - If the deletion was successful or if the entry did not exist.
    /// * `Err(DatabaseError)` - If there was an unexpected database error.
    fn delete(&self, id: i64) -> Result<(), DatabaseError> {
        debug!("Deleting history entry with id: {}", id);
        let query = Query::delete().table("history").r#where("id").to_owned();

        match self.conn.execute(&query.to_sql(), [id]) {
            Ok(rows_affected) => {
                if rows_affected <= 1 {
                    // 0 or 1 row affected is considered success
                    debug!(
                        "Deletion attempt for id {} resulted in {} rows affected.",
                        id, rows_affected
                    );
                    Ok(())
                } else {
                    // This case should ideally not happen with a primary key `id`
                    error!(
                        "Unexpected number of rows ({}) affected when deleting history entry with id: {}",
                        rows_affected, id
                    );
                    Err(DatabaseError {
                        msg: format!(
                            "Unexpected number of rows ({rows_affected}) affected during deletion for id {id}",
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

    /// Gets the total number of history entries in the database.
    ///
    /// # Returns
    ///
    /// * `Ok(i64)` - The total count of history entries.
    /// * `Err(DatabaseError)` - If there was an error during the database operation.
    fn get_history_total(&self) -> Result<i64, DatabaseError> {
        let query = Query::select()
            .count("*", "count")
            .from("history")
            .to_owned();
        let mut stmt = self.conn.prepare(query.to_sql().as_str())?;
        let count = stmt.query_row([], |row| row.get::<usize, i64>(0)); // Get count by index
        Ok(count?)
    }
}

/// Get the ``user_version`` PRAGMA from the ``SQLite`` database.
fn get_user_version(conn: &Connection) -> Result<u32, rusqlite::Error> {
    conn.query_row("PRAGMA user_version;", [], |row| row.get(0))
}

/// Set the ``user_version`` PRAGMA on the ``SQLite`` database.
fn set_user_version(conn: &Connection, version: u32) -> Result<(), rusqlite::Error> {
    conn.execute(&format!("PRAGMA user_version = {version};"), [])?;
    Ok(())
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
        Ok(mut connection) => {
            debug!("Opened {path}");
            match get_user_version(&connection) {
                Ok(current_version) => {
                    debug!("Current database version: {current_version}");
                    if current_version < LATEST_STABLE_SCHEMA.to_u32() {
                        run_migrations(
                            &mut connection,
                            current_version,
                            Some(LATEST_STABLE_SCHEMA),
                        )
                        .expect("Failure during migrations when opening database.");
                    }
                }
                Err(err) => {
                    debug!("Unable to verify Raven database version: {err}");
                }
            }
            connection
        }

        Err(err) => {
            error!("Could not open: {err}");
            match dbg!(Connection::open_with_flags(
                path,
                OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_CREATE
            )) {
                Ok(mut connection) => {
                    debug!("Created {path}");
                    if let Err(err) = run_migrations(
                        &mut connection,
                        SchemaVersion::V0.to_u32(),
                        Some(LATEST_STABLE_SCHEMA),
                    ) {
                        panic!("Error initializating database {err}")
                    } else {
                        connection
                    }
                }
                Err(err) => panic!("Error opening database: {err}"),
            }
        }
    }
}

/// Applies necessary schema migrations to bring the database up to the latest version.
///
/// * `conn`: Connection to the Raven database.
/// * `current_version`: The current schema version reported by the database.
fn run_migrations(
    conn: &mut Connection,
    current_version: u32,
    target_version: Option<SchemaVersion>,
) -> Result<(), DatabaseError> {
    debug!(
        "Running migrations from version {} up to {}",
        current_version,
        LATEST_STABLE_SCHEMA.to_u32()
    );

    let mut version_to_migrate_from = current_version;
    let target_version = match target_version {
        Some(target) => target.to_u32(),
        None => LATEST_STABLE_SCHEMA.to_u32(),
    };

    while version_to_migrate_from < target_version {
        let next_version = version_to_migrate_from + 1;

        let migration_name = format!("v{version_to_migrate_from}_to_v{next_version}");
        let migration_sql = match version_to_migrate_from {
            0 => MIGRATION_V0_TO_V1,
            1 => MIGRATION_V1_TO_V2,
            2 => MIGRATION_V2_TO_V3,
            _ => {
                let err_msg = format!("Migration script for {migration_name} not found.");
                error!("{err_msg}");
                return Err(DatabaseError { msg: err_msg });
            }
        };
        debug!("Attempting to apply migration: {migration_name}");

        let mut tx = match conn.transaction() {
            Ok(tx) => tx,
            Err(e) => {
                error!("Failed to start transaction for migration: {}", e);
                return Err(e.into());
            }
        };
        tx.set_drop_behavior(DropBehavior::Rollback); // Ensure rollback on drop if not committed

        if let Err(e) = tx.execute_batch(migration_sql) {
            error!("Failed to execute migration script {migration_name}: {e}");
            // Transaction will be rolled back automatically on drop
            return Err(DatabaseError {
                msg: format!("Migration script failed: {migration_name}. Error: {e}",),
            });
        }

        // Update the user_version *within* the transaction
        if let Err(e) = set_user_version(&tx, next_version) {
            error!(
                "Failed to set user_version to {} after migration {}: {}",
                next_version, migration_name, e
            );
            // Transaction will be rolled back automatically on drop
            return Err(e.into());
        }

        // Commit the transaction
        if let Err(e) = tx.commit() {
            error!(
                "Failed to commit transaction after migration {}: {}",
                migration_name, e
            );
            return Err(e.into());
        }
        debug!("Successfully applied migration: {}", migration_name);

        version_to_migrate_from = next_version;
    }

    debug!(
        "Migrations complete. Database is at version {}",
        version_to_migrate_from
    );
    Ok(())
}

/// Formats a vector of named parameters (`(&str, &dyn ToSql)`) into a vector of strings
/// suitable for debugging purposes.
///
/// Each element in the output vector represents a single named parameter in the format "key=value".
/// The value part is derived using the `ToSql` trait implementation of the parameter.
/// If conversion to SQL fails, an error message is included in the string.
/// Special handling is included for `ToSqlOutput::Borrowed` to attempt string representation,
/// falling back to "error" if the conversion isn't straightforward (e.g., for non-text types).
///
/// # Arguments
///
/// * `named_params_vec` - A reference to a vector of tuples, where each tuple contains
///   a string slice representing the parameter name and a reference to a type implementing `ToSql`.
///
/// # Returns
///
/// A `Vec<String>` where each string represents a formatted named parameter for debugging.
fn format_named_params_for_debug(named_params_vec: &Vec<(&str, &dyn ToSql)>) -> Vec<String> {
    named_params_vec
        .iter()
        .map(|(k, v)| {
            match v.to_sql() {
                // ValueRef implements Debug
                Ok(ToSqlOutput::Owned(value)) => format!("{k}={value:?}"),
                Ok(ToSqlOutput::Borrowed(value_ref)) => {
                    // Note: value_ref.as_str() only works well for Text types.
                    // It might return "error" inappropriately for non-text types (Integer, Real, Blob, Null).
                    // Consider using a match on value_ref if more precise formatting is needed.
                    format!("{k}={:?}", value_ref.as_str().unwrap_or("error"))
                }
                Err(e) => format!("{k}={{Error converting to SQL: {e:?}}}"),
                // Use a wildcard for any unexpected or future variants if you want to avoid compilation errors
                // on library updates, but this might hide issues.
                _ => format!("{k}={{Unhandled ToSqlOutput variant}}"),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::history::model::History;
    use rusqlite::Connection;
    use std::time::Duration;
    use time::OffsetDateTime;

    // Helper to create an in-memory database and initialize the schema
    fn memory_db(target_version: Option<SchemaVersion>) -> Sqlite {
        let mut conn = Connection::open_in_memory().expect("Failed to open in-memory database");
        let _ = run_migrations(&mut conn, SchemaVersion::V0.to_u32(), target_version);
        Sqlite { conn }
    }

    // Helper to create a sample history entry
    fn sample_history(id: i64, command: &str) -> History {
        History::builder()
            .id(id)
            .timestamp(OffsetDateTime::now_utc())
            .command(command.to_string())
            .cwd("/tmp".to_string())
            .exit_code(0)
            .build()
    }

    #[test]
    fn test_run_migrations_v0_to_latest_stable_success() {
        // Create database with version 0.
        let mut db = memory_db(Some(SchemaVersion::V0));
        let initial_version = get_user_version(&db.conn).expect("Get version failed");
        let result = run_migrations(&mut db.conn, initial_version, Some(LATEST_STABLE_SCHEMA));
        assert!(result.is_ok(), "Migration failed: {:?}", result.err());
    }

    #[test]
    fn test_run_migrations_v1_to_latest_stable_success() {
        // Create database with version 0.
        let mut db = memory_db(Some(SchemaVersion::V1));
        let initial_version = get_user_version(&db.conn).expect("Get version failed");
        let result = run_migrations(&mut db.conn, initial_version, Some(LATEST_STABLE_SCHEMA));
        assert!(result.is_ok(), "Migration failed: {:?}", result.err());
    }

    #[test]
    fn test_run_migrations_v1_to_v2_success() {
        // 1. Setup: Create DB, which initializes to V1 schema and version 1.
        let mut db = memory_db(Some(SchemaVersion::V1));
        let initial_version = get_user_version(&db.conn).expect("Get version failed");
        assert_eq!(initial_version, SchemaVersion::V1.to_u32());

        let get_history_old_exists = |conn: &Connection| {
            conn.query_row(
                "SELECT COUNT(*) FROM sqlite_master where type = 'table' AND name = 'history_old'",
                [],
                |row| row.get::<_, i64>(0),
            )
            .is_ok_and(|count| count == 1)
        };

        assert!(
            !get_history_old_exists(&db.conn),
            "history_old should not exist"
        );

        let result = run_migrations(&mut db.conn, initial_version, Some(SchemaVersion::V2));

        // 3. Assert: Check result, version update, and schema change.
        assert!(result.is_ok(), "Migration failed: {:?}", result.err());
        assert_eq!(
            get_user_version(&db.conn).expect("Get version failed"),
            SchemaVersion::V2.to_u32(),
            "Database version should be updated to V2"
        );
        assert!(
            !get_history_old_exists(&db.conn),
            "history_old should not exist"
        );
    }

    #[test]
    fn test_run_migrations_v2_to_v3_success() {
        // 1. Setup: Create DB, which initializes to V1 schema and version 1.
        let mut db = memory_db(Some(SchemaVersion::V2));
        let initial_version = get_user_version(&db.conn).expect("Get version failed");
        assert_eq!(initial_version, SchemaVersion::V2.to_u32());

        // Checks if the `history_fts` virtual table exists in the ``SQLite`` database.
        let get_history_fts_exists = |conn: &Connection| {
            conn.query_row(
                "SELECT COUNT(*) FROM sqlite_master where type = 'table' AND name = 'history_fts'",
                [],
                |row| row.get::<_, i64>(0),
            )
            .is_ok_and(|count| count == 1)
        };

        // Checks if the `history` table triggers exists in the ``SQLite`` database.
        let get_history_triggers_exist = |conn: &Connection| {
            conn.query_row(
                "SELECT COUNT(*) FROM sqlite_master where type = 'trigger' AND tbl_name = 'history'",
                [],
                |row| row.get::<_, i64>(0),
            )
            // The correct number is 3 triggers (insert/update/delete)
            .is_ok_and(|count| count == 3)
        };

        // Verify the history_fts table and associated triggers don't exist before migration.
        assert!(
            !get_history_fts_exists(&db.conn),
            "history_fts should not exist in V2 schema"
        );
        assert!(
            !get_history_triggers_exist(&db.conn),
            "history_triggers should not exist in V2 schema"
        );

        // 2. Act: Run migrations starting from the initial version.
        let result = run_migrations(&mut db.conn, initial_version, Some(SchemaVersion::V3));

        // 3. Assert: Check result, version update, and schema change.
        assert!(result.is_ok(), "Migration failed: {:?}", result.err());
        assert_eq!(
            get_user_version(&db.conn).expect("Get version failed"),
            SchemaVersion::V3.to_u32(),
            "Database version should be updated to V3"
        );

        // Verify the schema change occurred.
        assert!(
            get_history_fts_exists(&db.conn),
            "history_fts should exist in Schema V3."
        );
        assert!(
            get_history_triggers_exist(&db.conn),
            "history_triggers should exist in V3 schema"
        );
    }

    #[test]
    fn test_run_migrations_no_migration_needed() {
        let mut db = memory_db(Some(LATEST_STABLE_SCHEMA));
        let initial_version = get_user_version(&db.conn).expect("Get version failed");
        assert_eq!(initial_version, LATEST_STABLE_SCHEMA.to_u32());

        // 2. Act: Run migrations starting from the current (latest) version.
        let result = run_migrations(&mut db.conn, initial_version, Some(LATEST_STABLE_SCHEMA));

        // 3. Assert: Should succeed, version remains unchanged.
        assert!(
            result.is_ok(),
            "Migration failed unexpectedly: {:?}",
            result.err()
        );
        assert_eq!(
            get_user_version(&db.conn).expect("Get version failed"),
            LATEST_STABLE_SCHEMA.to_u32(),
            "Database version should remain unchanged"
        );
    }

    #[test]
    fn test_save_and_get() {
        let mut db = memory_db(Some(LATEST_STABLE_SCHEMA));
        let history_in = sample_history(1, "echo test");

        let id = db.save(&history_in).expect("Failed to save history");
        assert!(id > 0);

        let history_out = db
            .get(id)
            .expect("Failed to get history")
            .expect("History not found");

        assert_eq!(history_out.id, id);
        assert_eq!(history_out.command, history_in.command);
        assert_eq!(history_out.cwd, history_in.cwd);
        assert_eq!(history_out.exit_code, history_in.exit_code);
        // Timestamps might have slight precision differences, compare within a tolerance if needed
        assert_eq!(
            history_out.timestamp.unix_timestamp(),
            history_in.timestamp.unix_timestamp()
        );
    }

    #[test]
    fn test_save_bulk() {
        let mut db = memory_db(Some(LATEST_STABLE_SCHEMA));
        let histories = vec![
            sample_history(1, "echo 1"),
            sample_history(2, "echo 2"),
            sample_history(3, "echo 3"),
        ];

        let ids = db
            .save_bulk(&histories)
            .expect("Failed to save bulk history");
        assert_eq!(ids.len(), 3);
        assert!(ids.iter().all(|&id| id > 0));

        let total = db.get_history_total().expect("Failed to get total");
        assert_eq!(total, 3);
    }

    #[test]
    fn test_update() {
        let mut db = memory_db(Some(LATEST_STABLE_SCHEMA));
        let mut history = sample_history(1, "initial command");

        let id = db.save(&history).expect("Failed to save history");
        history.id = id; // Set the ID after saving
        history.command = "updated command".to_string();
        history.exit_code = 1;

        db.update(&history).expect("Failed to update history");

        let updated_history = db
            .get(id)
            .expect("Failed to get updated history")
            .expect("Updated history not found");

        assert_eq!(updated_history.id, id);
        assert_eq!(updated_history.command, "updated command");
        assert_eq!(updated_history.exit_code, 1);
    }

    #[test]
    fn test_update_unsaved_fails() {
        let db = memory_db(Some(LATEST_STABLE_SCHEMA));
        let history_unsaved = History::capture()
            .timestamp(OffsetDateTime::now_utc() - Duration::from_secs(30))
            .command("ls -l /home".to_string())
            .cwd("/home".to_string())
            .build();

        let result = db.update(&history_unsaved.into());
        assert!(result.is_err());
        if let Err(e) = result {
            assert!(e.msg.contains("Cannot update object with -1 ID"));
        }
    }

    #[test]
    fn test_search() {
        let mut db = memory_db(Some(LATEST_STABLE_SCHEMA));
        let h1 = History::builder()
            .id(1)
            .timestamp(OffsetDateTime::now_utc() - Duration::from_secs(30))
            .command("ls -l /home".to_string())
            .cwd("/home".to_string())
            .exit_code(0)
            .build();
        let h2 = History::builder()
            .id(2)
            .timestamp(OffsetDateTime::now_utc() - Duration::from_secs(20))
            .command("grep test file.txt".to_string())
            .cwd("/tmp".to_string())
            .exit_code(1)
            .build();
        let h3 = History::builder()
            .id(3)
            .timestamp(OffsetDateTime::now_utc() - Duration::from_secs(10))
            .command("cargo test --all".to_string())
            .cwd("/home/user/project".to_string())
            .exit_code(0)
            .build();

        db.save_bulk(&[h1.clone(), h2.clone(), h3.clone()])
            .expect("Failed to save for search");

        // Search by substring
        let results = db
            .search("test", HistoryFilters::default())
            .expect("Search failed");
        assert_eq!(results.len(), 2);
        assert!(results.iter().any(|h| h.command == h2.command));
        assert!(results.iter().any(|h| h.command == h3.command));

        // Search by prefix (suggest)
        let results = db
            .search(
                "ca",
                HistoryFilters {
                    ..Default::default()
                },
            )
            .expect("Search failed");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].command, h3.command);

        // Search with cwd filter
        let results = db
            .search(
                "",
                HistoryFilters {
                    cwd: Some("/tmp".to_string()),
                    ..Default::default()
                },
            )
            .expect("Search failed");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].command, h2.command);

        // Search with exit filter
        let results = db
            .search(
                "",
                HistoryFilters {
                    exit: Some(0),
                    ..Default::default()
                },
            )
            .expect("Search failed");
        assert_eq!(results.len(), 2); // h1 and h3
        assert!(results.iter().any(|h| h.command == h1.command));
        assert!(results.iter().any(|h| h.command == h3.command));

        // Search with limit and order (default is DESC)
        let results = db
            .search(
                "",
                HistoryFilters {
                    limit: Some(1),
                    ..Default::default()
                },
            )
            .expect("Search failed");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].command, h3.command); // Most recent
    }

    #[test]
    fn test_delete() {
        let mut db = memory_db(Some(LATEST_STABLE_SCHEMA));
        let history = sample_history(1, "to be deleted");

        let id = db.save(&history).expect("Failed to save history");
        assert!(db.get(id).expect("Get failed").is_some());

        db.delete(id).expect("Failed to delete history");

        let result = db.get(id);
        assert!(result.is_ok());
        // The result should be None
        assert!(result.unwrap().is_none());

        // Deleting non-existent should be Ok
        let delete_again_result = db.delete(id);
        assert!(delete_again_result.is_ok());

        // Deleting ID -1 should probably fail or be a no-op depending on desired behavior
        // Current implementation might error in execute due to parameter mismatch if not handled
        // Let's assume deleting -1 isn't a valid operation here or should be a no-op handled by execute
        let delete_invalid_result = db.delete(-1);
        // Depending on rusqlite behavior with [-1] param, this might Err or Ok(0 rows affected)
        // Given the code `execute(&query.to_sql(), [id])`, rusqlite likely handles it.
        // Let's assert Ok as the function intends to return Ok for 0 rows affected.
        assert!(delete_invalid_result.is_ok());
    }

    #[test]
    fn test_get_history_total() {
        let mut db = memory_db(Some(LATEST_STABLE_SCHEMA));

        let total_initial = db.get_history_total().expect("Failed to get initial total");
        assert_eq!(total_initial, 0);

        db.save(&sample_history(1, "cmd 1")).expect("Save 1 failed");
        db.save(&sample_history(2, "cmd 2")).expect("Save 2 failed");

        let total_after_saves = db
            .get_history_total()
            .expect("Failed to get total after saves");
        assert_eq!(total_after_saves, 2);

        let id = db.save(&sample_history(3, "cmd 3")).expect("Save 3 failed");
        db.delete(id).expect("Delete failed");

        let total_after_delete = db
            .get_history_total()
            .expect("Failed to get total after delete");
        assert_eq!(total_after_delete, 2); // Back to 2 after deleting one
    }

    #[test]
    fn test_generate_fts5_match_parameter_empty_query() {
        assert_eq!(generate_fts5_match_parameter("", MatchMode::Fuzzy), "");
        assert_eq!(generate_fts5_match_parameter("", MatchMode::Prefix), "");
    }

    #[test]
    fn test_generate_fts5_match_parameter_fuzzy() {
        assert_eq!(
            generate_fts5_match_parameter("hello world", MatchMode::Fuzzy),
            "\"hello\"* \"world\"*"
        );
        assert_eq!(
            generate_fts5_match_parameter("term1", MatchMode::Fuzzy),
            "\"term1\"*"
        );
        assert_eq!(
            generate_fts5_match_parameter("with\"quote", MatchMode::Fuzzy),
            "\"with\"\"quote\"*"
        );
        assert_eq!(
            generate_fts5_match_parameter("multiple words \"here\"", MatchMode::Fuzzy),
            "\"multiple\"* \"words\"* \"\"\"here\"\"\"*"
        );
    }

    #[test]
    fn test_generate_fts5_match_parameter_initial_prefix() {
        assert_eq!(
            generate_fts5_match_parameter("start phrase", MatchMode::Prefix),
            "^\"start phrase\"*"
        );
        assert_eq!(
            generate_fts5_match_parameter("prefix", MatchMode::Prefix),
            "^\"prefix\"*"
        );
        assert_eq!(
            generate_fts5_match_parameter("prefix with\"quote", MatchMode::Prefix),
            "^\"prefix with\"\"quote\"*"
        );
    }
}
