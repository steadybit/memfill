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

use crate::mem_info::{bytes_to_string_usize};


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