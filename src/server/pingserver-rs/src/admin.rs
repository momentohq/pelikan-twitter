// Copyright 2020 Twitter, Inc.
// Licensed under the Apache License, Version 2.0
// http://www.apache.org/licenses/LICENSE-2.0

use crate::event_loop::EventLoop;
use crate::session::*;
use crate::*;
use mio::net::TcpListener;

use std::convert::TryInto;
use std::io::BufRead;

/// A `Admin` is used to bind to a given socket address and handle out-of-band
/// admin requests.
pub struct Admin {
    addr: SocketAddr,
    config: Arc<PingserverConfig>,
    listener: TcpListener,
    poll: Poll,
    tls_config: Option<Arc<rustls::ServerConfig>>,
    sessions: Slab<Session>,
    metrics: Arc<Metrics<AtomicU64, AtomicU64>>,
}

pub const LISTENER_TOKEN: usize = usize::MAX;

impl Admin {
    /// Creates a new `Admin` event loop.
    pub fn new(
        config: Arc<PingserverConfig>,
        metrics: Arc<Metrics<AtomicU64, AtomicU64>>,
    ) -> Result<Self, std::io::Error> {
        let addr = config.admin().socket_addr().map_err(|e| {
            error!("{}", e);
            std::io::Error::new(std::io::ErrorKind::Other, "Bad listen address")
        })?;
        let mut listener = TcpListener::bind(addr).map_err(|e| {
            error!("{}", e);
            std::io::Error::new(std::io::ErrorKind::Other, "Failed to start tcp listener")
        })?;
        let poll = Poll::new().map_err(|e| {
            error!("{}", e);
            std::io::Error::new(std::io::ErrorKind::Other, "Failed to create epoll instance")
        })?;

        let tls_config = crate::common::load_tls_config(&config)?;

        // register listener to event loop
        poll.registry()
            .register(&mut listener, Token(LISTENER_TOKEN), Interest::READABLE)
            .map_err(|e| {
                error!("{}", e);
                std::io::Error::new(
                    std::io::ErrorKind::Other,
                    "Failed to register listener with epoll",
                )
            })?;

        let sessions = Slab::<Session>::new();

        Ok(Self {
            addr,
            config,
            listener,
            poll,
            tls_config,
            sessions,
            metrics,
        })
    }

    /// Runs the `Server` in a loop, accepting new sessions and moving them to
    /// the queue
    pub fn run(&mut self) {
        info!("running admin on: {}", self.addr);

        let mut events = Events::with_capacity(self.config.server().nevent());
        let timeout = Some(std::time::Duration::from_millis(
            self.config.server().timeout() as u64,
        ));

        // repeatedly run accepting new connections and moving them to the worker
        loop {
            self.increment_count(&Stat::AdminEventLoop);
            if self.poll.poll(&mut events, timeout).is_err() {
                error!("Error polling server");
            }
            self.increment_count_n(
                &Stat::AdminEventTotal,
                events.iter().count().try_into().unwrap(),
            );
            for event in events.iter() {
                if event.token() == Token(LISTENER_TOKEN) {
                    while let Ok((stream, addr)) = self.listener.accept() {
                        if let Some(tls_config) = &self.tls_config {
                            let mut session = Session::new(
                                addr,
                                stream,
                                State::Handshaking,
                                Some(rustls::ServerSession::new(&tls_config)),
                                self.metrics.clone(),
                            );
                            let s = self.sessions.vacant_entry();
                            let token = s.key();
                            session.set_token(Token(token));
                            let _ = session.register(&self.poll);
                            s.insert(session);
                        } else {
                            let mut session = Session::new(
                                addr,
                                stream,
                                State::Established,
                                None,
                                self.metrics.clone(),
                            );
                            trace!("accepted new admin session: {}", addr);
                            let s = self.sessions.vacant_entry();
                            let token = s.key();
                            session.set_token(Token(token));
                            let _ = session.register(&self.poll);
                            s.insert(session);
                        };
                    }
                } else {
                    let token = event.token();
                    trace!("got event for admin session: {}", token.0);

                    if event.is_error() {
                        self.increment_count(&Stat::AdminEventError);
                        self.handle_error(token);
                    }

                    if event.is_readable() {
                        self.increment_count(&Stat::AdminEventRead);
                        let _ = self.do_read(token);
                    };

                    if event.is_writable() {
                        self.increment_count(&Stat::AdminEventWrite);
                        self.do_write(token);
                    }
                }
            }
        }
    }
}

