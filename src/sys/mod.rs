#[cfg(target_os = "linux")]
pub mod linux;

#[cfg(windows)]
pub mod windows;

#[cfg(not(any(target_os = "linux", windows)))]
pub mod unsupported;

pub mod platform {
    #[cfg(target_os = "linux")]
    pub use super::linux::allocator::LinuxChunk as Chunk;
    #[cfg(not(any(target_os = "linux", windows)))]
    pub use super::unsupported::UnsupportedChunk as Chunk;
    #[cfg(windows)]
    pub use super::windows::allocator::WindowsChunk as Chunk;

    #[cfg(target_os = "linux")]
    pub use super::linux::mem_info::get_linux_mem_info as get_mem_info;
    #[cfg(not(any(target_os = "linux", windows)))]
    pub use super::unsupported::get_unsupported_mem_info as get_mem_info;
    #[cfg(windows)]
    pub use super::windows::mem_info::get_windows_mem_info as get_mem_info;

    // Memory-pressure signal (Linux PSI); `None` where the platform has no equivalent.
    #[cfg(target_os = "linux")]
    pub use super::linux::psi::memory_full_avg10 as memory_pressure;
    #[cfg(not(any(target_os = "linux", windows)))]
    pub use super::unsupported::memory_pressure;
    #[cfg(windows)]
    pub use super::windows::system::memory_pressure;

    // oom_score_adj adjustment (Linux); no-op where the platform has no equivalent.
    #[cfg(target_os = "linux")]
    pub use super::linux::system::adjust_oom_score;
    #[cfg(not(any(target_os = "linux", windows)))]
    pub use super::unsupported::adjust_oom_score;
    #[cfg(windows)]
    pub use super::windows::system::adjust_oom_score;
}
