use nix::unistd::Uid;
use std::fs;

pub fn adjust_oom_score() -> () {
	let is_privileged = Uid::current().is_root() || Uid::effective().is_root();
	match fs::write("/proc/self/oom_score_adj", if is_privileged { "-1000" } else { "0" }) {
		Ok(_) => {}
		Err(e) => {
			eprintln!("Failed to adjust OOM score: {}", e);
		}
	}
}