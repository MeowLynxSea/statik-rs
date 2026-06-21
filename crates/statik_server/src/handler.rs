use std::io;

use statik_core::prelude::*;
use tokio::sync::mpsc;

use crate::{connection::Connection, shutdown::Shutdown};

/// Per-connection handler. Reads packets sent from `connection` (a tcp stream
/// from a minecraft client) and sends responses accordingly.
#[derive(Debug)]
pub struct Handler {
    /// The TCP connection implemented using a buffered `TcpStream` for parsing
    /// minecraft packets.
    ///
    /// When the [`Server`] receives an inbound connection, the `TcpStream` is
    /// passed to `Connection::new`, which initializes the associated buffers.
    /// `Connection` allows the handler to operate at the "frame" level and keep
    /// the byte level protocol parsing details encapsulated in `Connection`.
    connection: Connection,

    /// Listen for shutdown notifications.
    ///
    /// A wrapper around the `broadcast::Receiver` paired with the sender in
    /// [`Server`]. The connection handler processes requests from the
    /// connection until the peer disconnects **or** a shutdown notification is
    /// received from `shutdown`. In the latter case, any in-flight work being
    /// processed for the peer is continued until it reaches a safe state, at
    /// which point the connection is terminated.
    shutdown: Shutdown,

    /// Not used directly. Instead, when `Handler` is dropped, this clone of the
    /// shutdown-complete sender is also dropped — all clones must be dropped
    /// for the server to shut down, so this is how the server detects when all
    /// connections have finished/been terminated.
    _shutdown_complete: mpsc::Sender<String>,
}

impl Handler {
    pub async fn new(
        connection: Connection,
        shutdown: Shutdown,
        _shutdown_complete: mpsc::Sender<String>,
    ) -> Self {
        Self {
            connection,
            shutdown,
            _shutdown_complete,
        }
    }

    pub async fn run(&mut self) -> Result<()> {
        // As long as the shutdown signal has not been received, try to read a
        // new packet.
        while !self.shutdown.is_shutdown() {
            // While reading a packet, also listen for the shutdown signal —
            // otherwise a long-running job could hang forever.
            tokio::select! {
                res = self.connection.handle_connection() => {
                    if let Err(e) = res {
                        warn!("{e:?}");

                        // EOF can happen if the client disconnects while
                        // joining, which isn't very erroneous.
                        if let Some(er) = e.downcast_ref::<io::Error>() {
                            if er.kind() == io::ErrorKind::UnexpectedEof {
                                return Ok(());
                            }
                        }
                        return Err(anyhow!("connection ended with error: {e:#}"));
                    }

                    // `handle_connection` only returns once the peer closes
                    // the stream or an error occurs, so reaching here is
                    // unexpected.
                    warn!("shouldn't be possible to be here!");
                },

                // If a shutdown signal is received, return from `run`,
                // terminating the task. The actual disconnect packet templating
                // is a future TODO (see TODO.md) — for now we just log.
                reason = self.shutdown.recv() => {
                    match reason {
                        Some(r) => debug!(
                            "Client connection from {} disconnected by server with reason: \"{r}\"",
                            self.connection.address
                        ),
                        None => debug!(
                            "Client connection from {} disconnected by server (no reason provided).",
                            self.connection.address
                        ),
                    }
                }
            };
        }

        Ok(())
    }
}
