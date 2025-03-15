use time::OffsetDateTime;
use typed_builder::TypedBuilder;

#[derive(Debug, Clone, PartialEq, Eq, TypedBuilder)]
/// Represents a full row for a history record in the database.
///
/// * `id`: unique identifier, or -1 if not set.
/// * `timestamp`: unix timestamp (since epoc, utc) when the command was run
/// * `command`: plain-text command that was run
/// * `cwd`: plain-text working directory
/// * `exit_code`: the exit code of the command or -1 if not set
pub struct History {
    pub id: i64,

    pub timestamp: OffsetDateTime,

    pub command: String,

    pub cwd: String,

    pub exit_code: i64,
}

impl History {
    fn new(timestamp: OffsetDateTime, command: String, cwd: String, exit_code: i64) -> Self {
        Self {
            id: -1,
            timestamp,
            command,
            cwd,
            exit_code,
        }
    }

    /// Typed builder for capturing new history objects.
    pub fn capture() -> HistoryCapturedBuilder {
        HistoryCaptured::builder()
    }

    pub fn import() -> HistoryImportedBuilder {
        HistoryImported::builder()
    }
}

#[derive(Debug, Clone, TypedBuilder)]
/// The data required before a history object can be inserted into the database.
///
/// * `timestamp`: unix timestamp (since epoc, utc) when the command was run
/// * `command`: plain-text command that was run
/// * `cwd`: plain-text working directory
pub struct HistoryCaptured {
    timestamp: OffsetDateTime,

    #[builder(setter(into))]
    command: String,

    #[builder(setter(into))]
    cwd: String,
}

impl From<HistoryCaptured> for History {
    fn from(captured: HistoryCaptured) -> Self {
        History::new(captured.timestamp, captured.command, captured.cwd, -1)
    }
}

#[derive(Debug, Clone, TypedBuilder)]
/// The data required to import a history object from an import source (such as a histfile)
///
/// * `timestamp`: unix timestamp (since epoc, utc) when the command was run
/// * `command`: plain-text command that was run
pub struct HistoryImported {
    timestamp: OffsetDateTime,

    #[builder(setter(into))]
    command: String,
}

impl From<HistoryImported> for History {
    fn from(imported: HistoryImported) -> Self {
        History::new(
            imported.timestamp,
            imported.command,
            String::from("unknown"),
            -1,
        )
    }
}
