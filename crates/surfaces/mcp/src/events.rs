//! What an MCP server records it did (ADR 0057): one typed [`McpEvent`] per
//! event the spec declares for it, recorded through the server's one
//! [`EventSink`]. Events never leave the process (ADR 0022 D7): a production
//! server's sink ([`Discard`]) keeps none, so what a server holds never
//! grows with the requests it answered; a test's ([`crate::testing::EventLog`])
//! keeps every event, stamped, in order.

use serde::Serialize;
use specforge_common::shape::Shape;

use crate::tool::Category;

/// One thing an MCP server did, as `spec/events/` declares it: its name is
/// the variant's (snake_case, the serialized `event` tag), its payload the
/// variant's fields under the spec's names. The `timestamp` every payload
/// declares is the keeping sink's, not the producer's.
/// `tests/event_spec.rs` holds each variant's derived schema to its spec
/// payload.
#[derive(Debug, Clone, PartialEq, Serialize, Shape)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum McpEvent {
    /// A listing answered (`tools/list`, `resources/list`,
    /// `resources/templates/list`, `prompts/list`), and how many entries it
    /// gave.
    #[serde(rename_all = "camelCase")]
    McpDiscoveryInvoked {
        discovery_type: Listing,
        result_count: usize,
    },
    /// A resource read that returned content; `format` is its MIME type.
    #[serde(rename_all = "camelCase")]
    McpResourceRead {
        resource_uri: String,
        format: String,
    },
    /// A tool found by name, before its target resolves: its listed
    /// category, the `entity_id` argument when it is a string, and every
    /// argument as JSON text.
    #[serde(rename_all = "camelCase")]
    McpToolInvoked {
        tool_name: String,
        category: Category,
        #[serde(skip_serializing_if = "Option::is_none")]
        entity_id: Option<String>,
        params: String,
    },
    /// A delta queued for a handshake subscriber
    /// ([`McpEvent::graph_delta`], [`McpEvent::diagnostics_delta`]).
    #[serde(rename_all = "camelCase")]
    McpDeltaNotified {
        notification_type: DeltaKind,
        subscriber_count: usize,
        #[serde(skip_serializing_if = "Option::is_none")]
        added_nodes: Option<usize>,
        #[serde(skip_serializing_if = "Option::is_none")]
        removed_nodes: Option<usize>,
        #[serde(skip_serializing_if = "Option::is_none")]
        modified_nodes: Option<usize>,
        #[serde(skip_serializing_if = "Option::is_none")]
        added_diagnostics: Option<usize>,
        #[serde(skip_serializing_if = "Option::is_none")]
        removed_diagnostics: Option<usize>,
    },
    /// A prompt found by name, with its `entity_id` and `kind` arguments
    /// when given.
    #[serde(rename_all = "camelCase")]
    McpPromptInvoked {
        prompt_name: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        entity_id: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        kind: Option<String>,
    },
    /// `initialize` answered: what the server registered (core plus the
    /// served project's extension surfaces).
    McpInitialized {
        tools_registered: usize,
        resources_registered: usize,
        prompts_registered: usize,
        extensions_loaded: usize,
        surface_tools_registered: usize,
        surface_resources_registered: usize,
        auto_promoted_tools: usize,
    },
    /// A mutation call finished (not a preview): the files it wrote, the
    /// entities it changed, and whether the client got a success.
    McpMutationCompleted {
        #[serde(rename = "toolName")]
        tool_name: String,
        files_changed: usize,
        entities_affected: usize,
        success: bool,
    },
    /// A subscription made: `resources/subscribe`, or one resource a listen
    /// stream names (`subscription_id`: the listen request's id).
    #[serde(rename_all = "camelCase")]
    McpSubscriptionCreated {
        resource_uri: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        subscription_id: Option<String>,
    },
    /// A subscription ended: unsubscribe, cancel, disconnect or shutdown.
    #[serde(rename_all = "camelCase")]
    McpSubscriptionRemoved {
        resource_uri: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        subscription_id: Option<String>,
    },
    /// `initialize` refused.
    McpInitializationFailed {
        error_kind: InitFailure,
        message: String,
    },
    /// A JSON-RPC error answered, and the method it answered when one was
    /// routed.
    #[serde(rename_all = "camelCase")]
    McpProtocolErrorHandled {
        error_code: i64,
        error_message: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        request_method: Option<String>,
    },
    /// A cancel acknowledged (requests run one at a time: never in
    /// progress).
    #[serde(rename_all = "camelCase")]
    McpRequestCancelled {
        request_id: String,
        was_in_progress: bool,
    },
    /// `shutdown` answered.
    McpServerShutdown {
        pending_notifications_flushed: usize,
        subscriptions_released: usize,
        wasm_engines_released: usize,
    },
    /// The extension surface table was built and some extension declares
    /// commands.
    #[serde(rename_all = "camelCase")]
    CommandsAutoPromoted {
        promoted_count: usize,
        conflict_count: usize,
    },
    /// An auto-promoted command's `cmd__` export returned.
    #[serde(rename_all = "camelCase")]
    SurfaceCommandDispatched {
        extension_name: String,
        command_id: String,
        exit_code: i32,
        duration_ms: u64,
    },
    /// An explicit extension tool's `mcp__` export returned.
    #[serde(rename_all = "camelCase")]
    SurfaceMcpToolDispatched {
        extension_name: String,
        tool_name: String,
        duration_ms: u64,
        success: bool,
    },
    /// An extension resource's `mcp__` export answered its content.
    #[serde(rename_all = "camelCase")]
    SurfaceMcpResourceDispatched {
        extension_name: String,
        uri_template: String,
        mime_type: String,
        duration_ms: u64,
    },
    /// A domain event of `specforge.add_extension` (one per specifier).
    ExtensionAdded(ExtensionAdded),
    /// A domain event of `specforge.init`.
    ProjectInitialized(ProjectInitialized),
}

