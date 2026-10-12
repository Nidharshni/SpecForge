//! What an MCP server records it did (ADR 0057), recorded through the
//! server's one [`EventSink`]. Events never leave the process (ADR 0022 D7):
//! a production server's sink ([`Discard`]) keeps none, so what a server
//! holds never grows with the requests it answered; a test's
//! ([`testing::EventLog`]) keeps every event, stamped, in order.

use serde_json::Value;

/// One thing an MCP server did, as `spec/events/` declares it: its name and
/// its payload. The `timestamp` a payload declares is the keeping sink's,
/// not the producer's.
#[derive(Debug, Clone, PartialEq)]
pub struct McpEvent {
    pub name: String,
    pub params: Value,
}

impl McpEvent {
    /// The event `name` with payload `params`.
    pub fn new(name: impl Into<String>, params: Value) -> Self {
        McpEvent {
            name: name.into(),
            params,
        }
    }
}

/// Where a server's events go: the one seam every module records through.
/// Two adapters: [`Discard`] (what a host runs: keeps nothing) and
/// [`testing::EventLog`] (a test's: keeps each event, stamped, in order).
pub trait EventSink: Send {
    /// Take `event`, which happened now.
    fn record(&mut self, event: McpEvent);
}

/// Keeps nothing: what a production server records costs it no memory, and
/// no event leaves the process (ADR 0022 D7).
#[derive(Debug, Default, Clone, Copy)]
pub struct Discard;

impl EventSink for Discard {
    fn record(&mut self, _: McpEvent) {}
}

/// The test adapter (`features = ["testing"]`).
#[cfg(feature = "testing")]
pub mod testing {
    use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

    use chrono::{DateTime, SecondsFormat, Utc};
    use serde_json::Value;

    use super::{EventSink, McpEvent};

    /// One event a log kept, with the time it was recorded.
    #[derive(Debug, Clone, PartialEq)]
    pub struct Recorded {
        pub event: McpEvent,
        pub timestamp: DateTime<Utc>,
    }

    impl Recorded {
        /// Its name as `spec/events` declares it.
        pub fn name(&self) -> String {
            self.event.name.clone()
        }

        /// Its payload as `spec/events` declares it, `timestamp` added
        /// (RFC 3339, UTC, milliseconds).
        pub fn payload(&self) -> Value {
            let mut payload = self.event.params.clone();
            if let Some(object) = payload.as_object_mut() {
                object.insert(
                    "timestamp".into(),
                    Value::String(self.timestamp.to_rfc3339_opts(SecondsFormat::Millis, true)),
                );
            }
            payload
        }
    }

    /// Keeps every event, in the order recorded. Clones share the log: a
    /// test keeps one and gives the server another
    /// (`McpServer::with_events`).
    #[derive(Debug, Clone, Default)]
    pub struct EventLog {
        kept: Arc<Mutex<Vec<Recorded>>>,
    }

    impl EventLog {
        pub fn new() -> Self {
            Self::default()
        }

        fn kept(&self) -> MutexGuard<'_, Vec<Recorded>> {
            self.kept.lock().unwrap_or_else(PoisonError::into_inner)
        }

        /// Everything recorded so far, oldest first.
        pub fn recorded(&self) -> Vec<Recorded> {
            self.kept().clone()
        }

        /// How many events were recorded.
        pub fn len(&self) -> usize {
            self.kept().len()
        }

        pub fn is_empty(&self) -> bool {
            self.kept().is_empty()
        }

        /// The names of every event recorded, oldest first.
        pub fn names(&self) -> Vec<String> {
            self.kept().iter().map(Recorded::name).collect()
        }

        /// The payloads of every `name` event, oldest first, without their
        /// `timestamp`.
        pub fn named(&self, name: &str) -> Vec<Value> {
            self.kept()
                .iter()
                .filter(|recorded| recorded.event.name == name)
                .map(|recorded| recorded.event.params.clone())
                .collect()
        }
    }

    impl EventSink for EventLog {
        fn record(&mut self, event: McpEvent) {
            self.kept().push(Recorded {
                event,
                timestamp: Utc::now(),
            });
        }
    }
}