impl EventLoop for Admin {
    fn metrics(&self) -> &Arc<Metrics<AtomicU64, AtomicU64>> {
        &self.metrics
    }

    fn get_mut_session<'a>(&'a mut self, token: Token) -> Option<&'a mut Session> {
        self.sessions.get_mut(token.0)
    }

    fn handle_data(&mut self, token: Token) {
        trace!("handling request for admin session: {}", token.0);
        if let Some(session) = self.sessions.get_mut(token.0) {
            loop {
                // TODO(bmartin): buffer should allow us to check remaining
                // write capacity.
                if session.buffer().write_pending() > (1024 - 6) {
                    // if the write buffer is over-full, skip processing
                    break;
                }
                match session.buffer().fill_buf() {
                    Ok(buf) => {
                        if buf.len() < 7 {
                            // Shortest request is "PING\r\n" at 6 bytes
                            // All complete responses end in CRLF

                            // incomplete request, stay in reading
                            break;
                        } else if &buf[0..7] == b"STATS\r\n" || &buf[0..7] == b"stats\r\n" {
                            let _ = self.metrics.increment_counter(&Stat::AdminRequestParse, 1);
                            session.buffer().consume(7);
                            let snapshot = self.metrics.snapshot();
                            let mut data = Vec::new();
                            for (metric, value) in snapshot {
                                let label = metric.statistic().name();
                                let output = metric.output();
                                match output {
                                    Output::Reading => {
                                        data.push(format!("STAT {} {}", label, value));
                                    }
                                    _ => {}
                                }
                            }
                            data.sort();
                            let mut content = data.join("\r\n");
                            content += "\r\n";
                            if session.write(content.as_bytes()).is_err() {
                                // error writing
                                let _ = self
                                    .metrics
                                    .increment_counter(&Stat::AdminResponseComposeEx, 1);
                                self.handle_error(token);
                                return;
                            } else {
                                let _ = self
                                    .metrics
                                    .increment_counter(&Stat::AdminResponseCompose, 1);
                            }
                        } else {
                            // invalid command
                            debug!("error");
                            self.increment_count(&Stat::AdminRequestParseEx);
                            self.handle_error(token);
                            return;
                        }
                    }
                    Err(e) => {
                        if e.kind() == std::io::ErrorKind::WouldBlock {
                            break;
                        } else {
                            // couldn't get buffer contents
                            debug!("error");
                            self.handle_error(token);
                            return;
                        }
                    }
                }
            }
        } else {
            // no session for the token
            trace!(
                "attempted to handle data for non-existent session: {}",
                token.0
            );
            return;
        }
        self.reregister(token);
    }

    fn take_session(&mut self, token: Token) -> Option<Session> {
        if self.sessions.contains(token.0) {
            let session = self.sessions.remove(token.0);
            Some(session)
        } else {
            None
        }
    }

    /// Reregister the session given its token
    fn reregister(&mut self, token: Token) {
        trace!("reregistering session: {}", token.0);
        if let Some(session) = self.sessions.get_mut(token.0) {
            if session.reregister(&self.poll).is_err() {
                error!("Failed to reregister");
                self.close(token);
            }
        } else {
            trace!("attempted to reregister non-existent session: {}", token.0);
        }
    }

    fn poll(&self) -> &Poll {
        &self.poll
    }
}
