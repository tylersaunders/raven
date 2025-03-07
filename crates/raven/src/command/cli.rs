use clap::Subcommand;
mod history;
mod init;
mod search;

#[derive(Subcommand, Debug)]
#[command(infer_subcommands = true)]
pub enum Cmd {
    /// Print Raven's shell init script
    #[command()]
    Init(init::Cmd),

    #[command(subcommand)]
    History(history::Cmd),

    Search(search::Cmd),
}

impl Cmd {
    pub fn run(self) {
        // CLI commands block the current thread until they resolve.
        match self {
            Self::Init(init) => {
                init.run();
            }
            Self::History(history) => {
                history.run();
            }
            Self::Search(search) => {
                search.run();
            }
        }
    }
}
