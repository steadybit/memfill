use crate::allocator::{new_allocator, parse_size, AllocationMode, Size};
use crate::mem_info::bytes_to_string_usize;
#[cfg(target_os = "linux")]
use crate::sys::linux::system::adjust_oom_score;
use crate::sys::platform::get_mem_info;
use duration_str::parse as parse_duration;
use std::thread::sleep;
use std::time::Duration;
use std::time::Instant;
use structopt::StructOpt;

#[cfg(windows)]
use crate::sys::windows::system::allocate_mode;

mod allocator;
mod mem_info;
mod sys;

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

    #[structopt(
        long,
        help = "ignore cgroup; computes total/usage from system information"
    )]
    #[cfg(target_os = "linux")]
    ignore_cgroup: bool,
}

fn main() {
    let mut args = std::env::args();
    args.next();

    if let Some(flag) = args.next() {
        if flag == "--allocate" {
            if let Some(size_str) = args.next() {
                let _size: usize = size_str.parse().expect("Invalid size");
                #[cfg(windows)]
                return allocate_mode(_size);
                #[cfg(not(windows))]
                unreachable!();
            }
        }
    }

    let opts = Opt::from_args();
    let mem_info = get_mem_info(&opts);

    #[cfg(target_os = "linux")]
    adjust_oom_score();

    let mut allocator = new_allocator(opts.alloc_mode, mem_info.as_ref(), opts.size);
    println!("Terminating after {}s", opts.duration.as_secs());
    let deadline = Instant::now() + opts.duration;
    let mut last_log = Instant::now() - Duration::from_secs(5);
    while Instant::now() < deadline {
        allocator.update();

        let now = Instant::now();
        if now - last_log > Duration::from_secs(5) {
            let mem = mem_info.mem_info();
            print!(
                "Available memory: {} ({}% of total memory); ",
                bytes_to_string_usize(mem.available),
                (mem.available as f64 / mem.total as f64 * 100.0).round() as i16
            );
            print!(
                "Allocated by memfill: {} ({}% of total memory)",
                bytes_to_string_usize(allocator.size()),
                (allocator.size() as f64 / mem.total as f64 * 100.0).round() as i16
            );
            println!();
            last_log = now;
        }

        sleep(Duration::from_millis(50));
    }
    allocator.free();
}
