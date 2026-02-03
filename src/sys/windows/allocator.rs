use std::process::{Child, Command};
use sysinfo::Pid;

use crate::mem_info::bytes_to_string_usize;

pub struct WindowsChunk {
    size: usize,
    pid: Pid,
    child: Option<Child>,
}

fn zero_pid() -> Pid {
    Pid::from_u32(0)
}

impl WindowsChunk {
    pub fn new(size: usize) -> Self {
        if size == 0 {
            return Self {
                size,
                pid: zero_pid(),
                child: None,
            };
        }

        let child = Command::new(std::env::current_exe().unwrap())
            .arg("--allocate")
            .arg(size.to_string())
            .spawn()
            .expect("Failed to spawn child process.");

        Self {
            size,
            pid: Pid::from_u32(child.id()),
            child: Some(child),
        }
    }

    pub fn size(&self) -> usize {
        if self.pid == zero_pid() {
            0
        } else {
            self.size
        }
    }

    pub fn check(&mut self) {
        self.wait();
    }

    pub fn wait(&mut self) {
        if let Some(child) = self.child.as_mut() {
            let wait_result = child.try_wait();
            match wait_result {
                Ok(Some(exit_code)) => {
                    println!(
                        "[{}] Exited({}) and de-allocated {}",
                        self.pid,
                        exit_code,
                        bytes_to_string_usize(self.size)
                    );
                    self.pid = zero_pid();
                }
                Ok(None) => {}
                Err(err) => {
                    println!("[{}] errno: {} ", self.pid, err);
                    self.pid = zero_pid();
                }
            }
        }
    }

    pub fn free(&mut self) -> usize {
        if let Some(child) = self.child.as_mut() {
            let _ = child.kill();
            let _ = child.wait();

            self.size
        } else {
            0
        }
    }
}
