//! Integration tests for memfill using testcontainers
//!
//! These tests build and run memfill inside a Linux container, enabling testing from macOS.
//!
//! Run with: cargo test --test integration_tests -- --nocapture
//!
//! Requirements:
//! - Docker must be running
//!
//! First run will take longer (~2-5 minutes) to build the binary inside the container.
//! Subsequent runs use cached artifacts (~5-10 seconds).

mod common;
mod memfill;

use memfill::{run_memfill, run_memfill_constrained};

fn count_pattern(text: &str, pattern: &str) -> usize {
    text.matches(pattern).count()
}

// ============================================================================
// Integration Tests
// ============================================================================

#[tokio::test]
async fn test_help_output() {
    let (stdout, stderr, exit) = run_memfill(&["--help"]).await;
    let output = format!("{}{}", stdout, stderr);

    println!("=== Test: Help Output ===");
    println!("Exit Code: {}", exit);
    println!("{}", output);

    assert_eq!(exit, 0, "Exit Code: {}", exit);
    assert!(
        output.contains("memfill") || output.contains("USAGE"),
        "Expected help output to contain 'memfill' or 'USAGE'. Output:\n{}",
        output
    );
}

#[tokio::test]
async fn test_invalid_arguments() {
    let (stdout, stderr, exit) = run_memfill(&["not-a-size", "absolute", "10s"]).await;
    let output = format!("{}{}", stdout, stderr);

    println!("=== Test: Invalid Arguments ===");
    println!("Exit Code: {}", exit);
    println!("{}", output);

    assert_ne!(exit, 0, "Exit Code: {}", exit);
    assert!(
        output.to_lowercase().contains("error") || output.to_lowercase().contains("invalid"),
        "Expected error message for invalid arguments. Output:\n{}",
        output
    );
}

#[tokio::test]
async fn test_successful_allocation_within_limits() {
    let (stdout, stderr, exit) = run_memfill_constrained(
        Some(100 * 1024 * 1024), // 100MB
        None,
        &["20M", "absolute", "3s"],
        10,
    )
    .await;

    let output = format!("{}{}", stdout, stderr);
    let allocated_count = count_pattern(&output, "Allocated");
    let fork_failures = count_pattern(&output, "Fork failed");
    let killed_count = count_pattern(&output, "Killed by SIGKILL");

    println!("=== Test: Successful Allocation ===");
    println!("Allocated: {}", allocated_count);
    println!("Fork failures: {}", fork_failures);
    println!("OOM kills: {}", killed_count);
    println!("--- Output ---");
    println!("Exit Code: {}", exit);
    println!("{}", output);

    assert_eq!(exit, 0, "Exit Code: {}", exit);
    assert!(
        allocated_count > 0,
        "Expected successful allocation within limits. Output:\n{}",
        output
    );

    assert_eq!(
        fork_failures, 0,
        "Expected no fork failures within limits. Output:\n{}",
        output
    );

    assert_eq!(
        killed_count, 0,
        "Expected no OOM kills within limits. Output:\n{}",
        output
    );
}

#[tokio::test]
async fn test_percentage_allocation() {
    let (stdout, stderr, exit) = run_memfill_constrained(
        Some(100 * 1024 * 1024), // 100MB
        None,
        &["10%", "absolute", "3s"],
        10,
    )
    .await;

    let output = format!("{}{}", stdout, stderr);
    let allocated_count = count_pattern(&output, "Allocated 10.0 MiB");

    println!("=== Test: Percentage Allocation ===");
    println!("Allocated: {}", allocated_count);
    println!("--- Output ---");
    println!("Exit Code: {}", exit);
    println!("{}", output);

    assert_eq!(exit, 0, "Exit Code: {}", exit);
    assert!(
        allocated_count > 0,
        "Expected successful percentage allocation. Output:\n{}",
        output
    );
}

