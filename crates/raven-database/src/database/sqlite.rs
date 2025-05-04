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
const LATEST_STABLE_SCHEMA: SchemaVersion = SchemaVersion::V1;

const MIGRATION_V0_TO_V1: &str = include_str!("./sqlite/sql/migrate/v0_to_v1.sql");
const MIGRATION_V1_TO_V2: &str = include_str!("./sqlite/sql/migrate/v1_to_v2.sql");

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
}

impl SchemaVersion {
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
    /// # Panics
    ///
    /// Panics if a data directory for the database file cannot be found.
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
#[allow(clippy::single_match_else)]
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
            "history_fts should not exist in V1 schema"
        );
        assert!(
            !get_history_triggers_exist(&db.conn),
            "history_triggers should not exist in V1 schema"
        );

        // 2. Act: Run migrations starting from the initial version.
        let result = run_migrations(&mut db.conn, initial_version, Some(SchemaVersion::V2));

        // 3. Assert: Check result, version update, and schema change.
        assert!(result.is_ok(), "Migration failed: {:?}", result.err());
        assert_eq!(
            get_user_version(&db.conn).expect("Get version failed"),
            SchemaVersion::V2.to_u32(),
            "Database version should be updated to V2"
        );

        // Verify the schema change occurred.
        assert!(
            get_history_fts_exists(&db.conn),
            "history_fts should exist in Schema v2."
        );
        assert!(
            get_history_triggers_exist(&db.conn),
            "history_triggers should exist in V2 schema"
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
                    suggest: true,
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
        assert!(result.is_err());

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
}
