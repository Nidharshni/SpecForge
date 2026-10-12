use serde_json::Value;
use specforge_common::ProjectRoot;
use std::path::PathBuf;

use crate::protocol::{JsonRpcResponse, error_codes};
use crate::state::{McpState, ServerPhase};
use crate::tool::CoreTool;
use crate::tools::{inspect, query, search, validate};
use crate::types::{
    McpCapabilities, McpCapabilityFlags, McpPromptCapability, McpResourceCapability, McpServerInfo,
    McpToolCapability,
};

/// The stateless revisions the server speaks, latest first: a request
/// names one in its `_meta` and needs no `initialize` (MCP 2026-07-28).
pub const MODERN_PROTOCOL_VERSIONS: [&str; 1] = ["2026-07-28"];
/// The handshake revisions the server speaks, latest first.
pub const PROTOCOL_VERSIONS: [&str; 3] = ["2025-11-25", "2025-06-18", "2025-03-26"];
/// The revision the server answers a client it can't match with.
pub const LATEST_PROTOCOL_VERSION: &str = PROTOCOL_VERSIONS[0];
/// The one revision with JSON-RPC batching; 2025-06-18 removed it.
pub const BATCHING_PROTOCOL_VERSION: &str = "2025-03-26";
/// The first revision with `structuredContent` in tool results.
pub const STRUCTURED_CONTENT_PROTOCOL_VERSION: &str = "2025-06-18";

/// The protocol revision one request is served under (ADR 0024 D8): the one
/// `initialize` negotiated, or the stateless one the request's `_meta`
/// names (MCP 2026-07-28). It travels with the request from
/// `McpServer::handle_message` to the reply; the server state keeps only
/// what `initialize` negotiated.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Revision {
    /// A handshake revision (`PROTOCOL_VERSIONS`), negotiated by
    /// `initialize`; the latest until it is.
    Negotiated(&'static str),
    /// A stateless revision (`MODERN_PROTOCOL_VERSIONS`) the request names.
    Stateless(&'static str),
}

impl Revision {
    /// The version, as `initialize` and `_meta` name it.
    pub fn version(self) -> &'static str {
        match self {
            Revision::Negotiated(version) | Revision::Stateless(version) => version,
        }
    }

    /// Whether the request is served without `initialize`.
    pub fn is_stateless(self) -> bool {
        matches!(self, Revision::Stateless(_))
    }

    /// The JSON-RPC code of a resource that does not exist: -32002 in a
    /// handshake session (2025-03-26 to 2025-11-25, server/resources),
    /// -32602 in a stateless request, which says "Invalid Params" and asks
    /// clients to accept -32002 as earlier revisions used it (ADR 0024 D3).
    pub fn resource_not_found_code(self) -> i64 {
        match self {
            Revision::Stateless(_) => error_codes::INVALID_PARAMS,
            Revision::Negotiated(_) => error_codes::RESOURCE_NOT_FOUND,
        }
    }

    /// Whether tool results carry `structuredContent` and tools list their
    /// `outputSchema` (2025-06-18 on).
    pub fn sends_structured_content(self) -> bool {
        self.version() >= STRUCTURED_CONTENT_PROTOCOL_VERSION
    }
}

/// The revision to speak with a client that asked for `requested`: that
/// one when the server speaks it, else the latest (MCP lifecycle, version
/// negotiation).
pub fn negotiate_protocol_version(requested: Option<&str>) -> &'static str {
    PROTOCOL_VERSIONS
        .into_iter()
        .find(|v| Some(*v) == requested)
        .unwrap_or(LATEST_PROTOCOL_VERSION)
}

pub fn handle_initialize(
    state: &mut McpState,
    params: Value,
    id: Option<Value>,
) -> JsonRpcResponse {
    if state.phase == ServerPhase::Initialized {
        state.push_event(
            "mcp_initialization_failed",
            serde_json::json!({
                "error_kind": "already_initialized",
                "message": "Server already initialized",
            }),
        );
        return JsonRpcResponse::error(
            id,
            error_codes::INVALID_REQUEST,
            "Server already initialized",
        );
    }

    let project_root = params
        .get("projectRoot")
        .or_else(|| params.get("project_root"))
        .and_then(|v| v.as_str())
        .map(PathBuf::from)
        .or_else(|| state.default_project_root.clone());

    serve_project(state, project_root);
    state.phase = ServerPhase::Initialized;
    state.protocol_version =
        negotiate_protocol_version(params.get("protocolVersion").and_then(Value::as_str));

    let capabilities = McpCapabilities {
        protocol_version: state.protocol_version.into(),
        capabilities: capability_flags(),
        server_info: server_info(),
        tools: crate::registry::listed_tools(state).collect(),
        resources: crate::registry::listed_resources(state).collect(),
        prompts: crate::prompts::descriptors(),
    };

    // The extension surfaces are the table's entries.
    let surfaces = state.surfaces();
    let surface_tools = surfaces.tools().len();
    let surface_resources = surfaces.resources().len();
    let auto_promoted_tools = surfaces
        .tools()
        .iter()
        .filter(|tool| matches!(tool.kind, crate::surface_table::ToolKind::Command(_)))
        .count();
    state.push_event(
        "mcp_initialized",
        serde_json::json!({
            "tools_registered": crate::tools::CORE_TOOLS.len() + surface_tools,
            "resources_registered": crate::resources::CORE_RESOURCES.len() + surface_resources,
            "prompts_registered": crate::prompts::CORE_PROMPTS.len(),
            "extensions_loaded": state.registries().extension_info().count(),
            "surface_tools_registered": surface_tools,
            "surface_resources_registered": surface_resources,
            "auto_promoted_tools": auto_promoted_tools,
        }),
    );

    let value = serde_json::to_value(capabilities).unwrap();
    JsonRpcResponse::success(id, value)
}

