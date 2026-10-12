//! The documents the server sends besides tool replies, each one typed
//! definition (ADR 0058): `initialize`'s and `server/discover`'s results,
//! the listing descriptors, the SpecForge notifications' params and the
//! McpError a refusal carries. Each serializes as what is sent and names the
//! spec type that states it; `tests/message_spec.rs` holds them together, as
//! it holds every core tool's reply (ADR 0048 D8) and every core prompt's
//! payload.

use serde::Serialize;
use serde_json::Value;
use specforge_common::shape::{self, Object, Shape};

/// A document the server sends whose content SpecForge decides.
pub trait Message: Serialize + Shape + Object + 'static {
    /// The spec type it is: an `Mcp*` type of `spec/types/mcp.spec`, or
    /// `GraphDelta` / `DiagnosticsDelta` of `spec/types/graph.spec`.
    const SPEC: &'static str;
    /// The JSON paths of its schema that are open values (content an
    /// extension or a client defines), as `CoreTool::OPEN_VALUES` lists a
    /// reply's.
    const OPEN_VALUES: &'static [&'static str] = &[];
}

/// What the reflection tests read of one message type.
#[derive(Debug, Clone, Copy)]
pub struct SentDocument {
    pub spec: &'static str,
    pub open_values: &'static [&'static str],
    /// Its JSON Schema, one document built once per process (ADR 0051 D4).
    pub schema: &'static Value,
}

fn sent<M: Message>() -> SentDocument {
    SentDocument {
        spec: M::SPEC,
        open_values: M::OPEN_VALUES,
        schema: shape::document::<M>(),
    }
}

/// Every message type the server sends, in protocol order.
pub fn documents() -> Vec<SentDocument> {
    vec![
        sent::<crate::lifecycle::InitializeResult>(),
        sent::<crate::lifecycle::DiscoverResult>(),
        sent::<specforge_project::GraphDelta>(),
        sent::<crate::subscriptions::DiagnosticsDelta>(),
        sent::<crate::tool::McpError>(),
    ]
}

impl Message for crate::lifecycle::InitializeResult {
    const SPEC: &'static str = "McpInitializeResult";
}

impl Message for crate::lifecycle::DiscoverResult {
    const SPEC: &'static str = "McpDiscoverResult";
}

impl Message for specforge_project::GraphDelta {
    const SPEC: &'static str = "GraphDelta";
}

impl Message for crate::subscriptions::DiagnosticsDelta {
    const SPEC: &'static str = "DiagnosticsDelta";
}

impl Message for crate::tool::McpError {
    const SPEC: &'static str = "McpError";
    const OPEN_VALUES: &'static [&'static str] = &["$.data"];
}
