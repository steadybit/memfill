use crate::mem_info::{MemInfo, MemInfoProvider};
use sysinfo::System;

pub struct SystemMemInfo {}

impl MemInfoProvider for SystemMemInfo {
	fn mem_info(&self) -> MemInfo {
        let mut sys = System::new();
        sys.refresh_memory();
        let total = sys.total_memory() as usize;
        let available = sys.available_memory() as usize;
		return MemInfo { available, total };
	}
}