/// Serve the project at or above `start` (the server's PATH, or the
/// client's `projectRoot`): the nearest directory holding `specforge.json`,
/// `start` included (`ProjectRoot::enclosing`, canonical). Its
/// registries, and the core tools, resources and prompts plus what its
/// extensions contribute are served; the core surface alone when there is
/// none. With none, nothing is served: what reads only the project view
/// answers over the empty session, and a call's `path` or `init` may serve
/// a project later (ADR 0014 D5). Subscribed clients learn what it
/// changed (C9-01).
pub fn serve_project(state: &mut McpState, start: Option<PathBuf>) {
    if let Some(root) = start
        .as_deref()
        .and_then(ProjectRoot::enclosing)
        .map(ProjectRoot::canonicalized)
    {
        state.serve(root.dir());
    }
    state.served = true;
}

/// The capabilities the server offers, in either era.
fn capability_flags() -> McpCapabilityFlags {
    McpCapabilityFlags {
        tools: McpToolCapability {
            list_changed: false,
        },
        resources: McpResourceCapability {
            subscribe: true,
            list_changed: false,
        },
        prompts: McpPromptCapability {
            list_changed: false,
        },
    }
}

/// The server's name and version, as `serverInfo` gives them.
pub fn server_info() -> McpServerInfo {
    McpServerInfo {
        name: "specforge-mcp".into(),
        version: env!("CARGO_PKG_VERSION").into(),
    }
}

/// `server/discover` (MCP 2026-07-28): the stateless revisions the server
/// speaks and its capabilities, answered with or without `initialize`. The
/// handshake revisions are not listed: a client reaches them through
/// `initialize`, never through per-request `_meta`.
pub fn handle_discover(id: Option<Value>) -> JsonRpcResponse {
    let instructions = format!(
        "SpecForge compiles .spec files into a graph of entities. Query it with {}, {} and {}; check it with {}.",
        query::Tool::NAME,
        search::Tool::NAME,
        inspect::Tool::NAME,
        validate::Tool::NAME,
    );
    JsonRpcResponse::success(
        id,
        serde_json::json!({
            "supportedVersions": MODERN_PROTOCOL_VERSIONS,
            "capabilities": capability_flags(),
            "instructions": instructions,
        }),
    )
}

pub fn handle_shutdown(state: &mut McpState, id: Option<Value>) -> JsonRpcResponse {
    if state.phase == ServerPhase::ShuttingDown {
        return JsonRpcResponse::error(
            id,
            error_codes::INVALID_REQUEST,
            "Server already shutting down",
        );
    }

    let pending_notifications = state.subscriptions().pending();
    // The served project's runtime goes with its session: no engine
    // outlives shutdown.
    let engines = usize::from(state.session().runtime().is_some());
    let subscriptions = state.shutdown();
    state.push_event(
        "mcp_server_shutdown",
        serde_json::json!({
            "pending_notifications_flushed": pending_notifications,
            "subscriptions_released": subscriptions,
            "wasm_engines_released": engines,
        }),
    );
    JsonRpcResponse::success(id, serde_json::json!({}))
}

pub fn handle_cancel(state: &mut McpState, params: Value, id: Option<Value>) -> JsonRpcResponse {
    // Cancelling a subscriptions/listen request ends its stream.
    if let Some(listened) = params.get("requestId") {
        state.subscriptions.end(listened, &mut state.events);
    }
    // JSON-RPC ids are strings or numbers; the event names either as a string.
    let request_id = params
        .get("requestId")
        .or_else(|| params.get("id"))
        .map(crate::protocol::id_text)
        .unwrap_or_default();
    // Requests run one at a time, so the one named has already completed:
    // it was never in progress, and cancelling it changes nothing.
    state.push_event(
        "mcp_request_cancelled",
        serde_json::json!({
            "requestId": request_id,
            "wasInProgress": false,
        }),
    );
    JsonRpcResponse::success(id, serde_json::json!({}))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_revision_shapes_its_reply() {
        let stateless = Revision::Stateless("2026-07-28");
        assert_eq!(stateless.resource_not_found_code(), -32602);
        assert!(stateless.sends_structured_content());
        assert!(stateless.is_stateless());

        let oldest = Revision::Negotiated("2025-03-26");
        assert_eq!(oldest.resource_not_found_code(), -32002);
        assert!(!oldest.sends_structured_content());
        assert!(!oldest.is_stateless());

        for version in ["2025-06-18", "2025-11-25"] {
            let negotiated = Revision::Negotiated(version);
            assert_eq!(negotiated.resource_not_found_code(), -32002, "{version}");
            assert!(negotiated.sends_structured_content(), "{version}");
            assert!(!negotiated.is_stateless(), "{version}");
            assert_eq!(negotiated.version(), version);
        }
    }
}
