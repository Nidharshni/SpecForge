// MCP Server events — signals emitted during MCP protocol interactions

use "types/diagnostics"
use "types/graph"
use "types/mcp"
use "types/output"

// MCP protocol treats tool/prompt/resource listing as tool-like operations.
// This event tracks discovery requests separately from regular tool invocations
// to distinguish capability negotiation from actual tool usage.
event mcp_discovery_invoked "MCP Discovery Invoked" {
  channel "mcp.discovery_invoked"
  payload {
    discoveryType string
    resultCount   integer
    timestamp     timestamp
  }
  verify integration "emits mcp_discovery_invoked with correct discoveryType when agent lists tools, prompts, or resources"
}

event mcp_resource_read "MCP Resource Read" {
  channel "mcp.resource_read"
  payload {
    resourceUri string
    format      string
    timestamp   timestamp
  }
  verify integration "emits mcp_resource_read with correct resourceUri when agent reads any MCP resource"
}

event mcp_tool_invoked "MCP Tool Invoked" {
  channel "mcp.tool_invoked"
  payload {
    toolName  string
    category  McpToolCategory
    entityId  string @optional
    params    string @optional
    timestamp timestamp
  }
  verify integration "emits mcp_tool_invoked with correct toolName, category, and parameters for any tool call"
}

event mcp_delta_notified "MCP Delta Notified" {
  channel "mcp.delta_notified"
  payload {
    notificationType   string
    subscriberCount    integer
    addedNodes         integer @optional
    removedNodes       integer @optional
    modifiedNodes      integer @optional
    addedDiagnostics   integer @optional
    removedDiagnostics integer @optional
    timestamp          timestamp
  }
  verify integration "emits mcp_delta_notified with correct notification type and delta summary"
}

event mcp_prompt_invoked "MCP Prompt Invoked" {
  channel "mcp.prompt_invoked"
  payload {
    promptName string
    entityId   string @optional
    kind       string @optional
    timestamp  timestamp
  }
  verify integration "emits mcp_prompt_invoked with correct promptName and arguments"
}

event mcp_initialized "MCP Initialized" {
  channel "mcp.initialized"
  payload {
    tools_registered             integer
    resources_registered         integer
    prompts_registered           integer
    extensions_loaded            integer
    surface_tools_registered     integer @optional
    surface_resources_registered integer @optional
    auto_promoted_tools          integer @optional
  }
  verify integration "mcp initialization emits event with tool counts"
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
  // answered.
  channel "mcp.mutation_completed"
  payload {
    toolName          string
    files_changed     integer
    entities_affected integer
    success           boolean
    timestamp         timestamp
  }
  verify integration "emits mcp_mutation_completed with structured outcome after each mutation tool"
  verify integration "files_changed is the number of files the call wrote, for every mutation tool"
  verify integration "a mutation that fails after writing reports the files it wrote"
  verify integration "a mutation's reply lists in files_written the files files_changed counts"
  verify integration "a call whose arguments ask for a preview records nothing and lists no files_written, whatever its operation answered"
}

// ── MCP Subscription Lifecycle Events ────────────────────────

event mcp_subscription_created "MCP Subscription Created" {
  channel "mcp.subscription_created"
  payload {
    resourceUri    string
    subscriptionId string @optional
    timestamp      timestamp
  }
  verify integration "emits mcp_subscription_created for each resource a client subscribes to or a listen stream names"
}

event mcp_subscription_removed "MCP Subscription Removed" {
  channel "mcp.subscription_removed"
  payload {
    resourceUri    string
    subscriptionId string @optional
    timestamp      timestamp
  }
  verify integration "emits mcp_subscription_removed for each resource whose subscription ends: unsubscribe, cancel, disconnect or shutdown"
}

event mcp_initialization_failed "MCP Initialization Failed" {
  channel "mcp.initialization_failed"
  payload {
    error_kind string
    message    string
    timestamp  timestamp
  }
  verify integration "emits mcp_initialization_failed when MCP server fails to initialize"
}

event mcp_protocol_error_handled "MCP Protocol Error Handled" {
  channel "mcp.protocol_error_handled"
  payload {
    errorCode    integer
    errorMessage string
    method       string @optional
    timestamp    timestamp
  }
  verify integration "emits mcp_protocol_error_handled with correct errorCode for each error type"
}

event mcp_request_cancelled "MCP Request Cancelled" {
  channel "mcp.request_cancelled"
  payload {
    requestId     string
    wasInProgress boolean
    timestamp     timestamp
  }
  verify integration "emits mcp_request_cancelled with correct requestId and wasInProgress flag"
}

event mcp_server_shutdown "MCP Server Shutdown" {
  channel "mcp.shutdown"
  payload {
    pending_notifications_flushed integer
    subscriptions_released        integer
    wasm_engines_released         integer
    timestamp                     timestamp
  }
  verify integration "emits mcp_server_shutdown with correct counts when MCP server shuts down"
}
