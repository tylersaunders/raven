use database::{Database, sqlite::Sqlite};
use raven_common::utils;

pub mod database;
pub mod history;
pub mod import;

/// Context object
///
/// * `cwd`: The current working directory of the shell.
/// * `db`: The Raven database implementation.
pub struct Context {
    pub cwd: String,
    pub db: Box<dyn Database>,
}

#[must_use]
/// Fetch the current Raven context
pub fn current_context() -> Context {
    let cwd = utils::get_current_dir();

    Context {
        cwd,
        db: Box::new(Sqlite::new()),
    }
}
