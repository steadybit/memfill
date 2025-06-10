pub mod linux;
pub mod windows;

pub mod platform{
    #[cfg(unix)]
    pub use super::linux::allocator::LinuxAbsoluteAllocator as AbsoluteAllocator;
    #[cfg(unix)]
    pub use super::linux::allocator::LinuxUsageAllocator as UsageAllocator;
    #[cfg(unix)]
    pub use super::linux::allocator::LinuxChunk as Chunk;
}