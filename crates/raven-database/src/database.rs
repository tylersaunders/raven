use core::fmt;

use crate::{HistoryFilters, history::model::History};
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
    fn save(&mut self, history: &History) -> Result<i64, DatabaseError>;

    /// Save a vec of `History` objects to the database.
    /// * `history`:
    ///
    /// # Errors
    /// Will return `Err` if the database Encountered an issue.
    fn save_bulk(&mut self, history: &[History]) -> Result<Vec<i64>, DatabaseError>;

    /// Fetch a `History` object by its id from the database.
    /// * `history`:
    ///
    /// # Errors
    /// Will return `Err` if the database Encountered an issue.
    fn get(&self, id: i64) -> Result<Option<History>, DatabaseError>;

    /// Gets the total number of rows in the history table.
    ///
    /// # Errors
    ///
    /// This function will return an error if the database encountered an issue.
    fn get_history_total(&self) -> Result<i64, DatabaseError>;

    /// Writes all `History` object fields back to the database.
    /// NOTE: This overrides existing data.
    ///
    /// * `history`:
    ///
    /// # Errors
    /// Will return `Err` if the database Encountered an issue.
    fn update(&self, history: &History) -> Result<(), DatabaseError>;

    /// Delete a history entry by its unique ID.
    ///
    /// * `id`: The ID of the history entry to delete.
    ///
    /// # Errors
    /// Will return `Err` if the database encountered an issue during deletion.
    fn delete(&self, id: i64) -> Result<(), DatabaseError>;

    /// Search over history records and return a list of matching results.
    ///
    /// * `limit`: The maximum amount of results to return.
    ///
    /// # Errors
    /// Will return `Err` if the database Encountered an issue.
    fn search(&self, query: &str, filters: HistoryFilters) -> Result<Vec<History>, DatabaseError>;
}
