//! The stateless revision (MCP 2026-07-28), served beside the handshake
//! revisions (ADR 0004 D4-c).
//!
//! A request whose `_meta` names a protocol version is served on its own:
//! no `initialize` before it, and nothing a prior request said changes how
//! it is answered. It never changes what the server serves: that follows
//! the server's root on disk ([`McpState::follow_root`]). Its result carries `resultType`, the server's identity
//! in `_meta`, and on a list or a read the caching hints. A request without
//! that `_meta` keeps the handshake rules of the revision `initialize`
//! negotiated.

use serde_json::{Value, json};

use crate::lifecycle::{MODERN_PROTOCOL_VERSIONS, Revision, server_info};
use crate::protocol::method::Method;
use crate::protocol::{JsonRpcError, JsonRpcResponse, error_codes};
use crate::state::{McpState, ServerPhase};

/// The `_meta` key naming the revision a request is made under.
pub const PROTOCOL_VERSION_META: &str = "io.modelcontextprotocol/protocolVersion";
/// The `_meta` key carrying the client's capabilities, required with it.
pub const CLIENT_CAPABILITIES_META: &str = "io.modelcontextprotocol/clientCapabilities";
/// The `_meta` key a result names the server in.
pub const SERVER_INFO_META: &str = "io.modelcontextprotocol/serverInfo";

/// The revision's error for a version the server does not speak.
pub const UNSUPPORTED_PROTOCOL_VERSION: i64 = -32022;

/// Whether `method` with `params` is a stateless request: `server/discover`,
/// which exists only in that revision, or any request whose `_meta` names a
/// protocol version.
pub fn is_modern(method: &str, params: &Value) -> bool {
    Method::named(method, true) == Some(Method::Discover)
        || params["_meta"].get(PROTOCOL_VERSION_META).is_some()
}

/// Serve one stateless request or notification. A `subscriptions/listen`
/// gets no response now: its acknowledgement is queued, and the stream
/// stays open until the client cancels it.
pub fn handle(
    state: &mut McpState,
    method: &str,
    params: Value,
    id: Option<Value>,
) -> Option<JsonRpcResponse> {
    if id.is_none() {
        // Cancellation is the revision's one client notification.
        if Method::named(method, true) == Some(Method::Cancelled) {
            crate::lifecycle::handle_cancel(state, params, None);
        }
        return None;
    }
    let version = match requested_version(&params) {
        Ok(version) => version,
        Err(error) => return Some(JsonRpcResponse::from_error(id, error)),
    };
    if state.phase == ServerPhase::ShuttingDown {
        return Some(JsonRpcResponse::error(
            id,
            error_codes::INVALID_REQUEST,
            "Server shutting down",
        ));
    }
    // What is served follows the server's root before the request is read:
    // while none is served, the project at or above it is served once one
    // exists there. The request itself never changes it (ADR 0077).
    state.follow_root();

    let revision = Revision::Stateless(version);
    let response = crate::protocol::router::route(state, revision, method, params, id);
    response.map(|response| decorate(response, method))
}

/// The revision a request names, checked: a request without the version or
/// the client capabilities is malformed (-32602); a version the server does
/// not speak is -32022 naming those it does.
fn requested_version(params: &Value) -> Result<&'static str, JsonRpcError> {
    let meta = &params["_meta"];
    let (Some(requested), true) = (
        meta.get(PROTOCOL_VERSION_META).and_then(Value::as_str),
        meta.get(CLIENT_CAPABILITIES_META)
            .is_some_and(Value::is_object),
    ) else {
        return Err(JsonRpcError::new(
            error_codes::INVALID_PARAMS,
            format!(
                "Invalid params: a request without initialize needs _meta[\"{PROTOCOL_VERSION_META}\"] and _meta[\"{CLIENT_CAPABILITIES_META}\"]"
            ),
        ));
    };
    MODERN_PROTOCOL_VERSIONS
        .into_iter()
        .find(|v| *v == requested)
        .ok_or_else(|| {
            JsonRpcError::new(UNSUPPORTED_PROTOCOL_VERSION, "Unsupported protocol version")
                .with_data(json!({ "supported": MODERN_PROTOCOL_VERSIONS, "requested": requested }))
        })
}

/// A successful result as the revision gives it: `resultType`, the server
/// in `_meta`, and the caching hints on a list or a read.
fn decorate(mut response: JsonRpcResponse, method: &str) -> JsonRpcResponse {
    let Some(Value::Object(result)) = response.result.as_mut() else {
        return response;
    };
    result
        .entry("resultType")
        .or_insert_with(|| Value::from("complete"));
    let meta = result.entry("_meta").or_insert_with(|| json!({}));
    if let Some(meta) = meta.as_object_mut() {
        meta.insert(
            SERVER_INFO_META.into(),
            serde_json::to_value(server_info()).unwrap_or_default(),
        );
    }
    if crate::protocol::method::cacheable(method) {
        result.insert("ttlMs".into(), Value::from(0));
        result.insert("cacheScope".into(), Value::from("private"));
    }
    response
}
