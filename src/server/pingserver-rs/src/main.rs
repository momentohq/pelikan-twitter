// Copyright 2020 Twitter, Inc.
// Licensed under the Apache License, Version 2.0
// http://www.apache.org/licenses/LICENSE-2.0

#[macro_use]
extern crate rustcommon_logger;

use std::net::SocketAddr;
use std::sync::mpsc::*;
use std::sync::Arc;

use config::PingserverConfig;
use mio::*;
use rustcommon_logger::{Level, Logger};
use rustcommon_metrics::*;
use slab::Slab;

mod admin;
mod common;
mod event_loop;
mod metrics;
mod server;
mod session;
mod worker;

use crate::admin::Admin;
use crate::metrics::Stat;
use crate::server::Server;
use crate::worker::Worker;

fn main() {
    // initialize logging
    Logger::new()
        .label("pingserver")
        .level(Level::Info)
        .init()
        .expect("Failed to initialize logger");

    // initialize metrics
    let metrics = crate::metrics::init();

    // load config from file
    let config = if let Some(file) = std::env::args().nth(1) {
        debug!("loading config: {}", file);
        match PingserverConfig::load(&file) {
            Ok(c) => Arc::new(c),
            Err(e) => {
                error!("{}", e);
                std::process::exit(1);
            }
        }
    } else {
        Arc::new(Default::default())
    };

    // create channel to move sessions from listener to worker
    let (sender, receiver) = sync_channel(128);

    // initialize admin
    let mut admin = Admin::new(config.clone(), metrics.clone()).unwrap_or_else(|e| {
        error!("{}", e);
        std::process::exit(1);
    });
    let admin_thread = std::thread::spawn(move || admin.run());

    // initialize worker
    let mut worker = Worker::new(config.clone(), metrics.clone(), receiver).unwrap_or_else(|e| {
        error!("{}", e);
        std::process::exit(1);
    });
    let waker = worker.waker();
    let worker_thread = std::thread::spawn(move || worker.run());

    // initialize server
    let mut server = Server::new(config, metrics, sender, waker).unwrap_or_else(|e| {
        error!("{}", e);
        std::process::exit(1);
    });
    let server_thread = std::thread::spawn(move || server.run());

    // join threads
    let _ = server_thread.join();
    let _ = worker_thread.join();
    let _ = admin_thread.join();
}
