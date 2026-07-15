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
    for line in content.lines() {
        if let Some(rest) = line.strip_prefix("full ") {
            for field in rest.split_whitespace() {
                if let Some(value) = field.strip_prefix("avg10=") {
                    return value.parse().ok();
                }
            }
        }
    }
    None
}
