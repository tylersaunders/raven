use std::{env, path::PathBuf};

#[must_use]
pub fn get_current_dir() -> String {
    // Prefer PWD environment variable over cwd if available to better support symbolic links
    match env::var("PWD") {
        Ok(v) => v,
        Err(_) => match env::current_dir() {
            Ok(dir) => dir.display().to_string(),
            Err(_) => String::new(),
        },
    }
}

/// Fetch the home directory on unix systems via the $HOME env variable.
///
/// # Panics
///
/// Panics if $HOME variable is not set.
#[must_use]
pub fn get_home_dir() -> PathBuf {
    let home = std::env::var("HOME").expect("$HOME not found");
    PathBuf::from(home)
}

/// Fetch the data directory for raven to store data.
///
/// Defaults to `$XDG_DATA_HOME` or `$HOME/.local/share/raven` if `$XDG_DATA_HOME` cannot be found.
pub fn get_data_dir() -> PathBuf {
    let data_dir = std::env::var("XDG_DATA_HOME").map_or_else(
        |_| get_home_dir().join(".local").join("share"),
        PathBuf::from,
    );
    data_dir.join("raven")
}

/// Fetch the config directory for locating any user set raven configuration.
///
/// Defaults to `$XDG_CONFIG_HOME` or `$HOME/.config/raven` if `$XDG_CONFIG_HOME` cannot be found.
#[must_use]
pub fn get_config_dir() -> PathBuf {
    let data_dir = std::env::var("XDG_CONFIG_HOME")
        .map_or_else(|_| get_home_dir().join(".config"), PathBuf::from);
    data_dir.join("raven")
}
