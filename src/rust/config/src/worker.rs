// Copyright 2020 Twitter, Inc.
// Licensed under the Apache License, Version 2.0
// http://www.apache.org/licenses/LICENSE-2.0

use core::time::Duration;
use serde::{Deserialize, Serialize};

// constants to define default values
const WORKER_TIMEOUT: u64 = 100;
const WORKER_NEVENT: usize = 1024;
const WORKER_THREADS: usize = 1;

// helper functions
fn timeout() -> u64 {
    WORKER_TIMEOUT
}

fn nevent() -> usize {
    WORKER_NEVENT
}

fn threads() -> usize {
    WORKER_THREADS
}

// definitions
#[derive(Serialize, Deserialize, Debug)]
pub struct Worker {
    #[serde(default = "timeout")]
    timeout: u64,
    #[serde(default = "nevent")]
    nevent: usize,
    #[serde(default = "threads")]
    threads: usize,
}

// implementation
impl Worker {
    /// Timeout to use for mio poll
    pub fn timeout(&self) -> Duration {
        Duration::from_micros(self.timeout)
    }

    pub fn nevent(&self) -> usize {
        self.nevent
    }

    pub fn threads(&self) -> usize {
        self.threads
    }

    pub fn set_threads(&mut self, threads: usize) {
        self.threads = threads
    }
}

// trait implementations
impl Default for Worker {
    fn default() -> Self {
        Self {
            timeout: timeout(),
            nevent: nevent(),
            threads: threads(),
        }
    }
}

pub trait WorkerConfig {
    fn worker(&self) -> &Worker;

    fn worker_mut(&mut self) -> &mut Worker;
}
