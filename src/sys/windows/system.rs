use std::alloc::Layout;
use std::alloc::{alloc, dealloc};
use std::io::Write;
use std::ptr::NonNull;

pub fn allocate_mode(size: usize) {
    let align: usize = std::mem::align_of::<u8>();
    let layout = Layout::from_size_align(size, align).expect("Invalid layout");
    let ptr = unsafe { alloc(layout) };
    let ptr = NonNull::new(ptr).expect("Allocation failed");
    unsafe {
        std::ptr::write_bytes(ptr.as_ptr(), 0u8, size);

        let mut sum: u8 = 0;
        for i in 0..size {
            sum = sum.wrapping_add(std::ptr::read_volatile(ptr.as_ptr().add(i)))
        }

        std::io::sink().write_all(&[sum]).ok();
    }

    std::thread::park();

    unsafe {
        dealloc(ptr.as_ptr(), layout);
    }

    return;
}

/// Windows has no PSI-equivalent memory-pressure signal.
pub fn memory_pressure() -> Option<f64> {
    None
}

/// No oom_score_adj concept on Windows.
pub fn adjust_oom_score(_score: Option<i32>) {}
