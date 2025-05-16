use std::{
    env,
    fs::File,
    io::{BufRead, BufReader},
    path::PathBuf,
};

use time::{Duration, OffsetDateTime};

use super::{ImportError, Importer, Loader};
use crate::history::model::History;

#[derive(Debug)]
pub struct Zsh {
    histpath: PathBuf,
}

/// Represents the type of command currently being accumulated.
#[derive(Debug, Clone, Copy)]
enum ActiveCommandContext {
    /// No command is currently being built, or the last one was finalized.
    None,
    /// Accumulating a simple (non-extended) command.
    Simple,
    /// Accumulating an extended command.
    Extended {
        timestamp: OffsetDateTime,
        /// True if the last line of this extended command ended with '\', expecting continuation.
        more_lines_expected: bool,
    },
}

/// Represents the parsed type of a single line from the history file.
#[derive(Debug)]
enum ParsedLine {
    /// A valid extended history header: `timestamp, command_part, ends_with_backslash`
    ExtendedHeader(OffsetDateTime, String, bool),
    /// A line that looks like an extended header but is malformed. Contains the original line content.
    MalformedExtended(String),
    /// A simple command line. Contains the line content.
    Simple(String),
    /// An empty or whitespace-only line.
    Empty,
}

impl Zsh {
    fn default_histpath() -> Result<PathBuf, ImportError> {
        let Ok(home_dir) = env::var("HOME") else {
            eprintln!("Error: $HOME is not set, cannot locate home directory");
            return Err(ImportError);
        };

        let home = PathBuf::from(home_dir);

        for candidate in &[".zhistory", ".zsh_history", ".histfile"] {
            let histpath = home.join(candidate);
            if histpath.exists() {
                // Using eprintln for consistency, or switch all to stdout for info messages
                eprintln!(
                    "Found histfile at {}",
                    histpath.to_str().unwrap_or("<invalid path>")
                );
                return Ok(histpath);
            }
        }

        eprintln!(
            "Error: Could not find a standard Zsh history file in {}.",
            home.display()
        );
        Err(ImportError)
    }

    /// Classifies a line and parses it if it's a valid extended header.
    fn classify_and_parse_line(line_text: &str) -> ParsedLine {
        let trimmed_line = line_text.trim_end();
        if trimmed_line.is_empty() {
            return ParsedLine::Empty;
        }

        if !trimmed_line.starts_with(": ") {
            return ParsedLine::Simple(trimmed_line.to_string());
        }

        // Potential extended header (starts with ": ")
        let original_line_for_error = trimmed_line.to_string(); // For error messages
        let Some(command_part_after_prefix) = trimmed_line.strip_prefix(": ") else {
            // Should not happen due to starts_with check, but defensive
            let err = concat!(
                "Warning: Line starts with ': ' but strip_prefix failed. Treating as simple ",
                "command: {original_line_for_error}"
            );
            eprintln!("{err}");
            return ParsedLine::MalformedExtended(original_line_for_error);
        };

        let parts: Vec<&str> = command_part_after_prefix.splitn(2, ':').collect();
        if parts.len() < 2 {
            let err = concat!(
                "Warning: Line starts with ': ' but missing timestamp separator ':'. Treating as ",
                "simple command: {original_line_for_error}"
            );
            eprintln!("{err}");
            return ParsedLine::MalformedExtended(original_line_for_error);
        }
        let timestamp_str = parts[0].trim();
        let rest_after_ts = parts[1];

        let parts2: Vec<&str> = rest_after_ts.splitn(2, ';').collect();
        if parts2.len() < 2 {
            let err = concat!(
                "Warning: Line starts with ': ' and has ':', but missing command separator ';'.  ",
                "Treating as simple command: {original_line_for_error}"
            );
            eprintln!("{err}");
            return ParsedLine::MalformedExtended(original_line_for_error);
        }
        let command_start_of_line = parts2[1].trim_start(); // command part

        if let Ok(ts_val) = timestamp_str.parse::<i64>() {
            if let Ok(timestamp) = OffsetDateTime::from_unix_timestamp(ts_val) {
                let ends_with_backslash = command_start_of_line.ends_with(r"\\");
                ParsedLine::ExtendedHeader(
                    timestamp,
                    command_start_of_line.to_string(),
                    ends_with_backslash,
                )
            } else {
                eprintln!(
                    "Warning: Line has extended format but invalid Unix timestamp value '{ts_val}'. Treating as simple command: {original_line_for_error}"
                );
                ParsedLine::MalformedExtended(original_line_for_error)
            }
        } else {
            eprintln!(
                "Warning: Line has extended format but non-numeric timestamp '{timestamp_str}'. Treating as simple command: {original_line_for_error}"
            );
            ParsedLine::MalformedExtended(original_line_for_error)
        }
    }

