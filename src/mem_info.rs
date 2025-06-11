use bytesize::ByteSize;

#[derive(Debug)]
pub struct MemInfo {
	pub available: usize,
	pub total: usize,
}


pub trait MemInfoProvider {
	fn mem_info(&self) -> MemInfo;
}

pub fn bytes_to_string_i64(bytes: i64) -> String {
	format!("{}{}", if bytes < 0 { "-" } else { "" }, ByteSize::b(bytes.unsigned_abs()).to_string_as(true))
}

pub fn bytes_to_string_usize(bytes: usize) -> String {
	ByteSize::b(bytes as u64).to_string_as(true)
}