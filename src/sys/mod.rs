#[cfg(target_os = "linux")]
pub mod linux;

#[cfg(windows)]
pub mod windows;

#[cfg(not(any(target_os = "linux", windows)))]
pub mod unsupported;

pub mod platform {
    #[cfg(target_os = "linux")]
    pub use super::linux::allocator::LinuxChunk as Chunk;
    #[cfg(windows)]
    pub use super::windows::allocator::WindowsChunk as Chunk;
    #[cfg(not(any(target_os = "linux", windows)))]
    pub use super::unsupported::UnsupportedChunk as Chunk;

    #[cfg(target_os = "linux")]
    pub use super::linux::mem_info::get_linux_mem_info as get_mem_info;
    #[cfg(windows)]
    pub use super::windows::mem_info::get_windows_mem_info as get_mem_info;
    #[cfg(not(any(target_os = "linux", windows)))]
    pub use super::unsupported::get_unsupported_mem_info as get_mem_info;
}