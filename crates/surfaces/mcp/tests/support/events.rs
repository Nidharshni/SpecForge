//! What a test reads of the events a server records (ADR 0057): every
//! server a test builds through support records into an
//! [`EventLog`](specforge_mcp::testing::EventLog) the test keeps a
//! clone of. A production server records into `Discard`, which keeps none.

use std::ops::{Deref, DerefMut};

use serde_json::{Value, json};
use specforge_mcp::McpServer;
use specforge_mcp::testing::EventLog;

/// What can show the events its server recorded.
pub trait Logs {
    /// The log the server records into.
    fn log(&self) -> &EventLog;
}

/// A server and the log it records into. Dereferences to the server.
pub struct Logged {
    /// The server, recording into [`Self::log`].
    pub(crate) server: McpServer,
    pub(crate) log: EventLog,
}

/// `server`, its events recorded into a log the test reads.
pub fn logged(server: McpServer) -> Logged {
    let log = EventLog::new();
    Logged {
        server: server.with_events(log.clone()),
        log,
    }
}

impl Logs for specforge_mcp::testing::Served {
    fn log(&self) -> &EventLog {
        specforge_mcp::testing::Served::log(self)
    }
}

impl Logs for Logged {
    fn log(&self) -> &EventLog {
        &self.log
    }
}

impl Deref for Logged {
    type Target = McpServer;

    fn deref(&self) -> &McpServer {
        &self.server
    }
}

impl DerefMut for Logged {
    fn deref_mut(&mut self) -> &mut McpServer {
        &mut self.server
    }
}

/// The payloads of every `name` event `server` recorded, oldest first,
/// without their `timestamp`.
pub fn events(server: &impl Logs, name: &str) -> Vec<Value> {
    server.log().named(name)
}

/// `mcp_mutation_completed`'s payload (without `timestamp`, as [`events`]
/// returns them) for `tool`.
pub fn completed(tool: &str, files: usize, entities: usize, success: bool) -> Value {
    json!({
        "toolName": tool,
        "files_changed": files,
        "entities_affected": entities,
        "success": success,
    })
}

/// Panics unless an `mcp_tool_invoked` event names `tool`.
pub fn assert_tool_invoked(server: &impl Logs, tool: &str) {
    let invoked = events(server, "mcp_tool_invoked");
    assert!(
        invoked.iter().any(|p| p["toolName"] == tool),
        "no mcp_tool_invoked for {tool}: {invoked:?}"
    );
}

/// Panics unless an `mcp_prompt_invoked` event names `prompt`.
pub fn assert_prompt_invoked(server: &impl Logs, prompt: &str) {
    let invoked = events(server, "mcp_prompt_invoked");
    assert!(
        invoked.iter().any(|p| p["promptName"] == prompt),
        "no mcp_prompt_invoked for {prompt}: {invoked:?}"
    );
}
