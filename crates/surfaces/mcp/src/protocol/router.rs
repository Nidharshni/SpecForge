use serde_json::Value;

use crate::lifecycle::Revision;
use crate::prompts::Prompts;
use crate::protocol::method::{Method, SessionMethod};
use crate::protocol::{JsonRpcResponse, error_codes};
use crate::resources::Resources;
use crate::state::McpState;
use crate::surface_call::{listed, serve};
use crate::tools::Tools;

/// Answer one request: the method the table names in this revision, or
/// -32601. A session method is refused before `initialize` (-32600), so no
/// handler guards itself. A `subscriptions/listen` gets no response now: its
/// acknowledgement is queued.
pub fn route(
    state: &mut McpState,
    revision: Revision,
    method: &str,
    params: Value,
    id: Option<Value>,
) -> Option<JsonRpcResponse> {
    // An unknown method is -32601 whether or not the session is
    // initialized; a known one that needs a session is refused before
    // `initialize`.
    let Some(named) = Method::named(method, revision.is_stateless()) else {
        return Some(JsonRpcResponse::error(
            id,
            error_codes::METHOD_NOT_FOUND,
            format!("Method not found: {method}"),
        ));
    };
    Some(match named {
        // Lifecycle
        Method::Initialize => crate::lifecycle::handle_initialize(state, params, id),
        Method::Shutdown => crate::lifecycle::handle_shutdown(state, id),
        Method::Ping => JsonRpcResponse::success(id, serde_json::json!({})),
        // Client acknowledges initialization, no-op
        Method::Initialized => JsonRpcResponse::success(id, serde_json::json!({})),
        // MCP's cancellation notification, and the LSP-style request.
        Method::Cancelled => crate::lifecycle::handle_cancel(state, params, id),
        Method::Discover => crate::lifecycle::handle_discover(id),
        Method::Session(_) if !(state.is_initialized() || revision.is_stateless()) => {
            JsonRpcResponse::error(id, error_codes::INVALID_REQUEST, "Server not initialized")
        }
        Method::Session(session) => {
            // What is served follows the server's root before the request
            // reads it (ADR 0077 D3).
            state.follow_root();
            return session_method(state, revision, session, params, id);
        }
    })
}

/// A request of an initialized session.
fn session_method(
    state: &mut McpState,
    revision: Revision,
    method: SessionMethod,
    params: Value,
    id: Option<Value>,
) -> Option<JsonRpcResponse> {
    if method == SessionMethod::Listen {
        return crate::subscriptions::requests::listen(state, &params, id);
    }
    Some(match method {
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
        SessionMethod::Listen => return None,
    })
}
