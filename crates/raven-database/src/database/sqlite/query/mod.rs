use delete::DeleteStatement;
use insert::InsertStatement;
use select::SelectStatement;
use update::UpdateStatement;

mod insert;
mod select;
mod update;
mod delete;

#[derive(Debug, Clone)]
/// Shorthand for constructing any table query
pub struct Query;

impl Query {
    /// Construct a table [`SelectStatement`]
    pub fn select<'a>() -> SelectStatement<'a> {
        SelectStatement::new()
    }

    /// Construct a table [`UpdateStatement`]
    pub fn update<'a>() -> UpdateStatement<'a> {
        UpdateStatement::new()
    }

    /// Construct a table [`InsertStatement`]
    pub fn insert<'a>() -> InsertStatement<'a> {
        InsertStatement::new()
    }

    /// Construct a table [`DeleteStatement`]
    pub fn delete<'a>() -> DeleteStatement<'a> {
        DeleteStatement::new()
    }
}

/// Trait for all queries to implement to translate to a runnable SQL string.
pub trait SqlString {
    fn to_sql(&self) -> String;
}
