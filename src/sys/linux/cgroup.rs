use std::{fs, io, num};
use std::fs::File;
use std::io::BufRead;
use std::path::{Path, PathBuf};
use nix::unistd;
use strum_macros::Display;

#[derive(Debug, Display)]
pub enum CGroupError {
	File(PathBuf, io::Error),
	Parse(PathBuf, num::ParseIntError),
	CgroupControllerNotFound(),
}

pub struct CGroupMemory {
	pub usage: usize,
	pub limit: usize,
	pub unlimited: bool,
}

pub fn read_cgroup_memory() -> Result<CGroupMemory, CGroupError> {
	if uses_cgroup_v2() {
		read_cgroup_v2_memory()
	} else {
		read_cgroup_v1_memory()
	}
}

fn uses_cgroup_v2() -> bool {
	return Path::new("/sys/fs/cgroup/cgroup.controllers").exists();
}

fn read_cgroup_v2_memory() -> Result<CGroupMemory, CGroupError> {
	let mut controller_path = Path::new("/sys/fs/cgroup").join(read_cgroupv2_controller()?.strip_prefix("/").unwrap_or(""));

	loop {
		let mem_max = controller_path.join("memory.max");
		let mem_current = controller_path.join("memory.current");

		if mem_max.exists() && mem_current.exists() {
			let (limit, unlimited) = read_file_usize(mem_max)?;
			let (usage, _) = read_file_usize(mem_current)?;

			return Ok(CGroupMemory { usage, limit, unlimited });
		}

		match controller_path.parent() {
			Some(p) => { controller_path = p.to_owned(); }
			None => return Err(CGroupError::CgroupControllerNotFound())
		}
	}
}

fn read_cgroupv2_controller() -> Result<String, CGroupError> {
	let path = PathBuf::from("/proc/self/cgroup");
	let file = File::open(path.as_path()).map_err(|e| CGroupError::File(path, e))?;
	let lines = io::BufReader::new(file).lines();

	for line in lines.flatten() {
		let parts: Vec<&str> = line.splitn(3, ":").collect();
		if parts[0] == "0" {
			return Ok(parts[2].to_string());
		}
	}
	Err(CGroupError::CgroupControllerNotFound())
}

fn read_cgroup_v1_memory() -> Result<CGroupMemory, CGroupError> {
	let controller_path = Path::new("/sys/fs/cgroup/memory").join(read_cgroupv1_controller()?.strip_prefix("/").unwrap_or(""));

	let (usage, _) = read_file_usize(controller_path.join("memory.usage_in_bytes"))?;
	let (limit, _) = read_file_usize(controller_path.join("memory.limit_in_bytes"))?;

	Ok(CGroupMemory { usage, limit, unlimited: limit == cgroup_v1_mem_unlimited() })
}

fn read_cgroupv1_controller() -> Result<String, CGroupError> {
	let path = PathBuf::from("/proc/self/cgroup");
	let file = File::open(path.as_path()).map_err(|e| CGroupError::File(path, e))?;
	let lines = io::BufReader::new(file).lines();

	for line in lines.flatten() {
		let parts: Vec<&str> = line.splitn(3, ":").collect();
		if parts[1] == "memory" {
			return Ok(parts[2].to_string());
		}
	}
	Err(CGroupError::CgroupControllerNotFound())
}

fn cgroup_v1_mem_unlimited() -> usize {
	if let Ok(Some(ps)) = unistd::sysconf(unistd::SysconfVar::PAGE_SIZE) {
		return ((i64::MAX / ps) * ps) as usize;
	}
	return 0;
}

fn read_file_usize<P: AsRef<Path>>(path: P) -> Result<(usize, bool), CGroupError> {
	let line = fs::read_to_string(path.as_ref()).map_err(|e| CGroupError::File(PathBuf::from(path.as_ref()), e))?;
	if line.trim() == "max" {
		Ok((0, true))
	} else {
		let i = line.trim().parse().map_err(|e| CGroupError::Parse(PathBuf::from(path.as_ref()), e))?;
		Ok((i, false))
	}
}
