mod cgroup;

use std::{fs, process, str};
use std::alloc::{alloc, dealloc, Layout};
use std::ptr::{copy, write_bytes};
use std::thread::sleep;
use std::time::{Duration, Instant};

use bytesize::ByteSize;
use duration_str::parse as parse_duration;
use nix::errno::Errno;
use nix::sys::{prctl, signal};
use nix::sys::signal::{SIGCONT, Signal};
use nix::sys::signal::{SaFlags, sigaction, SigAction, SigHandler, SigSet};
use nix::sys::wait::{waitpid, WaitPidFlag, WaitStatus};
use nix::{libc, unistd};
use nix::unistd::{fork, Pid, Uid};
use nix::unistd::ForkResult;
use procfs::{Current, Meminfo};
use procfs::process::{Process, ProcState};
use structopt::{StructOpt};
use strum_macros::EnumString;

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
	ignore_cgroup: bool,
}

#[derive(EnumString, Debug)]
enum AllocationMode {
	#[strum(serialize = "absolute")]
	Absolute,
	#[strum(serialize = "usage")]
	Usage,
}

#[derive(Debug)]
enum Size {
	Bytes(u64),
	Percent(u16),
}

fn parse_size(input: impl AsRef<str>) -> Result<Size, String> {
	let input = input.as_ref();
	if input.ends_with('%') {
		let percent = input.trim_end_matches('%').parse().map_err(|e| format!("{}", e))?;
		Ok(Size::Percent(percent))
	} else {
		let byte: ByteSize = input.parse().map_err(|e| format!("{}", e))?;
		Ok(Size::Bytes(byte.as_u64()))
	}
}

#[derive(Debug)]
struct MemInfo {
	available: usize,
	total: usize,
}

trait MemInfoProvider {
	fn mem_info(&self) -> MemInfo;
}

struct SystemMemInfo {}

impl MemInfoProvider for SystemMemInfo {
	fn mem_info(&self) -> MemInfo {
		let mem = Meminfo::current().unwrap();
		return MemInfo { available: mem.mem_available.unwrap() as usize, total: mem.mem_total as usize };
	}
}

struct CgroupMemInfo {}

impl MemInfoProvider for CgroupMemInfo {
	fn mem_info(&self) -> MemInfo {
		let mem_cgroup = cgroup::read_cgroup_memory().unwrap();
		let mut total = mem_cgroup.limit;
		if mem_cgroup.unlimited {
			total = Meminfo::current().unwrap().mem_total as usize;
		}
		let available = total - mem_cgroup.usage;

		return MemInfo { available, total };
	}
}

trait Allocator {
	fn update(&mut self);
}

fn new_allocator(mode: AllocationMode, mem_info_provider: Box<dyn MemInfoProvider>, size: Size) -> Box<dyn Allocator> {
	match mode {
		AllocationMode::Absolute => { Box::new(AbsoluteAllocator::new(mem_info_provider, size)) }
		AllocationMode::Usage => { Box::new(UsageAllocator::new(mem_info_provider, size)) }
	}
}

struct AbsoluteAllocator {
	bytes: usize,
	chunks: Chunks,
}

impl AbsoluteAllocator {
	fn new(provider: Box<dyn MemInfoProvider>, size: Size) -> Self {
		let mem = provider.mem_info();
		let (bytes, percent) = match size {
			Size::Bytes(bytes) => {
				let percent = (bytes as f64 / mem.total as f64 * 100.0).round() as u16;
				(bytes, percent)
			}
			Size::Percent(percent) => {
				let bytes = (mem.total as f64 * percent as f64 / 100.0) as u64;
				(bytes, percent)
			}
		};
		println!("Allocating {} ({}% of total memory)", ByteSize::b(bytes).to_string_as(true), percent);
		return Self { bytes: bytes as usize, chunks: Chunks::new() };
	}
}

impl Allocator for AbsoluteAllocator {
	fn update(&mut self) {
		self.chunks.check();
		self.chunks.resize(self.bytes)
	}
}

struct UsageAllocator {
	available_bytes: i64,
	chunks: Chunks,
	provider: Box<dyn MemInfoProvider>,
}

impl UsageAllocator {
	fn new(provider: Box<dyn MemInfoProvider>, size: Size) -> Self {
		let mem = provider.mem_info();
		let (available_bytes, available_percent) = match size {
			Size::Bytes(bytes) => {
				let available_bytes = mem.total as i64 - bytes as i64;
				let available_percent = (available_bytes as f64 / mem.total as f64 * 100.0).round() as i16;
				(available_bytes, available_percent)
			}
			Size::Percent(percent) => {
				let available_bytes = mem.total as i64 - (mem.total as f64 * percent as f64 / 100.0) as i64;
				let available_percent = (available_bytes as f64 / mem.total as f64 * 100.0).round() as i16;
				(available_bytes, available_percent)
			}
		};
		println!("Allocate until {}{} ({}% of total memory) available left", if available_bytes < 0 { "-" } else { "" }, ByteSize::b(available_bytes.unsigned_abs()).to_string_as(true), available_percent);
		return Self { available_bytes, chunks: Chunks::new(), provider };
	}
}

impl Allocator for UsageAllocator {
	fn update(&mut self) {
		let mem = self.provider.mem_info();
		let diff = mem.available as i64 - self.available_bytes;
		self.chunks.check();
		self.chunks.adjust_by(diff)
	}
}

