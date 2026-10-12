// MCP Server behaviors — Lifecycle, resources, and notifications
//
// 13 behaviors:
//   - Lifecycle (3): initialize, shutdown, list resources/tools/prompts
//   - Resources (6): graph, schema, context, brief, diagnostics, entity
//   - Notifications (2): graph delta, diagnostics delta

use "events/compilation"
use "events/mcp"
use "invariants/core"
use "invariants/mcp"
use "invariants/validation"
use "invariants/zero-entity-core"
use "ports/inbound"
use "ports/outbound"
use "types/config"
use "types/core"
use "types/diagnostics"
use "types/graph"
use "types/mcp"
use "types/output"

behavior mcp_initialize "MCP Initialize" {
  features   [mcp_lifecycle]
  types      [McpCapabilities, CompilerConfig]
  category   command
  ports      [CompilerApi, WasmRuntime]
  produces   [mcp_initialized, mcp_initialization_failed]
  invariants [
    zero_domain_knowledge_core,
    mcp_structured_error_responses,
    registry_population_before_validation,
    mcp_served_project_consistency,
    mcp_event_record,
  ]
  requires {
    compiler_api_available "CompilerApi port is available and project root has been located"
    wasm_runtime_available "WasmRuntime port is available for loading extension manifests"
  }
  ensures {
    capabilities_returned        "McpCapabilities returned containing all registered tools, resources, and prompts"
    surface_contributions_merged "Surface-contributed tools and resources merged with core before advertisement"
    mcp_initialized_emitted      "mcp_initialized event emitted on success, mcp_initialization_failed on failure"
  }
  contract   """
    The MCP server MUST serve the project at or above its root (the
    client's projectRoot, else the path it was started with: the nearest
    directory holding specforge.json or specforge.spec), or nothing when
    there is none. It MUST compile that project's graph, load all
    extension manifests, register all tools/resources/prompts from
    installed extensions (including surface-contributed MCP tools and
    resources from extension manifests), and return McpCapabilities.
    Surface-contributed tools and resources MUST be merged with core
    tools and resources before capability advertisement. The server
    MUST NOT accept tool/resource calls before initialization completes.
    The server MUST negotiate the protocol revision: it answers with the
    version the client requested when it supports that version
    (2025-11-25, 2025-06-18 or 2025-03-26), and with the latest one it
    supports (2025-11-25) otherwise.
  """
  verify unit "answers with the client's protocol version when it supports it"
  verify unit "answers an unsupported or missing protocol version with 2025-11-25"
  verify unit "initialization registers all tools from installed extensions"
  verify unit "initialization rejects tool calls before completion"
  verify unit "all core tools registered before accepting requests"
  verify unit "all core resources registered before accepting requests"
  verify unit "returns MCP-compliant init response"
  verify unit "compiles project when projectRoot is provided"
  verify contract "MCP Initialize: MCP initialization holds — compiler_api_available, wasm_runtime_available, capabilities_returned, surface_contributions_merged, mcp_initialized_emitted"
}

behavior mcp_shutdown "MCP Shutdown" {
  features   [mcp_lifecycle]
  types      [McpSubscription, timestamp]
  category   command
  ports      [CompilerApi, WasmRuntime]
  produces   [mcp_server_shutdown, mcp_subscription_removed]
  invariants [mcp_subscription_cleanup, mcp_structured_error_responses]
  requires {
    server_initialized "MCP server has been initialized (mcp_initialized has fired)"
  }
  ensures {
    notifications_flushed "All pending notifications flushed before exit"
    subscriptions_removed "Every subscription and listen stream ends, an mcp_subscription_removed emitted for each resource"
    wasm_engines_released "No Wasm engine instance outlives shutdown (the served project's runtime is released with its session)"
    shutdown_emitted      "mcp_server_shutdown event emitted"
  }
  contract   """
    The MCP server MUST flush all pending notifications, unsubscribe
    all active subscriptions, release Wasm engine instances, and exit
    cleanly.
  """
  verify unit "shutdown flushes pending notifications"
  verify unit "shutdown unsubscribes all active subscriptions"
  verify unit "shutdown ends every listen stream and records its removal"
  verify unit "shutdown rejects new tool calls during teardown"
  verify integration "shutdown completes within 5 seconds"
  verify contract "MCP Shutdown: MCP shutdown holds — server_initialized, notifications_flushed, subscriptions_removed, wasm_engines_released, shutdown_emitted"
}

