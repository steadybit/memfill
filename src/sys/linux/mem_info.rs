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