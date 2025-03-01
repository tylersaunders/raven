use core::fmt;

use crate::history::model::History;
pub mod sqlite;

#[derive(Debug, Clone)]
pub struct DatabaseError {
    pub msg: String,
}

impl fmt::Display for DatabaseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Encountered a database error: {}", self.msg)
    }
}

pub trait Database {

    /// Save a `History` object to the database.
    /// * `history`:
    ///
    /// # Errors
    /// Will return `Err` if the database Encountered an issue.
    fn save(&self, history: &History) -> Result<i64, DatabaseError>;

    /// Save a vec of `History` objects to the database.
    /// * `history`:
    ///
    /// # Errors
    /// Will return `Err` if the database Encountered an issue.
    fn save_bulk(&self, history: &[History]) -> Result<Vec<i64>, DatabaseError>;

    /// Fetch a `History` object by its id from the database.
    /// * `history`:
    ///
    /// # Errors
    /// Will return `Err` if the database Encountered an issue.
    fn get(&self, id:i64) -> Result<Option<History>, DatabaseError>;

    /// Writes all `History` object fields back to the database.
    /// NOTE: This overrides existing data.
    ///
    /// * `history`:
    ///
    /// # Errors
    /// Will return `Err` if the database Encountered an issue.
    fn update(&self, history: &History) -> Result<(), DatabaseError>;
}

