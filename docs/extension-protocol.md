# Extension Protocol

The Extension Protocol defines how extensions communicate with the SpecForge host through a Wasm-based bidirectional interface. An extension declares itself: the host reads its declaration from the binary, and no file beside it.

## Overview

Extensions are Wasm modules that the host loads, interrogates, and invokes at runtime. The protocol has four phases: **handshake**, **describe**, **operate**, and **disconnect**. The host drives all interactions. Extensions never call the host unprompted.

This design upholds Principle 2 (the compiler knows nothing about your domain) by ensuring that all domain vocabulary enters the compiler through a single, uniform protocol. The host does not know what an extension will contribute until it asks.

```
Host                                Extension (.wasm)
 |                                        |
 |-- load wasm binary ------------------>|
 |                                        |
 |-- __handshake(host_version) --------->|
 |<-- name, version, protocol_version, --|
 |    peer_dependencies, sandbox_policy   |
 |                                        |
 |-- __describe("entities") ------------>|
 |<-- entity kind descriptors ------------|
 |                                        |
 |-- __describe("edges") --------------->|
 |<-- edge type descriptors --------------|
 |                                        |
 |-- __describe("validation_rules") ---->|
 |<-- validation rule descriptors --------|
 |                                        |
 |   ... every declared category, once    |
 |                                        |
 |-- (register contributions) ---------->|  (internal)
 |                                        |
 |-- cmd__validate(args) --------------->|  (on demand)
 |<-- result -----------------------------|
 |                                        |
 |-- (disconnect) ---------------------->|  (graceful)
```

## Discovery Protocol

### Handshake

The handshake is the first call the host makes after loading a Wasm binary. It establishes identity, compatibility and the extension's metadata.

**Export:** `__handshake`

**Input:**

```json
{ "host_version": "1.4.0" }
```

**Output** (`specforge_protocol_types::HandshakeResponse`):

```json
{
  "protocol_version": "1.4.0",
  "name": "@specforge/software",
  "version": "1.0.0",
  "peer_dependencies": [
    { "name": "@specforge/product", "version": "^1.0", "optional": true }
  ],
  "sandbox_policy": { "max_memory_mb": 256, "max_execution_ms": 5000 },
  "starter_template": "spec \"{project}\" {\n  version \"{version}\"\n}\n",
  "theme_color": "#4a90d9",
  "ext_short": "software",
  "description": "Software design: behaviors, invariants, events, types and ports",
  "keywords": ["design", "contracts"]
}
```

Peer dependencies are SemVer ranges read as Cargo reads them; one rule judges them against the loaded versions (E027; a range that is not SemVer is E073, ADR 0041). Extensions load in `specforge.json` order, except that each loads after its peers. A cycle among **required** peers is one E027 naming its extensions. **Optional** peers are only a preference: extensions may name each other as optional peers, and the host adds the optional edges after the required ones in name order, skipping any that would close a cycle. A rule on another extension's kind that this one works without uses the rule's `target_extension` (see "Category: validation_rules"), not a peer.

