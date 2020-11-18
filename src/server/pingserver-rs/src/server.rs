// use mio::net::TcpListener;
use std::net::TcpListener;
use crate::session::*;
use crate::*;
use std::io::ErrorKind;

/// A `Server` is used to bind to a given socket address and accept new
/// sessions. These sessions are moved onto a MPSC queue, where they can be
/// handled by a `Worker`.
pub struct Server {
    addr: SocketAddr,
    config: Arc<PingserverConfig>,
    listener: TcpListener,
    // poll: Poll,
    sender: SyncSender<Session>,
    waker: Arc<Waker>,
}

impl Server {
    /// Creates a new `Server` that will bind to a given `addr` and push new
    /// `Session`s over the `sender`
    pub fn new(
        config: Arc<PingserverConfig>,
        sender: SyncSender<Session>,
        waker: Arc<Waker>,
    ) -> Result<Self, std::io::Error> {
        let addr = config.server().socket_addr().map_err(|e| {
            error!("{}", e);
            std::io::Error::new(std::io::ErrorKind::Other, "Bad listen address")
        })?;

        let listener = TcpListener::bind(&addr).map_err(|e| {
            error!("{}", e);
            std::io::Error::new(std::io::ErrorKind::Other, "Failed to start tcp listener")
        })?;
        listener.set_nonblocking(true).map_err(|e| {
            error!("{}", e);
            std::io::Error::new(std::io::ErrorKind::Other, "Failed to make tcp listener non-blocking")
        })?;

        // let poll = Poll::new().map_err(|e| {
        //     error!("{}", e);
        //     std::io::Error::new(std::io::ErrorKind::Other, "Failed to create epoll instance")
        // })?;

        // // register listener to event loop
        // poll.register(&mut listener, Token(0), Ready::readable(), PollOpt::edge())
        //     .map_err(|e| {
        //         error!("{}", e);
        //         std::io::Error::new(
        //             std::io::ErrorKind::Other,
        //             "Failed to register listener with epoll",
        //         )
        //     })?;

        Ok(Self {
            addr,
            config,
            listener,
            // poll,
            sender,
            waker,
        })
    }

    /// Runs the `Server` in a loop, accepting new sessions and moving them to
    /// the queue
    pub fn run(&mut self) {
        info!("running server on: {}", self.addr);

        // let mut events = Events::with_capacity(self.config.server().nevent());
        // let timeout = Some(std::time::Duration::from_millis(
        //     self.config.server().timeout() as u64,
        // ));

        // repeatedly run accepting new connections and moving them to the worker
        loop {
            match self.listener.accept() {
                Ok((stream, addr)) => {
                    stream.set_nonblocking(true).expect("failed to make stream non-blocking");
                    let mut tmp = vec![255_u8; 4096];
                    match stream.peek(&mut tmp) {
                        Ok(bytes) => {
                            info!("new stream has: {} pending bytes", bytes);
                        }
                        Err(e) => {
                            if e.kind() == ErrorKind::WouldBlock {
                                // just isn't ready
                            } else {
                                info!("peek on new stream returned some error");
                            }
                        }
                    }
                    let stream = mio::net::TcpStream::from_std(stream);
                    let client = Session::new(addr, stream, State::Reading);
                    if self.sender.send(client).is_err() {
                        println!("error sending client to worker");
                    } else {
                        let _ = self.waker.wake();
                    }
                }
                Err(e) => {
                    if e.kind() == ErrorKind::WouldBlock {
                        // just isn't ready
                        std::thread::sleep(std::time::Duration::from_millis(1));
                    } else {
                        info!("error accepting new stream");
                    }
                    
                }
            }

            // if self.poll.poll(&mut events, timeout).is_err() {
            //     error!("Error polling server");
            // }
            // for event in events.iter() {
            //     if event.token() == Token(0) {
                //     if let Ok((stream, addr)) = self.listener.accept() {
                //         let mut tmp = vec![255_u8; 4096];
                //         if let Ok(pending) = stream.peek(&mut tmp) {
                //             info!("new stream has: {} pending bytes", pending);
                //         } else {
                //             info!("peek on new stream returned some error");
                //         }
                //         let client = Session::new(addr, stream, State::Reading);
                //         if self.sender.send(client).is_err() {
                //             println!("error sending client to worker");
                //         } else {
                //             // let _ = self.waker.wake();
                //         }
                //     } else {
                //         if 
                //         println!("error accepting client");
                //     }
                // } else {
                //     println!("unknown token");
                // }
            // }
        }
    }
}