    /// Finalizes a command block, builds a History object, and pushes it to the loader.
    fn finalize_command_block(
        lines_buffer: &mut Vec<String>,
        context: ActiveCommandContext,
        non_extended_offset_seconds: &mut i64,
        now_for_simple: OffsetDateTime,
        loader: &mut impl Loader,
    ) -> Result<(), ImportError> {
        if lines_buffer.is_empty() {
            return Ok(());
        }

        let command_text = lines_buffer.join("\n").replace(r"\\", r"\");
        let timestamp = match context {
            ActiveCommandContext::Extended { timestamp, .. } => timestamp,
            ActiveCommandContext::Simple | ActiveCommandContext::None => {
                // None implies simple if buffer not empty
                let ts = now_for_simple - Duration::seconds(*non_extended_offset_seconds);
                *non_extended_offset_seconds += 1;
                ts
            }
        };

        let imported = History::import()
            .command(command_text)
            .timestamp(timestamp)
            .build();
        loader.push(imported.into()).map_err(|_| ImportError)?;

        lines_buffer.clear();
        Ok(())
    }
}

impl Importer for Zsh {
    const NAME: &'static str = "zsh";

    fn new() -> Result<Self, ImportError> {
        Ok(Self {
            histpath: Zsh::default_histpath()?,
        })
    }

