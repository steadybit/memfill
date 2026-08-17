pub mod allocator;
pub mod cgroup;
pub mod mem_info;
pub mod psi;
pub mod system;

/// Attaches this process to a target's memory cgroup and PID namespace.
///
/// Both steps are ordering-sensitive and must happen here, before anything
/// else runs: the cgroup join has to precede any memory introspection (or the
/// fill is sized against the caller's own limits instead of the target's), and
/// the PID-namespace entry has to precede the first chunk fork (setns only
/// affects children forked afterward). Keeping them in one function keeps that
/// invariant in a single place rather than spread across statements in main().
pub fn attach_to_target(cgroup_path: Option<&str>, pid: Option<i32>) -> Result<(), String> {
    if let Some(path) = cgroup_path {
        // Debug, not Display: CGroupError's derived Display prints only the
        // variant name, dropping the path and underlying errno.
        cgroup::join_cgroup(path)
            .map_err(|e| format!("failed to join target cgroup {path}: {e:?}"))?;
    }
    if let Some(pid) = pid {
        system::enter_pid_namespace(pid)
            .map_err(|e| format!("failed to enter PID namespace of pid {pid}: {e}"))?;
    }
    Ok(())
}
