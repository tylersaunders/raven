use std::path::PathBuf;

use serde::Deserialize;

use crate::utils::get_config_dir;
use log::debug;

/// Represents the main application configuration structure.
///
/// Holds settings related to different parts of the application.
#[derive(Deserialize, Debug, Default)]
pub struct Config {
    pub database: DatabaseConfig,
}

/// Configuration settings specific to the database.
///
/// Allows specifying the directory path and filename for the database.
#[derive(Deserialize, Debug, Default)]
pub struct DatabaseConfig {
    pub database_path: Option<PathBuf>,
    pub database_file: Option<String>,
}

/// Loads the application configuration from a `config.toml` file.
///
/// The configuration file is expected to be located in the platform-specific
/// configuration directory retrieved via `get_config_dir()`.
/// If the configuration file is not found at the expected path, a default
/// `Config` instance is returned.
///
/// # Panics
/// If the config path does not contain valid UTF-8 characters.
///
/// # Returns
///
/// Returns a `Result` containing:
/// - `Ok(Config)`: The loaded configuration, either from the file or the default.
///
/// # Errors
/// - `Err(Box<dyn std::error::Error>)`: An error occurred during file reading or TOML parsing.
pub fn load_config() -> Result<Config, Box<dyn std::error::Error>> {
    // Find the config file path
    let config_path = get_config_dir().join("config.toml");

    // Read the file if it exists
    let config_str = if config_path.exists() {
        std::fs::read_to_string(&config_path)?
    } else {
        debug!("Could not find config at supported paths, using default config.");
        // Return default config if file doesn't exist, or handle as error
        return Ok(Config::default());
    };

    debug!(
        "loading config from {}",
        config_path
            .to_str()
            .expect("Config path did not contain valid characters")
    );
    // Parse the TOML string
    let config: Config = toml::from_str(&config_str)?;

    Ok(config)
}
