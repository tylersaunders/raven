use clap::Subcommand;
use raven_database::Context;
mod history;
mod import;
mod init;
mod search;

#[derive(Subcommand, Debug)]
#[command(infer_subcommands = true)]
pub enum Cmd {
    #[command(subcommand)]
    History(history::Cmd),

    #[command(subcommand)]
    Import(import::Cmd),

    /// Print Raven's shell init script
    #[command()]
    Init(init::Cmd),

    Search(search::Cmd),
}

impl Cmd {
    pub fn run(self, context: &mut Context) {
        // CLI commands block the current thread until they resolve.
        match self {
            Self::Init(init) => {
                init.run(context);
            }
            Self::History(history) => {
                history.run(context);
            }
            Self::Search(search) => {
                search.run(context);
            }
            Self::Import(import) => {
                import.run(context);
            }
        }
    }
}