struct Chunks {
	chunks: Vec<Chunk>,
	last_allocation: Instant,
}

const MB: i64 = 1024 * 1024;

impl Chunks {
	fn new() -> Self {
		return Self { chunks: vec![], last_allocation: Instant::now() };
	}

	fn size(&mut self) -> usize {
		self.chunks.iter().map(|c| { c.size() }).sum()
	}

	fn check(&mut self) {
		self.chunks.iter_mut().for_each(|c| { c.check() });
	}

	fn resize(&mut self, size: usize) {
		let diff = size as i64 - self.size() as i64;
		self.adjust_by(diff)
	}

	fn adjust_by(&mut self, size: i64) {
		let now = Instant::now();
		if now - self.last_allocation < Duration::from_secs(1) && size < 2 * MB {
			return
		}
		self.last_allocation = now;

		let mut freed = 0;
		while freed < -size {
			match self.chunks.pop() {
				None => { break; }
				Some(mut c) => {
					freed += c.free() as i64
				}
			}
		}
		let allocate = freed + size;
		if allocate > 0 {
			let count = if allocate < (16 * MB) { 1 } else { 2.max(allocate / (1024 * MB)) };
			for _i in 0..count {
				self.chunks.push(Chunk::new((allocate / count) as usize))
			}
		}
	}
}

struct Chunk {
	size: usize,
	pid: Pid,
}

extern "C" fn handle(_: libc::c_int, _: *mut libc::siginfo_t, _: *mut libc::c_void) {
	//NOOP
}

const PID_ZERO: Pid = Pid::from_raw(0);

impl Chunk {
	fn new(size: usize) -> Self {
		if size <= 0 {
			return Self { size, pid: PID_ZERO };
		}

		match unsafe { fork() } {
			Ok(ForkResult::Child) => unsafe {
				prctl::set_pdeathsig(Some(Signal::SIGTERM)).unwrap();
				sigaction(SIGCONT, &SigAction::new(SigHandler::SigAction(handle), SaFlags::empty(), SigSet::empty())).unwrap();

				let layout = Layout::array::<u8>(size).unwrap();
				let ptr = alloc(layout);
				if ptr.is_null() {
					panic!("[{}] Failed to allocate memory", process::id());
				}

				let rand = rand::random();
				write_bytes(ptr, rand, size);
				let mut res = 0u8;
				copy(ptr, &mut res, 1);
				if res != rand {
					panic!("[{}] Memory pattern assertion failed", process::id());
				}
				println!("[{}] Allocated {}", process::id(), ByteSize::b(size as u64).to_string_as(true));

				unistd::pause();

				dealloc(ptr, layout);
				process::exit(0);
			}
			Ok(ForkResult::Parent { child, .. }) => {
				match Process::new(child.into()) {
					Ok(child_process) => {
						loop {
							match child_process.stat().and_then(|s| s.state()) {
								Ok(ProcState::Sleeping) => { break; }
								Ok(_) => { sleep(Duration::from_millis(100)); }
								Err(e) => { eprintln!("Failed to read state for new child: {}", e); }
							}
						}
						Self { size, pid: child }
					}
					Err(e) => {
						eprintln!("Failed to find new child: {}", e);
						Self { size: 0, pid: PID_ZERO }
					}
				}
			}
			Err(e) => {
				eprintln!("Fork failed: {}", e);
				Self { size: 0, pid: PID_ZERO }
			}
		}
	}

	fn size(&self) -> usize {
		if self.pid == PID_ZERO {
			0
		} else {
			self.size
		}
	}

	fn check(&mut self) {
		self.wait(Some(WaitPidFlag::WNOHANG));
	}

	fn wait(&mut self, option: Option<WaitPidFlag>) {
		match waitpid(self.pid, option) {
			Ok(WaitStatus::Exited(pid, code)) => {
				println!("[{}] Exited({}) and de-allocated {}", pid, code, ByteSize::b(self.size as u64).to_string_as(true));
				self.pid = PID_ZERO;
			}
			Ok(WaitStatus::Signaled(pid, signal, _)) => {
				println!("[{}] Killed by {} and de-allocated {}", pid, signal, ByteSize::b(self.size as u64).to_string_as(true));
				self.pid = PID_ZERO;
			}
			Ok(_) => {}
			Err(Errno::ECHILD) => { self.pid = PID_ZERO; }
			Err(e) => {
				println!("[{}] errno: {} ", self.pid, e);
				self.pid = PID_ZERO;
			}
		}
	}

	fn free(&mut self) -> usize {
		if self.pid != PID_ZERO {
			signal::kill(self.pid, SIGCONT).unwrap();
			self.wait(None);
			self.size
		} else {
			0
		}
	}
}

fn main() {
	let opts = Opt::from_args();
	adjust_oom_score();

	let mem_info: Box<dyn MemInfoProvider> = if opts.ignore_cgroup {
		Box::new(SystemMemInfo {})
	} else {
		Box::new(CgroupMemInfo {})
	};

	let mut allocator = new_allocator(opts.alloc_mode, mem_info, opts.size);
	println!("Terminating after {}s", opts.duration.as_secs());
	let deadline = Instant::now() + opts.duration;
	while Instant::now() < deadline {
		allocator.update();
		sleep(Duration::from_millis(1000));
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
