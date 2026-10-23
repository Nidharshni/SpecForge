// MCP Server events — what an MCP server records it did. Each is one variant of
// specforge_mcp::events::McpEvent, recorded through the server's event sink: a
// production server keeps none (events never leave the process), a test's sink
// keeps each one stamped with the time it was recorded (timestamp).

use "types/diagnostics"
use "types/graph"
use "types/mcp"
use "types/output"

// MCP protocol treats tool/prompt/resource listing as tool-like operations.
// This event tracks discovery requests separately from regular tool invocations
// to distinguish capability negotiation from actual tool usage.
event mcp_discovery_invoked "MCP Discovery Invoked" {
  channel "mcp.discovery_invoked"
  payload McpDiscoveryInvokedPayload
  verify integration "emits mcp_discovery_invoked with correct discoveryType when agent lists tools, prompts, or resources"
}

type McpDiscoveryInvokedPayload "Mcp Discovery Invoked Payload" {
  discoveryType string
  resultCount   integer
  timestamp     timestamp
  verify unit "McpDiscoveryInvokedPayload schema is valid"
}

event mcp_resource_read "MCP Resource Read" {
  channel "mcp.resource_read"
  payload McpResourceReadPayload
  verify integration "emits mcp_resource_read with correct resourceUri when agent reads any MCP resource"
}

type McpResourceReadPayload "Mcp Resource Read Payload" {
  resourceUri string
  format      string
  timestamp   timestamp
  verify unit "McpResourceReadPayload schema is valid"
}

event mcp_tool_invoked "MCP Tool Invoked" {
  channel "mcp.tool_invoked"
  payload McpToolInvokedPayload
  verify integration "emits mcp_tool_invoked with correct toolName, category, and parameters for any tool call"
}

type McpToolInvokedPayload "Mcp Tool Invoked Payload" {
  toolName  string
  category  McpToolCategory
  entityId  string @optional
  params    string
  timestamp timestamp
  verify unit "McpToolInvokedPayload schema is valid"
}

event mcp_delta_notified "MCP Delta Notified" {
  channel "mcp.delta_notified"
  payload McpDeltaNotifiedPayload
  verify integration "emits mcp_delta_notified with correct notification type and delta summary"
}

type McpDeltaNotifiedPayload "Mcp Delta Notified Payload" {
  notificationType   string
  subscriberCount    integer
  addedNodes         integer @optional
  removedNodes       integer @optional
  modifiedNodes      integer @optional
  addedDiagnostics   integer @optional
  removedDiagnostics integer @optional
  timestamp          timestamp
  verify unit "McpDeltaNotifiedPayload schema is valid"
}

event mcp_prompt_invoked "MCP Prompt Invoked" {
  channel "mcp.prompt_invoked"
  payload McpPromptInvokedPayload
  verify integration "emits mcp_prompt_invoked with correct promptName and arguments"
}

type McpPromptInvokedPayload "Mcp Prompt Invoked Payload" {
  promptName string
  entityId   string @optional
  kind       string @optional
  timestamp  timestamp
  verify unit "McpPromptInvokedPayload schema is valid"
}

event mcp_initialized "MCP Initialized" {
  channel "mcp.initialized"
  payload McpInitializedPayload
  verify integration "mcp initialization emits event with tool counts"
}

type McpInitializedPayload "Mcp Initialized Payload" {
  tools_registered             integer
  resources_registered         integer
  prompts_registered           integer
  extensions_loaded            integer
  surface_tools_registered     integer
  surface_resources_registered integer
  auto_promoted_tools          integer
  timestamp                    timestamp
  verify unit "McpInitializedPayload schema is valid"
}

