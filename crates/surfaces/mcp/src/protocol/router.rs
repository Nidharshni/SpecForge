use serde_json::Value;

use crate::lifecycle::Revision;
use crate::prompts::Prompts;
use crate::protocol::{JsonRpcResponse, error_codes};
use crate::resources::Resources;
use crate::state::McpState;
use crate::surface_call::{listed, serve};
use crate::tools::Tools;

/// The methods that need a session: every request but the lifecycle's. The
/// router refuses them once, before `initialize` (-32600), so no handler
/// guards itself.
#[derive(Clone, Copy)]
enum SessionMethod {
    ListTools,
    ListResources,
    ListResourceTemplates,
    ListPrompts,
    CallTool,
    ReadResource,
    SubscribeResource,
    UnsubscribeResource,
    GetPrompt,
}

impl SessionMethod {
    fn named(method: &str) -> Option<Self> {
        Some(match method {
            "tools/list" => Self::ListTools,
            "resources/list" => Self::ListResources,
            "resources/templates/list" => Self::ListResourceTemplates,
            "prompts/list" => Self::ListPrompts,
            "tools/call" => Self::CallTool,
            "resources/read" => Self::ReadResource,
            "resources/subscribe" => Self::SubscribeResource,
            "resources/unsubscribe" => Self::UnsubscribeResource,
            "prompts/get" => Self::GetPrompt,
            _ => return None,
        })
    }
}

pub fn route(
    state: &mut McpState,
    revision: Revision,
    method: &str,
    params: Value,
    id: Option<Value>,
) -> JsonRpcResponse {
    match method {
        // Lifecycle
        "initialize" => crate::lifecycle::handle_initialize(state, params, id),
        "shutdown" => crate::lifecycle::handle_shutdown(state, id),
        "ping" => JsonRpcResponse::success(id, serde_json::json!({})),

        // Notifications (no response for notifications — id is None)
        "notifications/initialized" => {
            // Client acknowledges initialization, no-op
            JsonRpcResponse::success(id, serde_json::json!({}))
        }
        // MCP's cancellation notification, and the LSP-style request.
        "notifications/cancelled" | "$/cancelRequest" => {
            crate::lifecycle::handle_cancel(state, params, id)
        }

        // An unknown method is -32601 whether or not the session is
        // initialized; a known one that needs a session is refused before
        // `initialize`.
        _ => match SessionMethod::named(method) {
            None => JsonRpcResponse::error(
                id,
                error_codes::METHOD_NOT_FOUND,
                format!("Method not found: {}", method),
            ),
            Some(_) if !(state.is_initialized() || revision.is_stateless()) => {
                JsonRpcResponse::error(id, error_codes::INVALID_REQUEST, "Server not initialized")
            }
            Some(session) => {
                // What is served follows the server's root before the
                // request reads it (ADR 0077 D3).
                state.follow_root();
                session_method(state, revision, session, params, id)
            }
        },
    }
}

/// A request of an initialized session.
fn session_method(
    state: &mut McpState,
    revision: Revision,
    method: SessionMethod,
    params: Value,
    id: Option<Value>,
) -> JsonRpcResponse {
    match method {
        // Listing: an environment change on disk changes the extension
        // tools and resources listed (the pipeline brings the served project
        // up to date; no extension declares a prompt).
        SessionMethod::ListTools => listed::<Tools, _>(state, |state| {
            crate::registry::handle_list_tools(state, revision, id)
        }),
        SessionMethod::ListResources => listed::<Resources, _>(state, |state| {
            crate::registry::handle_list_resources(state, id)
        }),
        SessionMethod::ListResourceTemplates => listed::<Resources, _>(state, |state| {
            crate::registry::handle_list_resource_templates(state, id)
        }),
        SessionMethod::ListPrompts => listed::<Prompts, _>(state, |state| {
            crate::registry::handle_list_prompts(state, id)
        }),

        // Calls and reads: the target of each brings the project up to
        // date.
        SessionMethod::ReadResource => serve::<Resources>(state, revision, params, id),
        SessionMethod::CallTool => serve::<Tools>(state, revision, params, id),
        SessionMethod::GetPrompt => serve::<Prompts>(state, revision, params, id),

        SessionMethod::SubscribeResource => {
            crate::subscriptions::requests::subscribe(state, revision, params, id)
        }
        SessionMethod::UnsubscribeResource => {
            crate::subscriptions::requests::unsubscribe(state, params, id)
        }
    }
}
