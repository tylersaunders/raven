use clap::ValueEnum;
use database::{Database, sqlite::Sqlite};
use raven_common::{
    config::{Config, load_config},
    utils,
};

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
    pub config: Config,
}

/// Specifies the matching mode for queries.
#[derive(Clone, Debug, Copy, ValueEnum)]
pub enum MatchMode {
    /// Query is an initial prefix.
    Prefix,
    /// Uses a fuzzy matching algorithm to find terms.
    Fuzzy,
}

impl Default for MatchMode {
    fn default() -> Self {
        Self::Fuzzy
    }
}

/// Optional filters that can be used for searching for History objects.
#[derive(Default, Clone, Debug)]
pub struct HistoryFilters {
    pub exit: Option<i64>,
    pub cwd: Option<String>,
    pub limit: Option<usize>,
    pub mode: MatchMode,
}

#[must_use]
/// Fetch the current Raven context
pub fn current_context() -> Context {
    let cwd = utils::get_current_dir();
    let config = load_config().unwrap_or_default();

    Context {
        cwd,
        db: Box::new(Sqlite::new(&config)),
        config,
    }
}
