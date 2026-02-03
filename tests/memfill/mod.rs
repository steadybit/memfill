//! Memfill-specific test utilities
//!
//! Provides functions for running the memfill binary in test containers.

use super::common::{ensure_binary_built, run_in_container, ContainerConfig};

/// Path to the built binary inside container
const BINARY_PATH: &str = "/build/target/release/memfill";

/// Run memfill with given args in a test container
pub async fn run_memfill(args: &[&str]) -> (String, String, i64) {
    ensure_binary_built().await;

    let mut cmd = vec![BINARY_PATH];
    cmd.extend(args);

    run_in_container(ContainerConfig::new(), &cmd).await
}

/// Execute memfill in a container with custom configuration
///
/// More flexible version that accepts a ContainerConfig directly.
pub async fn run_memfill_with_config(
    config: ContainerConfig,
    args: &[&str],
    timeout_secs: u64,
) -> (String, String, i64) {
    ensure_binary_built().await;

    // Build command with timeout wrapper
    let timeout_str = format!("{}s", timeout_secs);
    let mut cmd = vec!["timeout", &timeout_str, BINARY_PATH];
    cmd.extend(args);

    run_in_container(config, &cmd).await
}

/// Execute memfill in a constrained container
///
/// This is a convenience function that combines container creation and command execution.
pub async fn run_memfill_constrained(
    memory_bytes: Option<i64>,
    pids_limit: Option<i64>,
    args: &[&str],
    timeout_secs: u64,
) -> (String, String, i64) {
    let mut config = ContainerConfig::new();
    if let Some(mem) = memory_bytes {
        config = config.with_memory(mem);
    }
    if let Some(pids) = pids_limit {
        config = config.with_pids_limit(pids);
    }
    run_memfill_with_config(config, args, timeout_secs).await
}
