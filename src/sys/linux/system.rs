use nix::sched::{setns, CloneFlags};
use nix::unistd::Uid;
use std::fs::{self, File};

/// Adjusts the oom_score_adj of the current process.
///
/// When `score` is provided it is written verbatim (clamped to the valid
/// -1000..=1000 range). When it is `None` the historical default is kept:
/// -1000 when running privileged (so the fill survives), 0 otherwise.
///
/// Callers filling *host* memory should pass a high value so that the kernel
/// OOM killer targets the fill process first, instead of node-critical
/// processes such as the kubelet.
pub fn adjust_oom_score(score: Option<i32>) {
    let value = match score {
        Some(v) => v.clamp(-1000, 1000),
        None => {
            let is_privileged = Uid::current().is_root() || Uid::effective().is_root();
            if is_privileged {
                -1000
            } else {
                0
            }
        }
    };
    match fs::write("/proc/self/oom_score_adj", value.to_string()) {
        Ok(_) => {}
        Err(e) => {
            eprintln!("Failed to adjust OOM score: {}", e);
        }
    }
}

/// Sets pid_for_children to `pid`'s PID namespace via `setns(CLONE_NEWPID)`.
///
/// Per pid_namespaces(7)/setns(2), this does NOT move the calling process
/// itself into that namespace — it only affects children forked *after* this
/// call. That's exactly what's needed here: it's the per-chunk allocation
/// processes (forked later, one per chunk) that must land in the target's
/// PID namespace, not memfill itself.
pub fn enter_pid_namespace(pid: i32) -> std::io::Result<()> {
    let file = File::open(format!("/proc/{}/ns/pid", pid))?;
    Ok(setns(file, CloneFlags::CLONE_NEWPID)?)
}
