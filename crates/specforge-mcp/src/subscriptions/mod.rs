//! Who hears about what (ADR 0024 D6): the resources the client subscribed
//! to with `resources/subscribe` (the handshake revisions), the streams it
//! opened with `subscriptions/listen` (MCP 2026-07-28), and the
//! notifications queued for it until the host sends them. One rule says what
//! a resource's content changes with ([`Watched::of`]); one update's changes
//! are computed once ([`Changes`]: the graph delta, and, for a listener, the
//! diagnostics delta, the session update's plus the I017 of the extension
//! surfaces), in `McpState::applied`, the one place every update of the
//! served project passes (ADR 0035 D3, ADR 0074), and every
//! subscription they touched hears about them ([`Subscriptions::updated`]).
//! Subscriptions belong to the connection: they end with it, or at shutdown.
//! The requests that change them are in [`requests`]; the notifications,
//! typed, in `wire`.

pub(crate) mod requests;
mod wire;

use serde_json::Value;
use specforge_project::{DiagnosticsDelta, GraphDelta, Update, UpdateKind};

pub use wire::{Acknowledged, Honoured, Notification, ResourceUpdated, StreamMeta};

use crate::events::{EventSink, McpEvent};

/// What a resource's content changes with: the one rule both eras read.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Watched {
    /// The graph as the graph documents write it, each node's place
    /// included: `specforge://graph` and `specforge://graph/{entity_id}`.
    Graph,
    /// The graph's content, no position written: `context`, `brief`,
    /// `context/{entity_id}`, `entities/{kind}`, and every extension
    /// resource (its module reads a graph that carries no position).
    Content,
    /// The environment alone: `specforge://schema`.
    Schema,
    /// What the server reports: `specforge://diagnostics`.
    Diagnostics,
}

impl Watched {
    /// What `uri`'s content changes with: the core resource that names it
    /// says ([`crate::resources::ResourceSpec::changes_with`]); an extension resource changes
    /// with the graph's content. `uri` is one the server serves (the
    /// requests check it first).
    pub fn of(uri: &str) -> Self {
        crate::resources::CORE_RESOURCES
            .iter()
            .find(|spec| spec.names(uri))
            .map_or(Watched::Content, |spec| spec.changes_with)
    }
}

/// What one update of the served project changed, computed once and read by
/// both eras.
pub struct Changes<'u> {
    graph: &'u GraphDelta,
    /// Some node changed place (`Update::moved` is not empty).
    moved: bool,
    environment: bool,
    diagnostics: Option<DiagnosticsDelta>,
}

impl<'u> Changes<'u> {
    /// What `update` changed. `diagnostics` is the delta of what the server
    /// reports (the update's, then the I017 its extension surfaces added and
    /// removed), computed only when someone hears about the diagnostics
    /// ([`Subscriptions::hears_diagnostics`]); `None` reads as "unchanged".
    pub fn of(update: &'u Update, diagnostics: Option<DiagnosticsDelta>) -> Self {
        Self::new(
            &update.delta,
            !update.moved.is_empty(),
            update.kind == UpdateKind::Environment,
            diagnostics,
        )
    }

    /// The same from its parts: the graph delta, whether a node only moved,
    /// whether the environment loaded again (or another project was
    /// served), and the diagnostics delta.
    pub fn new(
        graph: &'u GraphDelta,
        moved: bool,
        environment: bool,
        diagnostics: Option<DiagnosticsDelta>,
    ) -> Self {
        Changes {
            graph,
            moved,
            environment,
            diagnostics,
        }
    }

    /// Whether what `watched` names changed: the graph delta is not empty,
    /// a node moved or the environment loaded again (`Graph`); the delta is
    /// not empty or the environment loaded again (`Content`); the
    /// environment loaded again (`Schema`); a diagnostic was added or removed, compared
    /// whole (`Diagnostics`, [`DiagnosticsDelta`]).
    pub fn touched(&self, watched: Watched) -> bool {
        match watched {
            Watched::Graph => !self.graph.is_empty() || self.moved || self.environment,
            Watched::Content => !self.graph.is_empty() || self.environment,
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
    outbox: Vec<Notification>,
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
        self.outbox.push(Notification::acknowledged(&id, &named));
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

    /// Whether anyone hears about the diagnostics: only then is an update's
    /// diagnostics delta computed.
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
                        .push(Notification::resource_updated(uri, Some(&stream.id)));
                }
            }
        }
        for uri in &self.resources {
            if changes.touched(Watched::of(uri)) {
                self.outbox.push(Notification::resource_updated(uri, None));
            }
        }
        let subscribed =
            |watched: Watched| self.resources.iter().any(|uri| Watched::of(uri) == watched);
        if !changes.graph.is_empty() && (subscribed(Watched::Graph) || subscribed(Watched::Content))
        {
            self.outbox
                .push(Notification::GraphChanged(changes.graph.clone()));
            events.record(McpEvent::graph_delta(changes.graph));
        }
        if let Some(delta) = changes.diagnostics.as_ref().filter(|d| !d.is_empty())
            && subscribed(Watched::Diagnostics)
        {
            self.outbox
                .push(Notification::DiagnosticsChanged(delta.clone()));
            events.record(McpEvent::diagnostics_delta(
                delta.added.0.len(),
                delta.removed.0.len(),
            ));
        }
    }

    /// Take the queued notifications, oldest first.
    pub fn drain(&mut self) -> Vec<Notification> {
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
        let expected = [
            ("specforge://graph", Watched::Graph),
            ("specforge://graph/alpha", Watched::Graph),
            ("specforge://graph?depth=1", Watched::Graph),
            ("specforge://schema", Watched::Schema),
            ("specforge://context", Watched::Content),
            ("specforge://context/alpha", Watched::Content),
            ("specforge://brief", Watched::Content),
            ("specforge://entities/behavior", Watched::Content),
            ("specforge://diagnostics", Watched::Diagnostics),
            ("specforge://context?scope=alpha", Watched::Content),
            ("acme://doc/1", Watched::Content),
        ];
        for (uri, watched) in expected {
            assert_eq!(Watched::of(uri), watched, "{uri}");
        }
    }

    #[specforge_test_macros::test(
        behavior = "listen_for_mcp_resource_updates",
        verify = "both eras decide what a change touches by one rule"
    )]
    fn a_move_touches_what_writes_a_place_and_a_delta_touches_both() {
        use specforge_project::NodeChange;
        let none = GraphDelta::default();
        let some = GraphDelta {
            added_nodes: vec![NodeChange {
                id: "a".into(),
                kind: "behavior".into(),
                file: "a.spec".into(),
                line: 1,
            }],
            ..GraphDelta::default()
        };
        for (delta, moved) in [(&none, false), (&none, true), (&some, false), (&some, true)] {
            let changes = Changes::new(delta, moved, false, None);
            let graph = !delta.is_empty() || moved;
            let content = !delta.is_empty();
            assert_eq!(changes.touched(Watched::Graph), graph, "{moved} {delta:?}");
            assert_eq!(
                changes.touched(Watched::Content),
                content,
                "{moved} {delta:?}"
            );
        }
    }
}