behavior list_mcp_resources "List MCP Resources" {
  features   [mcp_discovery]
  invariants [mcp_structured_error_responses, mcp_tool_idempotency]
  category   query
  ports      [McpProtocol, CompilerApi]
  types      [McpResourceDescriptor]
  produces   [mcp_discovery_invoked]
  requires {
    server_initialized "MCP server has been initialized and all extensions loaded"
  }
  ensures {
    complete_list_returned "All registered resource descriptors returned including extension-contributed"
    discovery_emitted      "mcp_discovery_invoked event emitted"
  }
  contract   """
    The MCP server MUST return all registered resource descriptors,
    including both core-provided and extension-contributed capabilities.
    Extension-contributed MCP resources (from manifest surfaces.mcp_resources)
    MUST be included alongside core resources. The list MUST be complete
    and reflect the current set of loaded extensions. Every core resource it lists MUST be readable.
    A resource whose URI is a template (it holds a {placeholder}) MUST be
    listed by resources/templates/list as a resource template, not by
    resources/list.
  """
  verify unit "templated resources are listed by resources/templates/list, not resources/list"
  verify unit "returns all registered resource descriptors after extension load"
  verify unit "returns core-provided descriptors when no extensions installed"
  verify unit "reflects resources from newly loaded extension"
  verify contract "List MCP Resources: listing MCP resources holds — server_initialized, complete_list_returned, discovery_emitted"
  verify unit "every listed core resource is readable"
}

behavior list_mcp_tools "List MCP Tools" {
  features   [mcp_discovery]
  invariants [mcp_structured_error_responses, mcp_tool_idempotency]
  category   query
  ports      [McpProtocol, CompilerApi]
  types      [McpToolDescriptor, McpToolCategory, McpToolGroup, McpToolAnnotations]
  produces   [mcp_discovery_invoked]
  requires {
    server_initialized "MCP server has been initialized and all extensions loaded"
  }
  ensures {
    complete_list_returned "All registered tool descriptors returned including auto-promoted CLI commands"
    discovery_emitted      "mcp_discovery_invoked event emitted"
    listed_once            "Every tool listed is the one tools/call dispatches under that name, each name once: core tools first, then explicit extension tools in extension load order, then auto-promoted commands; a contribution not served under its name is reported with I017 saying why"
  }
  contract   """
    The MCP server MUST return all registered tool descriptors,
    including both core-provided and extension-contributed capabilities.
    Extension-contributed MCP tools (from manifest surfaces.mcp_tools)
    and auto-promoted CLI commands MUST be included alongside core tools.
    The list MUST be complete and reflect the current set of loaded
    extensions. Every core tool it lists MUST be callable: a call never
    fails as an unknown tool or operation. Each core tool's inputSchema MUST be derived from the
    typed arguments its handler reads (read_mcp_arguments_as_declared):
    exactly those arguments and its target's listed ones, with no other
    property allowed. Each listed tool's category is its role,
    one of McpToolCategory, and its source says where it comes from: core,
    or the contributing extension's name. An extension tool is listed once,
    however often the project recompiles, with the output_schema it
    declares as its outputSchema.
    Each core tool's category and MCP annotations derive from the one
    declaration of what it does: a tool that only reads is readOnlyHint and
    is listed in its group (core, navigation or management); a tool that
    writes output artifacts is listed in its group, and a tool that writes
    its target's project files is a mutation whose outputSchema declares
    files_written (and, for rename, the diagnostics it returns after the
    write): fields the mutation pipeline fills, which no tool reply
    declares itself; both say whether they are destructive (they may
    overwrite or remove), idempotent (a repeat changes nothing) and open
    world. An extension tool is annotated readOnlyHint and not
    openWorldHint, since the host grants an extension no capability, and is
    listed in the group it declares, core when it declares none or another
    name: never as a mutation.
    Each core tool is one module: its name, description, the declaration
    of what it does, the spec type its reply is, its typed arguments, what
    its handler is given (its call target) and its typed reply are stated
    there once, and everything listed and dispatched for it is derived
    from them; a declaration that contradicts its handler (a mutation whose
    handler answers no mutation, a spec type for a text reply) does not
    build.
  """
  verify unit "returns all registered tool descriptors after extension load"
  verify unit "returns core-provided descriptors when no extensions installed"
  verify unit "reflects tools from newly loaded extension"
  verify contract "List MCP Tools: listing MCP tools holds — server_initialized, complete_list_returned, discovery_emitted, listed_once"
  verify unit "tools have categories"
  verify unit "every listed core tool dispatches to its handler"
  verify unit "each core tool's input schema advertises exactly the arguments its handler reads"
  verify unit "every listed tool has a spec category and a source"
  verify unit "core tools are annotated: read-only tools readOnlyHint, writing tools how they write"
  verify unit "a mutation's outputSchema declares files_written, derived from its effect"
  verify unit "what the pipeline adds to a mutation's reply is declared once, beside its reply: files_written, and rename's diagnostics"
  verify unit "a writing tool's hints say what it does: one that overwrites is destructive, one whose repeat changes nothing is idempotent"
  verify unit "an extension tool is annotated read-only and is never listed as a mutation"
  verify unit "an extension tool is listed once across recompiles"
  verify unit "an extension tool's declared output_schema is listed as its outputSchema"
  verify unit "a tool's path and use_cached are declared once, by its target"
  verify unit "a core tool's spec is derived from its one module: its name, description, effect, arguments, target and reply"
  verify unit "every listed extension tool is the one dispatched under its name, listed once"
}

