use std::time::{Duration};
use duration_str::parse as parse_duration;
use structopt::{StructOpt};
use crate::allocator::{parse_size, AllocationMode, Size};

mod sys;
mod mem_info;
mod allocator;
#[cfg(unix)]
mod linux_main;
#[cfg(windows)]
mod windows_main;

#[derive(StructOpt, Debug)]
#[structopt(name = "memfill", about = "Fills memory")]
struct Opt {
	#[structopt(help = "Size of memory to fill up; suffixes: K, M, G or %", parse(
		try_from_str = parse_size
	))]
	size: Size,

	#[structopt(help = "Allocation mode; [absolute, usage]")]
	alloc_mode: AllocationMode,

	#[structopt(help = "Duration; suffixes: s, m, h, d", parse(try_from_str = parse_duration))]
	duration: Duration,

	#[structopt(long, help = "ignore cgroup; computes total/usage from system information")]
	#[cfg(unix)]
	ignore_cgroup: bool,
}

fn main() {
	let mut args = std::env::args();
	args.next();

	if let Some(flag) = args.next(){
		if flag == "--allocate" {
			if let Some(size_str) = args.next(){
				let size: usize = size_str.parse().expect("Invalid size");
                #[cfg(windows)]
                return windows_main::allocate_mode(size);
                #[cfg(unix)]
                unreachable!();
			}
		}
	}

	let opts = Opt::from_args();

	#[cfg(unix)]
	return linux_main::linux_main(opts);

	#[cfg(windows)]
	return windows_main::windows_main(opts);
}

