// Surface contribution behaviors — CLI commands, MCP tools, MCP resources
//
// Extensions declare surface contributions in their manifest's `surfaces`
// field. The registry build registers and checks them; the CLI and MCP
// dispatch to the cmd__{id} and mcp__{name} exports. What the host cannot
// check (export presence: a component guest routes every export through
// one call) and what nothing configures (a per-contribution toggle) are
// not specified (ADR 0011). On the guest side, an extension built with the
// SDK declares each surface with its handler: the SDK derives the
// `surfaces` payload and routes the exports from that one declaration, so
// the guest routes every export it declares (ADR 0011, Adjustments).

use "events/surface-contributions"
use "events/wasm-extensions"
use "invariants/mcp"
use "invariants/surface"
use "invariants/wasm"
use "ports/inbound"
use "ports/outbound"
use "types/errors"
use "types/mcp"
use "types/surface"
use "types/wasm"
use "types/zero-entity-core"

// ── Registration & Validation ───────────────────────────────

behavior register_surface_contributions "Register Surface Contributions" {
  features   [surface_contributions]
  invariants [surface_contribution_uniqueness]
  category   command
  types      [ExtensionDeclaration, SurfaceDescriptor, SurfaceRegistryEntry, SurfaceType, SurfaceError]
  consumes   [manifest_loaded]
  requires {
    manifest_loaded_fired "manifest_loaded event has fired, confirming extension manifests are parsed and available"
  }
  ensures {
    all_surfaces_registered "All CLI commands, MCP tools, and MCP resources from manifest surfaces fields are registered in the SurfaceRegistry"
    duplicates_detected     "Duplicate contribution names within each surface type across extensions produce E039"
    unparsed_fails_load     "A surfaces description that does not parse fails the extension's load with E028"
  }
  contract   """
    When extension manifests are loaded, the compiler MUST parse the
    surfaces field from each manifest and register all declared CLI
    commands, MCP tools, and MCP resources in the SurfaceRegistry.
    Registration MUST detect duplicate contribution names within each
    surface type across all extensions — duplicates MUST produce E039.
    Registration happens when the project's registries are built from
    the manifests its extensions describe, before any surface export is
    dispatched: surface contributions are declarative, like entity
    kinds. A surfaces description that does not parse (an arg type
    outside CommandArgType, a missing required field) MUST fail the
    extension's load with E028, as any described category that does not
    parse does: its surfaces are never silently dropped. The registry
    build is pure; its diagnostics are its record, and it emits no event.
  """
  verify unit "commands parsed from manifest surfaces field"
  verify unit "MCP tools parsed from manifest surfaces field"
  verify unit "MCP resources parsed from manifest surfaces field"
  verify unit "duplicate command ID across extensions produces E039"
  verify unit "duplicate MCP tool name across extensions produces E039"
  verify unit "registration succeeds with no duplicates"
  verify unit "a surfaces description that does not parse fails the extension's load"
  verify contract "Register Surface Contributions: surface contribution registration holds — manifest_loaded_fired, all_surfaces_registered, duplicates_detected, unparsed_fails_load"
}

behavior validate_mcp_tool_schemas "Validate MCP Tool Schemas" {
  features   [surface_contributions]
  invariants [surface_schema_validity]
  category   validation
  types      [McpToolContribution, JsonSchema]
  ensures {
    malformed_refused "An explicit MCP tool whose input_schema or output_schema is not a JSON object produces E055 and is not registered"
    wellformed_kept   "An explicit MCP tool whose schemas are JSON objects is registered"
  }
  contract   """
    When the registries are built, every explicit MCP tool contribution
    MUST have an input_schema that is a JSON object, and an output_schema,
    when it declares one, that is a JSON object. A tool that does not MUST
    produce E055 naming the tool, its extension and the schema, and MUST
    NOT be registered, listed or dispatched. A tool's description is a
    required field of its declaration. Command arg types need no check:
    CommandArgType is closed, so an unknown one fails the surfaces
    description's parse (register_surface_contributions).
  """
  verify unit "a tool whose input_schema is not a JSON object is E055 and not registered"
  verify unit "a tool whose output_schema is not a JSON object is E055 and not registered"
  verify unit "a tool whose schemas are JSON objects is registered"
}

