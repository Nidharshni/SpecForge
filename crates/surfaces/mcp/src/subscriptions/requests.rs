//! The requests that change who hears about what. A resource is served by
//! the rule `resources/read` applies (ADR 0024 D2): a core one, or an
//! extension's, the served project brought up to date first
//! (`resources::is_served`).

use serde_json::{Value, json};

use crate::lifecycle::Revision;
use crate::protocol::{JsonRpcResponse, error_codes};
use crate::resources::{self, Resources};
use crate::state::McpState;
use crate::surface_call::Invocation;

/// `resources/subscribe` (the handshake revisions). A URI the server does
/// not serve is refused as `resources/read` refuses it: not found, in the
/// code of the request's revision.
pub(crate) fn subscribe(
    state: &mut McpState,
    revision: Revision,
    params: Value,
    id: Option<Value>,
) -> JsonRpcResponse {
    let uri = match Invocation::read::<Resources>(&params) {
        Ok(invocation) => invocation.name,
        Err(error) => return JsonRpcResponse::from_error(id, error),
    };
    if !resources::is_served(state, &uri) {
        return JsonRpcResponse::from_error(
            id,
            resources::unknown_resource(revision.resource_not_found_code(), &uri),
        );
    }
    state.subscriptions.subscribe(&uri, &mut *state.events);
    JsonRpcResponse::success(id, json!({}))
}

/// `resources/unsubscribe`: never refuses a URI, so a client can drop a
/// subscription to a resource its extension stopped serving.
pub(crate) fn unsubscribe(
    state: &mut McpState,
    params: Value,
    id: Option<Value>,
) -> JsonRpcResponse {
    let uri = match Invocation::read::<Resources>(&params) {
        Ok(invocation) => invocation.name,
        Err(error) => return JsonRpcResponse::from_error(id, error),
    };
    state.subscriptions.unsubscribe(&uri, &mut *state.events);
    JsonRpcResponse::success(id, json!({}))
}

/// `subscriptions/listen` (MCP 2026-07-28): open a stream on which the
/// server tells the client when the resources it names change. The server
/// honours resource subscriptions to any resource it serves, and no
/// list-changed types (it offers none); the acknowledgement, queued first,
/// names the subset honoured. No response follows until the stream ends.
pub(crate) fn listen(
    state: &mut McpState,
    params: &Value,
    id: Option<Value>,
) -> Option<JsonRpcResponse> {
    let Some(filter) = params.get("notifications").filter(|f| f.is_object()) else {
        return Some(JsonRpcResponse::error(
            id,
            error_codes::INVALID_PARAMS,
            "Invalid params: subscriptions/listen needs notifications",
        ));
    };
    let id = id.expect("a request has an id");
    let mut uris: Vec<String> = Vec::new();
    for uri in filter["resourceSubscriptions"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
    {
        if resources::is_served(state, uri) {
            uris.push(uri.to_string());
        }
    }
    state.subscriptions.listen(id, uris, &mut *state.events);
    None
}
