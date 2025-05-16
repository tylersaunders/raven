use std::io::Error as IoError;

use crate::history::model::History;

pub mod zsh;

#[derive(Debug)]
pub struct ImportError;

// Allow converting std::io::Error to ImportError
impl From<IoError> for ImportError {
    fn from(_: IoError) -> Self {
        ImportError
    }
}

#[derive(Debug)]
pub struct LoadError;

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
