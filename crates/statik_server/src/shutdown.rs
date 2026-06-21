use statik_core::prelude::warn;
use tokio::sync::broadcast;

/// Listens for the server shutdown signal.
///
/// Shutdown is signalled using a `broadcast::Receiver`. Only a single value is
/// ever sent. Once a value has been sent via the broadcast channel, the server
/// should shutdown.
///
/// The `Shutdown` struct listens for the signal and tracks that the signal has
/// been received. Callers may query for whether the shutdown signal has been
/// received or not.
#[derive(Debug)]
pub struct Shutdown {
    /// `true` if the shutdown signal has been received - should be a one way
    /// change (you can't 'un-shutdown' a server).
    is_shutdown: bool,

    /// The receive half of the channel used to listen for shutdown.
    recv: broadcast::Receiver<String>,
}

impl Shutdown {
    /// Create a new `Shutdown` backed by the given `broadcast::Receiver`.
    pub(crate) fn new(recv: broadcast::Receiver<String>) -> Shutdown {
        Shutdown {
            is_shutdown: false,
            recv,
        }
    }

    /// Returns `true` if the shutdown signal has been received.
    pub(crate) fn is_shutdown(&self) -> bool {
        self.is_shutdown
    }

    /// Receive the shutdown notice, waiting if necessary.
    ///
    /// Returns the disconnect reason string, or `None` if the shutdown
    /// sender was dropped without ever signalling (in which case there is
    /// no meaningful reason to report).
    pub(crate) async fn recv(&mut self) -> Option<String> {
        // The broadcast channel has capacity 1 and only a single value is ever
        // sent. A `Lagged` error is still possible if a slow subscriber missed
        // the send, and `Closed` happens if all senders are dropped. Neither
        // should panic the server.
        match self.recv.recv().await {
            Ok(reason) => {
                self.is_shutdown = true;
                Some(reason)
            }
            Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                warn!("shutdown channel closed without a signal; no disconnect reason.");
                None
            }
            Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                warn!("shutdown channel lagged, missed {n} signals; treating as shutdown.");
                self.is_shutdown = true;
                None
            }
        }
    }
}