    #[allow(clippy::too_many_lines)]
    fn load(self, loader: &mut impl Loader) -> Result<(), ImportError> {
        let file = File::open(&self.histpath)?;
        let reader = BufReader::new(file);

        let mut non_extended_offset_seconds: i64 = 0;
        let now = OffsetDateTime::now_utc();

        let mut lines_buffer: Vec<String> = Vec::new();
        let mut active_context = ActiveCommandContext::None;

        for read_line_result in reader.lines() {
            let line_text = match read_line_result {
                Ok(l) => l,
                Err(e) => {
                    eprintln!("Warning: Error reading line from history file: {e}");
                    continue;
                }
            };

            let parsed_line = Zsh::classify_and_parse_line(&line_text);

            match parsed_line {
                ParsedLine::ExtendedHeader(timestamp, cmd_part, ends_with_backslash) => {
                    Zsh::finalize_command_block(
                        &mut lines_buffer,
                        active_context,
                        &mut non_extended_offset_seconds,
                        now,
                        loader,
                    )?;
                    lines_buffer.push(cmd_part);
                    active_context = ActiveCommandContext::Extended {
                        timestamp,
                        more_lines_expected: ends_with_backslash,
                    };
                    if !ends_with_backslash {
                        Zsh::finalize_command_block(
                            &mut lines_buffer,
                            active_context,
                            &mut non_extended_offset_seconds,
                            now,
                            loader,
                        )?;
                        active_context = ActiveCommandContext::None;
                    }
                }
                ParsedLine::MalformedExtended(original_line) => {
                    Zsh::finalize_command_block(
                        &mut lines_buffer,
                        active_context,
                        &mut non_extended_offset_seconds,
                        now,
                        loader,
                    )?;
                    lines_buffer.push(original_line);
                    active_context = ActiveCommandContext::Simple; // Treat as simple
                    Zsh::finalize_command_block(
                        // Malformed is always single line
                        &mut lines_buffer,
                        active_context,
                        &mut non_extended_offset_seconds,
                        now,
                        loader,
                    )?;
                    active_context = ActiveCommandContext::None;
                }
                ParsedLine::Simple(simple_content) => {
                    match active_context {
                        ActiveCommandContext::Extended {
                            timestamp,
                            more_lines_expected,
                        } => {
                            if more_lines_expected {
                                lines_buffer.push(simple_content.clone());
                                let current_line_ends_backslash = simple_content.ends_with(r"\\");
                                active_context = ActiveCommandContext::Extended {
                                    timestamp,
                                    more_lines_expected: current_line_ends_backslash,
                                };
                                if !current_line_ends_backslash {
                                    // Last line of multi-line extended
                                    Zsh::finalize_command_block(
                                        &mut lines_buffer,
                                        active_context,
                                        &mut non_extended_offset_seconds,
                                        now,
                                        loader,
                                    )?;
                                    active_context = ActiveCommandContext::None;
                                }
                            } else {
                                // Extended command wasn't expecting more, so it's done.
                                Zsh::finalize_command_block(
                                    &mut lines_buffer,
                                    active_context, // Finalize the preceding extended command
                                    &mut non_extended_offset_seconds,
                                    now,
                                    loader,
                                )?;
                                // Now start new simple command
                                lines_buffer.push(simple_content.clone());
                                active_context = ActiveCommandContext::Simple;
                                if !simple_content.ends_with(r"\\") {
                                    Zsh::finalize_command_block(
                                        &mut lines_buffer,
                                        active_context,
                                        &mut non_extended_offset_seconds,
                                        now,
                                        loader,
                                    )?;
                                    active_context = ActiveCommandContext::None;
                                }
                            }
                        }
                        ActiveCommandContext::Simple | ActiveCommandContext::None => {
                            // If context was None, it becomes Simple. If it was Simple, it continues.
                            lines_buffer.push(simple_content.clone());
                            active_context = ActiveCommandContext::Simple;
                            if !simple_content.ends_with(r"\\") {
                                Zsh::finalize_command_block(
                                    &mut lines_buffer,
                                    active_context,
                                    &mut non_extended_offset_seconds,
                                    now,
                                    loader,
                                )?;
                                active_context = ActiveCommandContext::None;
                            }
                        }
                    }
                }
                ParsedLine::Empty => {
                    match active_context {
                        ActiveCommandContext::Extended {
                            more_lines_expected,
                            ..
                        } => {
                            if more_lines_expected {
                                lines_buffer.push(String::new());
                                // `more_lines_expected` doesn't change by an empty line, assumes previous `\` still holds.
                            }
                            // else, ignore empty line if not part of an expected continuation.
                        }
                        ActiveCommandContext::Simple => {
                            if lines_buffer.last().is_some_and(|l| l.ends_with(r"\\")) {
                                lines_buffer.push(String::new());
                            }
                            // else, ignore if not after a `\` in simple mode.
                        }
                        ActiveCommandContext::None => {} // Ignore isolated empty lines.
                    }
                }
            }
        }

        // After the loop, process any remaining accumulated lines.
        Zsh::finalize_command_block(
            &mut lines_buffer,
            active_context,
            &mut non_extended_offset_seconds,
            now,
            loader,
        )?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        history::model::History,
        import::{LoadError, Loader},
    };
    use std::io::Write;
    use tempfile::NamedTempFile;
    use time::OffsetDateTime;

    // Mock Loader implementation for testing
    struct MockLoader {
        history: Vec<History>,
    }

    impl MockLoader {
        fn new() -> Self {
            MockLoader {
                history: Vec::new(),
            }
        }
    }

    impl Loader for MockLoader {
        fn push(&mut self, hist: History) -> Result<(), LoadError> {
            self.history.push(hist);
            Ok(())
        }
    }