// A tool's or prompt's arguments are one typed definition (ADR 0033): its
// listing and its reading derive from it, by one argument rule.
behavior read_mcp_arguments_as_declared "Read MCP Arguments as Declared" {
  features   [mcp_core_tools, mcp_prompts]
  invariants [mcp_structured_error_responses, dry_run_side_effect_freedom]
  category   query
  types      [McpToolDescriptor, McpPromptArgument, McpError]
  ports      [McpProtocol]
  requires {
    arguments_declared "Each core tool and prompt reads its arguments into one typed definition"
  }
  ensures {
    listing_is_the_reading "The input schema or prompt listing states each argument's type, description, default, enumerated values and whether it is required, as the reading applies them"
    absent_is_the_default  "An absent or null argument reads as the default its listing states; a missing required one is refused naming it"
    value_read_by_type     "A value is read by its declared type, a boolean or count also from a string holding one, as extension command tools read them; any other value is invalid input naming the argument"
    undeclared_refused     "An argument neither the entry nor its call target declares is invalid input naming it, with the declared argument it is close to"
  }
  contract   """
    Every core MCP tool and prompt MUST read its call's arguments into one
    typed definition from which its listing is derived: a tool's inputSchema
    (each property's type, description, default, enum, minimum and whether
    it is required, then its target's path and use_cached, with no other
    property allowed) and a prompt's listed arguments (name, description,
    required). An absent or null argument MUST read as the default the
    listing states, an option as none, a list as empty; a required argument
    that is absent MUST be refused as "Missing required parameter: <name>".
    A value MUST be read by its declared type: a boolean is true or false or
    the string "true" or "false", a count a non-negative integer or a string
    holding one, a string a string, a list a list of strings; an enumerated
    argument by its option table (name_enumerated_options_once). Any other
    value MUST be refused as invalid input naming the argument, with the
    wording extension command tools use. An argument that neither the entry
    nor its call target declares MUST be refused as invalid input naming it,
    with the closest declared name as a suggestion. An argument whose
    default depends on others (specforge.format's write) MUST state no
    default; its description states the rule.
  """
  verify unit "a listing is derived from the typed arguments: each argument's type, description, default, enumerated values and whether it is required"
  verify unit "an absent or null argument reads as the default the listing advertises, and a missing required one is refused naming it"
  verify unit "a boolean or count sent as a string is read as one, as an extension command reads it; any other value of the wrong type is refused naming the argument"
  verify unit "an argument neither the tool nor its target declares is refused naming it, with the declared one it is close to"
  verify unit "a prompt reads its arguments by the same rule its listing states"
}

behavior list_mcp_prompts "List MCP Prompts" {
  features   [mcp_discovery]
  invariants [mcp_structured_error_responses, mcp_tool_idempotency]
  category   query
  ports      [McpProtocol, CompilerApi]
  types      [McpPromptDescriptor, McpPromptArgument]
  produces   [mcp_discovery_invoked]
  requires {
    server_initialized "MCP server has been initialized and all extensions loaded"
  }
  ensures {
    complete_list_returned "Every core prompt's descriptor returned, derived from its Prompt spec"
    discovery_emitted      "mcp_discovery_invoked event emitted"
  }
  contract   """
    The MCP server MUST return the descriptor of every core prompt, derived
    from its Prompt spec (serve_mcp_prompt). No extension contributes a
    prompt: an extension's surfaces are commands, MCP tools and MCP
    resources. Every prompt it lists MUST resolve to a handler.
  """
  verify unit "lists every core prompt, each one prompts/get serves"
  verify unit "returns core-provided descriptors when no extensions installed"
  verify contract "List MCP Prompts: listing MCP prompts holds — server_initialized, complete_list_returned, discovery_emitted"
  verify unit "every listed core prompt resolves to a handler"
}

// ---------------------------------------------------------------------------
// Section 1: Resources
// ---------------------------------------------------------------------------

behavior expose_graph_as_mcp_resource "Expose Graph as MCP Resource" {
  features   [mcp_resource_exposure]
  invariants [
    graph_traversal_integrity,
    graph_schema_completeness,
    diagnostic_determinism,
    mcp_structured_error_responses,
  ]
  category   command
  types      [Graph, GraphProtocolSchema, McpResourceDescriptor]
  ports      [McpProtocol, CompilerApi]
  consumes   [validation_complete]
  produces   [mcp_resource_read]
  requires {
    validation_complete_fired "validation_complete event has fired, confirming compilation is done"
  }
  ensures {
    graph_json_returned   "Graph Protocol JSON returned with embedded schema and schema_version"
    resource_read_emitted "mcp_resource_read event emitted"
  }
  contract   """
    In MCP server mode, the system MUST register a specforge://graph resource
    that returns the current compiled graph as Graph Protocol JSON. The resource
    MUST refresh on each recompilation. The output MUST be equivalent to
    specforge export --format=graph. The resource MUST include the embedded
    GraphProtocolSchema and schema_version field. Delegates graph serialization
    to serve_graph_resource (behaviors/output-schema.spec) for actual graph serving;
    this behavior's role is exposing it via the MCP transport protocol.
  """
  verify unit "specforge://graph resource returns full Graph Protocol JSON"
  verify unit "resource refreshes after recompilation"
  verify unit "output includes embedded schema and schema_version"
  verify contract "Expose Graph as MCP Resource: graph MCP resource holds — validation_complete_fired, graph_json_returned, resource_read_emitted"
  verify unit "returns error for unknown URI"
}