`protocol_version`, `name`, `version`, `peer_dependencies` and `sandbox_policy` are required on the wire (`sandbox_policy` may be `null`: the extension then runs under the host's ceiling; see Sandbox). The others are optional and omitted when absent:

- `starter_template`: the text of the starter `.spec` file `specforge init` writes for a project that enables the extension, `{project}` standing for the project's entity id and `{version}` for its version. When several enabled extensions declare one, `init` uses the first listed in `specforge.json`. SDK: `ContributionsBuilder::starter_template`.
- `migration_hook`: the export `specforge migrate` calls after migrating the project's files. SDK: `ContributionsBuilder::migration_hook`.
- `theme_color`: the hex colour (`#rgb`, `#rrggbb` or `#rrggbbaa`) the `model` and `outline` diagrams draw the extension in; grey otherwise. SDK: `ContributionsBuilder::theme_color`.
- `ext_short`: the short name that routes the extension's commands, `specforge <ext_short> <command>` on the CLI and `specforge.<ext_short>.<id>` over MCP. Lowercase kebab case (`[a-z][a-z0-9-]*`); a malformed one is E030. Absent, it is the name's last segment (`@specforge/product` is `product`). SDK: `#[extension(short = "...")]`, checked at compile time.
- `description` and `keywords`: what a package registry shows for the extension. `specforge publish` uploads them with the rest of the declaration. SDK: `#[extension(description = "...")]`, `ExtensionMeta::keywords`.
- `providers`: `true` when the extension implements configured providers (a `specforge.json` `providers` entry may name it; W118 otherwise). SDK: `ContributionsBuilder::providers`.

What an extension contributes is what it declares in the describe categories; nothing on the handshake says it (ADR 0050).

The host checks `protocol_version`: a major version other than its own fails the extension's load (E028), and its handshake's `sandbox_policy` limits every later call into it (see Sandbox).

### Describe

After the handshake, the host calls `__describe(category)` once for each category it reads, always, in this order: `entities`, `edges`, `shared_fields`, `enhancements`, `validation_rules`, `surfaces`, `collectors`, `analyzers`, `passes`, `feature_flags` (`specforge_protocol_types::DECLARED_CATEGORIES`). Together with the handshake they are the extension's **declaration** (`specforge_protocol_types::ExtensionDeclaration`, ADR 0012): the host loads it once per environment load (`specforge_wasm::protocol::load_declaration`) and nothing describes a category again. An extension answers every supported category, `[]` when it contributes nothing to it; the SDK does.

A category whose answer fails or doesn't parse as its descriptors fails the extension's load (E028), naming the category. An item key a descriptor doesn't define is ignored and reported (W138), naming the extension, the category, the item and the key.

**Export:** `__describe`

**Input:**

```json
{
  "category": "entities"
}
```

**Output:**

```json
{
  "category": "entities",
  "items": [ ... ]
}
```

`items` is an array of the category's descriptors.

### Describe Categories

| Category | What it returns | Read by the host |
|----------|----------------|--------------------------|
| `entities` | Entity kind descriptors (keyword, fields, LSP and DOT metadata) | Always |
| `edges` | Edge type descriptors (label, source/target kind, visual style) | Always |
| `shared_fields` | Field descriptors every kind of the extension gets (a kind's own field of the same name wins) | Always |
| `enhancements` | Fields and verify kinds added to other extensions' entity kinds | Always |
| `validation_rules` | Declarative and custom validation rule descriptors | Always |
| `surfaces` | One descriptor of the CLI commands, MCP tools and MCP resources, or none | Always |
| `collectors` | Test result collector descriptors | Always |
| `analyzers` | Language analyzer descriptors (`specforge infer`) | Always |
| `passes` | Compiler pass descriptors with ordering constraints | Always |
| `feature_flags` | Feature flag descriptors | Always |

### Category: entities

Returns entity kind descriptors. Each descriptor declares a DSL keyword, its fields, and metadata for LSP and DOT rendering.

```json
{
  "category": "entities",
  "items": [
    {
      "name": "Behavior",
      "keyword": "behavior",
      "description": "Behavioral contract for a single operation",
      "testable": true,
      "singleton": false,
      "supports_verify": true,
      "allowed_verify_kinds": ["smoke", "contract"],
      "incremental": true,
      "has_body_parser": false,
      "open_fields": false,
      "semantic_token": "function",
      "lsp_icon": "Method",
      "dot_shape": "box",
      "dot_color": "#1565C0",
      "dot_fillcolor": "#E3F2FD",
      "fields": [
        {
          "name": "contract",
          "field_type": "block",
          "required": true,
          "description": "The behavioral contract"
        },
        {
          "name": "invariants",
          "field_type": "reference_list",
          "edge": "BehaviorEnforcesInvariant",
          "target_kind": "invariant",
          "description": "Invariants this behavior enforces"
        }
      ]
    }
  ]
}
```

Optional flags, `false` and omitted from the wire unless set (ADR 0007): on
a kind, `contract_target` (a reference field that targets the kind is a
contract obligation, A010) and `declares_types` (its entity ids are the type
names custom validators receive as `declared_types`); on a field, `normative`
(the context export keeps it), `headline` (the context export lifts it to the
node's top level; with `normative`, it is the statement MCP `inspect` reports
as `contract`) and `exempts_obligations` (an entity that sets it owes no
`verify` obligations).

Field types (`specforge_protocol_types::FieldType`): `string`, `integer`,
`bool`, `enum` (values in `enum_values`), `string_list`, `reference`,
`reference_list`, `block`. The host also reads the `_type`-suffixed
spellings (`string_type`, ...) and `boolean`; an unknown type drops the
field with W019. Every host output names a field type by these names — the
Graph Protocol schema and exports, the published JSON Schema, the model,
hover and E061; the older spellings are read, never written (ADR 0034).

What a field takes and what is written for it (ADR 0064): `string` takes text, a bare word, a date, a type
union or an `expr { }` group, and is written as a string (a group as its conjuncts, one per line); `enum` one of
its `enum_values`; `integer` and `bool` their JSON scalars; `reference` one id, written as a string;
`string_list` and `reference_list` a list (a single value or a mixed list is stored as a list), written as an
array of strings; `block` a block, written as an object of its entries. Anything else is E061. A command's
`graph` input, the exports and MCP carry exactly these forms.

### Category: edges

Returns edge type descriptors. Each descriptor declares a labeled relationship between entity kinds.

```json
{
  "category": "edges",
  "items": [
    {
      "label": "BehaviorEnforcesInvariant",
      "description": "This behavior enforces these invariants",
      "source_kind": "behavior",
      "target_kind": "invariant",
      "edge_style": "dashed",
      "edge_color": "#C62828",
      "edge_arrowhead": "normal"
    }
  ]
}
```

### Category: shared_fields

Returns the field descriptors every entity kind of the extension gets; a kind's own field of the same name wins.

```json
{
  "category": "shared_fields",
  "items": [
    {
      "name": "tags",
      "field_type": "string_list",
      "description": "Freeform labels"
    }
  ]
}
```

### Category: enhancements

Returns field enhancements that add fields to entity kinds owned by other extensions. This is the mechanism for cross-extension composition.

```json
{
  "category": "enhancements",
  "items": [
    {
      "target_kind": "module",
      "source_extension": "@specforge/product",
      "fields": [
        {
          "name": "ports",
          "field_type": "reference_list",
          "edge": "ModuleConsumesPort",
          "target_kind": "port",
          "description": "Port interfaces this module consumes"
        }
      ]
    }
  ]
}
```

### Category: validation_rules

Returns both declarative rules (pattern-based, evaluated by the host) and custom rules (Wasm-backed, evaluated by calling extension exports).

```json
{
  "category": "validation_rules",
  "items": [
    {
      "code": "W001",
      "severity": "warning",
      "message_template": "behavior '{id}' does not implement any feature",
      "check": "no_outgoing_edges",
      "target_kind": "behavior",
      "edge_type": "BehaviorImplementsFeature"
    },
    {
      "code": "W009",
      "severity": "warning",
      "message_template": "{kind} '{id}' has verify kind '{value}' not in allowed set {allowed}",
      "check": "custom",
      "wasm_function": "validate__verify_kind_allowlist"
    }
  ]
}
```

Check kinds (`specforge_protocol_types::CheckKind`). The descriptor stays flat
and string-typed on the wire; the host's registry build turns each rule into a
typed rule (`specforge_registry::rules`, ADR 0020) and checks its shape against
this table. A rule missing what its check **requires** (or with an unknown
check) is **W112** and is not registered. A property its check does **not read**
is **W147**: the rule is registered without it. `field` is never W147: every
check's message reads it as the default `{field}`, and its text as the default
`{value}`. `{allowed}` is the values of a `one_of` constraint (on
`field_value_constraint` or `verify_kind_allowlist`), joined by `, `; any other
check leaves it as written.

| Check | Fires when | Requires (W112 when missing) | Reads | W147 when set |
|-------|------------|------------------------------|-------|---------------|
| `no_incoming_edges` | no edge points at the entity (only edges of `edge_type`, from its source kind, when set) | — | `edge_type` | `constraint`, `wasm_function` |
| `no_outgoing_edges` | the entity points at nothing (only edges of `edge_type`, to its target kind, when set) | — | `edge_type` | `constraint`, `wasm_function` |
| `no_edges` | the entity has no edges in either direction | — | — | `edge_type`, `constraint`, `wasm_function` |
| `missing_field_when_flag_set` | `field` is absent (an entity owing no `verify` statements is exempt for `verify`) | `field` | `field` | `edge_type`, `constraint`, `wasm_function` |
| `missing_required_field` | `field` is absent | `field` | `field` | `edge_type`, `constraint`, `wasm_function` |
| `file_exists` | the path in `field` (each item of a list field) does not exist, relative to the spec root | `field` | `field` | `edge_type`, `constraint`, `wasm_function` |
| `field_value_constraint` | `field`'s value breaks the constraint | `field`; a constraint `non_empty`, `one_of` with values, or `matches` with a `pattern` that compiles | the constraint | `edge_type`, `wasm_function`; `pattern` on `non_empty`/`one_of`; `values` on `non_empty`/`matches` |
| `conditional_field_required` | `field` is absent or empty while the field named by `constraint.pattern` holds one of `constraint.values` | `field`; `constraint.pattern` and non-empty `constraint.values` | constraint kind `when_field_equals` | `edge_type`, `wasm_function`; any other constraint kind (read as `when_field_equals`) |
| `cycle_detection` | the entity sits on a cycle of `edge_type` edges, following every field that writes it | `edge_type` | `target_kind` (unset: every entity) | `constraint`, `wasm_function` |
| `verify_kind_allowlist` | a `verify` kind is not in `constraint.values` | a constraint with non-empty `values`; a target kind that accepts `verify` | constraint kind `one_of` | `edge_type`, `wasm_function`; `pattern`; any other constraint kind (read as `one_of`) |
| `no_verify_statements` | a testable entity declares no `verify` obligations (or does not write `field` when it names another obligation field) | a target kind that accepts `verify` (for `verify` statements) | `field` (default `verify`) | `edge_type`, `constraint`, `wasm_function` |
| `custom` | the rule's `wasm_function` export answers `fail` | `wasm_function` | — | `edge_type`, `constraint` |

Older spellings earlier SDK releases emitted (`missing_field`,
`field_constraint`, `cycle`, `conditional_required`) are still read.

Constraint kinds (`specforge_protocol_types::ConstraintKind`): `non_empty`,
`one_of` and `matches` (regex in `pattern`) for `field_value_constraint`;
`when_field_equals` (condition field in `pattern`, triggering values in
`values`) for `conditional_field_required`; `one_of` for
`verify_kind_allowlist`. Any other kind on a `field_value_constraint` rule
drops the rule with W112.

References resolve against the loaded registries. A rule whose target kind or
edge type no loaded extension declares reports nothing (it belongs to an
extension that is not installed); an edge type resolves through the edge
registry only, never as a field name. A target kind or edge type that neither
the extension, its declared peers nor the rule's `target_extension` declare is
**W021** (anything goes while a named peer is not loaded).

`target_extension` (optional, protocol `1.1.0`) names the extension whose kind
or edge type a rule is about when that is neither the declaring extension nor
one of its declared peers: a rule on another extension's kind that this one
works without. No peer dependency is needed (a peer orders extension loading
and pins a version range). While the named extension is not loaded, that rule
alone is inert and costs no W021; loaded but without the kind or edge type, it
is W021. With no `target_extension`, a kind only a non-peer extension declares
is W021 suggesting it. A `custom` rule's function is probed once at load
(W112 when it cannot answer); a function that fails on real entities during a
check is **W148**, once per rule, its data listing every entity that was not
checked.

### Category: surfaces

Returns CLI command, MCP tool, and MCP resource descriptors. CLI commands auto-promote to MCP tools. Each surface declares its arguments and export name.

```json
{
  "category": "surfaces",
  "items": [{
    "commands": [
      {
        "id": "validate",
        "title": "Run validation",
        "description": "Run product validation rules",
        "category": "analysis",
        "export": "cmd__validate",
        "args": [
          { "name": "target", "arg_type": "string", "required": true, "description": "The entity to validate" },
          { "name": "profile", "arg_type": { "enum": { "values": ["default", "strict"] } }, "default_value": "default", "description": "Lint profile" },
          { "name": "limit", "arg_type": "integer", "minimum": 0, "description": "Report at most this many" },
          { "name": "details", "arg_type": "bool", "description": "Show each finding" }
        ]
      }
    ],
    "mcp_tools": [
      {
        "name": "acme.model",
        "description": "Generate entity model",
        "category": "visualization",
        "export": "mcp__acme_model",
        "input_schema": { "type": "object", "properties": { "format": { "type": "string" } } }
      }
    ],
    "mcp_resources": [
      {
        "uri_template": "specforge://ext/acme/{kind}",
        "name": "entity_list",
        "description": "List entities by kind",
        "mime_type": "application/json",
        "export": "mcp__entity_list"
      }
    ]
  }]
}
```

A command arg declares `name`, `arg_type` (`"string"`, `"path"`, `"bool"`, `"integer"`, or
`{"enum": {"values": [..]}}`), and optionally `required`, `default_value` (a string, read as the arg's
type), `description` and `minimum` (the least value of an integer arg: `0` for a count; a host or guest
that does not know the field ignores it). The host reads the declaration by one rule, the SDK's too
(`specforge_protocol_types::command_args`, ADR 0017):

- **Refused declarations.** A command with an arg named `path`, `format` or `help` (the host's own
  options), two args whose names spell one option (`all_kinds`, `all-kinds`), a flag or a required arg
  with a default, or a default its type refuses, runs on no surface: the CLI refuses it (exit 2) and MCP
  serves no tool for it (I017). The SDK refuses to build such an extension.
- **Normalized args.** Both surfaces send a `cmd__` export the same args for the same input: every
  declared arg the caller set, as its type (an integer or a flag may come as the string a command line
  gives); an absent arg its default; an unset flag `false` (a flag is never required). A missing
  required arg, a value of another type or below the minimum, or an undeclared argument is refused
  before the export runs, as one `{"code": "INVALID_INPUT", "message", "suggestion"?}` object on both
  surfaces.
- **The MCP tool** of a command is `specforge.<short>.<id>`; its `inputSchema` states each arg's type,
  values, minimum, default and description, `required` lists the required args that are not flags, and
  `additionalProperties` is `false`.

MCP serves each tool name and resource URI once: the core ones, then explicit tools and resources in
extension load order, then the commands. A contribution not served under its name (an explicit tool
named as a core tool, a resource whose URIs a core resource serves, a command whose tool name is taken)
is reported with I017. A resource template may use any scheme; its read failing (a trap, an answer that
is not `{content, mime_type}`) is a JSON-RPC internal error whose `data` is an McpError carrying E028.

### Body syntax

Extensions cannot contribute grammars or body parsers (ADR 0004, D5-a, ADR 0050). A kind whose
body carries syntax of its own may set `has_body_parser` on its entity descriptor; the flag changes
nothing in the compiler: the core grammar reads every body and every parse error in it is reported.

### Category: collectors

Returns the test-result collectors a runner extension contributes
([ADR 0002](adr/0002-test-runner-extensions.md)): the project-root files that
select it (`auto_detect.file_patterns`, last segment may use `*`), the command
`specforge collect` runs with the user's consent (`run`; `{report}` expands to
the absolute report path, also exported as `SPECFORGE_REPORT`), where the
report lands (`report`, a file or directory inside the project; default
`.specforge/reports/<name>.json`), whether the host also keeps the command's
standard output (`capture: "stdout"`, for runners whose results only appear
there), and the pure export that maps the report to entities.

```json
{
  "category": "collectors",
  "items": [
    {
      "name": "cargo-test",
      "input_formats": ["specforge-test-json"],
      "export": "collect__cargo_test",
      "auto_detect": { "file_patterns": ["Cargo.toml"], "env_vars": [] },
      "run": ["cargo", "test", "--workspace", "--no-fail-fast"],
      "report": "target/specforge",
      "capture": "stdout"
    }
  ]
}
```

The host calls the export with a `CollectInput`, `{"reports": [{"path",
"content"}], "stdout"?}` (`stdout` only when the collector captures it), and
reads a `CollectOutput`, `{"entity_results": [{"entity_id", "test_results":
[{"name", "status", "verify"?, "duration_ms"?}]}], "unlinked"?: [{"name",
"path", "status"}]}` (each test a `TestResult`), with `status` a `TestStatus`:
`passed`, `failed` or `skipped`, the one vocabulary of the collect wire, the
recorded test report and a pass's test results. `entity_results`, each test's
`name` and its `status` are required, and a status outside the three is not a
`CollectOutput`: an answer without them is E028 naming the collector, never
empty results or a skipped test.
`unlinked` lists tests the report doesn't link to an entity (`path` is the
test's name split into segments, its own name last); the host links them by
naming convention when it can (`entity_id__obligation_slug`, or a module
named after an entity).

### Category: passes

Returns compiler pass descriptors. Each pass declares ordering constraints relative to the built-in resolve phase and other passes.

`phase: "check"` makes the pass part of every compile: it runs after the
graph checks, and its diagnostics join the compile's (`specforge check`,
watch, the LSP, MCP). Passes with any other phase, or none, run only under
`specforge analyze`. The `__pass_<name>` export receives a `PassInput`
(`{"entities", "edges", "test_results"?, "proved_claims"?, "previous"?}`;
`test_results` is the recorded test report as `TestResults`, `{"runner"?,
"results": {"<id>": {"tests": [TestResult]}}}`;
each entity is `{"id", "kind", "fields", "incoming_edge_count",
"outgoing_edge_count", "span"?, "testable", "owes_none", "verify_kinds",
"verify_texts"}`, `owes_none` meaning it owes no obligations of its own, decided
by the host from the registries) and answers host diagnostics, bare or as
`{"diagnostics", "summary"}` (a `PassAnswer`). A diagnostic may carry
`"entity": "<id>"`; with no `span`, the host attaches that entity's.
`previous` (check passes only) is the build cache, `{"statuses": {"<id>":
{"kind", "status"}}}`, when `specforge-cache.json` exists. See
[Operate](#operate) for every operation's input and answer.

```json
{
  "category": "passes",
  "items": [
    {
      "name": "condition_check",
      "after": "resolve",
      "description": "Validate structured condition consistency"
    },
    {
      "name": "layering_verify",
      "after": "condition_check",
      "description": "Verify specification layering constraints"
    }
  ]
}
```

### Category: feature_flags

Returns feature flag descriptors. Each flag declares allowed values and a default.

```json
{
  "category": "feature_flags",
  "items": [
    {
      "name": "warning_level",
      "values": ["default", "strict"],
      "default": "default",
      "description": "Controls which warnings are emitted"
    }
  ]
}
```

## Operate

Once the declaration is loaded, the host calls an extension's exports on
demand, each through one call of the bridge `call(name, export, input)` with a
JSON input, reading a JSON answer ([ADR 0013](adr/0013-typed-extension-calls.md)).
Every input and answer is a type of `specforge_protocol_types` (module
`calls`), the same definitions the SDK re-exports, so a Rust guest built with
the SDK cannot answer the wrong shape; a guest in another language reads the
table below and the goldens in `crates/runtime/wasm/tests/wire/`.

| Operation | Export | Input | Answer |
|---|---|---|---|
| Handshake | `__handshake` | `HandshakeRequest` `{"host_version"}` | `HandshakeResponse` |
| Describe | `__describe` | `DescribeRequest` `{"category"}` | `DescribeResponse` `{"category", "items"}` |
| Command | the command's `export` (`cmd__<id>`) | `CommandInput` `{"args", "cwd", "format", "today", "graph"}`; `cwd` is the project root, absolute and canonical | `CommandOutput` `{"exit_code", "stdout"?, "stderr"?}` |
| MCP tool | the tool's `export` (`mcp__<name>`) | the JSON its `input_schema` describes | the JSON its `output_schema` describes |
| MCP resource | the resource's `export` (`mcp__<name>`) | `McpResourceRequest` `{"uri"}` | `McpResourceContent` `{"content", "mime_type"}` |
| Compiler pass | `__pass_<name>` | `PassInput` | `PassAnswer`: `[PassDiagnostic]` or `{"diagnostics", "summary"?}` |
| Collector | the collector's `export` (`collect__<name>`) | `CollectInput` | `CollectOutput` |
| Custom validator | the rule's `wasm_function` (`validate__<code>`) | `ValidatorContext` `{"entity", "referenced", "declared_types", "primitives"}` | `ValidatorVerdict` `{"verdict": "pass"}` or `{"verdict": "fail", "field"?, "value"?}` |
| Scanner | the analyzer's `scan_export` (`scan__<language>`) | `ScanRequest` `{"file_path", "content"}` | `ScanResponse` `{"items": [{"name", "item_kind", "line", "visibility"?, "signature"?}], "language"?}` |
| Migration hook | the handshake's `migration_hook` | `MigrationInput` `{"from", "to", "files"}` | not read |

An analyzer also declares `classify_export` and `map_export`; the host does
not call them.

The wire rules, on both sides:

- **Absent, never null.** An optional field that is unset is left out (the
  host leaves out a collect input's `stdout`, a pass input's `test_results`,
  `proved_claims` and `previous`, an entity's `span`), and an absent optional
  field reads as its default.
- **Strict on shape, lenient on unknown fields.** A required field is required:
  a command answer without `exit_code`, a resource answer without `mime_type`,
  a collected test without `name` or with a status outside `passed`,
  `failed` and `skipped` is not that type. A field the reader does
  not know is ignored, so a newer peer may add one.

Every failure of a call is E028, in one shape: `<operation> <export>() of
'<extension>' trapped: <kind>: <message>` (the export trapped, ran out of time
or fuel, or the guest does not route it: `guest_error: unknown export
'<export>'`), `... answered output that is not a <Type>: <reason>`, or `... is
not loaded`, with the suggestion to report it to the extension's author. What
the failure costs is the operation's: a check pass's is a compile error, an
analyze pass's an E028 finding of that pass (the analysis fails), a command's
its error (exit 1; under `--format json` one `{code, message, suggestion}`
object on stderr), an MCP tool's or resource's a structured MCP error, a
scanner's an entry of the gap report's `scan_failures` (the report is then
approximate), a collector's the `collect` error, a custom validator's the
probe's W112 at load, a migration hook's a failure line that rolls the
migration back.

With the SDK, every one of these exports is declared together with its
handler (`ContributionsBuilder::command`, `mcp_tool`, `mcp_resource`, `pass`
with `run`, `collector` with `collect`, `rule` with `validate`, `analyzer`
with `scan`, `migration_hook_handler`); the SDK decodes the input and encodes
the answer. See [the SDK guide](extension-sdk.md).

## Host Functions

The protocol is bidirectional in design: the host drives the conversation
(calling exports on the Wasm module), and extensions may call back into the
host through imported functions to read the graph, emit diagnostics, and
access files.

> **Status: not callable from guests today.** The old extism-era import
> surface was removed with the component-model cutover. Guests are
> pure-compute wasip2 components; the host passes all needed context as call
> input (e.g. the `ValidatorContext` snapshot for `validate__*` exports). A
> typed component host-import surface is future work — the names below are
> the planned surface, specified in `spec/behaviors/wasm-host-functions.spec`.
> The permissions those functions would check come with them; today's sandbox grants none
> (Sandbox, ADR 0037).

### Host Function Table

| Function | Signature | Purpose | Allowed call sites |
|----------|-----------|---------|--------------------|
| `host_query_graph` | `(pattern: &str) -> Vec<Entity>` | Scope-limited query of the entity graph by kind, field values, or graph pattern | all |
| `host_emit_diagnostic` | `(severity: Severity, code: &str, msg: &str)` | Emit a diagnostic to the host's diagnostic collection | all |
| `host_read_file` | `(path: &str) -> Option<String>` | Read a file from the project (subject to sandbox policy) | Validator, Provider, Parser, Analyzer |
| `host_emit_file` | `(path: &str, content: &[u8])` | Write renderer/collector output (subject to sandbox policy) | Renderer, Collector |
| `host_http_get` | `(url: &str) -> Vec<u8>` | Fetch from an allowlisted domain | Provider |
| `host_add_graph_node` | `(kind: &str, id: &str, fields: ...)` | Add a graph node | Parser |
| `host_add_graph_edge` | `(label: &str, source: &str, target: &str)` | Add a graph edge | Parser |

Reference resolution — resolving an entity ID to its typed node, with a
`None`/null kind for dangling IDs — is part of `host_query_graph` semantics
(the scope-filtered graph carries nodes and edges) and of the precomputed
`ValidatorContext.referenced` snapshot handed to `validate__*` exports.

### Host API Versioning

The host-function surface is versioned alongside the wire protocol (`protocol_version` in the handshake response). A compatible major version guarantees the same function names and signatures.

| Protocol Version | Functions Available |
|-----------------|-------------------|
| `1.0.0` | `host_query_graph`, `host_emit_diagnostic`, `host_read_file`, `host_emit_file`, `host_http_get`, `host_add_graph_node`, `host_add_graph_edge` |
| `1.1.0` | the same as `1.0.0` |
| `1.2.0` | the same as `1.0.0` |
| `1.3.0` | the same as `1.0.0` |
| `1.4.0` | the same as `1.0.0` |

### Protocol Versions

The protocol version is semver (`specforge_protocol_types::PROTOCOL_VERSION`). The host loads every
guest of its major version, whatever minor it was built with, and sends its own version as
`host_version` in every handshake request. A guest built with the SDK declares the SDK's version in
its handshake and its registry manifest.

The minor moves when a payload changes in a way every older guest survives: a field added, removed or
given a new meaning. The major moves only when an older guest could no longer be decoded
or answered.

| Version | What it guarantees |
|---------|--------------------|
| `1.0.0` | The baseline: the handshake, describe and operate payloads of this document. |
| `1.1.0` | A validation rule's optional `target_extension` (ADR 0020). Field text (ADR 0019): every field an entity writes has one text, the same in a pass's `PassEntity.fields`, a validator's `ValidatorField.value` and what declarative rules match (see "Field text" in `extension-sdk.md`). A written field is present even when empty (`""`). A validator's field `value` is always a string (a variant list, mixed list, expression or type union was `null`). `PassEntity.exempt` follows the host's one obligation rule: a `no_verify_statements` rule without a target kind obliges every kind that accepts `verify` statements (an entity of a kind that accepts none is exempt). |
| `1.2.0` | The declared categories are the only describe categories (`fields`, `grammars`, `body_parsers` are gone); the handshake request carries only `host_version`; the handshake response carries `providers` and no `contribution_flags`; a pass's `PassEntity` says `owes_none` where it said `exempt` (ADR 0050). A 1.2 host loads every 1.x guest and ignores an older guest's flags; a 1.1 host cannot read a 1.2 guest's handshake. |
| `1.3.0` | An enhancement declares no `edge_types`: an edge type is declared once, in `edges` (an enhancement's fields name it as their `edge`). A 1.3 host reports a 1.2 guest's enhancement `edge_types` key as W138 and ignores it (ADR 0060). |
| `1.4.0` | A recorded test is one `TestResult` with a `TestStatus`: `passed`, `failed` or `skipped`, decoded strictly on the collect wire (a collector's other status is E028), and a pass's `test_results` carry the same, `duration_ms` included (they said `pass`/`fail`). A 1.3 pass that reads `test_results` misreads 1.4 values (ADR 0069). |

A host of `1.0.x` still loads a `1.1.0` guest and hands it the `1.0.0` values; a guest that needs the
`1.1.0` values can read `host_version` in its handshake request.

### Sandbox

An extension runs with no capability: its component's WASI context preopens no directory, passes
no environment, arguments or stdin, discards stdout and stderr, and refuses sockets and name
lookup. What it needs it is handed in each call's input (a scanner gets each file's content, a
command the graph). No declaration grants more (ADR 0037).

The handshake's `sandbox_policy` declares limits, which the host enforces on every call:

```json
{ "max_execution_ms": 5000, "max_memory_mb": 256 }
```

| Limit | Ceiling (and value when undeclared) | Enforced by | A call past it |
|-------|-------------------------------------|-------------|----------------|
| `max_execution_ms` | 30000 | epoch interruption, checked every 10 ms; never before the budget | traps `deadline_exceeded` |
| `max_memory_mb` | 512 | a limit on the instance's linear memory | traps `memory_limit_exceeded` |
| (fuel, not declared) | a fixed instruction budget, whole for every call | fuel metering | traps `fuel_exhausted` |

A declared limit above its ceiling is held to it. A call that traps on a limit fails with E028
naming the limit, and the extension's next call gets a fresh instance under the same limits. A
`sandbox_policy` key other than the two limits that asks for something (`network_access: true`, a
non-empty `allowed_paths`, which older SDKs wrote), a surface's `sandbox` override, and a limit
above its ceiling are W153 at load.

## Hot Plug and Unplug

Extensions can connect and disconnect at runtime without restarting the host. This enables live extension management in the LSP and MCP server contexts.

### Connect

Connection follows the full lifecycle: load, handshake, describe, register.

```
1. Host loads .wasm binary from extension directory
2. Host calls __handshake(host_version)
3. Host validates protocol_version compatibility
4. Host checks peer_dependencies are satisfied
5. Host calls __describe(category) for every declared category, once
6. The registry build checks the declarations and registers them in KindRegistry, FieldRegistry, EdgeRegistry
7. Extension is now "connected"
8. Host calls cmd__*, validate__*, mcp__* exports as needed
```

### Disconnect

When an extension disconnects (removed, crashed, or explicitly unloaded), the host performs graceful degradation:

**Entities** from the disconnected extension become untyped nodes in the graph. They retain their IDs, fields, and connections, but lose:
- Field validation (unknown fields accepted)
- Type checking on reference targets
- Entity-specific DOT/LSP metadata

**Edges** declared by the disconnected extension remain in the graph as untyped connections. They retain source and target, but lose label semantics.

**Validation rules** from the disconnected extension stop firing. No false positives from rules that reference entity kinds that no longer have type information.

**Surfaces** (CLI commands, MCP tools, MCP resources) from the disconnected extension are removed from the registry. Calls to those surfaces return "extension not available" errors.

**Enhancements** contributed to other extensions' entity kinds are removed. Enhanced fields on those entity kinds revert to unrecognized fields (accepted but not validated).

### Reconnect

When a previously disconnected extension reconnects, the host repeats the full lifecycle from step 1. All entities and edges from the extension regain their type information. Validation rules resume. Surfaces become available again.

This matches SpecForge's existing soft reference philosophy. An entity referencing another entity from an uninstalled extension produces an `I004` info diagnostic rather than an error. The same principle applies at runtime: disconnection degrades gracefully, reconnection restores full semantics.

### Disconnect Diagnostics

When the host detects entities or references that belong to a disconnected extension, it emits `I004`:

```
info[I004]: Unknown entity 'create_user' in field 'behaviors'
  |
3 |   behaviors [create_user]
  |              ^^^^^^^^^^^ extension '@specforge/software' not connected
  |
  = help: The extension may have been disconnected. Reconnect it to restore validation.
```

## Lifecycle Summary

```
                    ┌─────────────────────────────────────────┐
                    │               HOST                       │
                    │                                         │
   ┌────────────┐  │  ┌──────────┐   ┌──────────────────┐   │
   │  .wasm     │──┼─>│  LOAD    │──>│  __handshake()   │   │
   │  binary    │  │  └──────────┘   │  -> metadata     │   │
   └────────────┘  │                 │  -> flags        │   │
                    │                 │  -> sandbox      │   │
                    │                 │  -> peers        │   │
                    │                 └────────┬─────────┘   │
                    │                          │              │
                    │                          v              │
                    │                 ┌──────────────────┐   │
                    │                 │  __describe()    │   │
                    │                 │  per category    │   │
                    │                 │  -> entities     │   │
                    │                 │  -> edges        │   │
                    │                 │  -> rules        │   │
                    │                 │  -> surfaces     │   │
                    │                 └────────┬─────────┘   │
                    │                          │              │
                    │                          v              │
                    │                 ┌──────────────────┐   │
                    │                 │  REGISTER        │   │
                    │                 │  KindRegistry    │   │
                    │                 │  FieldRegistry   │   │
                    │                 │  EdgeRegistry    │   │
                    │                 └────────┬─────────┘   │
                    │                          │              │
                    │                          v              │
                    │                 ┌──────────────────┐   │
                    │                 │  CONNECTED       │   │
                    │                 │  cmd__*          │   │
                    │                 │  validate__*     │   │
                    │                 │  mcp__*          │   │
                    │                 │  __pass_*        │   │
                    │                 │  collect__*      │   │
                    │                 │  scan__*         │   │
                    │                 └────────┬─────────┘   │
                    │                          │              │
                    │                          v              │
                    │                 ┌──────────────────┐   │
                    │                 │  DISCONNECT      │   │
                    │                 │  - entities ->   │   │
                    │                 │    untyped nodes │   │
                    │                 │  - rules stop   │   │
                    │                 │  - surfaces     │   │
                    │                 │    removed      │   │
                    │                 └────────┬─────────┘   │
                    │                          │              │
                    │                          v              │
                    │                 ┌──────────────────┐   │
                    │                 │  RECONNECT       │   │
                    │                 │  (repeat from    │   │
                    │                 │   LOAD)          │   │
                    │                 └──────────────────┘   │
                    │                                         │
                    └─────────────────────────────────────────┘
```

## Wasm Export Naming Conventions

All extension exports follow a strict naming convention that the host uses to dispatch calls:

| Prefix | Purpose | Example |
|--------|---------|---------|
| `__handshake` | Protocol handshake | `__handshake` |
| `__describe` | Category description | `__describe` |
| `cmd__` | CLI command execution | `cmd__product_features` |
| `mcp__` | MCP tool or resource execution | `mcp__model` |
| `__pass_` | Compiler pass | `__pass_coverage` |
| `collect__` | Collector execution | `collect__cargo_test` |
| `validate__` | Custom validation logic (a rule's `wasm_function`) | `validate__port_methods` |
| `scan__` | Source scanner (an analyzer's `scan_export`) | `scan__rust` |
| `classify__`, `map__` | Declared by analyzers; the host does not call them | `classify__rust` |
| (any name) | Migration hook (the handshake's `migration_hook`) | `migrate_acme` |

## Design Principles

The Extension Protocol embodies three SpecForge principles:

**Principle 2 (zero domain knowledge in core):** The host never hardcodes knowledge of what extensions will contribute. It discovers everything through `__handshake` and `__describe`.

**Principle 7 (extensions over built-ins):** The protocol is the sole mechanism for adding domain vocabulary. There is no alternative path that bypasses it.

**Principle 8 (seconds to value):** The declaration is read once per environment load, the handshake and one `__describe` call per declared category, and nothing describes a category again; the operations that follow call only the exports they need.
