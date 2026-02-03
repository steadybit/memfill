//! Stub module for unsupported platforms (e.g., macOS)
//!
//! This module allows the crate to compile on unsupported platforms for development
//! and testing purposes. The actual functionality is not available.

use crate::mem_info::{MemInfoProvider};

/// Stub chunk type for unsupported platforms
pub struct UnsupportedChunk;

impl UnsupportedChunk {
    pub fn new(_size: usize) -> Self {
        panic!("memfill is not supported on this platform. Supported platforms: Linux, Windows");
    }

    pub fn size(&self) -> usize {
        0
    }

    pub fn check(&mut self) {}

    pub fn free(&mut self) -> usize {
        0
    }
}

/// Stub function to get memory info on unsupported platforms
pub fn get_unsupported_mem_info<T>(_opts: &T) -> Box<dyn MemInfoProvider> {
    panic!("memfill is not supported on this platform. Supported platforms: Linux, Windows");
}
