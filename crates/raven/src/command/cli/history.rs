//! History module for storing shell history in the raven db.
use clap::Subcommand;
use raven_common::utils;
use raven_database::{current_context, history::model::History, Context};
use time::OffsetDateTime;

/// `History` subcommands for storing shell history in the raven db.
#[derive(Subcommand, Debug)]
#[command(infer_subcommands = true)]
pub enum Cmd {
    /// Stores an initial command to the history database.
    Start { command: Vec<String> },

    /// Updates a command with the commands results
    End {
        id: String,

        #[arg(long, short)]
        exit: i64,
    },
}

impl Cmd {
    /// Runs the matching [History] subcommand.
    pub fn run(self, context:&mut Context) {
        match self {
            Self::Start { command } => Self::handle_start(context, &command),
            Self::End { id, exit } => Self::handle_end(&id, exit),
        }
    }

    /// Hook for when the next command being run is known, but has not yet been executed.
    /// For ZSH, this is the preexec hook.
    ///
    /// * `command`: The shell command that is about to be run by the shell.
    fn handle_start(context: &mut Context, command: &[String]) {
        let captured = History::capture()
            .cwd(utils::get_current_dir())
            .command(command.join(" "))
            .timestamp(OffsetDateTime::now_utc())
            .build();
        match context.db.save(&captured.into()) {
            // Print the ID to stdout, it will be used for history end {id}
            Ok(id) => println!("{id}"),
            Err(err) => panic!("{err}"),
        };
    }

    /// Hook for after the command is finished running.
    /// For ZSH, this is the precmd hook.
    ///
    /// * `id`: The raven db id for the command that just finished.
    /// * `exit`: the exit code for the command
    fn handle_end(id: &str, exit: i64) {
        if id.trim() == "" {
            return;
        }

        // Return if we cant get an i64 out of the provided str
        let Ok(parsed_id) = id.parse::<i64>() else {
            return;
        };

        let context = current_context();
        let Ok(Some(mut h)) = context.db.get(parsed_id) else {
            return;
        };

        h.exit_code = exit;
        let _ = context.db.update(&h);
    }
}
