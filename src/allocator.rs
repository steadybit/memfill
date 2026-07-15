use std::time::{Duration, Instant};

use bytesize::ByteSize;
use strum_macros::EnumString;

use crate::mem_info::MemInfoProvider;
use crate::mem_info::{bytes_to_string_i64, bytes_to_string_usize};
use crate::sys::platform::Chunk;

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
        let percent = input
            .trim_end_matches('%')
            .parse()
            .map_err(|e| format!("{}", e))?;
        Ok(Size::Percent(percent))
    } else {
        let byte: ByteSize = input.parse()?;
        Ok(Size::Bytes(byte.as_u64() as usize))
    }
}

/// Resolves a `Size` to an absolute number of bytes, interpreting a percentage
/// against `total`.
pub fn size_to_bytes(size: &Size, total: usize) -> usize {
    match size {
        Size::Bytes(bytes) => *bytes,
        Size::Percent(percent) => (total as f64 * *percent as f64 / 100.0) as usize,
    }
}

pub trait Allocator {
    fn update(&mut self);
    fn size(&self) -> usize;
    fn free(&mut self);
    /// Increase the amount of memory kept free (used by adaptive back-off under
    /// memory pressure). No-op for allocators that do not support it.
    /// Only wired up on Linux (adaptive/PSI), hence `allow(dead_code)` elsewhere.
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    fn bump_reserve(&mut self, _delta: i64) {}
    /// Decrease the adaptive reserve again once pressure has eased.
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    fn relax_reserve(&mut self, _delta: i64) {}
}

pub fn new_allocator<'a>(
    mode: AllocationMode,
    mem_info_provider: &'a dyn MemInfoProvider,
    size: Size,
    reserve_bytes: Option<usize>,
) -> Box<dyn Allocator + 'a> {
    match mode {
        AllocationMode::Absolute => {
            Box::new(AbsoluteAllocator::new(mem_info_provider, size, reserve_bytes))
        }
        AllocationMode::Usage => {
            Box::new(UsageAllocator::new(mem_info_provider, size, reserve_bytes))
        }
    }
}

pub struct AbsoluteAllocator {
    bytes: usize,
    chunks: Chunks,
}

impl AbsoluteAllocator {
    pub fn new(provider: &dyn MemInfoProvider, size: Size, reserve_bytes: Option<usize>) -> Self {
        let mem = provider.mem_info();
        let mut bytes = size_to_bytes(&size, mem.total);
        let percent = (bytes as f64 / mem.total as f64 * 100.0).round() as u16;
        if let Some(reserve) = reserve_bytes {
            let capped = bytes.min(mem.total.saturating_sub(reserve));
            if capped < bytes {
                println!(
                    "Capping allocation to {} to keep {} reserved",
                    bytes_to_string_usize(capped),
                    bytes_to_string_usize(reserve)
                );
            }
            bytes = capped;
        }
        println!(
            "Allocating {} ({}% of total memory)",
            bytes_to_string_usize(bytes),
            percent
        );
        Self {
            bytes,
            chunks: Chunks::new(),
        }
    }
}

impl Allocator for AbsoluteAllocator {
    fn update(&mut self) {
        self.chunks.check();
        self.chunks.resize(self.bytes)
    }

    fn size(&self) -> usize {
        self.chunks.size()
    }

    fn free(&mut self) {
        self.chunks.free();
    }
}

pub struct UsageAllocator<'a> {
    /// Requested amount of memory to leave available (from size, floored by the
    /// static reserve).
    available_bytes: i64,
    /// Additional memory kept free on top of `available_bytes`, raised by the
    /// adaptive back-off when the host is under memory pressure.
    extra_reserve: i64,
    chunks: Chunks,
    provider: &'a dyn MemInfoProvider,
}

impl<'a> UsageAllocator<'a> {
    pub fn new(provider: &'a dyn MemInfoProvider, size: Size, reserve_bytes: Option<usize>) -> Self {
        let mem = provider.mem_info();
        let mut available_bytes = mem.total as i64 - size_to_bytes(&size, mem.total) as i64;
        let available_percent = (available_bytes as f64 / mem.total as f64 * 100.0).round() as i16;
        if let Some(reserve) = reserve_bytes {
            if (reserve as i64) > available_bytes {
                println!(
                    "Raising memory left free to the reserve of {}",
                    bytes_to_string_i64(reserve as i64)
                );
                available_bytes = reserve as i64;
            }
        }
        println!(
            "Allocate until {} ({}% of total memory) available left",
            bytes_to_string_i64(available_bytes),
            available_percent
        );
        Self {
            available_bytes,
            extra_reserve: 0,
            chunks: Chunks::new(),
            provider,
        }
    }

    fn target_available(&self) -> i64 {
        self.available_bytes + self.extra_reserve
    }
}

