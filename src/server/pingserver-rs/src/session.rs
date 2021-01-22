// Copyright 2020 Twitter, Inc.
// Licensed under the Apache License, Version 2.0
// http://www.apache.org/licenses/LICENSE-2.0

use crate::server::Stream;
use crate::*;

use rustcommon_buffer::*;

use std::convert::TryInto;
use std::io::Write;

#[allow(dead_code)]
/// A `Session` is the complete state of a TCP stream
pub struct Session {
    token: Token,
    addr: SocketAddr,
    stream: Stream,
    state: State,
    buffer: Buffer,
    metrics: Arc<Metrics<AtomicU64, AtomicU64>>,
}

impl Session {
    /// Create a new `Session` from an address, stream, and state
    pub fn new(
        addr: SocketAddr,
        stream: Stream,
        state: State,
        metrics: Arc<Metrics<AtomicU64, AtomicU64>>,
    ) -> Self {
        let _ = metrics.increment_counter(&Stat::TcpAccept, 1);
        Self {
            token: Token(0),
            addr: addr,
            stream,
            state,
            buffer: Buffer::with_capacity(1024, 1024),
            metrics,
        }
    }

    pub fn buffer(&mut self) -> &mut Buffer {
        &mut self.buffer
    }

    /// Register the `Session` with the event loop
    pub fn register(&mut self, poll: &Poll) -> Result<(), std::io::Error> {
        let interest = self.readiness();
        match &mut self.stream {
            Stream::Plain(s) => poll.registry().register(s, self.token, interest),
            Stream::Tls(s) => poll.registry().register(s.get_mut(), self.token, interest),
        }
    }

    /// Deregister the `Session` from the event loop
    pub fn deregister(&mut self, poll: &Poll) -> Result<(), std::io::Error> {
        match &mut self.stream {
            Stream::Plain(s) => poll.registry().deregister(s),
            Stream::Tls(s) => poll.registry().deregister(s.get_mut()),
        }
    }

    /// Reregister the `Session` with the event loop
    pub fn reregister(&mut self, poll: &Poll) -> Result<(), std::io::Error> {
        let interest = self.readiness();
        match &mut self.stream {
            Stream::Plain(s) => poll.registry().reregister(s, self.token, interest),
            Stream::Tls(s) => poll
                .registry()
                .reregister(s.get_mut(), self.token, interest),
        }
    }

    /// Reads from the stream into the session buffer
    pub fn read(&mut self) -> Result<Option<usize>, std::io::Error> {
        let _ = self.metrics.increment_counter(&Stat::TcpRecv, 1);

        match &mut self.stream {
            Stream::Plain(s) => match self.buffer.read_from(s) {
                Ok(Some(0)) => Ok(Some(0)),
                Ok(Some(bytes)) => {
                    let _ = self
                        .metrics
                        .increment_counter(&Stat::TcpRecvByte, bytes.try_into().unwrap());
                    Ok(Some(bytes))
                }
                Ok(None) => Ok(None),
                Err(e) => {
                    let _ = self.metrics.increment_counter(&Stat::TcpRecvEx, 1);
                    Err(e)
                }
            },
            Stream::Tls(s) => match self.buffer.read_from(s) {
                Ok(Some(0)) => Ok(Some(0)),
                Ok(Some(bytes)) => {
                    let _ = self
                        .metrics
                        .increment_counter(&Stat::TcpRecvByte, bytes.try_into().unwrap());
                    Ok(Some(bytes))
                }
                Ok(None) => Ok(None),
                Err(e) => {
                    let _ = self.metrics.increment_counter(&Stat::TcpRecvEx, 1);
                    Err(e)
                }
            },
        }
    }

    /// Write to the session buffer
    pub fn write(&mut self, buf: &[u8]) -> Result<usize, std::io::Error> {
        self.buffer.write(buf)
    }

    /// Flush the session buffer to the stream
    pub fn flush(&mut self) -> Result<Option<usize>, std::io::Error> {
        match &mut self.stream {
            Stream::Plain(s) => {
                let _ = self.metrics.increment_counter(&Stat::TcpSend, 1);
                match self.buffer.write_to(s) {
                    Ok(Some(bytes)) => {
                        let _ = self
                            .metrics
                            .increment_counter(&Stat::TcpSendByte, bytes.try_into().unwrap());
                        Ok(Some(bytes))
                    }
                    Ok(None) => Ok(None),
                    Err(e) => {
                        let _ = self.metrics.increment_counter(&Stat::TcpSendEx, 1);
                        Err(e)
                    }
                }
            }
            Stream::Tls(s) => {
                let _ = self.metrics.increment_counter(&Stat::TcpSend, 1);
                match self.buffer.write_to(s) {
                    Ok(Some(bytes)) => {
                        let _ = self
                            .metrics
                            .increment_counter(&Stat::TcpSendByte, bytes.try_into().unwrap());
                        Ok(Some(bytes))
                    }
                    Ok(None) => Ok(None),
                    Err(e) => {
                        let _ = self.metrics.increment_counter(&Stat::TcpSendEx, 1);
                        Err(e)
                    }
                }
            }
        }
    }

    /// Set the state of the session
    pub fn set_state(&mut self, state: State) {
        // TODO(bmartin): validate state transitions
        self.state = state;
    }

    /// Set the token which is used with the event loop
    pub fn set_token(&mut self, token: Token) {
        self.token = token;
    }

    /// Get the set of readiness events the session is waiting for
    fn readiness(&self) -> Interest {
        if self.buffer.write_pending() != 0 {
            Interest::READABLE | Interest::WRITABLE
        } else {
            Interest::READABLE
        }
    }

    pub fn is_handshaking(&self) -> bool {
        self.state == State::Handshaking
    }

    pub fn do_handshake(&mut self) -> Result<(), openssl::ssl::Error> {
        if self.state == State::Handshaking {
            if let Stream::Tls(s) = &mut self.stream {
                match s.do_handshake() {
                    Ok(()) => {
                        self.state = State::Established;
                        Ok(())
                    }
                    Err(e) => Err(e),
                }
            } else {
                Ok(())
            }
        } else {
            Ok(())
        }
    }

    pub fn close(&mut self) {
        trace!("closing session");
        let _ = self.metrics.increment_counter(&Stat::TcpClose, 1);
        // let _ = self.stream.shutdown(std::net::Shutdown::Both);
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum State {
    Handshaking,
    Established,
}
