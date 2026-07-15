use crate::allocator::{new_allocator, parse_size, size_to_bytes, AllocationMode, Size};
use crate::mem_info::bytes_to_string_usize;
#[cfg(target_os = "linux")]
use crate::sys::linux::psi::memory_full_avg10;
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

// Adaptive back-off tuning (Linux/PSI only). Not exposed on the CLI on purpose.
#[cfg(target_os = "linux")]
const ADAPTIVE_INTERVAL_SECS: u64 = 1;
#[cfg(target_os = "linux")]
const ADAPTIVE_STEP_BYTES: i64 = 256 * 1024 * 1024;
#[cfg(target_os = "linux")]
const ADAPTIVE_RELAX_BYTES: i64 = 64 * 1024 * 1024;
/// "full avg10" memory pressure (%) above which we free memory to keep the host alive.
#[cfg(target_os = "linux")]
const PSI_HIGH_PCT: f64 = 10.0;
/// "full avg10" below which we let the adaptive reserve relax again.
#[cfg(target_os = "linux")]
const PSI_LOW_PCT: f64 = 3.0;

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

    #[structopt(
        long,
        help = "Minimum memory to keep available (reserve); suffixes: K, M, G or %",
        parse(try_from_str = parse_size)
    )]
    reserve: Option<Size>,

    #[structopt(
        long,
        allow_hyphen_values = true,
        help = "oom_score_adj to set on the fill process (-1000..1000). Defaults to -1000 when privileged, 0 otherwise."
    )]
    #[cfg(target_os = "linux")]
    oom_score_adj: Option<i32>,

    #[structopt(
        long,
        help = "Adaptively free memory when the host is under memory pressure (PSI), keeping the host responsive. Requires usage mode."
    )]
    #[cfg(target_os = "linux")]
    adaptive: bool,
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

    #[cfg(target_os = "linux")]
    if opts.adaptive && matches!(opts.alloc_mode, AllocationMode::Absolute) {
        eprintln!("--adaptive is only supported in usage mode");
        std::process::exit(2);
    }

    let mem_info = get_mem_info(&opts);

    let total = mem_info.mem_info().total;
    let reserve_bytes = opts.reserve.as_ref().map(|s| size_to_bytes(s, total));

    #[cfg(target_os = "linux")]
    adjust_oom_score(opts.oom_score_adj);

    let mut allocator = new_allocator(opts.alloc_mode, mem_info.as_ref(), opts.size, reserve_bytes);
    println!("Terminating after {}s", opts.duration.as_secs());
    let deadline = Instant::now() + opts.duration;
    let mut last_log = Instant::now() - Duration::from_secs(5);
    #[cfg(target_os = "linux")]
    let mut last_adaptive = Instant::now() - Duration::from_secs(ADAPTIVE_INTERVAL_SECS);
    while Instant::now() < deadline {
        allocator.update();
        let now = Instant::now();

        #[cfg(target_os = "linux")]
        if opts.adaptive
            && now.duration_since(last_adaptive) >= Duration::from_secs(ADAPTIVE_INTERVAL_SECS)
        {
            last_adaptive = now;
            if let Some(full10) = memory_full_avg10() {
                if full10 > PSI_HIGH_PCT {
                    allocator.bump_reserve(ADAPTIVE_STEP_BYTES);
                    println!(
                        "Memory pressure high (full avg10={:.1}%); backing off to keep the host responsive",
                        full10
                    );
                } else if full10 < PSI_LOW_PCT {
                    allocator.relax_reserve(ADAPTIVE_RELAX_BYTES);
                }
            }
        }

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