impl Allocator for UsageAllocator<'_> {
    fn update(&mut self) {
        let mem = self.provider.mem_info();
        let diff = mem.available as i64 - self.target_available();
        self.chunks.check();
        self.chunks.adjust_by(diff)
    }
    fn size(&self) -> usize {
        self.chunks.size()
    }
    fn free(&mut self) {
        self.chunks.free();
    }
    fn bump_reserve(&mut self, delta: i64) {
        self.extra_reserve += delta;
    }
    fn relax_reserve(&mut self, delta: i64) {
        self.extra_reserve = (self.extra_reserve - delta).max(0);
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
        Self {
            chunks: vec![],
            last_allocation: Instant::now(),
        }
    }

    pub fn size(&self) -> usize {
        self.chunks.iter().map(|c| c.size()).sum()
    }

    pub fn check(&mut self) {
        self.chunks.iter_mut().for_each(|c| c.check());
    }

    pub fn resize(&mut self, size: usize) {
        let diff = size as i64 - self.size() as i64;
        self.adjust_by(diff)
    }

    pub fn free(&mut self) {
        self.chunks.iter_mut().for_each(|c| {
            c.free();
        });
        self.chunks.truncate(0);
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
                None => {
                    break;
                }
                Some(mut c) => freed += c.free() as i64,
            }
        }
        let allocate = freed + size;
        if allocate > 0 {
            let count = if allocate < (16 * MB) {
                1
            } else {
                2.max(allocate / (1024 * MB))
            };
            for _i in 0..count {
                self.chunks.push(Chunk::new((allocate / count) as usize))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mem_info::{MemInfo, MemInfoProvider};

    const GB: usize = 1024 * 1024 * 1024;
    const MB: usize = 1024 * 1024;

    struct FakeProvider {
        total: usize,
        available: usize,
    }
    impl MemInfoProvider for FakeProvider {
        fn mem_info(&self) -> MemInfo {
            MemInfo {
                available: self.available,
                total: self.total,
            }
        }
    }

    #[test]
    fn size_to_bytes_resolves_bytes_and_percent() {
        assert_eq!(size_to_bytes(&Size::Bytes(1234), 8 * GB), 1234);
        assert_eq!(size_to_bytes(&Size::Percent(50), 1000), 500);
        assert_eq!(size_to_bytes(&Size::Percent(0), 1000), 0);
        assert_eq!(size_to_bytes(&Size::Percent(150), 1000), 1500);
    }

    #[test]
    fn usage_reserve_raises_the_free_floor() {
        let p = FakeProvider { total: 8 * GB, available: 4 * GB };
        // usage 100% => leave 0 free, but a 512 MiB reserve floors it.
        let a = UsageAllocator::new(&p, Size::Percent(100), Some(512 * MB));
        assert_eq!(a.available_bytes, (512 * MB) as i64);
    }

    #[test]
    fn usage_reserve_below_target_is_ignored() {
        let p = FakeProvider { total: 8 * GB, available: 4 * GB };
        // usage 50% of 8 GiB => leave 4 GiB free; a 512 MiB reserve is already satisfied.
        let a = UsageAllocator::new(&p, Size::Percent(50), Some(512 * MB));
        assert_eq!(a.available_bytes, (4 * GB) as i64);
    }

    #[test]
    fn usage_without_reserve_leaves_nothing_at_100_percent() {
        let p = FakeProvider { total: 8 * GB, available: 4 * GB };
        let a = UsageAllocator::new(&p, Size::Percent(100), None);
        assert_eq!(a.available_bytes, 0);
    }

    #[test]
    fn absolute_reserve_caps_the_allocation() {
        let p = FakeProvider { total: 8 * GB, available: 8 * GB };
        // ask for 8 GiB but keep 1 GiB reserved => cap at 7 GiB.
        let a = AbsoluteAllocator::new(&p, Size::Bytes(8 * GB), Some(GB));
        assert_eq!(a.bytes, 7 * GB);
    }

    #[test]
    fn absolute_without_reserve_is_uncapped() {
        let p = FakeProvider { total: 8 * GB, available: 8 * GB };
        let a = AbsoluteAllocator::new(&p, Size::Bytes(3 * GB), None);
        assert_eq!(a.bytes, 3 * GB);
    }

    #[test]
    fn adaptive_reserve_bumps_then_relaxes_to_the_floor() {
        let p = FakeProvider { total: 8 * GB, available: 4 * GB };
        let mut a = UsageAllocator::new(&p, Size::Percent(100), Some(GB));
        assert_eq!(a.target_available(), GB as i64);
        a.bump_reserve((256 * MB) as i64);
        assert_eq!(a.target_available(), (GB + 256 * MB) as i64);
        // relaxing past the static floor clamps at it, never below.
        a.relax_reserve((10 * GB) as i64);
        assert_eq!(a.extra_reserve, 0);
        assert_eq!(a.target_available(), GB as i64);
    }
}
