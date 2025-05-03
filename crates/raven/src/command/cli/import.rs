use std::env;

use clap::Parser;
use raven_database::{
    Context,
    database::DatabaseError,
    history::model::History,
    import::{ImportError, Importer, LoadError, Loader, zsh::Zsh},
};

#[derive(Debug, Parser)]
pub enum Cmd {
    // Import history for the current shell
    Auto,

    // Import history from the zsh history file
    Zsh,
}

impl Cmd {
    pub fn run(self, context: &mut Context) {
        match self {
            Self::Auto => {
                let shell = env::var("SHELL").unwrap_or_else(|_| String::from("NO_SHELL"));
                if shell.ends_with("/zsh") {
                    println!("Detected ZSH!");
                    import::<Zsh>(context).expect("expected zsh import");
                    return;
                }
                panic!("not able to detect a supported shell type.")
            }
            Self::Zsh => {
                println!("Importing zsh");
                import::<Zsh>(context).expect("Expected zsh import");
            }
        }
    }
}

/// Imports Shell history for the provided shell type.
///
/// * `context`: The current raven context
fn import<I: Importer>(context: &mut Context) -> Result<(), ImportError> {
    let importer = I::new()?;
    println!("Importing history for {}", I::NAME);
    let mut loader = HistoryLoader::new(context);
    let _ = importer.load(&mut loader);
    let _ = loader.flush();
    println!("done! Imported {} commands", loader.count);
    Ok(())
}

pub struct HistoryLoader<'a> {
    buf: Vec<History>,
    context: &'a mut Context,
    count: usize,
}

impl<'a> HistoryLoader<'a> {
    fn new(context: &'a mut Context) -> Self {
        Self {
            buf: Vec::with_capacity(1000),
            context,
            count: 0,
        }
    }

    fn flush(&mut self) -> Result<(), DatabaseError> {
        if !self.buf.is_empty() {
            self.context.db.save_bulk(&self.buf)?;
        }
        self.count += self.buf.len();
        Ok(())
    }
}

impl Loader for HistoryLoader<'_> {
    fn push(&mut self, hist: History) -> Result<(), raven_database::import::LoadError> {
        self.buf.push(hist);
        if self.buf.len() == self.buf.capacity() {
            match self.context.db.save_bulk(&self.buf) {
                Ok(_) => self.buf.clear(),
                Err(_) => return Err(LoadError),
            }
            self.count += self.buf.capacity();
        }
        Ok(())
    }
}
