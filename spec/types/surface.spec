// Surface contribution types — CLI commands, MCP tools, MCP resources
//
// Extensions declare surface contributions in their manifest to extend
// the CLI and MCP server dynamically. Core dispatches to Wasm exports
// using the naming convention cmd__{id} and mcp__{name}.
//
// An extension written with the SDK declares each command, MCP tool and MCP
// resource together with the handler that answers it (ContributionsBuilder
// command, mcp_tool, mcp_resource). The SDK derives both the describe
// payload below and the routing of each export to its handler from that
// one declaration, and checks a command's args against their declared
// types before the handler reads them, through the declaration
// (CommandCall) and by the host's own rule (command_args, ADR 0017): a
// missing required arg, a value of another type or an arg the command does
// not declare is INVALID_INPUT, exit 2. Two surfaces cannot share an export (names that
// differ only in characters an export spells _, or a tool and a resource
// of one name). On the wire an unset optional field is absent, not null.

use "types/core"
use "types/graph"
use "types/mcp"
use "types/wasm"

// ── Surface Contribution Container ──────────────────────────

type SurfaceDescriptor {
  commands      CommandContribution[]     @optional
  mcp_tools     McpToolContribution[]     @optional
  mcp_resources McpResourceContribution[] @optional
  verify unit "SurfaceDescriptor schema is valid"
}

// ── CLI Command Contributions ───────────────────────────────

type CommandContribution {
  id          string       @readonly
  title       string
  description string       @optional
  category    string       @optional
  // Wasm export name: cmd__{id}, or cmd__{prefix}_{id} under the SDK's
  // command_prefix (the builtins use their short name: cmd__product_features)
  export      string       @readonly
  args        CommandArg[] @optional
  // A sandbox key (from a guest built before ADR 0037) grants nothing and is W153 at load
  verify unit "CommandContribution schema is valid"
}

// On the command line (specforge {ext_short} {id with _ as -}), a required
// arg is positional, in declaration order; any other, and every bool_arg, is
// --{name with _ as -}. --path names the project and is the host's on every
// command, as --help is, and --format (human, the default, or json): a
// command with an arg named path, help or format, or two args of one name,
// is refused on the command line (exit 2). A list of values is a string arg
// the command splits (--tags a,b): there is no list arg type (ADR 0011).
// default_value is the arg's value when the caller leaves it out, on every
// surface: the CLI fills it, and the SDK applies it to a call without it
// (an MCP tool call). A required arg or a bool_arg has none (a bool_arg is
// false unless set): the SDK refuses such a declaration, and a default its
// type refuses, when the extension is built.
type CommandArg {
  name          string   @readonly
  arg_type      CommandArgType
  required      boolean  @optional
  default_value string   @optional
  description   string   @optional
  // For enum_arg: allowed values
  values        string[] @optional
  // The least integer value: 0 for a count
  minimum       integer  @optional
  verify unit "CommandArg schema is valid"
}

type CommandArgType = string_arg | path_arg | bool_arg | enum_arg | integer_arg

// What a cmd__{id} export receives. args holds the declared args the
// caller set (the CLI's parsed command line, or the auto-promoted MCP
// tool's arguments), typed as declared; cwd is the project root, absolute and canonical (symlinks resolved), the same on both surfaces; graph is
// the compiled project's graph in the graph export's shape (entities
// sorted by id, edges by source, target and label); format is the output
// the caller asked for (the CLI's --format, json over MCP); today is the
// host's date at the call, UTC. The export reads no files and no clock:
// the same call serves the CLI and MCP (ADR 0011).
type CommandInput {
  args     FieldMap
  cwd      string
  graph    Graph
  format   CommandFormat
  today    string
  evidence CommandEvidence @optional
  verify unit "CommandInput schema is valid"
}

// What a command's input says the project's recorded test report proves
// (ADR 0039): absent without a report; `unreadable` with the reason when
// the report cannot be used; else, per entity that counts toward
// coverage, its obligations, the ones a passing test names and its
// failing tests, scored by the coverage rule `specforge stats` reports.
type CommandEvidence {
  state    EvidenceState
  entities EntityEvidence[] @optional
  reason   string           @optional
  verify unit "a command input without evidence leaves the field off the wire, and evidence is tagged by its state"
}

type EvidenceState = none | recorded | unreadable

type EntityEvidence {
  obligations integer
  proven      integer
  failing     integer @optional
  verify unit "an entity is proven when it declares an obligation, every one is proven and no test fails"
}

// human: the extension's layout for a reader (a table with a header row
// where the payload is tabular); json: one root object, the command's
// payload type. The host owns the flag; the extension renders both.
type CommandFormat = human | json

type CommandOutput {
  exit_code integer
  stdout    string @optional
  stderr    string @optional
  verify unit "CommandOutput schema is valid"
}

// ── MCP Tool Contributions ──────────────────────────────────

// The categories a tool that is no mutation is listed in, and the ones an
// extension tool may declare: only a core tool that writes its target's
// project files is a mutation.
type McpToolGroup = "core" | "navigation" | "management"

// input_schema and output_schema must be JSON objects: a tool with
// another value is E055 and is not registered.
type McpToolContribution {
  name          string       @readonly
  description   string
  category      McpToolGroup @optional
  // Wasm export name: mcp__{name}, with . and - as _
  export        string       @readonly
  input_schema  JsonSchema
  output_schema JsonSchema   @optional
  verify unit "McpToolContribution schema is valid"
}

// ── MCP Resource Contributions ──────────────────────────────

type McpResourceContribution {
  uri_template string @readonly
  name         string @readonly
  description  string @optional
  // Wasm export name: mcp__{name}, with . and - as _
  export       string @readonly
  mime_type    string @optional
  verify unit "McpResourceContribution schema is valid"
}

// ── Extension Commands ──────────────────────────────────────

// One command an extension declares, derived once for every surface:
// its CLI name, its MCP tool name, its args' command-line shapes, its
// input schema, the args both surfaces send, and why the host refuses it.
type ExtensionCommand {
  extension    string @readonly
  short        string @readonly
  cli_name     string @readonly
  // specforge.{short}.{id}
  tool_name    string @readonly
  args         CommandArg[]
  input_schema JsonSchema
  refusal      string @optional
  verify unit "ExtensionCommand schema is valid"
}

// ── Surface Registry ────────────────────────────────────────

type SurfaceRegistryEntry {
  surface_type      SurfaceType
  contribution_name string @readonly
  extension_name    string @readonly
  export_name       string @readonly
  verify unit "SurfaceRegistryEntry schema is valid"
}

type SurfaceType = command | mcp_tool | mcp_resource

// ── Surface Errors ──────────────────────────────────────────

type SurfaceError {
  extension_name  string @readonly
  surface_type    SurfaceType
  contribution_id string
  message         string
  export_name     string @optional
  verify unit "SurfaceError schema is valid"
}