// ── Auto-Promotion ──────────────────────────────────────────

behavior auto_promote_commands_to_mcp_tools "Auto-Promote Commands to MCP Tools" {
  features   [surface_contributions]
  invariants [surface_contribution_uniqueness]
  category   command
  types      [CommandContribution, ExtensionCommand]
  produces   [commands_auto_promoted]
  requires {
    surfaces_registered "the registry build has registered the project's CLI command and MCP tool contributions"
  }
  ensures {
    all_commands_promoted          "Every CLI command contribution the host does not refuse is auto-promoted to an MCP tool"
    naming_convention_enforced     "Auto-promoted tools follow the specforge.{ext_short}.{cmd_id} naming pattern"
    explicit_tool_wins             "Explicit MCP tool contributions take precedence over auto-promoted tools with I017 emitted"
    commands_auto_promoted_emitted "commands_auto_promoted event is emitted after promotion completes"
    schema_is_the_declaration      "The derived input_schema states each arg's type, one_of values, minimum, default and description; required lists the required args that are not flags; no undeclared property is accepted"
  }
  contract   """
    After surface contributions are registered, the compiler MUST
    auto-promote every CLI command contribution to an MCP tool with
    the naming convention specforge.{ext_short}.{cmd_id}, but one the
    host refuses on the command line (an arg named path, format or help,
    or two args of one name): the one rule keeps both surfaces alike. The derived
    input_schema MUST be computed from the command's args declaration
    alone (specforge_ops::command::ExtensionCommand): each arg's type,
    its one_of values, its minimum, its default and its description;
    required lists the required args that are not flags, since a flag is
    false unless set; no undeclared property is accepted.
    If an explicit MCP tool contribution already exists with the same
    name, the explicit tool MUST win and I017 MUST be emitted. Auto-
    promoted tools appear in list_mcp_tools alongside explicit tools.
  """
  verify unit "CLI command auto-promoted to MCP tool"
  verify unit "auto-promoted tool name follows specforge.{ext}.{cmd} pattern"
  verify unit "derived input_schema computed from command args"
  verify unit "explicit MCP tool wins over auto-promoted tool with I017"
  verify unit "a command the CLI refuses, such as one declaring an arg named format, is not promoted"
  verify unit "the derived input_schema states each arg's default and minimum and accepts no undeclared argument"
  verify unit "a required flag is not required over MCP, as on the command line"
  verify unit "an explicit extension tool named as a core tool is not listed, with I017"
  verify contract "Auto-Promote Commands to MCP Tools: command-to-MCP-tool auto-promotion holds — surfaces_registered, all_commands_promoted, naming_convention_enforced, explicit_tool_wins, commands_auto_promoted_emitted, schema_is_the_declaration"
}

// ── Dispatch ────────────────────────────────────────────────