behavior expose_schema_as_mcp_resource "Expose Schema as MCP Resource" {
  features   [mcp_resource_exposure]
  invariants [graph_schema_completeness, diagnostic_determinism, mcp_structured_error_responses]
  category   command
  types      [GraphProtocolSchema, McpResourceDescriptor]
  ports      [McpProtocol, CompilerApi]
  consumes   [validation_complete]
  produces   [mcp_resource_read]
  requires {
    validation_complete_fired "validation_complete event has fired, confirming compilation is done"
  }
  ensures {
    schema_json_returned  "GraphProtocolSchema returned as JSON reflecting current compilation state"
    resource_read_emitted "mcp_resource_read event emitted"
  }
  contract   """
    In MCP server mode, the system MUST register a specforge://schema resource
    that returns the current GraphProtocolSchema as JSON. The resource MUST
    reflect the current compilation state and update when extensions are added
    or removed. This allows agents to introspect the graph structure without
    parsing the full graph. Delegates schema serialization to
    serve_schema_resource (behaviors/output-schema.spec).
  """
  verify unit "specforge://schema resource returns GraphProtocolSchema JSON"
  verify unit "schema updates when extensions change"
  verify contract "Expose Schema as MCP Resource: schema MCP resource holds — validation_complete_fired, schema_json_returned, resource_read_emitted"
}

behavior expose_context_as_mcp_resource "Expose Context as MCP Resource" {
  features   [mcp_resource_exposure]
  invariants [
    graph_traversal_integrity,
    graph_schema_completeness,
    diagnostic_determinism,
    mcp_structured_error_responses,
  ]
  category   command
  types      [Graph, AgentExportConfig, McpResourceDescriptor]
  ports      [McpProtocol, CompilerApi]
  consumes   [validation_complete]
  produces   [mcp_resource_read]
  requires {
    validation_complete_fired "validation_complete event has fired, confirming compilation is done"
  }
  ensures {
    context_format_returned "Token-optimized context format returned equivalent to --format=context"
    resource_read_emitted   "mcp_resource_read event emitted"
  }
  contract   """
    In MCP server mode, the system MUST register a specforge://context resource
    that returns the current graph in a token-optimized context format. The output
    MUST be equivalent to specforge export --format=context. The resource MUST
    refresh after each recompilation. Agents SHOULD prefer this resource when they
    need full project understanding within a constrained token budget.
  """
  verify unit "specforge://context resource returns token-optimized format"
  verify unit "resource refreshes after recompilation"
  verify unit "the context resource is the context export of the same request"
  verify contract "Expose Context as MCP Resource: context MCP resource holds — validation_complete_fired, context_format_returned, resource_read_emitted"
}

behavior expose_brief_as_mcp_resource "Expose Brief as MCP Resource" {
  features   [mcp_resource_exposure]
  invariants [
    graph_traversal_integrity,
    graph_schema_completeness,
    diagnostic_determinism,
    mcp_structured_error_responses,
  ]
  category   command
  types      [Graph, AgentExportConfig, McpResourceDescriptor]
  ports      [McpProtocol, CompilerApi]
  consumes   [validation_complete]
  produces   [mcp_resource_read]
  requires {
    validation_complete_fired "validation_complete event has fired, confirming compilation is done"
  }
  ensures {
    brief_format_returned "Minimal IDs-and-edges format returned equivalent to --format=brief"
    resource_read_emitted "mcp_resource_read event emitted"
  }
  contract   """
    In MCP server mode, the system MUST register a specforge://brief resource
    that returns the graph in a minimal IDs-and-edges format. The output MUST
    be equivalent to specforge export --format=brief. The resource MUST refresh
    after each recompilation. This format is intended for agents that only need
    structural awareness without full entity details.
  """
  verify unit "specforge://brief resource returns minimal IDs and edges format"
  verify unit "resource refreshes after recompilation"
  verify unit "the brief resource is the brief export of the same request"
  verify contract "Expose Brief as MCP Resource: brief MCP resource holds — validation_complete_fired, brief_format_returned, resource_read_emitted"
}

behavior expose_diagnostics_as_mcp_resource "Expose Diagnostics as MCP Resource" {
  features   [mcp_resource_exposure]
  invariants [diagnostic_determinism, mcp_structured_error_responses]
  category   command
  types      [DiagnosticBag, McpResourceDescriptor]
  ports      [McpProtocol, CompilerApi]
  consumes   [validation_complete]
  produces   [mcp_resource_read]
  requires {
    validation_complete_fired "validation_complete event has fired, confirming compilation is done"
  }
  ensures {
    diagnostics_returned  "DiagnosticBag returned as JSON with severity, code, message, file, and span"
    resource_read_emitted "mcp_resource_read event emitted"
  }
  contract   """
    In MCP server mode, the system MUST register a specforge://diagnostics
    resource that returns the current DiagnosticBag as JSON. The resource MUST
    update after each recompilation. The output MUST include all diagnostics
    with severity, code, message, file path, and span, and the catalogue's
    title for each catalogued code (null otherwise). Agents MAY poll this
    resource to check project health without triggering a new compilation.
  """
  verify unit "specforge://diagnostics resource returns current DiagnosticBag as JSON"
  verify unit "resource updates after recompilation"
  verify unit "each diagnostic includes severity, code, message, file, and span"
  verify unit "each catalogued diagnostic in the resource carries its title"
  verify contract "Expose Diagnostics as MCP Resource: diagnostics MCP resource holds — validation_complete_fired, diagnostics_returned, resource_read_emitted"
}

