#[derive(Debug)]
struct MemInfo {
	available: usize,
	total: usize,
}


trait MemInfoProvider {
	fn mem_info(&self) -> MemInfo;
}