behavior dispatch_surface_command "Dispatch Surface Command" {
  features   [surface_contributions]
  invariants [
    surface_sandbox_ceiling,
    wasm_sandbox_integrity,
    extension_isolation,
    extensions_run_in_their_loading_runtime,
  ]
  category   command
  types      [CommandContribution, CommandInput, CommandOutput, SurfaceError, WasmTrapInfo]
  ports      [WasmRuntime]
  produces   [surface_command_dispatched]
  requires {
    command_declared "The command is one an extension the project enables declares in its surfaces, read when the project's environment loaded"
  }
  ensures {
    args_serialized                    "Command arguments, the project root (absolute and canonical, the same on both surfaces) and the graph are serialized as JSON and passed to the cmd__ export"
    sandbox_restricted                 "The cmd__ export is granted no capability (no preopened directory, environment, arguments, stdin or network), whatever sandbox override its declaration asks for"
    traps_caught                       "Wasm traps are caught and reported as ExtensionError diagnostics"
    output_returned                    "Exit code, stdout, and stderr are returned to the caller"
    surface_command_dispatched_emitted "Over MCP, a surface_command_dispatched event records the command and its exit code once its export returns; the CLI has no event sink"
    args_normalized_by_the_host        "Both surfaces send the export the args normalized by one rule before it runs: an absent arg takes its declared default, an unset flag is false, each value is its declared type, no undeclared arg is passed; a value the rule refuses is INVALID_INPUT, the same error object on both surfaces, and the export is not called"
    evidence_carried                   "The CommandInput carries what the project's recorded test report proves: absent without a report, unreadable with the reason when the report cannot be used, else the obligations, proven obligations and failing tests of every entity that counts toward coverage, scored by the coverage rule specforge stats reports"
    one_operation                      "Both surfaces run a command through one operation over the project view: it normalizes the given args, attaches the project root, the evidence and the host's date, and calls the export; a surface reads its arguments in and renders the outcome"
  }
  contract   """
    When a CLI command from an extension is invoked, the host MUST
    serialize the command's input as JSON (CommandInput: its args, the
    project root, absolute and canonical (symlinks resolved), the compiled graph, the format asked for, the
    host's date, UTC, and the evidence of the project's recorded test
    report) and call the cmd__{id} export,
    in the runtime that loaded the project's extensions to read their
    declarations: only the extensions the project enables are loaded,
    each compiled once per process (from the wasm compile cache when it
    is warm), and dispatching a command loads no module. The CLI routes
    specforge {ext_short} {command} to it, the command line built from
    the declared args and the host's --path, --help and --format (human,
    the default, or json; a command declaring an arg of one of those
    names is refused, exit 2); an auto-promoted MCP tool runs the same
    export with its arguments as the args, normalized by the rule the
    command line applies (its declared defaults, false for an unset flag,
    each value its declared type; an argument the rule refuses is an
    isError result carrying the INVALID_INPUT object the CLI writes, and
    the export is not called), over the served graph, always
    asking for json: a JSON object the command prints on success (with
    nothing on stderr) is the tool result's structured content too, and
    a failure that prints one JSON object on stderr and nothing on stdout
    is an isError result carrying that object; any other output (an
    array, a scalar, prose) is text blocks, isError when the exit code
    is not zero. The extension renders both
    formats; the host knows no payload (ADR 0011). The export
    MUST be granted no capability: its WASI context preopens no
    directory and passes no environment, arguments, inherited stdio or
    network, so cwd is a path it is told, not one it can open, and the
    graph is all it reads. A sandbox override a command declares (only a
    guest built before ADR 0037 can) is not applied and is W153 at load:
    with nothing granted there is nothing for it to withhold, and no
    declaration grants more (surface_sandbox_ceiling).
    Wasm traps MUST be caught and reported as ExtensionError
    diagnostics, as is a declared export the guest does not route (the
    host cannot list a component guest's exports, so presence is known
    only by calling). An answer that is not a CommandOutput is an
    ExtensionError (E028), like a trap: never exit 0 with the raw bytes.
    Under --format json the CLI writes the error to stderr as one error
    object of the shape commands write ({code, message, suggestion}),
    and exits 1. A
    usage error the command line catches before the command runs (a
    value outside a one_of, a missing required arg, an unknown flag, a
    value that is not an integer, or below the arg's minimum) is, under
    --format json (wherever it
    is on the command line), one INVALID_INPUT error object of that
    shape on stderr ({code, message, suggestion?}, the message naming
    the arg as declared and, for a one_of, its values), nothing on
    stdout, exit 2; under human it is clap's usage text, exit 2; --help
    is clap's, exit 0. The command's exit code, stdout, and stderr MUST be
    returned to the caller. Both surfaces run a command through one
    operation over the project view, so what the export receives for the
    same arguments is the same on both; the CLI runs it over its compiled
    project (its checks run, as MCP's served session's have). The MCP server records each command whose
    export returned as a surface_command_dispatched event.
  """
  verify unit "the command runs in the runtime that read the project's declarations, which loaded only the extensions the project enables"
  verify unit "args serialized as JSON to cmd__ export"
  verify unit "a cmd__ export is granted no capability, whatever sandbox its declaration asks for"
  verify unit "Wasm trap caught and reported as ExtensionError"
  verify unit "exit code, stdout, stderr returned to CLI"
  verify unit "a declared export the guest does not route is an ExtensionError when dispatched"
  verify unit "the CommandInput carries the format the caller asked for and the host's date"
  verify unit "one operation runs a command over the project view: it normalizes the args, attaches the evidence, the date and the project root, absolute and canonical, and calls the export"
  verify unit "the CommandInput carries what the recorded tests prove, per entity that counts toward coverage"
  verify unit "a command's input says when the recorded test report cannot be read, and carries no evidence without one"
  verify unit "a command declaring an arg named format is refused on the command line"
  verify unit "under --format json a command whose export trapped prints one JSON error object"
  verify unit "a command whose output is not a CommandOutput is an ExtensionError, not exit 0 with the raw bytes"
  verify unit "the host and the SDK normalize a command's args by the same rule"
  verify unit "over MCP an argument the command's declaration refuses is the INVALID_INPUT error object the CLI writes, and the export is not called"
  verify integration "the CLI and MCP send a command's export the same args for the same input, its declared defaults applied by the host"
  verify integration "under --format json a usage error the command line catches is one INVALID_INPUT error object on stderr, exit 2"
  verify integration "over MCP a command is asked for json and its JSON output is the tool's structured content"
  verify integration "the CLI and MCP send a command's export the same project root, absolute and canonical"
  verify integration "a trapped command is E028 on both surfaces: one error object on the CLI's stderr under json, a structured MCP error over MCP"
  verify integration "the CLI and MCP send a command the same evidence for the same recorded report"
  verify unit "over MCP a failure's JSON error object is an isError result carrying it, and output that is not one object is text"
  verify contract "Dispatch Surface Command: surface command dispatch holds — command_declared, args_serialized, sandbox_restricted, traps_caught, output_returned, surface_command_dispatched_emitted, args_normalized_by_the_host, evidence_carried, one_operation"
}

