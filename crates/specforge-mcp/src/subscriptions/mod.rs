//! Who hears about what (ADR 0024 D6): the resources the client subscribed
//! to with `resources/subscribe` (the handshake revisions), the streams it
//! opened with `subscriptions/listen` (MCP 2026-07-28), and the
//! notifications queued for it until the host sends them. One rule says what
//! a resource's content changes with ([`Watched::of`]); one update's changes
//! are computed once ([`Changes`]), in `McpState::applied`, the one place
//! every update of the served project passes (ADR 0035 D3), and every
//! subscription they touched hears about them ([`Subscriptions::updated`]).
//! Subscriptions belong to the connection: they end with it, or at shutdown.
//! The requests that change them are in [`requests`]; the notifications'
//! JSON shapes in `wire`.

pub(crate) mod requests;
mod wire;

use std::collections::HashSet;

use serde_json::Value;
use specforge_common::Diagnostic;
use specforge_project::{GraphDelta, Update, UpdateKind};

use crate::events::{EventSink, McpEvent};

/// What a resource's content changes with: the one rule both eras read.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Watched {
    /// The graph or the environment: every graph view (`graph`, `context`,
    /// `brief`, an entity's subgraph, a kind's entities), whose export is
    /// shaped by the schema, and every extension resource, which its module
    /// computes.
    Graph,
    /// The environment alone: `specforge://schema`.
    Schema,
    /// What the server reports: `specforge://diagnostics`.
    Diagnostics,
}

impl Watched {
    /// What `uri`'s content changes with. `uri` is one the server serves
    /// (the requests check it first).
    pub fn of(uri: &str) -> Self {
        match uri {
            "specforge://diagnostics" => Watched::Diagnostics,
            "specforge://schema" => Watched::Schema,
            _ => Watched::Graph,
        }
    }
}

/// The diagnostics added and removed by one update.
#[derive(Debug, Default)]
struct DiagnosticsDelta {
    added: Vec<Diagnostic>,
    removed: Vec<Diagnostic>,
}

impl DiagnosticsDelta {
    /// What names a diagnostic in a delta: its code, its message and its
    /// file.
    fn identity(diagnostic: &Diagnostic) -> (&str, &str, &str) {
        (
            diagnostic.code.as_str(),
            diagnostic.message.as_str(),
            diagnostic
                .span
                .as_ref()
                .map_or("", |span| span.file.as_str()),
        )
    }

    /// The diagnostics in `after` and not in `before`, and the reverse.
    fn between(before: &[Diagnostic], after: &[Diagnostic]) -> Self {
        let before_keys: HashSet<_> = before.iter().map(Self::identity).collect();
        let after_keys: HashSet<_> = after.iter().map(Self::identity).collect();
        DiagnosticsDelta {
            added: after
                .iter()
                .filter(|d| !before_keys.contains(&Self::identity(d)))
                .cloned()
                .collect(),
            removed: before
                .iter()
                .filter(|d| !after_keys.contains(&Self::identity(d)))
                .cloned()
                .collect(),
        }
    }

    fn is_empty(&self) -> bool {
        self.added.is_empty() && self.removed.is_empty()
    }
}

/// What one update of the served project changed, computed once and read by
/// both eras.
pub struct Changes<'u> {
    graph: &'u GraphDelta,
    environment: bool,
    diagnostics: Option<DiagnosticsDelta>,
}

impl<'u> Changes<'u> {
    /// What `update` changed. `diagnostics` is what the server reported
    /// before the update and reports after it, read only when someone hears
    /// about the diagnostics ([`Subscriptions::hears_diagnostics`]); `None`
    /// reads as "unchanged".
    pub fn of(update: &'u Update, diagnostics: Option<(&[Diagnostic], &[Diagnostic])>) -> Self {
        Self::new(
            &update.delta,
            update.kind == UpdateKind::Environment,
            diagnostics,
        )
    }