behavior expose_entity_as_mcp_resource "Expose Per-Entity MCP Resource" {
  features   [mcp_resource_exposure]
  invariants [
    graph_traversal_integrity,
    graph_schema_completeness,
    diagnostic_determinism,
    mcp_structured_error_responses,
  ]
  category   command
  types      [Graph, Node, Edge, McpResourceDescriptor]
  ports      [McpProtocol, CompilerApi]
  consumes   [validation_complete]
  produces   [mcp_resource_read]
  requires {
    validation_complete_fired "validation_complete event has fired, confirming compilation is done"
  }
  ensures {
    subgraph_returned     "Target node and all directly connected nodes and edges returned as subgraph"
    resource_read_emitted "mcp_resource_read event emitted"
  }
  contract   """
    In MCP server mode, the system MUST register a specforge://graph/{entity_id}
    resource template that returns a single entity and its immediate neighbors as
    a subgraph: the scoped graph export at depth 1 (specforge export --format
    graph --scope <entity_id>), which references the published schema with a
    schema_ref. The resource MUST include the target node, all directly connected
    nodes, and the edges between them. If the entity_id does not exist, the
    read MUST fail as not found (-32002 in a handshake session, -32602 in a
    2026-07-28 request) whose data is an entity_not_found McpError carrying
    E003 and the uri (the 404 case); a malformed entity_id is invalid input
    (the 400 case). The resource MUST refresh after recompilation.
  """
  verify unit "specforge://graph/{entity_id} returns entity and its neighbors"
  verify unit "non-existent entity_id returns 404 error"
  verify unit "malformed entity_id returns 400 error"
  verify unit "the entity resource is the scoped graph export with its schema_ref"
  verify unit "resource refreshes after recompilation"
  verify contract "Expose Per-Entity MCP Resource: per-entity MCP resource holds — validation_complete_fired, subgraph_returned, resource_read_emitted"
}

// ---------------------------------------------------------------------------
// Section 2: Notifications
// ---------------------------------------------------------------------------

// This is the MCP-specific implementation of notify_delta_subscribers
// (behaviors/incremental.spec). It adapts the delta notification to the
// MCP transport protocol.
behavior notify_graph_delta_via_mcp "Notify Graph Delta via MCP" {
  features   [mcp_delta_notifications]
  invariants [
    incremental_correctness,
    graph_traversal_integrity,
    mcp_structured_error_responses,
    mcp_subscription_cleanup,
  ]
  category   command
  types      [GraphDelta, McpSubscription]
  ports      [McpProtocol, CompilerApi]
  consumes   [graph_delta_computed]
  produces   [mcp_delta_notified, mcp_subscription_created, mcp_subscription_removed]
  requires {
    graph_delta_computed_fired "graph_delta_computed event has fired after incremental rebuild"
  }
  ensures {
    subscribers_notified       "A client subscribed to a resource the rebuild changed receives notifications/resources/updated for it, then specforge/graphChanged with the GraphDelta payload"
    no_notification_when_empty "Notification suppressed when no clients are subscribed"
    delta_notified_emitted     "mcp_delta_notified event emitted after notification delivery"
  }
  contract   """
    When an incremental rebuild completes in MCP server mode, a client that
    subscribed (resources/subscribe) to a resource the rebuild changed MUST
    receive notifications/resources/updated naming it, for each such
    resource, by the rule subscriptions/listen follows. When the graph
    changed and the client subscribed to a resource that changes with it,
    the system MUST then send specforge/graphChanged, whose payload MUST
    include the GraphDelta describing added, removed, and modified nodes and
    edges. A subscription is to one resource: unsubscribing one keeps the
    others, and the client's subscriptions end with its connection.
    resources/subscribe to a URI the server does not serve is refused as
    resources/read refuses it (not found: -32002, Unknown resource URI);
    resources/unsubscribe never fails. If no clients are subscribed, the
    notification MUST be suppressed.
  """
  verify unit "graph_changed notification sent after incremental rebuild"
  verify unit "notification includes GraphDelta payload"
  verify unit "a field-only edit is reported as a modified node"
  verify unit "moving an entity is not a modification"
  verify unit "unsubscribed clients do not receive notifications"
  verify unit "no notification when no clients subscribed"
  verify unit "clients can subscribe and unsubscribe from delta notifications"
  verify unit "a subscribed resource hears notifications/resources/updated when a rebuild changes it"
  verify unit "unsubscribing one resource keeps the client's other subscriptions"
  verify unit "resources/subscribe to a URI the server does not serve is refused as not found, as resources/read refuses it"
  verify contract "Notify Graph Delta via MCP: graph delta MCP notification holds — graph_delta_computed_fired, subscribers_notified, no_notification_when_empty, delta_notified_emitted"
}

