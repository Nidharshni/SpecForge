//! The MCP methods the server answers, in one table (ADR 0103 D5): which
//! name is which method, in which revision it is served, and whether the
//! stateless revision caches its result. The router dispatches from it and
//! [`crate::modern`] reads what it says of the stateless revision.

/// A request that needs an initialized session (or, in the stateless
/// revision, none): everything but the lifecycle's methods.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SessionMethod {
    ListTools,
    ListResources,
    ListResourceTemplates,
    ListPrompts,
    CallTool,
    ReadResource,
    SubscribeResource,
    UnsubscribeResource,
    GetPrompt,
    /// `subscriptions/listen`: the stateless revision's stream.
    Listen,
}

/// What a method name asks for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Method {
    Initialize,
    Shutdown,
    Ping,
    Initialized,
    Cancelled,
    /// `server/discover`: exists only in the stateless revision.
    Discover,
    Session(SessionMethod),
}

/// One method: its name, what it is, the revisions that serve it and
/// whether the stateless revision caches its result.
struct Row {
    name: &'static str,
    method: Method,
    /// Served in the handshake revisions (after `initialize` for a session
    /// method).
    handshake: bool,
    /// Served in the stateless revision (MCP 2026-07-28).
    stateless: bool,
    /// The result carries `ttlMs` and `cacheScope` (stateless only).
    cacheable: bool,
}

const fn row(
    name: &'static str,
    method: Method,
    handshake: bool,
    stateless: bool,
    cacheable: bool,
) -> Row {
    Row {
        name,
        method,
        handshake,
        stateless,
        cacheable,
    }
}

use Method::{Cancelled, Discover, Initialize, Initialized, Ping, Session, Shutdown};
use SessionMethod::{
    CallTool, GetPrompt, ListPrompts, ListResourceTemplates, ListResources, ListTools, Listen,
    ReadResource, SubscribeResource, UnsubscribeResource,
};

/// Every method, once. The server's results are about the project on disk,
/// which any request may recompile: a cacheable one is never fresh (`ttlMs`
/// 0) and never shared beyond its client (`private`).
static METHODS: &[Row] = &[
    row("initialize", Initialize, true, false, false),
    row("shutdown", Shutdown, true, false, false),
    row("ping", Ping, true, false, false),
    row("notifications/initialized", Initialized, true, false, false),
    row("notifications/cancelled", Cancelled, true, true, false),
    row("$/cancelRequest", Cancelled, true, false, false),
    row("server/discover", Discover, false, true, true),
    row("subscriptions/listen", Session(Listen), false, true, false),
    row("tools/list", Session(ListTools), true, true, true),
    row("prompts/list", Session(ListPrompts), true, true, true),
    row("resources/list", Session(ListResources), true, true, true),
    row(
        "resources/templates/list",
        Session(ListResourceTemplates),
        true,
        true,
        true,
    ),
    row("resources/read", Session(ReadResource), true, true, true),
    row("tools/call", Session(CallTool), true, true, false),
    row("prompts/get", Session(GetPrompt), true, true, false),
    row(
        "resources/subscribe",
        Session(SubscribeResource),
        true,
        false,
        false,
    ),
    row(
        "resources/unsubscribe",
        Session(UnsubscribeResource),
        true,
        false,
        false,
    ),
];

impl Method {
    /// The method `name` is in the stateless revision (`stateless`) or the
    /// handshake ones; None when that revision does not serve it.
    pub(crate) fn named(name: &str, stateless: bool) -> Option<Self> {
        METHODS
            .iter()
            .find(|row| row.name == name)
            .filter(|row| {
                if stateless {
                    row.stateless
                } else {
                    row.handshake
                }
            })
            .map(|row| row.method)
    }
}

/// Whether the stateless revision's result of `name` is cacheable.
pub(crate) fn cacheable(name: &str) -> bool {
    METHODS.iter().any(|row| row.name == name && row.cacheable)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_name_is_one_row() {
        for (at, row) in METHODS.iter().enumerate() {
            assert!(
                METHODS[at + 1..].iter().all(|other| other.name != row.name),
                "{} is two rows",
                row.name
            );
        }
        assert_eq!(METHODS.len(), 17);
    }
}
