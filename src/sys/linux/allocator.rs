use std::{process};
use std::alloc::{alloc, dealloc, Layout};
use std::os::fd::{AsFd, AsRawFd};
use std::ptr::{copy, write_bytes};

use nix::errno::Errno;
use nix::sys::{prctl, signal};
use nix::sys::signal::{SIGCONT, Signal};
use nix::sys::signal::{SaFlags, sigaction, SigAction, SigHandler, SigSet};
use nix::sys::wait::{waitpid, WaitPidFlag, WaitStatus};
use nix::{libc, unistd};
use nix::unistd::{fork, Pid};
use nix::unistd::ForkResult;

use crate::allocator::{Allocator, Chunks, Size};
use crate::mem_info::{bytes_to_string_i64, bytes_to_string_usize, MemInfoProvider};


pub struct LinuxAbsoluteAllocator {
	bytes: usize,
	chunks: Chunks,
}

impl LinuxAbsoluteAllocator {
	pub fn new(provider: &dyn MemInfoProvider, size: Size) -> Self {
		let mem = provider.mem_info();
		let (bytes, percent) = match size {
			Size::Bytes(bytes) => {
				let percent = (bytes as f64 / mem.total as f64 * 100.0).round() as u16;
				(bytes, percent)
			}
			Size::Percent(percent) => {
				let bytes = (mem.total as f64 * percent as f64 / 100.0) as usize;
				(bytes, percent)
			}
		};
		println!("Allocating {} ({}% of total memory)", bytes_to_string_usize(bytes), percent);
		return Self { bytes, chunks: Chunks::new() };
	}
}

impl Allocator for LinuxAbsoluteAllocator {
	fn update(&mut self) {
		self.chunks.check();
		self.chunks.resize(self.bytes)
	}

	fn size(&self) -> usize {
		self.chunks.size()
	}
}

pub struct LinuxUsageAllocator<'a> {
	available_bytes: i64,
	chunks: Chunks,
	provider: Box<&'a dyn MemInfoProvider>,
}

impl<'a> LinuxUsageAllocator<'a> {
	pub fn new(provider: &'a dyn MemInfoProvider, size: Size) -> Self {
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
		println!("Allocate until {} ({}% of total memory) available left", bytes_to_string_i64(available_bytes), available_percent);
		return Self { available_bytes, chunks: Chunks::new(), provider: Box::new(provider) };
	}
}

impl Allocator for LinuxUsageAllocator<'_> {
	fn update(&mut self) {
		let mem = self.provider.mem_info();
		let diff = mem.available as i64 - self.available_bytes;
		self.chunks.check();
		self.chunks.adjust_by(diff)
	}
	fn size(&self) -> usize {
		self.chunks.size()
	}
}


pub struct LinuxChunk {
	size: usize,
	pid: Pid,
}

extern "C" fn sig_cont(_: libc::c_int, _: *mut libc::siginfo_t, _: *mut libc::c_void) {
	//NOOP
}

const PID_ZERO: Pid = Pid::from_raw(0);

impl LinuxChunk {
	pub fn new(size: usize) -> Self {
		if size <= 0 {
			return Self { size, pid: PID_ZERO };
		}

		let pipe = unistd::pipe().unwrap();

		match unsafe { fork() } {
			Ok(ForkResult::Child) => unsafe {
				prctl::set_pdeathsig(Some(Signal::SIGTERM)).unwrap();
				sigaction(SIGCONT, &SigAction::new(SigHandler::SigAction(sig_cont), SaFlags::empty(), SigSet::empty())).unwrap();

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
				println!("[{}] Allocated {}", process::id(), bytes_to_string_usize(size));

				unistd::write(pipe.1.as_fd(), "0".as_bytes()).unwrap();
				unistd::pause();

				dealloc(ptr, layout);

				process::exit(0);
			}

			Ok(ForkResult::Parent { child, .. }) => {
				let buf =&mut [0u8];
				unistd::read(pipe.0.as_fd().as_raw_fd(), buf).unwrap();
				Self { size, pid: child }
			}

			Err(e) => {
				eprintln!("Fork failed: {}", e);
				Self { size: 0, pid: PID_ZERO }
			}
		}
	}

	pub fn size(&self) -> usize {
		if self.pid == PID_ZERO {
			0
		} else {
			self.size
		}
	}

	pub fn check(&mut self) {
		self.wait(Some(WaitPidFlag::WNOHANG));
	}

	pub fn wait(&mut self, option: Option<WaitPidFlag>) {
		match waitpid(self.pid, option) {
			Ok(WaitStatus::Exited(pid, code)) => {
				println!("[{}] Exited({}) and de-allocated {}", pid, code, bytes_to_string_usize(self.size));
				self.pid = PID_ZERO;
			}
			Ok(WaitStatus::Signaled(pid, signal, _)) => {
				println!("[{}] Killed by {} and de-allocated {}", pid, signal, bytes_to_string_usize(self.size));
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

	pub fn free(&mut self) -> usize {
		if self.pid != PID_ZERO {
			signal::kill(self.pid, SIGCONT).unwrap();
			self.wait(None);
			self.size
		} else {
			0
		}
	}
}