    // Helper function to write content to a temp file and run the importer
    fn run_importer_with_content(content: &str) -> Result<Vec<History>, ImportError> {
        let mut temp_file = NamedTempFile::new().expect("Failed to create temp file");
        // Write content as is, to simulate actual file lines. Add a trailing newline if content is not empty
        // and doesn't already end with one, as files often have it.
        if !content.is_empty() {
            write!(temp_file, "{content}").expect("Failed to write to temp file");
            if !content.ends_with('\n') {
                writeln!(temp_file).expect("Failed to write trailing newline");
            }
        }
        temp_file.flush().expect("Failed to flush temp file"); // Ensure content is written

        let histpath = temp_file.path().to_path_buf();
        let zsh_importer = Zsh { histpath };
        let mut mock_loader = MockLoader::new();
        zsh_importer.load(&mut mock_loader)?;
        Ok(mock_loader.history)
    }

    #[test]
    fn test_load_empty_file() -> Result<(), ImportError> {
        let history = run_importer_with_content("")?;
        assert_eq!(
            history.len(),
            0,
            "Should import 0 commands from an empty file"
        );
        Ok(())
    }

    #[test]
    fn test_load_only_empty_lines() -> Result<(), ImportError> {
        let history = run_importer_with_content("\n\n  \t\n")?;
        assert_eq!(
            history.len(),
            0,
            "Should import 0 commands from only empty/whitespace lines"
        );
        Ok(())
    }

    #[test]
    fn test_load_simple_extended_format() -> Result<(), ImportError> {
        let content = ": 1678886400:0;ls -l\n: 1678886500:0;cd /tmp";
        let history = run_importer_with_content(content)?;

        assert_eq!(history.len(), 2, "Should import 2 commands");
        assert_eq!(history[0].command, "ls -l");
        assert_eq!(
            history[0].timestamp,
            OffsetDateTime::from_unix_timestamp(1_678_886_400).unwrap()
        );
        assert_eq!(history[1].command, "cd /tmp");
        assert_eq!(
            history[1].timestamp,
            OffsetDateTime::from_unix_timestamp(1_678_886_500).unwrap()
        );
        Ok(())
    }

    #[test]
    fn test_load_simple_non_extended_format() -> Result<(), ImportError> {
        let content = "echo hello\npwd"; // Removed trailing \n to test exactness
        let history = run_importer_with_content(content)?;

        assert_eq!(history.len(), 2, "Should import 2 commands");
        assert_eq!(history[0].command, "echo hello");
        let now = OffsetDateTime::now_utc();
        assert!((now - history[0].timestamp).abs() < Duration::seconds(5));

        assert_eq!(history[1].command, "pwd");
        assert_eq!(
            history[0].timestamp - history[1].timestamp,
            Duration::seconds(1)
        );
        Ok(())
    }

    #[test]
    fn test_load_multi_line_extended_command() -> Result<(), ImportError> {
        // This content will result in three lines read by `reader.lines()`
        // 1. ": 1678887000:0;echo \\"
        // 2. "> line 2 \\"
        // 3. "> line 3"
        // Then the next command.
        let content =
            ": 1678887000:0;echo \\\\\n> line 2\\\\\n> line 3\n: 1678887100:0;another cmd";
        let history = run_importer_with_content(content)?;

        assert_eq!(history.len(), 2, "Should import 2 commands");
        assert_eq!(history[0].command, "echo \\\n> line 2\\\n> line 3");
        assert_eq!(
            history[0].timestamp,
            OffsetDateTime::from_unix_timestamp(1_678_887_000).unwrap()
        );
        assert_eq!(history[1].command, "another cmd");
        assert_eq!(
            history[1].timestamp,
            OffsetDateTime::from_unix_timestamp(1_678_887_100).unwrap()
        );
        Ok(())
    }

