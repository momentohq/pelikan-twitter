// Copyright 2021 Twitter, Inc.
// Licensed under the Apache License, Version 2.0
// http://www.apache.org/licenses/LICENSE-2.0

//! A simple memory backed datapool which stores a contiguous slice of bytes
//! heap-allocated in main memory.

use memmap2::{MmapMut, MmapOptions};
use crate::datapool::Datapool;

const PAGE_SIZE: usize = 4096;

/// A contiguous allocation of bytes in main memory, created as an anonymous
/// memory map.
pub struct Memory {
    mmap: MmapMut,
    size: usize,
}

impl Memory {
    /// Create a new `Memory` datapool with the specified size (in bytes)
    pub fn create(size: usize, prefault: bool) -> Result<Self, std::io::Error> {
        let mut mmap = MmapOptions::new().len(size).populate().map_anon()?;
        if prefault {
            let mut offset = 0;
            while offset < size {
                mmap[offset] = 0;
                offset += PAGE_SIZE;
            }
            mmap.flush()?;
        }
        Ok(Self { mmap, size })
    }
}

impl Datapool for Memory {
    fn as_slice(&self) -> &[u8] {
        &self.mmap[..self.size]
    }

    fn as_mut_slice(&mut self) -> &mut [u8] {
        &mut self.mmap[..self.size]
    }

    fn flush(&self) -> Result<(), std::io::Error> {
        self.mmap.flush()
    }
}
