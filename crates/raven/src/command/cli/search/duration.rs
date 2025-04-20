use core::fmt;
use std::{ops::ControlFlow, time::Duration};

/// Formats a `Duration` into a human-readable string, showing only the
/// most significant time unit.
///
/// Examples:
/// - 125 seconds -> "2m"
/// - 60 seconds -> "1m"
/// - 5 seconds -> "5s"
/// - 500 milliseconds -> "500ms"
/// - 0 duration -> "0s"
///
/// # Arguments
///
/// * `f` - The `Duration` to format.
///
/// # Returns
///
/// A `String` representation of the duration.
pub fn format_duration(f: Duration) -> String {
    struct F(Duration);
    impl fmt::Display for F {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            format_duration_into(self.0, f)
        }
    }
    F(f).to_string()
}

fn format_duration_into(dur: std::time::Duration, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    fn item(unit: &'static str, value: u64) -> ControlFlow<(&'static str, u64)> {
        if value > 0 {
            ControlFlow::Break((unit, value))
        } else {
            ControlFlow::Continue(())
        }
    }

    /// Helper function to format a `Duration` into a `fmt::Formatter`.
    ///
    /// This function calculates the time units and writes the largest non-zero
    /// unit and its value (e.g., "5m", "10s") to the formatter. If the duration
    /// is zero, it writes "0s".
    ///
    /// Based on implementation from the `humantime` crate:
    /// <https://github.com/tailhook/humantime/blob/master/src/duration.rs#L295-L331>
    ///
    /// # Arguments
    ///
    /// * `dur` - The `Duration` to format.
    /// * `f` - The `fmt::Formatter` to write the output to.
    ///
    /// # Returns
    ///
    /// A `fmt::Result` indicating success or failure.
    fn fmt(f: std::time::Duration) -> ControlFlow<(&'static str, u64), ()> {
        let secs = f.as_secs();
        let nanos = f.subsec_nanos();

        let years = secs / 31_557_600; // 365.25d
        let year_days = secs % 31_557_600;
        let months = year_days / 2_630_016; // 30.44d
        let month_days = year_days % 2_630_016;
        let days = month_days / 86400;
        let day_secs = month_days % 86400;
        let hours = day_secs / 3600;
        let minutes = day_secs % 3600 / 60;
        let seconds = day_secs % 60;

        let millis = nanos / 1_000_000;
        let micros = nanos / 1_000;

        // a difference from our impl than the original is that
        // we only care about the most-significant segment of the duration.
        // If the item call returns `Break`, then the `?` will early-return.
        // This allows for a very consise impl
        item("y", years)?;
        item("mo", months)?;
        item("d", days)?;
        item("h", hours)?;
        item("m", minutes)?;
        item("s", seconds)?;
        item("ms", u64::from(millis))?;
        item("us", u64::from(micros))?;
        item("ns", u64::from(nanos))?;
        ControlFlow::Continue(())
    }

    match fmt(dur) {
        ControlFlow::Break((unit, value)) => write!(f, "{value}{unit}"),
        ControlFlow::Continue(()) => write!(f, "0s"),
    }
}
