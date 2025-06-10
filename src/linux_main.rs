use std::{fs, thread::sleep};
use std::time::Duration;
use std::time::Instant;
use nix::unistd::Uid;
use crate::allocator::new_allocator;
use crate::mem_info::{bytes_to_string_usize, MemInfoProvider};
use crate::sys::linux::mem_info::{CgroupMemInfo, SystemMemInfo};
use crate::Opt;

pub fn linux_main(opts: Opt){
    adjust_oom_score();

	let mem_info: Box<dyn MemInfoProvider> = if opts.ignore_cgroup {
		Box::new(SystemMemInfo {})
	} else {
		Box::new(CgroupMemInfo {})
	};

	let mut allocator = new_allocator(opts.alloc_mode, mem_info.as_ref(), opts.size);
	println!("Terminating after {}s", opts.duration.as_secs());
	let deadline = Instant::now() + opts.duration;
	let mut last_log = Instant::now() - Duration::from_secs(5);
	while Instant::now() < deadline {
		allocator.update();

		let now = Instant::now();
		if now - last_log > Duration::from_secs(5) {
			let mem = mem_info.mem_info();
			print!("Available memory: {} ({}% of total memory); ", bytes_to_string_usize(mem.available), (mem.available as f64 / mem.total as f64 * 100.0).round() as i16);
			print!("Allocated by memfill: {} ({}% of total memory)", bytes_to_string_usize(allocator.size()), (allocator.size() as f64 / mem.total as f64 * 100.0).round() as i16);
			println!();
			last_log = now;
		}

		sleep(Duration::from_millis(50));
	}
}


fn adjust_oom_score() -> () {
	let is_privileged = Uid::current().is_root() || Uid::effective().is_root();
	match fs::write("/proc/self/oom_score_adj", if is_privileged { "-1000" } else { "0" }) {
		Ok(_) => {}
		Err(e) => {
			eprintln!("Failed to adjust OOM score: {}", e);
		}
	}
}