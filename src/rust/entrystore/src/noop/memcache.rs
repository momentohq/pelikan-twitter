// Copyright 2021 Twitter, Inc.
// Licensed under the Apache License, Version 2.0
// http://www.apache.org/licenses/LICENSE-2.0

use super::*;

use protocol::memcache::{MemcacheEntry, MemcacheStorage, MemcacheStorageError};

impl MemcacheStorage for Noop {
    fn get(&mut self, _keys: &[Vec<u8>]) -> Vec<MemcacheEntry> {
        Vec::new()
    }

    fn set(&mut self, _entry: &MemcacheEntry) -> Result<(), MemcacheStorageError> {
        Err(MemcacheStorageError::NotStored)
    }

    fn add(&mut self, _entry: &MemcacheEntry) -> Result<(), MemcacheStorageError> {
        Err(MemcacheStorageError::NotStored)
    }

    fn replace(&mut self, _entry: &MemcacheEntry) -> Result<(), MemcacheStorageError> {
        Err(MemcacheStorageError::NotFound)
    }

    fn delete(&mut self, _key: &[u8]) -> Result<(), MemcacheStorageError> {
        Err(MemcacheStorageError::NotFound)
    }

    fn cas(&mut self, _entry: &MemcacheEntry) -> Result<(), MemcacheStorageError> {
        Err(MemcacheStorageError::NotStored)
    }
}
