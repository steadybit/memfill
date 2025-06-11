#[cfg(unix)]
pub mod linux;

#[cfg(windows)]
pub mod windows;

pub mod platform{
    #[cfg(unix)]
    pub use super::linux::allocator::LinuxChunk as Chunk;
    #[cfg(windows)]
    pub use super::windows::allocator::WindowsChunk as Chunk;
}