behavior notify_diagnostics_delta_via_mcp "Notify Diagnostics Delta via MCP" {
  features   [mcp_delta_notifications]
  invariants [
    incremental_correctness,
    diagnostic_determinism,
    mcp_structured_error_responses,
    mcp_subscription_cleanup,
  ]
  category   command
  types      [DiagnosticsDelta, McpSubscription]
  ports      [McpProtocol, CompilerApi]
  consumes   [validation_complete]
  produces   [mcp_delta_notified, mcp_subscription_created, mcp_subscription_removed]
  requires {
    validation_complete_fired "validation_complete event has fired after compilation"
  }
  ensures {
    subscribers_notified   "A client subscribed to specforge://diagnostics receives notifications/resources/updated for it, then specforge/diagnosticsChanged"
    unchanged_suppressed   "Notification suppressed when diagnostics are unchanged or no clients subscribed"
    delta_notified_emitted "mcp_delta_notified event emitted after notification delivery"
  }
  contract   """
    When validation completes in MCP server mode and the diagnostics changed,
    a client subscribed to specforge://diagnostics MUST receive
    notifications/resources/updated for it and then a
    specforge/diagnosticsChanged notification.
    The notification payload MUST include added and removed diagnostics since the
    previous compilation. Clients MUST be able to subscribe and unsubscribe. If
    no clients are subscribed or the diagnostics are unchanged, the notification
    MUST be suppressed.
  """
  verify unit "diagnostics_changed notification sent after validation"
  verify unit "payload includes added and removed diagnostics"
  verify unit "unsubscribed clients do not receive notifications"
  verify unit "no notification when diagnostics are unchanged"
  verify contract "Notify Diagnostics Delta via MCP: diagnostics delta MCP notification holds — validation_complete_fired, subscribers_notified, unchanged_suppressed, delta_notified_emitted"
}

// ---------------------------------------------------------------------------
// Section 3: Protocol Compliance
// ---------------------------------------------------------------------------

behavior handle_mcp_protocol_error "Handle MCP Protocol Error" {
  features   [mcp_protocol_compliance]
  invariants [mcp_structured_error_responses]
  category   command
  types      [McpError, JsonRpcErrorCode, McpErrorCode]
  ports      [McpProtocol]
  produces   [mcp_protocol_error_handled]
  requires {
    mcp_protocol_available "McpProtocol port is available and server is accepting requests"
  }
  ensures {
    standard_error_returned "JSON-RPC 2.0 standard error code returned with human-readable message"
    no_state_leaked         "Error response does not leak internal state (stack traces, file paths, memory addresses)"
    server_operational      "Server remains operational after protocol error"
    error_handled_emitted   "mcp_protocol_error_handled event emitted"
  }
  contract   """
    When the MCP server receives a malformed JSON-RPC request (parse error,
    invalid method, missing required params of the JSON-RPC method:
    tools/call name, resources/read uri, prompts/get name), it MUST respond
    with the standard JSON-RPC 2.0 error codes: -32700 (Parse error), -32600
    (Invalid Request), -32601 (Method not found), -32602 (Invalid params),
    -32603 (Internal error). An unknown tool is -32602 too. A tool that
    detects invalid arguments is not a malformed request: it returns an
    isError result carrying an McpError, not -32602 (MCP 2025-11-25,
    SEP-1303). prompts/get and resources/read have no isError result: a
    prompt that cannot render, or a resource that cannot be read, answers
    -32602 (input the client can fix, an argument it named included) or
    -32603 with its McpError as the error's data. A resource that does not
    exist is -32002 in a handshake session (the MCP 2025-xx revisions) and
    -32602 in a 2026-07-28 request, with data naming its uri.
    Every method that needs a session refuses before initialize with -32600;
    an unknown method is -32601 whether or not the session is initialized.
    A handler that panics is a server fault: the request gets
    -32603 and the server keeps serving. The error response MUST NOT crash
    the server or leak internal state (stack traces, file paths, memory
    addresses). The error response MUST include a human-readable message
    field.
  """
  verify unit "malformed JSON produces -32700 Parse error"
  verify unit "invalid method produces -32601 Method not found"
  verify unit "missing required params produces -32602 Invalid params"
  verify unit "a tool that detects invalid arguments returns an isError result, not -32602"
  verify unit "tools/call arguments that are not an object produce -32602 Invalid params"
  verify unit "error response does not leak internal state"
  verify unit "server remains operational after protocol error"
  verify unit "returns -32600 for invalid request"
  verify unit "each request method that needs a session refuses before initialize with -32600"
  verify unit "a refusal naming an argument is -32602 for a prompt or a resource read"
  verify unit "a resource that does not exist is -32002 in a handshake session and -32602 in a 2026-07-28 request, its data naming the uri"
  verify unit "returns -32603 for internal error"
  verify unit "truly unknown tool returns -32602 Invalid params (MCP spec example)"
  verify contract "Handle MCP Protocol Error: MCP protocol error handling holds — mcp_protocol_available, standard_error_returned, no_state_leaked, server_operational, error_handled_emitted"
  verify unit "error response includes id from request"
  verify unit "notifications produce no response"
  verify unit "response always has jsonrpc 2.0 field"
  verify unit "success response includes id from request"
}