    /// The same from its parts: the graph delta, whether the environment
    /// loaded again (or another project was served), and the diagnostics
    /// before and after.
    pub fn new(
        graph: &'u GraphDelta,
        environment: bool,
        diagnostics: Option<(&[Diagnostic], &[Diagnostic])>,
    ) -> Self {
        Changes {
            graph,
            environment,
            diagnostics: diagnostics
                .map(|(before, after)| DiagnosticsDelta::between(before, after)),
        }
    }

    /// Whether what `watched` names changed: the graph delta is not empty
    /// or the environment loaded again (`Graph`); the environment loaded
    /// again (`Schema`); a diagnostic was added or removed, named by its
    /// code, message and file (`Diagnostics`).
    pub fn touched(&self, watched: Watched) -> bool {
        match watched {
            Watched::Graph => !self.graph.is_empty() || self.environment,
            Watched::Schema => self.environment,
            Watched::Diagnostics => self
                .diagnostics
                .as_ref()
                .is_some_and(|delta| !delta.is_empty()),
        }
    }
}

/// One `subscriptions/listen` stream (MCP 2026-07-28): the resources it
/// asked to hear about, and the listen request's id every notification on it
/// carries as `io.modelcontextprotocol/subscriptionId`.
#[derive(Debug, Clone, PartialEq)]
struct Stream {
    id: Value,
    uris: Vec<String>,
}

/// Who hears about what, and what waits to be sent.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Subscriptions {
    /// `resources/subscribe`: each resource once, in subscription order.
    resources: Vec<String>,
    /// `subscriptions/listen`: the open streams, in opening order.
    streams: Vec<Stream>,
    /// Queued notifications, oldest first.
    outbox: Vec<Value>,
}

impl Subscriptions {
    /// Nothing subscribed, nothing open, nothing queued.
    pub fn new() -> Self {
        Self::default()
    }

    /// `resources/subscribe`: the client hears when `uri` changes, until it
    /// unsubscribes or the connection ends. False when it already did. The
    /// request checked that the server serves `uri`. Records
    /// `mcp_subscription_created`.
    pub fn subscribe(&mut self, uri: &str, events: &mut dyn EventSink) -> bool {
        if self.resources.iter().any(|held| held == uri) {
            return false;
        }
        self.resources.push(uri.to_string());
        events.record(McpEvent::subscription_created(uri, None));
        true
    }

    /// `resources/unsubscribe`: the client no longer hears about `uri`; its
    /// other subscriptions stay. False when it did not. Records
    /// `mcp_subscription_removed`.
    pub fn unsubscribe(&mut self, uri: &str, events: &mut dyn EventSink) -> bool {
        let Some(position) = self.resources.iter().position(|held| held == uri) else {
            return false;
        };
        self.resources.remove(position);
        events.record(McpEvent::subscription_removed(uri, None));
        true
    }

    /// `subscriptions/listen`: open the stream `id` on `uris` (each once, in
    /// the order first named), queueing its acknowledgement before anything
    /// else on it. A stream already open under `id` ends first. Records
    /// `mcp_subscription_created` per resource.
    pub fn listen(&mut self, id: Value, uris: Vec<String>, events: &mut dyn EventSink) {
        self.end(&id, events);
        let mut named = Vec::with_capacity(uris.len());
        for uri in uris {
            if !named.contains(&uri) {
                named.push(uri);
            }
        }
        self.outbox.push(wire::acknowledged(&id, &named));
        for uri in &named {
            events.record(McpEvent::subscription_created(uri, Some(&id)));
        }
        self.streams.push(Stream { id, uris: named });
    }

    /// The client cancelled the listen request `id`: its stream ends, and
    /// nothing more is sent on it, not even a response. False when no stream
    /// had that id. Records `mcp_subscription_removed` per resource.
    pub fn end(&mut self, id: &Value, events: &mut dyn EventSink) -> bool {
        let Some(position) = self.streams.iter().position(|open| &open.id == id) else {
            return false;
        };
        let ended = self.streams.remove(position);
        for uri in &ended.uris {
            events.record(McpEvent::subscription_removed(uri, Some(&ended.id)));
        }
        true
    }

