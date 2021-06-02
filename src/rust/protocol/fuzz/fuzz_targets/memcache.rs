// Copyright 2021 Twitter, Inc.
// Licensed under the Apache License, Version 2.0
// http://www.apache.org/licenses/LICENSE-2.0

#![no_main]
use libfuzzer_sys::fuzz_target;

use protocol::Parse;
use protocol::memcache::MemcacheRequest;

fuzz_target!(|data: &[u8]| {
    let _ = MemcacheRequest::parse(data);
});
