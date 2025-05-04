use std::io::Write;

use clap::Parser;
use log::{debug, error};
use raven_database::{
    Context, HistoryFilters, current_context, database::DatabaseError, history::model::History,
};

mod app;
mod duration;
mod event;
mod interactive;
mod tui;

#[derive(Debug, Parser)]
pub struct Cmd {
    /// Filter search result by directory
    #[arg(long, short)]
    cwd: Option<String>,

    /// Filter search by exit code
    #[arg(long, short)]
    exit: Option<i64>,

    /// Limit the number of results
    #[arg(long, short)]
    limit: Option<usize>,

    /// The command to search for
    query: Option<Vec<String>>,

    /// Flag that tells raven it was invoked from a shell up-key binding.
    #[arg(long = "shell-up-key", hide = true)]
    shell_up_key: bool,

    /// Open interactive search UI
    #[arg(long, short)]
    interactive: bool,

    /// [QUERY] is used as a prefix to generate a suggestion.
    #[arg(long, short)]
    suggest: bool,
}

impl Cmd {
    pub fn run(self, _context: &mut Context) {
        // Unwrap the query
        let query = self.query.map_or_else(
            || {
                std::env::var("RAVEN_QUERY").map_or_else(
                    |_| vec![],
                    |query| {
                        query
                            .split(' ')
                            .map(std::string::ToString::to_string)
                            .collect()
                    },
                )
            },
            |query| query,
        );

        if self.interactive {
            let Some(h) = interactive::history(&query) else {
                std::process::exit(1);
            };
            write_command_out(&h.command);
        } else {
            let filters = HistoryFilters {
                exit: self.exit,
                cwd: self.cwd,
                limit: self.limit,
                suggest: self.suggest,
            };
            debug!("search with filters {filters:?}");
            let Ok(entries) = run_non_interactive(&query, filters) else {
                // All we can do is exit with failed at this point.
                std::process::exit(1)
            };

            debug!("search had {} results", entries.len());
            if let Some(first) = entries.first() {
                debug!("first: {}", first.command);
            }

            if entries.is_empty() {
                std::process::exit(1)
            }

            for entry in entries {
                write_command_out(&entry.command);
            }
        }
    }
}

/// Run a `query` against the raven database and return the first result.
fn run_non_interactive(
    query: &[String],
    filters: HistoryFilters,
) -> Result<Vec<History>, DatabaseError> {
    let context = current_context();
    context.db.search(query.join(" ").as_str(), filters)
}

/// Write the `command` out to stdout
fn write_command_out(command: &String) {
    let w = std::io::stdout();
    let mut w = w.lock();
    let write = writeln!(w, "{command}");
    if let Err(err) = write {
        error!("write error {}", err);
        std::process::exit(1);
    }
    let _ = w.flush();
}
