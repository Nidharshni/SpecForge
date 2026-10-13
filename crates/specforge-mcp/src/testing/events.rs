//! The test adapter of the event sink: a log that keeps every event a server records.

use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use chrono::{DateTime, SecondsFormat, Utc};
use serde_json::{Map, Value};

use crate::events::{EventSink, McpEvent};

/// One event a log kept, with the time it was recorded.
#[derive(Debug, Clone, PartialEq)]
pub struct Recorded {
    pub event: McpEvent,
    pub timestamp: DateTime<Utc>,
}

impl Recorded {
    /// Its name as `spec/events` declares it (the serialized `event`
    /// tag).
    pub fn name(&self) -> String {
        self.serialized().0
    }

    /// Its payload as `spec/events` declares it: the event serialized,
    /// its tag removed, `timestamp` added (RFC 3339, UTC,
    /// milliseconds).
    pub fn payload(&self) -> Value {
        let mut payload = self.serialized().1;
        payload.insert(
            "timestamp".into(),
            Value::String(self.timestamp.to_rfc3339_opts(SecondsFormat::Millis, true)),
        );
        Value::Object(payload)
    }

    /// The event's name and its payload without `timestamp`.
    fn serialized(&self) -> (String, Map<String, Value>) {
        let Value::Object(mut payload) =
            serde_json::to_value(&self.event).expect("an event serializes")
        else {
            unreachable!("a tagged event serializes to an object");
        };
        let Some(Value::String(name)) = payload.remove("event") else {
            unreachable!("a tagged event carries its name");
        };
        (name, payload)
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
    /// `timestamp`. Panics when no `McpEvent` variant is called `name`
    /// (read from `McpEvent`'s derived schema), so a misspelt name
    /// fails instead of matching nothing.
    pub fn named(&self, name: &str) -> Vec<Value> {
        let document = specforge_common::shape::document::<McpEvent>();
        let known = document["oneOf"].as_array().is_some_and(|branches| {
            branches
                .iter()
                .any(|branch| branch["properties"]["event"]["enum"][0] == name)
        });
        assert!(known, "no McpEvent is called {name}");
        self.kept()
            .iter()
            .filter_map(|recorded| {
                let (event, payload) = recorded.serialized();
                (event == name).then_some(Value::Object(payload))
            })
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
