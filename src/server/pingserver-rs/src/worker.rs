use crate::session::*;
use crate::*;
use std::sync::Arc;
use std::io::{Read, Write, ErrorKind};

/// A `Worker` handles events on `Session`s
pub struct Worker {
    config: Arc<PingserverConfig>,
    sessions: Slab<Session>,
    poll: Poll,
    receiver: Receiver<Session>,
    // waker: Arc<Waker>,
    // waker_token: Token,
}

pub const WAKER_TOKEN: usize = usize::MAX;

impl Worker {
    /// Create a new `Worker` which will get new `Session`s from the MPSC queue
    pub fn new(
        config: Arc<PingserverConfig>,
        receiver: Receiver<Session>,
    ) -> Result<Self, std::io::Error> {
        let poll = Poll::new().map_err(|e| {
            error!("{}", e);
            std::io::Error::new(std::io::ErrorKind::Other, "Failed to create epoll instance")
        })?;
        let sessions = Slab::<Session>::new();
        // let waker_token = Token(WAKER_TOKEN);
        // let waker = Arc::new(Waker::new(&poll.registry(), waker_token)?);

        Ok(Self {
            config,
            poll,
            receiver,
            sessions,
            // waker,
            // waker_token,
        })
    }

    /// Close a session given its token
    fn close(&mut self, token: Token) {
        let mut session = self.sessions.remove(token.0);
        if session.deregister(&self.poll).is_err() {
            error!("Error deregistering");
        }
    }

    /// Handle HUP and zero-length reads
    fn handle_hup(&mut self, token: Token) {
        debug!("Session closed by client");
        self.close(token);
    }

    /// Handle errors
    fn handle_error(&mut self, token: Token) {
        debug!("Error handling event");
        self.close(token);
    }

    /// Reregister the session given its token
    fn reregister(&mut self, token: Token) {
        let session = &mut self.sessions[token.0];
        if session.reregister(&self.poll).is_err() {
            error!("Failed to reregister");
            self.close(token);
        }
    }

    /// Handle a read event for the session given its token
    fn do_read(&mut self, token: Token) {
        let session = self.sessions.get_mut(token.0).unwrap();
        let mut buf = vec![255_u8; 4096];
        // read from stream to buffer
        match session.stream().read(&mut buf) {
            Ok(0) => {
                self.handle_hup(token);
            }
            Ok(bytes) => {
                trace!("got: {} bytes", bytes);
                buf.truncate(bytes);
                if buf.len() < 6 || &buf[buf.len() - 2..buf.len()] != b"\r\n" {
                    // Shortest request is "PING\r\n" at 6 bytes
                    // All complete responses end in CRLF

                    // incomplete request, stay in reading
                } else if buf.len() == 6 && &buf[..] == b"PING\r\n" {
                    trace!("PING");
                    // session.clear_buffer();
                    if session.stream().write(b"PONG\r\n").is_err() {
                        self.handle_error(token);
                    } else {
                        trace!("PONG");
                    }
                } else {
                    debug!("error");
                    self.handle_error(token);
                }
            }
            Err(e) => {
                if e.kind() == ErrorKind::WouldBlock {
                    trace!("spuriour wakeup");
                } else {
                    // some read error
                    self.handle_error(token);
                }
            }
        }
    }

    // /// Handle a write event for a session given its token
    // fn do_write(&mut self, token: Token) {
    //     let session = &mut self.sessions[token.0];
    //     match session.flush() {
    //         Ok(Some(_)) => {
    //             if !session.tx_pending() {
    //                 // done writing, transition to reading
    //                 session.set_state(State::Reading);
    //                 self.reregister(token);
    //             }
    //         }
    //         Ok(None) => {
    //             // spurious write
    //         }
    //         Err(_) => {
    //             // some error writing
    //             self.handle_error(token);
    //         }
    //     }
    // }

    /// Run the `Worker` in a loop, handling new session events
    pub fn run(&mut self) -> Self {
        let mut events = Events::with_capacity(self.config.worker().nevent());
        let timeout = Some(std::time::Duration::from_millis(
            self.config.worker().timeout() as u64,
        ));

        loop {
            // get client events with timeout
            if self.poll.poll(&mut events, timeout).is_err() {
                error!("Error polling");
            }

            // process all events
            for event in events.iter() {
                let token = event.token();
                // if token != self.waker_token {
                    if event.is_readable() {
                        self.do_read(token);
                    }

                    if event.is_writable() {
                        // self.do_write(token);
                    }
                // }
            }

            // let mut pending = Vec::new();

            // for (id, session) in self.sessions.iter_mut() {
            //     let mut tmp = vec![255_u8; 4096];
            //     match session.stream().peek(&mut tmp) {
            //         Ok(_) => {
            //             pending.push(id);
            //         }
            //         Err(_) => {
            //             // don't do anything
            //         }
            //     }
            // }

            // for id in pending {
            //     self.do_read(Token(id));
            // }

            // handle new connections
            while let Ok(mut s) = self.receiver.try_recv() {
                info!("new session");
                // reserve vacant slab
                let session = self.sessions.vacant_entry();

                // set client token to match slab
                s.set_token(Token(session.key()));
                // session.insert(s);

                // register tcp stream and insert into slab if successful
                match s.register(&self.poll) {
                    Ok(_) => {
                        session.insert(s);
                    }
                    Err(_) => {
                        error!("Error registering new socket");
                    }
                };
            }
        }
    }

    // pub fn waker(&self) -> Arc<Waker> {
    //     self.waker.clone()
    // }
}
