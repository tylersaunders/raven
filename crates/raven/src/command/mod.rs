use clap::Subcommand;
use raven_database::Context;

mod cli;

#[derive(Subcommand)]
#[command(infer_subcommands = true)]
pub enum RavenCmd {
    #[command(flatten)]
    Cli(cli::Cmd),
}

impl RavenCmd {
    pub fn run(self, context: &mut Context) {
        match self {
            Self::Cli(cli) => cli.run(context),
        }
    }
}
