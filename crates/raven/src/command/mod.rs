use clap::Subcommand;

mod cli;

#[derive(Subcommand)]
#[command(infer_subcommands = true)]
pub enum RavenCmd {
    #[command(flatten)]
    Cli(cli::Cmd),
}

impl RavenCmd {
    pub fn run(self) {
        match self {
            Self::Cli(cli) => cli.run(),
        }
    }
}
