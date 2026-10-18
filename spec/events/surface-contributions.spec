// Surface contribution events — what MCP records about extension surfaces,
// through its event sink like every MCP event (spec/events/mcp.spec). The
// registry build that registers and checks surfaces is pure: its diagnostics
// are its record, and it emits no event (ADR 0011).

use "types/mcp"
use "types/surface"

event commands_auto_promoted "Commands Auto-Promoted" {
  channel "surface.commands_auto_promoted"
  payload CommandsAutoPromotedPayload
  verify integration "emits commands_auto_promoted with correct promoted and conflict counts"
}

type CommandsAutoPromotedPayload "Commands Auto Promoted Payload" {
  promotedCount integer
  conflictCount integer
  timestamp     timestamp
  verify unit "CommandsAutoPromotedPayload schema is valid"
}

event surface_command_dispatched "Surface Command Dispatched" {
  channel "surface.command_dispatched"
  payload SurfaceCommandDispatchedPayload
  verify integration "emits surface_command_dispatched with correct commandId and exitCode"
}

type SurfaceCommandDispatchedPayload "Surface Command Dispatched Payload" {
  extensionName string
  commandId     string
  exitCode      integer
  durationMs    integer
  timestamp     timestamp
  verify unit "SurfaceCommandDispatchedPayload schema is valid"
}

event surface_mcp_tool_dispatched "Surface MCP Tool Dispatched" {
  channel "surface.mcp_tool_dispatched"
  payload SurfaceMcpToolDispatchedPayload
  verify integration "emits surface_mcp_tool_dispatched with correct toolName and success"
}

type SurfaceMcpToolDispatchedPayload "Surface Mcp Tool Dispatched Payload" {
  extensionName string
  toolName      string
  durationMs    integer
  success       boolean
  timestamp     timestamp
  verify unit "SurfaceMcpToolDispatchedPayload schema is valid"
}

event surface_mcp_resource_dispatched "Surface MCP Resource Dispatched" {
  channel "surface.mcp_resource_dispatched"
  payload SurfaceMcpResourceDispatchedPayload
  verify integration "emits surface_mcp_resource_dispatched with correct uriTemplate"
}

type SurfaceMcpResourceDispatchedPayload "Surface Mcp Resource Dispatched Payload" {
  extensionName string
  uriTemplate   string
  mimeType      string
  durationMs    integer
  timestamp     timestamp
  verify unit "SurfaceMcpResourceDispatchedPayload schema is valid"
}
