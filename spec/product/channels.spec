// Channels — interaction mediums for SpecForge
//
// First-class entity kinds declared by @specforge/product.
// Referenced by journeys via the channels field (JourneyUsesChannel edge).

channel cli "Command-Line Interface" {
  description       "The specforge CLI binary. Primary interaction surface for developers and CI pipelines. Supports init, check, export, format, watch, trace, stats, and extension management commands."
  interaction_model request_response
  status            active
  tags              ["primary", "interactive"]
}

channel ide "IDE / Editor" {
  description       "Editor integration via the specforge-lsp Language Server Protocol server. Provides go-to-definition, hover, autocomplete, rename, live diagnostics, semantic tokens, code actions, and outline navigation."
  interaction_model bidirectional
  status            active
  tags              ["primary", "interactive"]
}

channel ci_surface "CI/CD Surface" {
  description       "CI pipeline integration surface. Uses the CLI with --strict and --check flags, machine-readable JSON output, and exit codes for pipeline gating. Not interactive."
  interaction_model batch
  status            active
  tags              ["automation"]
}

channel ch_graph_protocol "Graph Protocol" {
  description       "The JSON graph output consumed by external tools, dashboards, and agents that don't use MCP. Direct file-based or piped export consumption."
  interaction_model batch
  status            active
  tags              ["agent_facing"]
}

channel mcp "MCP Server" {
  description       "Model Context Protocol server for AI agent integration. Exposes graph resources, query/mutation/navigation tools, delta notifications, and guided prompts. Agents connect without CLI invocation."
  interaction_model bidirectional
  status            active
  tags              ["primary", "agent_facing"]
}
