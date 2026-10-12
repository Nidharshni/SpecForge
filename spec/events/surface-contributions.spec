// Surface contribution events — what MCP records about extension surfaces.
// The registry build that registers and checks surfaces is pure: its
// diagnostics are its record, and it emits no event (ADR 0011).

use "types/mcp"
use "types/surface"

event commands_auto_promoted "Commands Auto-Promoted" {
  channel "surface.commands_auto_promoted"
  payload {
    promotedCount integer
    conflictCount integer
    timestamp     timestamp
  }
  verify integration "emits commands_auto_promoted with correct promoted and conflict counts"
}

event surface_command_dispatched "Surface Command Dispatched" {
  channel "surface.command_dispatched"
  payload {
    extensionName string
    commandId     string
    exitCode      integer
    durationMs    integer
    timestamp     timestamp
  }
  verify integration "emits surface_command_dispatched with correct commandId and exitCode"
}

event surface_mcp_tool_dispatched "Surface MCP Tool Dispatched" {
  channel "surface.mcp_tool_dispatched"
  payload {
    extensionName string
    toolName      string
    durationMs    integer
    success       boolean
    timestamp     timestamp
  }
  verify integration "emits surface_mcp_tool_dispatched with correct toolName and success"
}

event surface_mcp_resource_dispatched "Surface MCP Resource Dispatched" {
  channel "surface.mcp_resource_dispatched"
  payload {
    extensionName string
    uriTemplate   string
    mimeType      string @optional
    durationMs    integer
    timestamp     timestamp
  }
  verify integration "emits surface_mcp_resource_dispatched with correct uriTemplate"
}
