use std::time::{Duration};
use duration_str::parse as parse_duration;
use structopt::{StructOpt};

use crate::allocator::{parse_size, AllocationMode, Size};

mod sys;
mod mem_info;
#[cfg(unix)]
mod linux_main;
mod allocator;

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
	let opts = Opt::from_args();

	#[cfg(unix)]
	linux_main::linux_main(opts);

	#[cfg(windows)]
	windows_main();
}

