use std::time::{Duration, Instant};

use bytesize::ByteSize;
use strum_macros::EnumString;

use crate::{mem_info::MemInfoProvider};
use crate::sys::platform::{AbsoluteAllocator, Chunk, UsageAllocator};

#[derive(EnumString, Debug)]
pub enum AllocationMode {
	#[strum(serialize = "absolute")]
	Absolute,
	#[strum(serialize = "usage")]
	Usage,
}

#[derive(Debug)]
pub enum Size {
	Bytes(usize),
	Percent(u16),
}

pub fn parse_size(input: impl AsRef<str>) -> Result<Size, String> {
	let input = input.as_ref();
	if input.ends_with('%') {
		let percent = input.trim_end_matches('%').parse().map_err(|e| format!("{}", e))?;
		Ok(Size::Percent(percent))
	} else {
		let byte: ByteSize = input.parse().map_err(|e| format!("{}", e))?;
		Ok(Size::Bytes(byte.as_u64() as usize))
	}
}


pub trait Allocator {
	fn update(&mut self);
	fn size(&self) -> usize;
}

pub fn new_allocator<'a>(mode: AllocationMode, mem_info_provider: &'a dyn MemInfoProvider, size: Size) -> Box<dyn Allocator + 'a> {
	match mode {
		AllocationMode::Absolute => { Box::new(AbsoluteAllocator::new(mem_info_provider, size)) }
		AllocationMode::Usage => { Box::new(UsageAllocator::new(mem_info_provider, size)) }
	}
}


pub struct Chunks {
	chunks: Vec<Chunk>,
	last_allocation: Instant,
}

const KB: i64 = 1024;
const MB: i64 = 1024 * KB;

impl Chunks {
	pub fn new() -> Self {
		return Self { chunks: vec![], last_allocation: Instant::now() };
	}

	pub fn size(&self) -> usize {
		self.chunks.iter().map(|c| { c.size() }).sum()
	}

	pub fn check(&mut self) {
		self.chunks.iter_mut().for_each(|c| { c.check() });
	}

	pub fn resize(&mut self, size: usize) {
		let diff = size as i64 - self.size() as i64;
		self.adjust_by(diff)
	}

	pub fn adjust_by(&mut self, size: i64) {
		let now = Instant::now();
		if now - self.last_allocation < Duration::from_secs(1) && size.abs() < 2 * MB {
			return;
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