behavior follow_negotiated_mcp_revision "Follow the Negotiated MCP Revision" {
  features   [mcp_protocol_compliance]
  invariants [mcp_structured_error_responses]
  category   command
  types      [McpCapabilities]
  ports      [McpProtocol]
  produces   [mcp_protocol_error_handled]
  requires {
    server_initialized "MCP server has been initialized and a protocol revision negotiated"
  }
  ensures {
    batches_per_revision     "A 2025-03-26 session answers a JSON-RPC batch with its responses; a later revision rejects the batch"
    structured_content_added "From 2025-06-18, a tool result with a JSON object payload also carries it as structuredContent"
  }
  contract   """
    After initialize, the server MUST follow the negotiated revision. A
    2025-03-26 session MUST accept a JSON-RPC batch: the server answers
    with an array holding the response to each request in the batch,
    sends nothing when the batch holds only notifications, and answers an
    empty batch with a single -32600 error. Later revisions removed
    batching, so a session on one of them gets a single -32600 error for
    a batch. From 2025-06-18, a tool result whose payload is a JSON object
    MUST also carry that object as structuredContent, alongside the text
    block holding its JSON, and tools/list MUST give each tool whose result
    is an object an outputSchema that every structured result conforms to.
    Each core tool's outputSchema is derived from its typed reply: its root
    is an object schema (type object, a union's included), every object it
    closes lists its keys and states additionalProperties false, every
    array states its items. A type the reply holds in more than one place,
    or a domain type another spec file declares (Diagnostic, SourceSpan,
    MigrationResult, ...), is defined once under the outputSchema's $defs
    and referred to by a same-document $ref; the root is never a
    reference. A core tool's reply that its outputSchema
    refuses MUST be a schema_mismatch error naming each violation, never
    structured content.
    A failed call of a tool with an outputSchema carries no
    structuredContent: its McpError is in the text block. A 2025-03-26
    session is listed no outputSchema.
  """
  verify unit "a core tool's reply its output schema refuses is a schema_mismatch error, never structured content"
  verify unit "a core tool's output schema is derived from its typed reply: every object it closes lists its keys, every array its items"
  verify unit "every core tool's outputSchema is an object schema at its root"
  verify unit "a type a reply holds more than once, or a domain type another spec file declares, is defined once under the outputSchema's $defs and referred to by $ref"
  verify unit "every $ref in a core tool's outputSchema is a same-document pointer to a definition it holds"
  verify unit "a 2025-03-26 session answers a batch with the response to each request"
  verify unit "a batch of notifications gets no response"
  verify unit "an empty batch is an invalid request"
  verify unit "a session on a later revision rejects a batch with -32600"
  verify unit "tool results carry an object payload as structuredContent from 2025-06-18"
  verify unit "a 2025-03-26 session gets no structuredContent"
  verify unit "each core tool with an object result declares an outputSchema its structured results conform to"
  verify unit "a failed call of a tool with an outputSchema carries no structuredContent"
  verify unit "a 2025-03-26 session is listed no outputSchema"
}

behavior serve_stateless_mcp_requests "Serve Stateless MCP Requests" {
  features   [mcp_protocol_compliance]
  invariants [mcp_structured_error_responses]
  category   command
  types      [McpCapabilities]
  ports      [McpProtocol, CompilerApi]
  produces   [mcp_protocol_error_handled]
  requires {
    version_in_meta "The request names a protocol version and the client's capabilities in its _meta"
  }
  ensures {
    served_without_handshake "The request is answered without initialize, under the revision its _meta names"
    result_typed             "The result carries resultType, the server in _meta, and caching hints on a list or a read"
  }
  contract   """
    Beside the handshake revisions, the server MUST speak MCP 2026-07-28,
    the stateless revision. A request whose _meta names a protocol version
    is served on its own: no initialize before it, and nothing a prior
    request said changes its answer; a request without that _meta follows
    the revision initialize negotiated. server/discover MUST be answered
    with or without initialize, listing the stateless revisions the server
    speaks and its capabilities. A stateless request missing the version or
    the client capabilities is malformed (-32602); one naming a version the
    server does not speak gets -32022 whose data lists the supported
    versions and the requested one. Every stateless result carries
    resultType "complete" and the server's name and version in _meta; the
    results of server/discover, the lists and resources/read also carry
    ttlMs and cacheScope. ping, logging/setLevel and resources/subscribe
    are not methods of the revision (-32601).
  """
  verify unit "server/discover is answered without initialize and lists 2026-07-28"
  verify unit "a request with a protocol version in its _meta is served without initialize"
  verify unit "a stateless result carries resultType complete and the server info"
  verify unit "list and read results carry ttlMs and cacheScope"
  verify unit "a stateless request without the client capabilities is -32602"
  verify unit "an unsupported protocol version is -32022 naming the supported versions"
  verify unit "ping and resources/subscribe are not stateless methods"
  verify unit "a stateless request after initialize is served under its own revision"
  verify unit "a stateless request's revision ends with the request"
}

