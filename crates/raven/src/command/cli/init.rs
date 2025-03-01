use clap::{Parser, ValueEnum};
mod zsh;

#[derive(Parser, Debug)]
/// Initialization Command for Raven
///
/// * `shell`: The shell type Raven is running under.
pub struct Cmd {
    shell: Shell,
}

#[derive(Clone, Copy, ValueEnum, Debug)]
/// An enumeration of supported Shells.
pub enum Shell {
    Zsh,
}

impl Cmd {
    /// Command runner to init raven for the selected shell.
    pub fn run(self) {
        match self.shell {
            Shell::Zsh => {
                zsh::init();
            }
        }
    }
}
