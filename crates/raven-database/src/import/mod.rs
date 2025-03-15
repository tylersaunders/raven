use std::{
    fs::File,
    io::{BufRead, BufReader, Error, Lines},
    path::Path,
};

use crate::history::model::History;

pub mod zsh;

#[derive(Debug)]
pub struct ImportError;

/// The importer handles parsing individual history items from an import source (such as a history
/// file ), transforming them to `History` objects and passing them to the Loader to be persisted.
pub trait Importer: Sized {
    const NAME: &'static str;

    /// Create a new Importer for the import source type
    ///
    /// # Errors
    ///
    /// This function will return an error if the importer has issues reading from the import source.
    fn new() -> Result<Self, ImportError>;

    /// Load the `History` data in the import source and pass it to the loader.
    ///
    /// # Errors
    ///
    /// This function will return an error if the importer has issues reading from the import source.
    fn load(self, loader: &mut impl Loader) -> Result<(), ImportError>;
}

pub struct LoadError;

/// The Loader handles persisting imported `History` objects into the raven
/// database.
pub trait Loader {
    /// Add the provided `History` object to this loader.
    ///
    /// # Errors
    ///
    /// This function will return an error if the loader encounters an
    /// issue with persisting the `History` object.
    fn push(&mut self, hist: History) -> Result<(), LoadError>;
}

fn read_lines<P>(filename: P) -> Result<Lines<BufReader<File>>, Error>
where
    P: AsRef<Path>,
{
    let file = File::open(filename)?;
    Ok(BufReader::new(file).lines())
}