behavior listen_for_mcp_resource_updates "Listen for MCP Resource Updates" {
  features   [mcp_protocol_compliance]
  invariants [mcp_structured_error_responses]
  category   command
  types      [McpResourceDescriptor]
  ports      [McpProtocol, CompilerApi]
  produces   [mcp_subscription_created, mcp_subscription_removed]
  requires {
    listen_requested "A stateless subscriptions/listen request names the resources to hear about"
  }
  ensures {
    acknowledged_first "notifications/subscriptions/acknowledged, naming the honoured subset, precedes every notification on the stream"
    updates_tagged     "A recompile that changes a listened resource sends notifications/resources/updated carrying the listen request's id"
  }
  contract   """
    subscriptions/listen (MCP 2026-07-28) MUST open a stream on which the
    server tells the client when the resources it names change. The server
    honours resource subscriptions to resources it serves, each once, and no
    list-changed types; it MUST first send
    notifications/subscriptions/acknowledged naming the honoured subset,
    with the listen request's id as _meta
    io.modelcontextprotocol/subscriptionId. After a recompile that changes
    a listened resource (the graph's views and every extension resource
    when the graph changed or the environment loaded again,
    specforge://schema when the environment loaded again,
    specforge://diagnostics when the diagnostics did), it sends
    notifications/resources/updated for it with the same id. It MUST NOT
    send on the stream any notification type the client did not ask for.
    notifications/cancelled naming the listen request ends the stream, and
    so does the end of the connection; no response is sent for it.
  """
  verify unit "subscriptions/listen is acknowledged first with the resources honoured"
  verify unit "a recompile that changes a listened resource sends resources/updated with the subscription id"
  verify unit "a listen stream receives no notification type it did not ask for"
  verify unit "both eras decide what a change touches by one rule"
  verify unit "cancelling the listen request ends the stream"
  verify unit "the end of the connection ends the stream"
  verify unit "a resource a listen names twice is honoured once"
  verify unit "a listen stream's subscription events name its request id"
  verify unit "an environment reload is heard by the schema, the graph views and the extension resources"
}

behavior handle_mcp_request_cancellation "Handle MCP Request Cancellation" {
  features   [mcp_protocol_compliance]
  invariants [mcp_structured_error_responses]
  category   command
  types      [McpError]
  ports      [McpProtocol, CompilerApi]
  produces   [mcp_request_cancelled]
  requires {
    mcp_protocol_available "McpProtocol port is available and server is accepting requests"
  }
  ensures {
    cancellation_safe         "Server state remains consistent after cancellation — no crash or corruption"
    request_cancelled_emitted "mcp_request_cancelled event emitted when operation is cancelled"
  }
  contract   """
    The server handles requests one at a time, so a notifications/cancelled
    message (or $/cancelRequest) can only arrive after the request it names
    has completed. The server MUST accept it as a no-op — no response to
    the notification, no change to its state — and emit
    mcp_request_cancelled naming the request. The server MUST NOT crash or
    enter an inconsistent state due to cancellation.
  """
  verify unit "cancellation of completed request is a no-op"
  verify unit "server state remains consistent after cancellation"
  verify unit "notifications/cancelled is accepted without a response"
  verify contract "Handle MCP Request Cancellation: MCP request cancellation holds — mcp_protocol_available, cancellation_safe, request_cancelled_emitted"
}

behavior guard_mcp_reinitialization "Guard MCP Reinitialization" {
  features   [mcp_protocol_compliance]
  invariants [mcp_structured_error_responses]
  category   command
  types      [McpCapabilities]
  ports      [McpProtocol]
  produces   [mcp_protocol_error_handled]
  requires {
    server_initialized "MCP server has already been initialized (first initialize completed)"
  }
  ensures {
    reinit_rejected       "JSON-RPC error -32600 returned for duplicate initialize request"
    session_unaffected    "Existing session continues unaffected — no state reset or resource leak"
    error_handled_emitted "mcp_protocol_error_handled event emitted"
  }
  contract   """
    When an already-initialized MCP server receives another initialize
    request, it MUST respond with a JSON-RPC error (-32600 Invalid Request)
    per MCP protocol specification. The server MUST NOT re-initialize,
    reset state, or leak resources. The existing session MUST continue
    unaffected.
  """
  verify unit "second initialize request returns -32600 error"
  verify unit "existing session continues after rejected reinitialization"
  verify unit "no resources leaked on rejected reinitialization"
  verify contract "Guard MCP Reinitialization: MCP reinitialization guard holds — server_initialized, reinit_rejected, session_unaffected, error_handled_emitted"
  verify unit "can reinitialize after shutdown"
}
