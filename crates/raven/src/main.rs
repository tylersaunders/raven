use std::fs::File;

use clap::Parser;
use command::RavenCmd;
use env_logger::{Builder, Env, Target};
use raven_common::utils::get_data_dir;
mod command;

const VERSION: &str = env!("CARGO_PKG_VERSION");
const LOG_FILE: &str = "raven.log";

static HELP_TEMPLATE: &str = "\
    {before-help} {name} {version}
    {author}
    {about}

    {usage-heading}
      {usage}


    {all-args}
    {after-help}";

#[derive(Parser)]
#[command(
    author = "Tyler Saunders <tyler@thesummit.dev>",
    version = VERSION,
    help_template(HELP_TEMPLATE),
)]
struct Raven {
    #[command(subcommand)]
    raven: RavenCmd,
}

impl Raven {
    fn run(self) {
        self.raven.run();
    }
}

fn main() {
    #[cfg(debug_assertions)]
    {
        // For debug builds, write logging to a raven.log file in the data dir.
        let log_file =
            Box::new(File::create(get_data_dir().join(LOG_FILE)).expect("Cannot create log file"));
        let env = Env::new().filter_or("RAVEN_LOG", "debug");
        let mut builder = Builder::from_env(env);
        builder.target(Target::Pipe(log_file));
        builder.init();
    }

    #[cfg(not(debug_assertions))]
    {
        let log_file =
            Box::new(File::create(get_data_dir().join(LOG_FILE)).expect("Cannot create log file"));
        let env = Env::new().filter_or("RAVEN_LOG", "error");
        let mut builder = Builder::from_env(env);
        builder.target(Target::Pipe(log_file));
        builder.init();
    }

    Raven::parse().run();
}
