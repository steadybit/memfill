use std::fs;

/// Reads the "full" avg10 memory pressure from `/proc/pressure/memory`.
///
/// The value is the percentage (0.0-100.0) of the last 10 seconds during which
/// *all* non-idle tasks were stalled waiting on memory. A sustained non-trivial
/// value means the host is thrashing and close to becoming unresponsive.
///
/// Returns `None` when PSI is not available (kernel without `CONFIG_PSI`, or the
/// file is unreadable).
pub fn memory_full_avg10() -> Option<f64> {
    let content = fs::read_to_string("/proc/pressure/memory").ok()?;
    parse_full_avg10(&content)
}

/// Parses the `full` line's `avg10` field from `/proc/pressure/memory` content.
fn parse_full_avg10(content: &str) -> Option<f64> {
    content
        .lines()
        .find_map(|line| line.strip_prefix("full "))
        .and_then(|rest| rest.split_whitespace().find_map(|f| f.strip_prefix("avg10=")))
        .and_then(|value| value.parse().ok())
}

#[cfg(test)]
mod tests {
    use super::parse_full_avg10;

    const SAMPLE: &str = "some avg10=1.23 avg60=0.50 avg300=0.10 total=12345\n\
                          full avg10=4.56 avg60=0.20 avg300=0.05 total=6789\n";

    #[test]
    fn parses_the_full_avg10() {
        assert_eq!(parse_full_avg10(SAMPLE), Some(4.56));
    }

    #[test]
    fn does_not_confuse_the_some_line() {
        let s = "some avg10=9.99 avg60=0.00 avg300=0.00 total=1\n\
                 full avg10=0.00 avg60=0.00 avg300=0.00 total=1\n";
        assert_eq!(parse_full_avg10(s), Some(0.0));
    }

    #[test]
    fn returns_none_without_a_full_line() {
        assert_eq!(parse_full_avg10("some avg10=1.0 avg60=0 avg300=0 total=1\n"), None);
    }

    #[test]
    fn returns_none_on_unparseable_content() {
        assert_eq!(parse_full_avg10("garbage"), None);
        assert_eq!(parse_full_avg10("full avg10=nan? avg60=0 total=1\n"), None);
    }
}
