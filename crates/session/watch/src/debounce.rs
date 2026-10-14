use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::{Duration, Instant};

use crate::Coalescer;

/// The debounce rule ([`Coalescer`]) on a channel.
pub struct Debouncer {
    window: Duration,
}

impl Debouncer {
    pub fn new(window: Duration) -> Self {
        Self { window }
    }

    /// The next batch from a std channel ([`crate::SpecWatcher`]'s): blocks
    /// for its first change, then until `window` passes with none. `None`
    /// once the channel is closed and nothing is pending.
    pub fn coalesce<T: Ord>(&self, receiver: &mpsc::Receiver<T>) -> Option<Vec<T>> {
        let mut pending = Coalescer::new(self.window);
        loop {
            let received = match pending.due() {
                None => receiver.recv().ok(),
                Some(due) => {
                    match receiver.recv_timeout(due.saturating_duration_since(Instant::now())) {
                        Ok(change) => Some(change),
                        Err(RecvTimeoutError::Timeout) => return pending.flush(),
                        Err(RecvTimeoutError::Disconnected) => None,
                    }
                }
            };
            match received {
                Some(change) => pending.push(change, Instant::now()),
                None => return pending.flush(),
            }
        }
    }

    /// [`Self::coalesce`] on a tokio channel (feature `tokio`): what the
    /// LSP's reparse worker batches edited documents through.
    #[cfg(feature = "tokio")]
    pub async fn coalesce_async<T: Ord>(
        &self,
        receiver: &mut tokio::sync::mpsc::UnboundedReceiver<T>,
    ) -> Option<Vec<T>> {
        let mut pending = Coalescer::new(self.window);
        loop {
            let received = match pending.due() {
                None => receiver.recv().await,
                Some(due) => match tokio::time::timeout_at(due.into(), receiver.recv()).await {
                    Ok(change) => change,
                    Err(_) => return pending.flush(),
                },
            };
            match received {
                Some(change) => pending.push(change, Instant::now()),
                None => return pending.flush(),
            }
        }
    }
}