/// `extension_added` (`spec/events/compilation.spec`).
#[derive(Debug, Clone, PartialEq, Serialize, Shape)]
#[serde(rename_all = "camelCase")]
pub struct ExtensionAdded {
    /// The specifier as the call gave it.
    pub extension_specifier: String,
    /// How many extensions `specforge.json` enables after the add.
    pub total_extensions: usize,
    /// Already installed or enabled, or named by an earlier specifier of
    /// the same add.
    pub was_duplicate: bool,
}

/// `project_initialized` (`spec/events/compilation.spec`).
#[derive(Debug, Clone, PartialEq, Serialize, Shape)]
#[serde(rename_all = "camelCase")]
pub struct ProjectInitialized {
    pub project_name: String,
    pub extension_count: usize,
    /// The starter entity spec file, relative to the new root (not
    /// `specforge.json`).
    pub spec_file_path: String,
}

/// Which listing a discovery answered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Shape)]
#[serde(rename_all = "snake_case")]
pub enum Listing {
    Tools,
    Resources,
    ResourceTemplates,
    Prompts,
}

/// Which delta a subscriber was sent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Shape)]
#[serde(rename_all = "snake_case")]
pub enum DeltaKind {
    Graph,
    Diagnostics,
}

/// Why `initialize` refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Shape)]
#[serde(rename_all = "snake_case")]
pub enum InitFailure {
    AlreadyInitialized,
}

impl McpEvent {
    /// `mcp_delta_notified` for a graph delta sent to the one handshake
    /// subscriber.
    pub(crate) fn graph_delta(delta: &specforge_project::GraphDelta) -> Self {
        McpEvent::McpDeltaNotified {
            notification_type: DeltaKind::Graph,
            subscriber_count: 1,
            added_nodes: Some(delta.added_nodes.len()),
            removed_nodes: Some(delta.removed_nodes.len()),
            modified_nodes: Some(delta.modified_nodes.len()),
            added_diagnostics: None,
            removed_diagnostics: None,
        }
    }

    /// `mcp_delta_notified` for a diagnostics delta of `added` and
    /// `removed` diagnostics.
    pub(crate) fn diagnostics_delta(added: usize, removed: usize) -> Self {
        McpEvent::McpDeltaNotified {
            notification_type: DeltaKind::Diagnostics,
            subscriber_count: 1,
            added_nodes: None,
            removed_nodes: None,
            modified_nodes: None,
            added_diagnostics: Some(added),
            removed_diagnostics: Some(removed),
        }
    }

    /// `mcp_subscription_created`, `stream` the listen request's id when on
    /// a stream.
    pub(crate) fn subscription_created(uri: &str, stream: Option<&serde_json::Value>) -> Self {
        McpEvent::McpSubscriptionCreated {
            resource_uri: uri.to_string(),
            subscription_id: stream.map(crate::protocol::id_text),
        }
    }

    /// `mcp_subscription_removed`, likewise.
    pub(crate) fn subscription_removed(uri: &str, stream: Option<&serde_json::Value>) -> Self {
        McpEvent::McpSubscriptionRemoved {
            resource_uri: uri.to_string(),
            subscription_id: stream.map(crate::protocol::id_text),
        }
    }
}

/// Where a server's events go: the one seam every module records through.
/// Two adapters: [`Discard`] (what a host runs: keeps nothing) and
/// [`crate::testing::EventLog`] (a test's: keeps each event, stamped, in order).
pub trait EventSink: Send {
    /// Take `event`, which happened now.
    fn record(&mut self, event: McpEvent);
}

/// Keeps nothing: what a production server records costs it no memory, and
/// no event leaves the process (ADR 0022 D7).
#[derive(Debug, Default, Clone, Copy)]
pub struct Discard;

impl EventSink for Discard {
    fn record(&mut self, _: McpEvent) {}
}