behavior dispatch_surface_mcp_tool "Dispatch Surface MCP Tool" {
  features   [surface_contributions]
  invariants [
    surface_sandbox_ceiling,
    wasm_sandbox_integrity,
    extension_isolation,
    mcp_structured_error_responses,
  ]
  category   command
  types      [McpToolContribution, SurfaceError, WasmTrapInfo, JsonSchema]
  ports      [WasmRuntime, McpProtocol]
  consumes   [commands_auto_promoted]
  produces   [surface_mcp_tool_dispatched]
  requires {
    tool_registered              "the tool is one the registry build registered: its schemas are JSON objects (validate_mcp_tool_schemas)"
    commands_auto_promoted_fired "commands_auto_promoted event has fired, confirming auto-promoted tools are registered"
  }
  ensures {
    input_validated                     "Input is validated against the tool's declared input_schema before dispatch"
    sandbox_restricted                  "The mcp__ export is granted no capability, whatever sandbox override its declaration asks for"
    traps_as_mcp_errors                 "Wasm traps are caught and returned as structured MCP error responses"
    tool_result_returned                "Tool output is returned as a standard MCP tool result"
    surface_mcp_tool_dispatched_emitted "surface_mcp_tool_dispatched event is emitted after tool execution completes"
  }
  contract   """
    When an MCP tool contributed by an extension is invoked, the MCP
    server MUST validate the input against the tool's declared
    input_schema and call the mcp__{name} export with the validated
    input JSON, in the runtime the served project's compile loaded. The
    export MUST be granted no capability, like every surface export: a
    sandbox override a tool declares (W153 at load) is not applied, and no
    override grants more (surface_sandbox_ceiling). Wasm traps MUST be
    caught and returned as structured MCP error responses. The tool
    output MUST be returned as a standard MCP tool result. When the tool
    declares an output_schema, an output that does not match it MUST be
    returned as a schema_mismatch MCP error naming each violation, never
    as the tool's structured result. A key the output_schema does not
    allow (additionalProperties false) is such a violation, and so is one
    that breaks a definition the schema refers to by a same-document $ref;
    a $ref to anything outside the schema is never fetched and refuses
    every output. The MCP server records each tool
    whose export returned as a surface_mcp_tool_dispatched event. An
    export the guest does not route is an E028 error, like a trap.
  """
  verify unit "input validated against declared input_schema"
  verify unit "output that does not match the declared output_schema is a schema_mismatch error"
  verify unit "an output with a key its declared output_schema does not allow is a schema_mismatch error"
  verify unit "an output that breaks a definition its declared output_schema refers to by $ref is a schema_mismatch error"
  verify unit "the served project's runtime is the one its compile loaded and serves later calls until the project reloads"
  verify unit "input JSON passed to mcp__ export"
  verify unit "an mcp__ tool export is granted no capability, whatever sandbox its declaration asks for"
  verify unit "Wasm trap returned as structured MCP error"
  verify unit "tool output returned as MCP tool result"
  verify integration "a returned tool call is recorded as a surface_mcp_tool_dispatched event"
  verify contract "Dispatch Surface MCP Tool: surface MCP tool dispatch holds — tool_registered, commands_auto_promoted_fired, input_validated, sandbox_restricted, traps_as_mcp_errors, tool_result_returned, surface_mcp_tool_dispatched_emitted"
}

