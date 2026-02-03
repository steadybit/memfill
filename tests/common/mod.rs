//! Common test utilities for container-based integration tests
//!
//! Provides functions for building and running executable inside Docker containers
//! using testcontainers.

#[allow(deprecated)]
use bollard::container::{LogsOptions, WaitContainerOptions};
use bollard::service::HostConfig;
use bollard::Docker;
use futures::StreamExt;
use std::path::PathBuf;
use std::time::Duration;
use testcontainers::core::{AccessMode, Mount};
use testcontainers::runners::AsyncRunner;
use testcontainers::{ContainerAsync, GenericImage, ImageExt};
use tokio::sync::OnceCell;

const RUST_IMAGE: &str = "rust";
const RUST_TAG: &str = "1-trixie";

/// Tracks whether the binary has been built (container is not kept)
static BUILD_COMPLETE: OnceCell<()> = OnceCell::const_new();

/// Project directory (where Cargo.toml lives)
pub fn project_dir() -> PathBuf {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(manifest_dir)
}

/// Cache directory for cargo artifacts
pub fn test_cache_dir() -> PathBuf {
    project_dir().join(".testcontainers")
}

/// Ensure cache directories exist
fn ensure_cache_dirs() -> std::io::Result<()> {
    let cache = test_cache_dir();
    std::fs::create_dir_all(cache.join("registry"))?;
    std::fs::create_dir_all(cache.join("git"))?;
    std::fs::create_dir_all(cache.join("target"))?;
    Ok(())
}

// ============================================================================
// Container Configuration
// ============================================================================

/// Configuration for creating a test container
#[derive(Default, Clone)]
pub struct ContainerConfig {
    /// Memory limit in bytes
    pub memory_bytes: Option<i64>,
    /// Memory + swap limit in bytes (set equal to memory to disable swap)
    pub memory_swap_bytes: Option<i64>,
    /// Maximum number of PIDs
    pub pids_limit: Option<i64>,
    /// CPU quota (microseconds per period)
    pub cpu_quota: Option<i64>,
    /// CPU period (microseconds)
    pub cpu_period: Option<i64>,
    /// Startup timeout
    pub startup_timeout: Duration,
}

impl ContainerConfig {
    /// Create a new container configuration with sensible defaults
    pub fn new() -> Self {
        Self {
            startup_timeout: Duration::from_secs(30),
            ..Default::default()
        }
    }

    /// Set memory limit (also disables swap by default)
    pub fn with_memory(mut self, bytes: i64) -> Self {
        self.memory_bytes = Some(bytes);
        self.memory_swap_bytes = Some(bytes); // Disable swap by default
        self
    }

    /// Set PID limit
    pub fn with_pids_limit(mut self, limit: i64) -> Self {
        self.pids_limit = Some(limit);
        self
    }

    /// Set CPU quota (in microseconds per period)
    #[allow(dead_code)]
    pub fn with_cpu_quota(mut self, quota: i64, period: i64) -> Self {
        self.cpu_quota = Some(quota);
        self.cpu_period = Some(period);
        self
    }

    /// Set startup timeout
    #[allow(dead_code)]
    pub fn with_startup_timeout(mut self, timeout: Duration) -> Self {
        self.startup_timeout = timeout;
        self
    }

    /// Convert this config into a host config modifier function
    pub fn into_host_config_modifier(self) -> impl Fn(&mut HostConfig) + Send + Sync + 'static {
        move |hc: &mut HostConfig| {
            if let Some(mem) = self.memory_bytes {
                hc.memory = Some(mem);
            }
            if let Some(swap) = self.memory_swap_bytes {
                hc.memory_swap = Some(swap);
            }
            if let Some(pids) = self.pids_limit {
                hc.pids_limit = Some(pids);
            }
            if let Some(quota) = self.cpu_quota {
                hc.cpu_quota = Some(quota);
            }
            if let Some(period) = self.cpu_period {
                hc.cpu_period = Some(period);
            }
        }
    }
}

// ============================================================================
// Build
// ============================================================================

