#[cfg(unix)]
pub mod linux;

#[cfg(windows)]
pub mod windows;

pub mod platform{
    #[cfg(unix)]
    pub use super::linux::allocator::LinuxChunk as Chunk;
    #[cfg(windows)]
    pub use super::windows::allocator::WindowsChunk as Chunk;

    #[cfg(unix)]
    pub use super::linux::mem_info::get_linux_mem_info as get_mem_info;

    #[cfg(windows)]
    pub use super::windows::mem_info::get_windows_mem_info as get_mem_info;
}