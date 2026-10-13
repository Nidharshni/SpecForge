extern crate self as specforge_mcp;

pub mod args;
pub mod events;
mod extension_dispatch;
pub mod lifecycle;
pub mod message;
pub mod modern;
pub mod mutation;
pub mod prompt;
pub mod prompts;
pub mod protocol;
mod refusal;
pub mod registry;
pub mod reply;
pub mod resources;
pub mod state;
pub mod stdio;
pub mod subscriptions;
mod surface_call;
pub mod surface_table;
pub mod target;
pub mod tool;
pub mod tools;
mod uri_template;

use events::{EventSink, McpEvent};
use protocol::router::route;
use protocol::{JsonRpcResponse, parse_request_value};
use serde_json::Value;
use state::McpState;

pub struct McpServer {
    state: McpState,
}

impl Default for McpServer {
    fn default() -> Self {
        Self::new()
    }
}

impl McpServer {
    pub fn new() -> Self {
        Self {
            state: McpState::new(),
        }
    }

    /// A server that compiles `root` when the client's `initialize` does not
    /// name a `projectRoot` — standard MCP clients never send one.
    pub fn with_project_root(root: std::path::PathBuf) -> Self {
        let mut server = Self::new();
        server.state.default_project_root = Some(root);
        server
    }

    /// The same server, its events recorded into `sink` instead of
    /// discarded (a test's `events::testing::EventLog`). Every server
    /// records into [`events::Discard`] until given another sink.
    pub fn with_events(mut self, sink: impl EventSink + 'static) -> Self {
        self.state.events = Box::new(sink);
        self
    }

    /// Handle one incoming message, a request, a notification or (in a
    /// 2025-03-26 session) a JSON-RPC batch of them, and return the reply
    /// to send, if any.
    pub fn handle_message(&mut self, input: &str) -> Option<String> {
        self.handle_bytes(input.as_bytes())
    }

    /// [`Self::handle_message`] over the message's bytes: bytes that are not
    /// JSON, invalid UTF-8 included (RFC 8259 section 8.1), are a -32700
    /// Parse error.
    fn handle_bytes(&mut self, input: &[u8]) -> Option<String> {
        let message: Value = match serde_json::from_slice(input) {
            Ok(message) => message,
            Err(_) => {
                let response =
                    JsonRpcResponse::error(None, protocol::error_codes::PARSE_ERROR, "Parse error");
                self.report_protocol_error(&response, None);
                return Some(serialize_response(&response));
            }
        };
        match message {
            Value::Array(batch) => self.handle_batch(batch),
            single => self
                .handle_request(single)
                .map(|response| serialize_response(&response)),
        }
    }

    /// A JSON-RPC batch: the response to each request in it, in order, or
    /// nothing when it holds only notifications. Only a 2025-03-26 session
    /// accepts batches (later revisions removed them); an empty batch is an
    /// invalid request.
    fn handle_batch(&mut self, batch: Vec<Value>) -> Option<String> {
        let refusal = if batch.is_empty() {
            Some("Invalid Request: empty batch".to_string())
        } else if !self.state.accepts_batches() {
            Some(format!(
                "Invalid Request: JSON-RPC batches need protocol version {}",
                lifecycle::BATCHING_PROTOCOL_VERSION
            ))
        } else {
            None
        };
        if let Some(message) = refusal {
            let response =
                JsonRpcResponse::error(None, protocol::error_codes::INVALID_REQUEST, message);
            self.report_protocol_error(&response, None);
            return Some(serialize_response(&response));
        }

        let responses: Vec<JsonRpcResponse> = batch
            .into_iter()
            .filter_map(|member| self.handle_request(member))
            .collect();
        if responses.is_empty() {
            return None;
        }
        Some(
            serde_json::to_string(&responses).expect("JSON-RPC response serialization cannot fail"),
        )
    }

    /// One request or notification; notifications get no response.
    fn handle_request(&mut self, message: Value) -> Option<JsonRpcResponse> {
        let request = match parse_request_value(message) {
            Ok(req) => req,
            Err(err_response) => {
                self.report_protocol_error(&err_response, None);
                return Some(err_response);
            }
        };

        // Notifications (no id) don't get responses in JSON-RPC
        let is_notification = request.id.is_none();

        let method = request.method.clone();
        // A handler that panics is a server fault: the request gets -32603
        // and the server keeps serving (ADR 0004 D4-a). The reply names no
        // panic message, path or backtrace.
        let id = request.id.clone();
        // A request whose _meta names a revision is served on its own
        // (MCP 2026-07-28); the rest follow what initialize negotiated.
        let modern = modern::is_modern(&request.method, &request.params);
        let revision = self.state.negotiated();
        let response = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            if modern {
                modern::handle(&mut self.state, &request.method, request.params, request.id)
            } else {
                Some(route(
                    &mut self.state,
                    revision,
                    &request.method,
                    request.params,
                    request.id,
                ))
            }
        }))
        .unwrap_or_else(|_| {
            Some(JsonRpcResponse::error(
                id,
                protocol::error_codes::INTERNAL_ERROR,
                "Internal error: the request failed unexpectedly",
            ))
        });
        if let Some(response) = &response {
            self.report_protocol_error(response, Some(&method));
        }

        if is_notification {
            return None;
        }
        response
    }

    /// Record `mcp_protocol_error_handled` when `response` is an error.
    fn report_protocol_error(&mut self, response: &JsonRpcResponse, method: Option<&str>) {
        let Some(error) = &response.error else {
            return;
        };
        self.state.events.record(McpEvent::McpProtocolErrorHandled {
            error_code: error.code,
            error_message: error.message.clone(),
            request_method: method.map(str::to_string),
        });
    }

    pub fn state(&self) -> &McpState {
        &self.state
    }

    /// The notifications queued since the last call, oldest first: the host
    /// writes them after the response of the message that queued them
    /// ([`McpServer::serve_stdio`] is the stdio host).
    pub fn take_notifications(&mut self) -> Vec<serde_json::Value> {
        self.state
            .subscriptions
            .drain()
            .iter()
            .map(subscriptions::Notification::framed)
            .collect()
    }

    /// The connection ended ([`McpServer::serve_stdio`] calls it when the
    /// stdio connection ends): every subscription and listen stream ends.
    pub fn disconnect(&mut self) {
        self.state.subscriptions.disconnect(&mut *self.state.events);
    }

    /// The same server, its projects' extensions running in `runtime` instead of the runtime each
    /// project builds for itself (`specforge_component::ComponentRuntime::with_user_cache`): how a
    /// test serves extensions in process (`specforge_wasm::testing::InProcessRuntime`).
    pub fn with_extension_runtime(mut self, runtime: specforge_project::SharedRuntime) -> Self {
        self.state.extension_runtime = Some(runtime);
        self
    }

    /// The same server, doctor checking `solver` instead of z3 from PATH: how a test fixes what
    /// doctor's toolchain finding says (`specforge_ops::testing::ScriptedSolver`).
    pub fn with_solver(
        mut self,
        solver: std::sync::Arc<dyn specforge_ops::solver::Solver>,
    ) -> Self {
        self.state.solver = solver;
        self
    }
}

fn serialize_response(response: &JsonRpcResponse) -> String {
    serde_json::to_string(response).expect("JSON-RPC response serialization cannot fail")
}
