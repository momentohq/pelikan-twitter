// Copyright 2021 Twitter, Inc.
// Licensed under the Apache License, Version 2.0
// http://www.apache.org/licenses/LICENSE-2.0

mod request;
mod response;

pub use request::*;
pub use response::*;

use super::*;
use crate::*;

use metrics::Stat;

impl<'a, T> Execute<MemcacheRequest, MemcacheResponse> for T
where
    T: MemcacheStorage,
{
    fn execute(&mut self, request: MemcacheRequest) -> Option<MemcacheResponse> {
        let response = match request {
            MemcacheRequest::Get { keys } => {
                increment_counter!(&Stat::Get);

                let entries = self.get(&keys);

                increment_counter_by!(&Stat::GetKey, keys.len() as u64);
                increment_counter_by!(&Stat::GetKeyHit, entries.len() as u64);
                increment_counter_by!(&Stat::GetKeyMiss, keys.len() as u64 - entries.len() as u64);

                MemcacheResponse::Values {
                    entries,
                    cas: false,
                }
            }
            MemcacheRequest::Gets { keys } => {
                increment_counter!(&Stat::Gets);

                let entries = self.get(&keys);

                increment_counter_by!(&Stat::GetsKey, keys.len() as u64);
                increment_counter_by!(&Stat::GetsKeyHit, entries.len() as u64);
                increment_counter_by!(&Stat::GetsKeyMiss, keys.len() as u64 - entries.len() as u64);

                MemcacheResponse::Values { entries, cas: true }
            }
            MemcacheRequest::Set { entry, noreply } => {
                increment_counter!(&Stat::Set);
                let response = match self.set(entry) {
                    Ok(_) => {
                        increment_counter!(&Stat::SetStored);
                        MemcacheResponse::Stored
                    }
                    Err(MemcacheStorageError::NotStored) => {
                        increment_counter!(&Stat::SetNotstored);
                        MemcacheResponse::NotStored
                    }
                    _ => {
                        unreachable!()
                    }
                };
                if noreply {
                    return None;
                }
                response
            }
            MemcacheRequest::Add { entry, noreply } => {
                increment_counter!(&Stat::Add);
                let response = match self.add(entry) {
                    Ok(_) => {
                        increment_counter!(&Stat::AddStored);
                        MemcacheResponse::Stored
                    }
                    Err(MemcacheStorageError::NotStored) => {
                        increment_counter!(&Stat::AddNotstored);
                        MemcacheResponse::NotStored
                    }
                    _ => {
                        unreachable!()
                    }
                };
                if noreply {
                    return None;
                }
                response
            }
            MemcacheRequest::Replace { entry, noreply } => {
                increment_counter!(&Stat::Replace);
                let response = match self.replace(entry) {
                    Ok(_) => MemcacheResponse::Stored,
                    Err(MemcacheStorageError::NotStored) => MemcacheResponse::NotStored,
                    _ => {
                        unreachable!()
                    }
                };
                if noreply {
                    return None;
                }
                response
            }
            MemcacheRequest::Delete { key, noreply } => {
                increment_counter!(&Stat::Delete);
                let response = match self.delete(&key) {
                    Ok(_) => MemcacheResponse::Deleted,
                    Err(MemcacheStorageError::NotFound) => MemcacheResponse::NotFound,
                    _ => {
                        unreachable!()
                    }
                };
                if noreply {
                    return None;
                }
                response
            }
            MemcacheRequest::Cas { entry, noreply } => {
                increment_counter!(&Stat::Cas);
                let response = match self.cas(entry) {
                    Ok(_) => MemcacheResponse::Stored,
                    Err(MemcacheStorageError::NotFound) => MemcacheResponse::NotFound,
                    Err(MemcacheStorageError::Exists) => MemcacheResponse::Exists,
                    Err(MemcacheStorageError::NotStored) => MemcacheResponse::NotStored,
                };
                if noreply {
                    return None;
                }
                response
            }
        };

        Some(response)
    }
}