event mcp_mutation_completed "MCP Mutation Completed" {
  // files_changed: the files the call created, rewrote or removed, as its
  // operation recorded them where it wrote — a failed call's partial writes
  // included, a migration's backups included, each file a removal deleted
  // named on its own. The reply names the same files in files_written
  // (relative to the project root), so a client sees what the event
  // counts. entities_affected: the entities the call changed (the renamed
  // one, the ones a removal strands, the ones an inference step produced).
  // A preview is no mutation and emits nothing: a call whose arguments ask
  // for one (dry_run; check or diff without write), whatever its operation
  // answered, and whatever refused it once its arguments read (no project
  // served, a path naming none).
  channel "mcp.mutation_completed"
  payload McpMutationCompletedPayload
  verify integration "emits mcp_mutation_completed with structured outcome after each mutation tool"
  verify integration "files_changed is the number of files the call wrote, for every mutation tool"
  verify integration "a mutation that fails after writing reports the files it wrote"
  verify integration "a mutation's reply lists in files_written the files files_changed counts"
  verify integration "a call whose arguments ask for a preview records nothing and lists no files_written, whatever its operation answered"
  verify integration "a preview refused before its handler runs, for no project or a path naming none, lists no files_written and records nothing"
}

type McpMutationCompletedPayload "Mcp Mutation Completed Payload" {
  toolName          string
  files_changed     integer
  entities_affected integer
  success           boolean
  timestamp         timestamp
  verify unit "McpMutationCompletedPayload schema is valid"
}

// ── MCP Subscription Lifecycle Events ────────────────────────

event mcp_subscription_created "MCP Subscription Created" {
  channel "mcp.subscription_created"
  payload McpSubscriptionCreatedPayload
  verify integration "emits mcp_subscription_created for each resource a client subscribes to or a listen stream names"
}

type McpSubscriptionCreatedPayload "Mcp Subscription Created Payload" {
  resourceUri    string
  subscriptionId string @optional
  timestamp      timestamp
  verify unit "McpSubscriptionCreatedPayload schema is valid"
}

event mcp_subscription_removed "MCP Subscription Removed" {
  channel "mcp.subscription_removed"
  payload McpSubscriptionRemovedPayload
  verify integration "emits mcp_subscription_removed for each resource whose subscription ends: unsubscribe, cancel, disconnect or shutdown"
}

type McpSubscriptionRemovedPayload "Mcp Subscription Removed Payload" {
  resourceUri    string
  subscriptionId string @optional
  timestamp      timestamp
  verify unit "McpSubscriptionRemovedPayload schema is valid"
}

event mcp_initialization_failed "MCP Initialization Failed" {
  channel "mcp.initialization_failed"
  payload McpInitializationFailedPayload
  verify integration "emits mcp_initialization_failed when MCP server fails to initialize"
}

type McpInitializationFailedPayload "Mcp Initialization Failed Payload" {
  error_kind string
  message    string
  timestamp  timestamp
  verify unit "McpInitializationFailedPayload schema is valid"
}

event mcp_protocol_error_handled "MCP Protocol Error Handled" {
  channel "mcp.protocol_error_handled"
  payload McpProtocolErrorHandledPayload
  verify integration "emits mcp_protocol_error_handled with correct errorCode for each error type"
}

type McpProtocolErrorHandledPayload "Mcp Protocol Error Handled Payload" {
  errorCode     integer
  errorMessage  string
  requestMethod string @optional
  timestamp     timestamp
  verify unit "McpProtocolErrorHandledPayload schema is valid"
}

event mcp_request_cancelled "MCP Request Cancelled" {
  channel "mcp.request_cancelled"
  payload McpRequestCancelledPayload
  verify integration "emits mcp_request_cancelled with correct requestId and wasInProgress flag"
}

type McpRequestCancelledPayload "Mcp Request Cancelled Payload" {
  requestId     string
  wasInProgress boolean
  timestamp     timestamp
  verify unit "McpRequestCancelledPayload schema is valid"
}

event mcp_server_shutdown "MCP Server Shutdown" {
  channel "mcp.shutdown"
  payload McpServerShutdownPayload
  verify integration "emits mcp_server_shutdown with correct counts when MCP server shuts down"
}

type McpServerShutdownPayload "Mcp Server Shutdown Payload" {
  pending_notifications_flushed integer
  subscriptions_released        integer
  wasm_engines_released         integer
  timestamp                     timestamp
  verify unit "McpServerShutdownPayload schema is valid"
}
