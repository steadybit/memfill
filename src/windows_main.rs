use std::alloc::{alloc, dealloc};
use std::alloc::{Layout};
use std::ptr::NonNull;
use std::{thread::sleep};
use std::time::Duration;
use std::time::Instant;
use crate::allocator::new_allocator;
use crate::mem_info::{bytes_to_string_usize, MemInfoProvider};
use crate::sys::windows::mem_info::SystemMemInfo;
use crate::Opt;

pub fn allocate_mode(size: usize){
	let align: usize = std::mem::align_of::<u8>();
	let layout = Layout::from_size_align(size, align).expect("Invalid layout");
	let ptr = unsafe { alloc(layout) };
	let ptr = NonNull::new(ptr).expect("Allocation failed");
	unsafe {
		std::ptr::write_bytes(ptr.as_ptr(), 0u8, size); // zero out memory
	}

	std::thread::park();

	unsafe {
		dealloc(ptr.as_ptr(), layout);
	}

	return;
}

pub fn windows_main(opts: Opt) {

    let mem_info = Box::new(SystemMemInfo {});

	let mut allocator = new_allocator(opts.alloc_mode, mem_info.as_ref(), opts.size);
	println!("Terminating after {}s", opts.duration.as_secs());
	let deadline = Instant::now() + opts.duration;
	let mut last_log = Instant::now() - Duration::from_secs(5);
	while Instant::now() < deadline {
		allocator.update();

		let now = Instant::now();
		if now - last_log > Duration::from_secs(5) {
			let mem = mem_info.mem_info();
			print!("Available memory: {} ({}% of total memory); ", bytes_to_string_usize(mem.available), (mem.available as f64 / mem.total as f64 * 100.0).round() as i16);
			print!("Allocated by memfill: {} ({}% of total memory)", bytes_to_string_usize(allocator.size()), (allocator.size() as f64 / mem.total as f64 * 100.0).round() as i16);
			println!();
			last_log = now;
		}

		sleep(Duration::from_millis(50));
	}
	allocator.free();
}