/// Ensure the binary is built (runs a one-shot build container if needed)
///
/// The build container runs `cargo build --release` directly and exits when done.
/// Build artifacts are cached in `.testcontainers/target/` for subsequent runs.
pub async fn ensure_binary_built() {
    BUILD_COMPLETE
        .get_or_init(|| async {
            ensure_cache_dirs().expect("Failed to create cache directories");

            let project = project_dir();
            let cache = test_cache_dir();

            println!("Starting build container...");
            println!("  Project dir: {:?}", project);
            println!("  Cache dir: {:?}", cache);

            // Create build container that runs cargo build directly
            let container = GenericImage::new(RUST_IMAGE, RUST_TAG)
                .with_mount(
                    Mount::bind_mount(project.to_string_lossy(), "/app")
                        .with_access_mode(AccessMode::ReadOnly),
                )
                .with_mount(Mount::bind_mount(
                    cache.join("registry").to_string_lossy(),
                    "/usr/local/cargo/registry",
                ))
                .with_mount(Mount::bind_mount(
                    cache.join("git").to_string_lossy(),
                    "/usr/local/cargo/git",
                ))
                .with_mount(Mount::bind_mount(
                    cache.join("target").to_string_lossy(),
                    "/build/target",
                ))
                .with_env_var("CARGO_TARGET_DIR", "/build/target")
                .with_env_var("RUST_BACKTRACE", "1")
                .with_working_dir("/app")
                .with_cmd(["cargo", "build", "--release"])
                .with_startup_timeout(Duration::from_secs(300)) // 5 min for build
                .start()
                .await
                .expect("Failed to start build container");

            println!("Building binary (this may take a while on first run)...");

            // Wait for container to exit and get output
            let (stdout, stderr, exit_code) = wait_and_get_output(&container).await;

            if !stdout.is_empty() {
                print!("{}", stdout);
            }
            if !stderr.is_empty() {
                eprint!("{}", stderr);
            }

            if exit_code != 0 {
                panic!("Build failed with exit code: {}", exit_code);
            }

            println!("Build complete!");
            // Container is automatically removed on drop
        })
        .await;
}

// ============================================================================
// Command Execution
// ============================================================================

/// Run a command in a container with the given configuration
///
/// Creates a container that runs the command directly (not via exec),
/// waits for it to complete, and returns stdout/stderr.
pub async fn run_in_container(config: ContainerConfig, cmd: &[&str]) -> (String, String, i64) {
    let cache = test_cache_dir();
    let modifier = config.into_host_config_modifier();

    let cmd_strings: Vec<String> = cmd.iter().map(|s| s.to_string()).collect();

    let mut image = GenericImage::new(RUST_IMAGE, RUST_TAG)
        .with_cmd(cmd_strings)
        .with_startup_timeout(Duration::from_secs(60));

    // Mount build target (read-only)
    image = image.with_mount(
        Mount::bind_mount(cache.join("target").to_string_lossy(), "/build/target")
            .with_access_mode(AccessMode::ReadOnly),
    );

    // Apply resource constraints
    image = image.with_host_config_modifier(modifier);

    let container = image.start().await.expect("Failed to start container");

    wait_and_get_output(&container).await
}

/// Wait for a container to exit and collect its output
async fn wait_and_get_output(container: &ContainerAsync<GenericImage>) -> (String, String, i64) {
    let docker = Docker::connect_with_local_defaults().expect("Failed to connect to Docker");
    let container_id = container.id();

    // Wait for the container to exit
    #[allow(deprecated)]
    let wait_options = WaitContainerOptions {
        condition: "not-running",
    };

    let mut wait_stream = docker.wait_container(container_id, Some(wait_options));
    let exit_code = match wait_stream.next().await {
        Some(Ok(response)) => response.status_code,
        Some(Err(e)) => {
            eprintln!("Error waiting for container: {}", e);
            1
        }
        None => 1,
    };

    // Get container logs
    #[allow(deprecated)]
    let log_options = LogsOptions::<String> {
        stdout: true,
        stderr: true,
        ..Default::default()
    };

    let mut logs_stream = docker.logs(container_id, Some(log_options));
    let mut stdout = String::new();
    let mut stderr = String::new();

    while let Some(result) = logs_stream.next().await {
        match result {
            Ok(output) => match output {
                bollard::container::LogOutput::StdOut { message } => {
                    stdout.push_str(&String::from_utf8_lossy(&message));
                }
                bollard::container::LogOutput::StdErr { message } => {
                    stderr.push_str(&String::from_utf8_lossy(&message));
                }
                _ => {}
            },
            Err(e) => {
                eprintln!("Error reading logs: {}", e);
                break;
            }
        }
    }

    (stdout, stderr, exit_code)
}