behavior dispatch_surface_mcp_resource "Dispatch Surface MCP Resource" {
  features   [surface_contributions]
  invariants [
    surface_sandbox_ceiling,
    wasm_sandbox_integrity,
    extension_isolation,
    mcp_structured_error_responses,
  ]
  category   command
  types      [
    McpResourceContribution,
    McpResourceRequest,
    McpResourceContent,
    SurfaceError,
    WasmTrapInfo,
  ]
  ports      [WasmRuntime, McpProtocol]
  produces   [surface_mcp_resource_dispatched]
  requires {
    resource_registered "the resource is one the registry build registered for an extension the project enables"
  }
  ensures {
    uri_matched                             "Requested URI is matched against registered URI templates"
    fs_write_denied                         "MCP resources have no fs_write access (no capability at all), whatever their sandbox override asks for"
    traps_as_mcp_errors                     "Wasm traps are caught and returned as an internal error whose data is an McpError with the E028 diagnostic"
    content_returned                        "Resource content and mime_type are returned to the MCP client"
    surface_mcp_resource_dispatched_emitted "surface_mcp_resource_dispatched event is emitted after resource read completes"
  }
  contract   """
    When an MCP resource contributed by an extension is read, the MCP
    server MUST match the requested URI against registered URI templates
    and call the mcp__{name} export with the URI, in the runtime the
    served project's compile loaded. MCP resources MUST NOT have
    fs_write access: like every surface export, a resource's export is
    granted no capability, whatever its sandbox override asks for
    (surface_sandbox_ceiling). The export receives only the URI, not
    the graph: a resource serves content that needs no project data
    (graph queries are commands, served as tools). A read whose export
    trapped, or that the guest does not route, MUST be a JSON-RPC
    internal error (-32603) whose data is an McpError carrying the
    diagnostic (E028) in diagnostic.code, never only in the message; so
    is an answer that is not the resource's content and MIME type
    (McpResourceContent), an ExtensionError (E028) like a trap: it is
    never served as raw bytes. The URI is matched against the registered
    templates after the core resources, whatever its scheme; a template a
    core resource already serves is not served, and is reported with
    I017. The resource
    content and mime_type MUST be returned to the MCP client. The MCP
    server records each read whose export returned as a
    surface_mcp_resource_dispatched event.
  """
  verify unit "URI matched against registered templates"
  verify unit "URI passed to mcp__ export"
  verify unit "fs_write denied for resource contributions"
  verify unit "Wasm trap returned as structured MCP error"
  verify unit "resource content and mime_type returned to client"
  verify unit "a resource whose answer is not its content and mime type is a structured MCP error"
  verify unit "an extension resource template outside specforge://ext/ is read through its export"
  verify unit "an extension resource template a core resource serves is not listed, with I017"
  verify integration "a returned resource read is recorded as a surface_mcp_resource_dispatched event"
  verify contract "Dispatch Surface MCP Resource: surface MCP resource dispatch holds — resource_registered, uri_matched, fs_write_denied, traps_as_mcp_errors, content_returned, surface_mcp_resource_dispatched_emitted"
}
