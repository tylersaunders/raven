use std::{env, path::PathBuf};

use time::OffsetDateTime;

use super::{ImportError, Importer, read_lines};
use crate::history::model::History;

#[derive(Debug)]
pub struct Zsh {
    histpath: PathBuf,
}

impl Zsh {
    /// Attempts to locate the known default history file sources for the ZSH shell.
    fn default_histpath() -> Result<PathBuf, ImportError> {
        let Ok(home_dir) = env::var("HOME") else {
            panic!("$HOME is not set, cannot locate home directory")
        };

        let home = PathBuf::from(home_dir);

        let mut candidates = [".zhistory", ".zsh_history", ".histfile"].iter();

        loop {
            match candidates.next() {
                Some(candidate) => {
                    let histpath = home.join(candidate);
                    if histpath.exists() {
                        println!("Found histfile at {}", histpath.to_str().unwrap());
                        break Ok(histpath);
                    }
                }
                None => break Err(ImportError),
            }
        }
    }
}

impl Importer for Zsh {
    const NAME: &'static str = "zsh";

    fn new() -> Result<Self, ImportError> {
        Ok(Self {
            histpath: Zsh::default_histpath()?,
        })
    }

    fn load(self, loader: &mut impl super::Loader) -> Result<(), ImportError> {
        if let Ok(lines) = read_lines(self.histpath) {
            for line in lines.map_while(Result::ok) {
                if let Some(command) = line.strip_prefix(": ") {
                    let (time, elapsed) = command.split_once(':').unwrap();
                    let (_, command) = elapsed.split_once(';').unwrap();

                    let time = time
                        .parse::<i64>()
                        .ok()
                        .and_then(|t| OffsetDateTime::from_unix_timestamp(t).ok())
                        .unwrap_or_else(OffsetDateTime::now_utc);

                    let imported = History::import().command(command).timestamp(time).build();

                    let _ = loader.push(imported.into());
                }
            }
        }
        Ok(())
    }
}