    #[test]
    fn test_load_mixed_formats() -> Result<(), ImportError> {
        let content = "simple cmd 1\n: 1678888000:0;extended cmd 1\nsimple cmd 2\n: 1678888100:0;multi\\\\\nline\\\\\ncmd 2\nsimple cmd 3";
        let history = run_importer_with_content(content)?;

        assert_eq!(history.len(), 5, "Should import 5 commands");

        let now = OffsetDateTime::now_utc();

        assert_eq!(history[0].command, "simple cmd 1"); // Simple (offset 0)
        assert!((now - history[0].timestamp).abs() < Duration::seconds(5));

        assert_eq!(history[1].command, "extended cmd 1"); // Extended
        assert_eq!(
            history[1].timestamp,
            OffsetDateTime::from_unix_timestamp(1_678_888_000).unwrap()
        );

        assert_eq!(history[2].command, "simple cmd 2"); // Simple (offset 1)
        assert_eq!(
            history[0].timestamp - history[2].timestamp,
            Duration::seconds(1)
        );

        assert_eq!(history[3].command, "multi\\\nline\\\ncmd 2"); // Extended multi-line
        assert_eq!(
            history[3].timestamp,
            OffsetDateTime::from_unix_timestamp(1_678_888_100).unwrap()
        );

        assert_eq!(history[4].command, "simple cmd 3"); // Simple (offset 2)
        assert_eq!(
            history[2].timestamp - history[4].timestamp,
            Duration::seconds(1)
        );
        Ok(())
    }

    #[test]
    fn test_load_malformed_extended_lines() -> Result<(), ImportError> {
        let content = ": 1234567890;command_missing_colon\n: invalid_timestamp:0;valid_command\n: 1234567890::command_missing_semicolon";
        let history = run_importer_with_content(content)?;

        assert_eq!(
            history.len(),
            3,
            "Should import 3 commands (treated as simple)"
        );
        let now = OffsetDateTime::now_utc();

        assert_eq!(history[0].command, ": 1234567890;command_missing_colon");
        assert!((now - history[0].timestamp).abs() < Duration::seconds(5));

        assert_eq!(history[1].command, ": invalid_timestamp:0;valid_command");
        assert_eq!(
            history[0].timestamp - history[1].timestamp,
            Duration::seconds(1)
        );

        assert_eq!(
            history[2].command,
            ": 1234567890::command_missing_semicolon"
        );
        assert_eq!(
            history[1].timestamp - history[2].timestamp,
            Duration::seconds(1)
        );
        Ok(())
    }

    #[test]
    fn test_load_multi_line_non_extended_command() -> Result<(), ImportError> {
        // Corrected test content: original used \n plus spaces, this is direct line continuation.
        let content = "curl -fLo ~/some/dir/in/home --create-dirs \\\\\n    https://some-random-webside.thing";
        let history = run_importer_with_content(content)?;

        assert_eq!(history.len(), 1, "Should import as single command");
        // Corrected assertion for the command content
        assert_eq!(
            history[0].command,
            "curl -fLo ~/some/dir/in/home --create-dirs \\\n    https://some-random-webside.thing"
        );

        let now = OffsetDateTime::now_utc();
        assert!((now - history[0].timestamp).abs() < Duration::seconds(5));
        Ok(())
    }

    #[test]
    fn test_load_file_ends_mid_extended_command() -> Result<(), ImportError> {
        let content = "cmd1\n: 1678889000:0;line1\\\\\nline2\\\\\nline3"; // File ends here
        let history = run_importer_with_content(content)?;

        assert_eq!(history.len(), 2, "Should import 2 commands");
        let now = OffsetDateTime::now_utc();
        assert_eq!(history[0].command, "cmd1");
        assert!((now - history[0].timestamp).abs() < Duration::seconds(5));

        assert_eq!(history[1].command, "line1\\\nline2\\\nline3");
        assert_eq!(
            history[1].timestamp,
            OffsetDateTime::from_unix_timestamp(1_678_889_000).unwrap()
        );
        Ok(())
    }

    #[test]
    fn test_load_extended_command_with_no_extra_lines() -> Result<(), ImportError> {
        let content = ": 1678890000:0;single line command";
        let history = run_importer_with_content(content)?;

        assert_eq!(history.len(), 1, "Should import 1 command");
        assert_eq!(history[0].command, "single line command");
        assert_eq!(
            history[0].timestamp,
            OffsetDateTime::from_unix_timestamp(1_678_890_000).unwrap()
        );
        Ok(())
    }
}