    /// The connection ended, or the server shuts down: every subscription
    /// and every stream ends, each removal recorded. The queue stays: the
    /// host sends it after the last response. How many subscriptions ended
    /// (a stream counts one per resource).
    pub fn disconnect(&mut self, events: &mut dyn EventSink) -> usize {
        let mut ended = 0;
        for uri in self.resources.drain(..) {
            events.record(McpEvent::subscription_removed(&uri, None));
            ended += 1;
        }
        for stream in self.streams.drain(..) {
            for uri in &stream.uris {
                events.record(McpEvent::subscription_removed(uri, Some(&stream.id)));
                ended += 1;
            }
        }
        ended
    }

    /// Whether anyone hears about the diagnostics: only then does an update
    /// read them before and after it applies.
    pub fn hears_diagnostics(&self) -> bool {
        self.resources
            .iter()
            .chain(self.streams.iter().flat_map(|stream| &stream.uris))
            .any(|uri| Watched::of(uri) == Watched::Diagnostics)
    }

    /// After an update of the served project: queue, in this order, for
    /// each open stream, `notifications/resources/updated` (with the
    /// stream's id) for each resource it names that `changes` touched; for
    /// each subscribed resource it touched, `notifications/resources/updated`;
    /// `specforge/graphChanged` when the graph delta is not empty and a
    /// subscribed resource changes with the graph; `specforge/diagnosticsChanged`
    /// when the diagnostics changed and `specforge://diagnostics` is
    /// subscribed. Records `mcp_delta_notified` per delta queued.
    pub fn updated(&mut self, changes: &Changes<'_>, events: &mut dyn EventSink) {
        for stream in &self.streams {
            for uri in &stream.uris {
                if changes.touched(Watched::of(uri)) {
                    self.outbox
                        .push(wire::resource_updated(uri, Some(&stream.id)));
                }
            }
        }
        for uri in &self.resources {
            if changes.touched(Watched::of(uri)) {
                self.outbox.push(wire::resource_updated(uri, None));
            }
        }
        let subscribed =
            |watched: Watched| self.resources.iter().any(|uri| Watched::of(uri) == watched);
        if !changes.graph.is_empty() && subscribed(Watched::Graph) {
            self.outbox.push(wire::graph_changed(changes.graph));
            events.record(McpEvent::graph_delta(changes.graph));
        }
        if let Some(delta) = changes.diagnostics.as_ref().filter(|d| !d.is_empty())
            && subscribed(Watched::Diagnostics)
        {
            self.outbox.push(wire::diagnostics_changed(delta));
            events.record(McpEvent::diagnostics_delta(
                delta.added.len(),
                delta.removed.len(),
            ));
        }
    }

    /// Take the queued notifications, oldest first.
    pub fn drain(&mut self) -> Vec<Value> {
        std::mem::take(&mut self.outbox)
    }

    /// How many notifications wait (what shutdown reports flushing).
    pub fn pending(&self) -> usize {
        self.outbox.len()
    }

    /// The resources the client subscribed to, in order.
    pub fn subscribed(&self) -> &[String] {
        &self.resources
    }

    /// Nothing subscribed and no stream open.
    pub fn is_empty(&self) -> bool {
        self.resources.is_empty() && self.streams.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[specforge_test_macros::test(
        behavior = "listen_for_mcp_resource_updates",
        verify = "both eras decide what a change touches by one rule"
    )]
    fn watched_follows_the_uri() {
        assert_eq!(Watched::of("specforge://diagnostics"), Watched::Diagnostics);
        assert_eq!(Watched::of("specforge://schema"), Watched::Schema);
        for uri in [
            "specforge://graph",
            "specforge://graph/alpha",
            "specforge://context?scope=alpha",
            "acme://doc/1",
        ] {
            assert_eq!(Watched::of(uri), Watched::Graph, "{uri}");
        }
    }
}