/// Test: Fork failure (EAGAIN) with PID limit
///
/// Setting pids_limit to a low value causes fork() to fail with EAGAIN.
/// We use PID limit 3 which allows the container's init, timeout, and memfill to run,
/// but prevents memfill from forking child processes for allocation.
#[tokio::test]
async fn test_fork_failure_eagain_with_pid_limit() {
    let (stdout, stderr, exit) = run_memfill_constrained(
        Some(100 * 1024 * 1024), // 100MB memory - enough for memfill itself
        Some(3),                 // Allow: init + timeout + memfill, but no child forks
        &["50M", "absolute", "10s"],
        15,
    )
    .await;

    let output = format!("{}{}", stdout, stderr);
    let fork_failures = count_pattern(&output, "Fork failed: EAGAIN");

    println!("=== Test: Fork Failure (EAGAIN) ===");
    println!("Fork failures: {}", fork_failures);
    println!("--- Output ---");
    println!("Exit Code: {}", exit);
    println!("--- First 50 lines of output ---");
    for line in output.lines().take(50) {
        println!("{}", line);
    }

    assert_eq!(exit, 0, "Exit Code: {}", exit);
    assert!(
        fork_failures > 0 ,
        "Expected fork failures, resource unavailable, memfill to start, or container start failure. Output:\n{}",
        output
    );
}

#[tokio::test]
async fn test_oom_killer() {
    let (stdout, stderr, exit) = run_memfill_constrained(
        Some(50 * 1024 * 1024), // 50MB memory limit
        None,
        &["100M", "absolute", "30s"],
        10,
    )
    .await;

    let output = format!("{}{}", stdout, stderr);
    let killed_count = count_pattern(&output, "Killed by SIGKILL");
    let allocated_count = count_pattern(&output, "Allocated");

    println!("=== Test: OOM Killer Thrashing ===");
    println!("Allocated events: {}", allocated_count);
    println!("Killed events: {}", killed_count);
    println!("--- Output ---");
    println!("Exit Code: {}", exit);
    println!("--- First 50 lines of output ---");
    for line in output.lines().take(50) {
        println!("{}", line);
    }

    assert!(
        killed_count > 0,
        "Expected OOM kills when exceeding memory limit. Output:\n{}",
        output
    );
}

#[tokio::test]
async fn test_cgroup_memory_detection() {
    let (stdout, stderr, exit) = run_memfill_constrained(
        Some(64 * 1024 * 1024), // 64MB limit
        None,
        &["10M", "absolute", "2s"],
        10,
    )
    .await;

    let output = format!("{}{}", stdout, stderr);

    println!("=== Test: Cgroup Memory Detection ===");
    println!("Exit Code: {}", exit);
    println!("{}", output);

    assert_eq!(exit, 0, "Exit Code: {}", exit);

    // The output should show it detected the cgroup limit (around 64MB)
    // and successfully allocated ~10MB within it
    let matched = output.contains("Allocating 9.5 MiB (15% of total memory)");
    assert!(
        matched,
        "Expected allocation in cgroup-constrained container. Output:\n{}",
        output
    );
}

#[tokio::test]
async fn test_ignore_cgroup_flag() {
    let (stdout, stderr, exit) = run_memfill_constrained(
        Some(100 * 1024 * 1024), // 100MB limit
        None,
        &["10M", "absolute", "2s", "--ignore-cgroup"],
        10,
    )
    .await;

    let output = format!("{}{}", stdout, stderr);

    println!("=== Test: Ignore Cgroup Flag ===");
    println!("Exit Code: {}", exit);
    println!("{}", output);

    assert_eq!(exit, 0, "Exit Code: {}", exit);

    // Verify it's using system memory (GiB scale), not cgroup limit (100MB)
    // When --ignore-cgroup works, "Available memory" shows in GiB (system RAM)
    // If it were using the 100MB cgroup limit, it would show in MiB
    assert!(
        output.contains("GiB"),
        "Expected system memory (GiB scale) with --ignore-cgroup, not cgroup limit. Output:\n{}",
        output
    );
}

#[tokio::test]
async fn test_usage_mode_allocation() {
    let (stdout, stderr, exit) = run_memfill_constrained(
        Some(100 * 1024 * 1024), // 100MB limit
        None,
        &["80M", "usage", "3s"], // Allocate until 80MB remains available
        10,
    )
    .await;

    let output = format!("{}{}", stdout, stderr);

    println!("=== Test: Usage Mode Allocation ===");
    println!("Exit Code: {}", exit);
    println!("{}", output);

    assert_eq!(exit, 0, "Exit Code: {}", exit);

    let matched = output.contains("Allocate until 23.");
    assert!(
        matched,
        "Expected allocation in cgroup-constrained container. Output:\n{}",
        output
    );
}
