//! Memfill-specific test utilities
//!
//! Provides functions for running the memfill binary in test containers.

use super::common::{
    docker, ensure_binary_built, run_in_container, start_container, ContainerConfig,
};
use futures::StreamExt;
use std::time::Duration;
use testcontainers::{ContainerAsync, GenericImage};
use tokio::sync::OnceCell;

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

// ============================================================================
// Cross-namespace attach: run memfill against a *separate* target container's
// cgroup/PID namespace via --target-cgroup-path/--target-pid, mirroring how
// the Go wrapper attaches to an arbitrary already-running target in production.
// ============================================================================

/// A running "target" container plus the two coordinates needed to attach to
/// it. Keep it alive for as long as the target should stay up — the container
/// is removed when this is dropped.
pub struct Target {
    pub container: ContainerAsync<GenericImage>,
    /// PID in the Docker host/VM's PID namespace, as `docker inspect` reports
    /// it — the same kind of value the Go wrapper resolves in production.
    pub pid: i64,
    /// Cgroup path as read from `/proc/<pid>/cgroup`.
    pub cgroup_path: String,
}

impl Target {
    pub fn id(&self) -> &str {
        self.container.id()
    }
}

/// Starts a memory-limited target container and resolves its host PID and
/// cgroup path. Builds the binary first so a cold build can't run down the
/// target's `sleep` before memfill ever starts.
pub async fn start_target(memory_bytes: i64, sleep_secs: u64) -> Target {
    ensure_binary_built().await;

    let sleep_arg = sleep_secs.to_string();
    let container = start_container(
        ContainerConfig::new().with_memory(memory_bytes),
        &["sleep", &sleep_arg],
    )
    .await;

    let pid = host_pid(&container).await;
    let cgroup_path = cgroup_path_of(pid).await;
    Target {
        container,
        pid,
        cgroup_path,
    }
}

async fn host_pid(container: &ContainerAsync<GenericImage>) -> i64 {
    let info = docker()
        .inspect_container(
            container.id(),
            None::<bollard::query_parameters::InspectContainerOptions>,
        )
        .await
        .expect("Failed to inspect target container");
    info.state
        .and_then(|s| s.pid)
        .filter(|&pid| pid > 0)
        .expect("Target container has no running PID")
}

/// A single long-lived host-attached container, kept for the lifetime of the
/// test binary so that reading `/proc/<pid>/cgroup` costs one exec rather than
/// a fresh privileged container per lookup.
static PROC_READER: OnceCell<ContainerAsync<GenericImage>> = OnceCell::const_new();

async fn proc_reader() -> &'static ContainerAsync<GenericImage> {
    PROC_READER
        .get_or_init(|| async {
            start_container(
                ContainerConfig::new().with_host_attach(),
                &["sleep", "infinity"],
            )
            .await
        })
        .await
}

/// Resolves a host PID's cgroup path, mirroring `parseProcCgroupFile` in the
/// Go wrapper's `ociruntime/utils.go`.
///
/// Read from a host-attached container for the same reason production reads it
/// under `nsenter -t 1 -C`: in a private cgroup namespace `/proc/<pid>/cgroup`
/// is rendered relative to the *reader's* cgroup root, yielding unusable paths
/// like "/../<id>".
async fn cgroup_path_of(host_pid: i64) -> String {
    let out = exec_capture(
        proc_reader().await.id(),
        &["cat", &format!("/proc/{}/cgroup", host_pid)],
    )
    .await;
    parse_proc_cgroup(&out)
        .unwrap_or_else(|| panic!("Could not parse a cgroup path out of:\n{}", out))
}

/// Picks the lowest-hierarchy-id v1 line, falling back to the v2 unified line.
/// Note this selects by hierarchy id, not by controller name.
fn parse_proc_cgroup(contents: &str) -> Option<String> {
    let rows: Vec<(u32, &str)> = contents
        .lines()
        .filter_map(|line| {
            let mut fields = line.splitn(3, ':');
            let hid = fields.next()?.parse().ok()?;
            let _controllers = fields.next()?;
            Some((hid, fields.next()?))
        })
        .collect();

    let v1 = rows.iter().filter(|(h, _)| *h != 0).min_by_key(|(h, _)| *h);
    let v2 = rows.iter().find(|(h, _)| *h == 0);
    v1.filter(|(_, path)| *path != "/")
        .or(v2)
        .map(|(_, path)| path.to_string())
}

/// Runs memfill in a separate host-attached "attacker" container, joined to the
/// target's memory cgroup and PID namespace via the `--target-*` flags — what
/// the Go wrapper's `nsenter -t 1 -C -- memfill --target-...` does in production.
pub async fn run_memfill_targeting(
    target: &Target,
    args: &[&str],
    timeout_secs: u64,
) -> (String, String, i64) {
    let pid = target.pid.to_string();
    let mut full = vec![
        "--target-pid",
        &pid,
        "--target-cgroup-path",
        &target.cgroup_path,
    ];
    full.extend(args);

    run_memfill_with_config(
        ContainerConfig::new().with_host_attach(),
        &full,
        timeout_secs,
    )
    .await
}

/// Runs a command inside an existing container and returns its combined output.
async fn exec_capture(container_id: &str, cmd: &[&str]) -> String {
    let exec = docker()
        .create_exec(
            container_id,
            bollard::exec::CreateExecOptions {
                attach_stdout: Some(true),
                attach_stderr: Some(true),
                cmd: Some(cmd.to_vec()),
                ..Default::default()
            },
        )
        .await
        .expect("Failed to create exec");

    match docker()
        .start_exec(&exec.id, None)
        .await
        .expect("Failed to start exec")
    {
        bollard::exec::StartExecResults::Attached { mut output, .. } => {
            let mut out = String::new();
            while let Some(Ok(chunk)) = output.next().await {
                out.push_str(&chunk.to_string());
            }
            out
        }
        bollard::exec::StartExecResults::Detached => String::new(),
    }
}

/// Polls the target's own `/proc` until a process whose name contains `needle`
/// appears, or the timeout elapses.
///
/// `/proc` inside the container is rendered for that container's PID namespace,
/// so a process only shows up here if it is genuinely a member of it.
/// Deliberately NOT `docker top`, which filters by *cgroup* membership — that
/// would also list a process that merely joined the target's cgroup, and so
/// would pass even if the `setns(CLONE_NEWPID)` did nothing.
pub async fn wait_for_process_inside_container(
    container_id: &str,
    needle: &str,
    timeout: Duration,
) -> bool {
    tokio::time::timeout(timeout, async {
        while !exec_capture(
            container_id,
            &["sh", "-c", "cat /proc/[0-9]*/comm 2>/dev/null"],
        )
        .await
        .contains(needle)
        {
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
    })
    .await
    .is_ok()
}

/// Force-kills a container (used to simulate the target dying mid-attack).
pub async fn kill_container(container_id: &str) {
    docker()
        .kill_container(
            container_id,
            None::<bollard::query_parameters::KillContainerOptions>,
        )
        .await
        .expect("Failed to kill target container");
}
