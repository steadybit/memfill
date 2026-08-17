use nix::unistd;
use std::fs::File;
use std::io::BufRead;
use std::path::{Path, PathBuf};
use std::{fs, io, num};
use strum_macros::Display;

#[derive(Debug, Display)]
pub enum CGroupError {
    File(PathBuf, io::Error),
    Parse(PathBuf, num::ParseIntError),
    CgroupControllerNotFound(),
    /// No cgroup.procs exists for the requested path under either the v1
    /// memory controller or the v2 unified hierarchy.
    CgroupProcsNotFound(String),
}

pub struct CGroupMemory {
    pub usage: usize,
    pub limit: usize,
    pub unlimited: bool,
}

pub fn read_cgroup_memory() -> Result<CGroupMemory, CGroupError> {
    if uses_cgroup_v2() {
        read_cgroup_v2_memory()
    } else {
        read_cgroup_v1_memory()
    }
}

fn uses_cgroup_v2() -> bool {
    Path::new("/sys/fs/cgroup/cgroup.controllers").exists()
}

/// Joins the cgroup at `path` (a plain path fragment, e.g.
/// "/kubepods/besteffort/pod123/container456", no controller prefix) by
/// writing "0" to its `cgroup.procs` (kernel shorthand for "the writing
/// process"). Must be called before any introspection (`read_cgroup_memory`)
/// or forking, and while the caller's mount namespace still sees the host's
/// /sys/fs/cgroup (i.e. after the outer `nsenter -t 1 -C` has already run).
///
/// The layout is probed rather than inferred from `uses_cgroup_v2()`: the
/// caller resolved `path` from /proc/<pid>/cgroup and prefers the v1 line
/// when both are present, so the root that actually holds this path is the
/// one to trust. v1's memory controller is tried first to match that
/// preference, with the v2 unified hierarchy as fallback.
pub fn join_cgroup(path: &str) -> Result<(), CGroupError> {
    let stripped = path.strip_prefix('/').unwrap_or(path);
    let procs_path = ["/sys/fs/cgroup/memory", "/sys/fs/cgroup"]
        .into_iter()
        .map(|root| Path::new(root).join(stripped).join("cgroup.procs"))
        .find(|p| p.exists())
        .ok_or_else(|| CGroupError::CgroupProcsNotFound(path.to_string()))?;
    fs::write(&procs_path, "0").map_err(|e| CGroupError::File(procs_path, e))
}

fn read_cgroup_v2_memory() -> Result<CGroupMemory, CGroupError> {
    let mut controller_path = Path::new("/sys/fs/cgroup")
        .join(read_cgroupv2_controller()?.strip_prefix("/").unwrap_or(""));

    loop {
        let mem_max = controller_path.join("memory.max");
        let mem_current = controller_path.join("memory.current");

        if mem_max.exists() && mem_current.exists() {
            let (limit, unlimited) = read_file_usize(mem_max)?;
            let (usage, _) = read_file_usize(mem_current)?;

            return Ok(CGroupMemory {
                usage,
                limit,
                unlimited,
            });
        }

        match controller_path.parent() {
            Some(p) => {
                controller_path = p.to_owned();
            }
            None => return Err(CGroupError::CgroupControllerNotFound()),
        }
    }
}

fn read_cgroupv2_controller() -> Result<String, CGroupError> {
    let path = PathBuf::from("/proc/self/cgroup");
    let file = File::open(path.as_path()).map_err(|e| CGroupError::File(path, e))?;
    let lines = io::BufReader::new(file).lines();

    for line in lines.flatten() {
        let parts: Vec<&str> = line.splitn(3, ":").collect();
        if parts[0] == "0" {
            return Ok(parts[2].to_string());
        }
    }
    Err(CGroupError::CgroupControllerNotFound())
}

fn read_cgroup_v1_memory() -> Result<CGroupMemory, CGroupError> {
    let controller_path = Path::new("/sys/fs/cgroup/memory")
        .join(read_cgroupv1_controller()?.strip_prefix("/").unwrap_or(""));

    let (usage, _) = read_file_usize(controller_path.join("memory.usage_in_bytes"))?;
    let (limit, _) = read_file_usize(controller_path.join("memory.limit_in_bytes"))?;

    Ok(CGroupMemory {
        usage,
        limit,
        unlimited: limit == cgroup_v1_mem_unlimited(),
    })
}

fn read_cgroupv1_controller() -> Result<String, CGroupError> {
    let path = PathBuf::from("/proc/self/cgroup");
    let file = File::open(path.as_path()).map_err(|e| CGroupError::File(path, e))?;
    let lines = io::BufReader::new(file).lines();

    for line in lines.flatten() {
        let parts: Vec<&str> = line.splitn(3, ":").collect();
        if parts[1] == "memory" {
            return Ok(parts[2].to_string());
        }
    }
    Err(CGroupError::CgroupControllerNotFound())
}

fn cgroup_v1_mem_unlimited() -> usize {
    if let Ok(Some(ps)) = unistd::sysconf(unistd::SysconfVar::PAGE_SIZE) {
        return ((i64::MAX / ps) * ps) as usize;
    }
    0
}

fn read_file_usize<P: AsRef<Path>>(path: P) -> Result<(usize, bool), CGroupError> {
    let line = fs::read_to_string(path.as_ref())
        .map_err(|e| CGroupError::File(PathBuf::from(path.as_ref()), e))?;
    if line.trim() == "max" {
        Ok((0, true))
    } else {
        let i = line
            .trim()
            .parse()
            .map_err(|e| CGroupError::Parse(PathBuf::from(path.as_ref()), e))?;
        Ok((i, false))
    }
}
