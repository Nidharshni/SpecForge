---
id: RES-24
kind: research
title: "Surface Contribution Model — 10-Expert Analysis of Extension-Contributed CLI, MCP, and LSP Extensions"
status: active
date: 2026-03-04
depends_on: [RES-23, RES-18, RES-11a]
priority: critical
tags: [cli, mcp, lsp, extensions, extensions, surfaces, ai-agents, wasm]
---

# RES-24: Surface Contribution Model

## Executive Summary

RES-23 established the contribution-based extension model for the **compilation pipeline** (entities, edges, validators, ref_schemes, query_extensions). These are *graph contributions* — they extend SpecForge's internal spec graph. But extensions cannot yet extend the **tooling surfaces** through which users and AI agents interact with SpecForge: CLI, MCP, and LSP.

This creates a capability asymmetry:
- `@specforge/rust` needs `specforge collect rust` as a CLI command but it's hardcoded
- `@specforge/gh` should expose GitHub ref resolution as an MCP tool but can't
- `@specforge/product` could provide entity-aware completions in LSP but has no hook

The core insight driving this research: **MCP >= CLI + LSP**. MCP is the union surface for AI agents. Every CLI command should be available as an MCP tool. Every LSP feature should be queryable via MCP. Extensions that extend one surface should automatically extend the superset.

A **10-expert panel** evaluated six alternative architectures for surface contributions, ranging from minimal auto-promotion to full VS Code-style contribution points. This document presents all six alternatives with their tradeoffs, then synthesizes the panel's recommendation.

---

## Problem Statement

### 1. Hardcoded Surface Commands

`specforge collect rust` is described in RES-17 as a CLI command, but there is no mechanism for `@specforge/rust` to register it. The command must be hardcoded in `specforge-cli`. Every new language adapter requires a code change to the core CLI binary.

### 2. No MCP Server

SpecForge has no MCP server implementation. When one is built, every extension's capabilities should be automatically discoverable as MCP tools — but the current model has no concept of "surface exposure."

### 3. LSP is Static

The LSP server has fixed capabilities. An extension that adds `epic` and `story` entity kinds cannot contribute:
- Completion items for those kinds
- Hover documentation
- Code actions (e.g., "Extract behavior from this scenario")
- Diagnostic providers beyond `emit_diagnostic`

### 4. Surface Parity Gap

Users interact with SpecForge through three surfaces:

```
┌────────────────────────────────────────────────┐
│                 MCP (superset)                 │
│  ┌─────────────┐       ┌─────────────────┐    │
│  │    CLI      │       │      LSP        │    │
│  │  commands   │       │  completions    │    │
│  │  flags      │       │  hover          │    │
│  │  output     │       │  diagnostics    │    │
│  │             │       │  code actions   │    │
│  └─────────────┘       └─────────────────┘    │
│  + resources, prompts, sampling, streaming    │
└────────────────────────────────────────────────┘
```

AI agents prefer MCP. Humans prefer CLI and LSP/IDE. If extensions can only extend one surface, the others become second-class. The principle: **CLI + LSP < MCP**, therefore MCP must be the primary surface and the other two must derive from it or be co-equal.

### 5. Cold Start vs. Discovery

CLI startup must be fast (<100ms). Loading all Wasm modules to discover their surface contributions would destroy cold start. But MCP and LSP are long-running — they can afford initialization cost. This creates a tension between surface types that any solution must resolve.

---

## 10-Expert Panel

### Expert #1 — VS Code Extension Architect

**Prior art**: VS Code's `package.json` has a `contributes` object with 30+ contribution points: `commands`, `menus`, `views`, `configuration`, `languages`, `debuggers`, `themes`, `snippets`, `keybindings`, `taskDefinitions`, `customEditors`, `notebooks`, etc.

**Key insight**: Surface contributions are **statically declared** in the manifest and **dynamically activated** on demand. VS Code reads manifests at startup (fast — just JSON), but loads extension code lazily when a contribution is first invoked. This is the **lazy activation pattern**:

```
manifest read (cold)  →  contribution registered (warm)  →  code loaded (hot)
     ~1ms/ext              ~0ms (index update)                ~50ms (first use)
```

**Verdict**: Static manifest declaration with lazy Wasm loading. The manifest declares what surfaces an extension extends. The host indexes contributions at startup without loading Wasm. Wasm is loaded on first invocation.

### Expert #2 — Terraform/Cobra CLI Architect

**Prior art**: Terraform providers contribute resource types and data sources, but NOT CLI commands. Terraform's CLI is fixed. This is a deliberate design choice — CLI stability is a feature. Cobra (Go CLI framework) supports dynamic subcommand registration but discourages it because it makes `--help` non-deterministic.

**Counter-example**: Kubernetes `kubectl` supports plugins via the PATH — any `kubectl-foo` binary becomes `kubectl foo`. No registration needed. Discovery is filesystem-based.

**Key insight**: CLI command contributions should be **discoverable without loading Wasm** and should have a **clear namespace** to avoid collisions. The extension name IS the namespace.

**Verdict**: Extension-contributed CLI commands live under a namespace: `specforge <extension-action> [args]`. Example: `specforge collect rust` = extension `@specforge/rust`, action `collect`. The host reads this from the manifest, creates a clap subcommand dynamically, and loads Wasm only when invoked.

### Expert #3 — MCP Protocol Designer

**Prior art**: The MCP specification (2024-11-05) defines three primitives: **tools** (callable functions), **resources** (readable data), and **prompts** (templated text). Servers declare capabilities in `initialize` response. Tools have JSON Schema input definitions.

**Key insight**: MCP is inherently a **contribution aggregation** protocol. The server's tool list is the union of all capabilities. SpecForge's MCP server should be a thin dispatcher that routes tool calls to the appropriate extension's Wasm export.

**Architecture**:
```
AI Agent → MCP Client → SpecForge MCP Server → tool dispatch → Extension Wasm export
                              ↓
                      Built-in tools (compile, query, trace, ...)
                              +
                      Extension-contributed tools (collect_rust, resolve_github, ...)
```

**Verdict**: Every extension can contribute MCP tools, resources, and prompts. These are declared in the manifest with full JSON Schema. The MCP server aggregates them. Auto-promotion: every CLI command is also an MCP tool (with JSON input instead of argv).

### Expert #4 — LSP Protocol Expert

**Prior art**: LSP servers declare capabilities in `InitializeResult.capabilities`. Dynamic registration via `client/registerCapability` allows servers to add capabilities after initialization. The key capability types for extensions:

- `completionProvider` — trigger characters, resolve support
- `hoverProvider` — hover documentation
- `codeActionProvider` — quick fixes, refactors
- `diagnosticProvider` — pull diagnostics (3.17+)

**Key insight**: LSP capability contribution is **different in kind** from CLI/MCP contribution. CLI and MCP are request-response (extension handles full request). LSP is **collaborative** — multiple sources contribute to a single response (e.g., completions from core + completions from extension, merged into one list).

**Verdict**: LSP contributions should use a **provider pattern** — extensions register as additional providers for existing LSP capabilities, not as replacement implementations. The host merges results from all providers. This is fundamentally different from CLI/MCP where each tool is independent.

### Expert #5 — Wasm Component Model Expert

**Prior art**: The Wasm Component Model uses WIT (Wasm Interface Types) to define typed interfaces between components. A component declares its imports (what it needs) and exports (what it provides). This is compile-time checked.

**Key insight**: Surface contributions are really **interface contracts**. A CLI command has a contract (args → exit code + stdout). An MCP tool has a contract (JSON → JSON). An LSP provider has a contract (position → completions). These contracts should be explicit, not ad-hoc JSON blobs.

**Proposed WIT interfaces**:
```wit
interface cli-command {
    record cli-input {
        args: list<string>,
        flags: list<tuple<string, option<string>>>,
        stdin: option<string>,
    }
    record cli-output {
        exit-code: u8,
        stdout: string,
        stderr: string,
    }
    run: func(input: cli-input) -> cli-output
}

interface mcp-tool {
    run: func(input-json: string) -> string
}

interface lsp-completion-provider {
    record completion-context {
        uri: string,
        position: tuple<u32, u32>,
        trigger-char: option<string>,
        entity-kind: option<string>,
    }
    record completion-item {
        label: string,
        kind: u8,
        detail: option<string>,
        insert-text: option<string>,
    }
    provide: func(ctx: completion-context) -> list<completion-item>
}
```

**Verdict**: Use WIT-style typed interfaces for surface contributions. Even if not using the full Component Model, the typed contract pattern ensures correctness. Extism's PDK can implement these interfaces via JSON serialization over the existing host function mechanism.

### Expert #6 — AI Agent Systems Architect

**Prior art**: Claude Code, Cursor, and other AI agents discover tools dynamically. Tool count directly impacts token economics (RES-18): each tool definition costs ~100-200 tokens in the system prompt. An MCP server exposing 50 extension-contributed tools adds 5,000-10,000 tokens of overhead per conversation turn.

**Key insight**: Surface contributions must be **discoverable** but not necessarily **always loaded**. Agents need to know what tools exist (tool names + descriptions) but shouldn't pay the token cost of all tools in every conversation. This argues for tool categories, lazy tool loading, and agent-side tool filtering.

**Critical design principle**: Tool names must be **self-describing** and **greppable**. An agent looking at `specforge.collect_rust` instantly knows what it does. `specforge.pkg_3_action_1` is useless.

**Token budget analysis**:
```
Built-in tools:        ~10 tools × 150 tokens = 1,500 tokens
Extension tools (10 pkgs): ~30 tools × 150 tokens = 4,500 tokens
Total MCP overhead:    ~6,000 tokens per turn

Budget at 128K context:  ~5% overhead (acceptable)
Budget at 8K context:    ~75% overhead (unacceptable)
```

**Verdict**: MCP tools must have categories and support lazy listing. The MCP server should support `tools/list` with filtering (by extension, by category). Default: only show built-in tools + tools from extensions declared in project config. This prevents tool pollution.

### Expert #7 — Security / Sandbox Architect

**Prior art**: Chrome extensions declare permissions in manifest. Users grant permissions at install time. Extension code cannot access APIs it didn't declare.

**Key insight**: Surface contributions dramatically expand the attack surface. A CLI command can write to the filesystem. An MCP tool can execute arbitrary logic when called by an AI agent. An LSP provider runs in the editor process. Each surface type needs its own permission model, extending RES-23's per-call-site sandbox.

**Permission matrix**:
```
                    fs_read  fs_write  network  graph_read  graph_write  exec
CLI command           ✓        ✓*       ✓*        ✓           -          -
MCP tool              ✓*       ✓*       ✓*        ✓           -          -
MCP resource          ✓*       -        ✓*        ✓           -          -
LSP completion        -        -        -         ✓           -          -
LSP hover             -        -        -         ✓           -          -
LSP code action       -        ✓*       -         ✓           -          -

* = scoped by sandbox policy
```

**Verdict**: Each surface contribution type has a fixed permission ceiling. Sandbox policy can only restrict further, never expand. LSP providers are read-only by default (completion, hover). LSP code actions can write but only to spec files. CLI commands and MCP tools inherit from the extension's sandbox policy. The user confirms permissions at `specforge add` time.

### Expert #8 — Developer Experience Expert

**Prior art**: Building a VS Code extension requires understanding activation events, contribution points, extension API, packaging, and marketplace. This is a significant learning curve. Terraform provider development is simpler — implement a Go interface, done.

**Key insight**: Most extension authors will contribute 1-2 surface items, not dozens. The DX should optimize for the common case: "I want to add one CLI command to my extension." This should be ~10 lines of code on top of the existing implementation.

**Verdict**: The scaffolding (`specforge extension init`) should ask about surface contributions and generate the minimal boilerplate. Surface-specific test helpers in the Extism PDK: `test_cli_command(args, expected_output)`, `test_mcp_tool(input_json, expected_json)`. Documentation: one page per surface type, not a 50-page surface contribution guide.

### Expert #9 — Platform / Ecosystem Strategist

**Prior art**: VS Code's marketplace shows extensions by category (Themes, Linters, Debuggers). npm has keywords. Terraform Registry has provider/module distinction.

**Key insight**: Surface contributions create a **capability marketplace** on top of RES-23's contribution marketplace. Users browse by what they can DO, not what the extension IS. "Show me all extensions that add CLI commands" or "Show me extensions with MCP tools for GitHub."

**Critical success factor**: The first 5 extensions must demonstrate surface contributions working. If `@specforge/product`, `@specforge/governance`, `@specforge/gh`, `@specforge/rust`, and `@specforge/software` all contribute surface items, the pattern is validated. If only core has surfaces, extensions feel second-class.

**Verdict**: Ship surface contributions for the 5 launch extensions simultaneously with the core feature. Surface contribution is not a "Phase 2" afterthought — it's table stakes for the extension model to be credible.

### Expert #10 — Compiler Pipeline / Runtime Architect

**Prior art**: RES-23 maps contribution types to pipeline phases (parse → register → build → resolve → validate → export). Surface contributions don't fit this model — they're not compilation phases. They're **runtime extension points** that happen outside the compilation pipeline.

**Key insight**: There are two distinct extension planes:

```
COMPILE-TIME PLANE (RES-23):
  Parse → Register → Build Graph → Resolve → Validate → Export
  ↑ entities, edges, enhancements, ref_schemes, validators

RUNTIME PLANE (this research):
  CLI dispatch → MCP dispatch → LSP dispatch
  ↑ commands, tools, resources, prompts, completions, hover, code_actions
```

These planes share the same Wasm module and extension manifest but operate at different times with different lifecycles:
- Compile-time contributions run during `specforge compile`
- CLI contributions run when a command is invoked
- MCP contributions run when the MCP server handles a request
- LSP contributions run when the editor sends a notification

**Verdict**: Add a top-level `surfaces` key to the manifest, parallel to `contributes`. `contributes` is for compile-time pipeline extensions. `surfaces` is for runtime surface extensions. This cleanly separates the two planes.

---

## Alternative Architectures

The 10 experts converged on six distinct architectural options for implementing surface contributions. Each is presented with full schema, mechanics, pros, and cons.

---

### Option A: Static Manifest + Lazy Wasm Loading (VS Code Model)

**Championed by**: Experts #1, #2, #6, #8

#### Manifest Schema

```json
{
  "extension": "@specforge/rust",
  "version": "1.0.0",
  "wasm": "rust.wasm",

  "contributes": {
    "collectors": [{ "name": "rust", "description": "Parse cargo-nextest JUnit XML into specforge-report.json" }]
  },

  "surfaces": {
    "commands": [
      {
        "id": "collect",
        "title": "Collect Rust test results",
        "description": "Parse JUnit XML and emit specforge-report.json",
        "export": "cmd__collect",
        "args": [
          { "name": "junit-xml", "type": "path", "required": true },
          { "name": "out", "type": "path", "default": "specforge-report.json" },
          { "name": "format", "type": "enum", "values": ["json", "summary"], "default": "json" }
        ],
        "sandbox": { "fs_read": true, "fs_write": true }
      }
    ],
    "mcp_tools": [
      {
        "name": "collect_rust_results",
        "description": "Parse JUnit XML and return test-to-entity mappings",
        "export": "mcp__collect_results",
        "input_schema": {
          "type": "object",
          "properties": {
            "junit_xml_path": { "type": "string" },
            "entity_filter": { "type": "array", "items": { "type": "string" } }
          },
          "required": ["junit_xml_path"]
        },
        "sandbox": { "fs_read": true }
      }
    ],
    "mcp_resources": [
      {
        "uri_template": "specforge://rust/coverage/{entity_id}",
        "name": "Rust entity test coverage",
        "description": "Get test coverage data for a specific entity",
        "export": "mcp__coverage_resource",
        "mime_type": "application/json"
      }
    ],
    "lsp": {
      "completion_providers": [
        {
          "entity_kinds": ["behavior", "invariant"],
          "trigger_characters": ["\""],
          "export": "lsp__complete_verify_kinds"
        }
      ],
      "hover_providers": [
        {
          "entity_kinds": ["behavior", "invariant"],
          "export": "lsp__hover_coverage"
        }
      ]
    }
  }
}
```

#### Wasm Export Protocol

```
IF surfaces.commands declared:
  fn cmd__{id}(args_json_ptr: u64) -> u64
     Input:  { "args": {...}, "flags": {...}, "cwd": "..." }
     Output: { "exit_code": 0, "stdout": "...", "stderr": "..." }
     Host enables:  query_graph, emit_diagnostic, fs_read*, fs_write*, http_get*
     (* based on per-command sandbox)

IF surfaces.mcp_tools declared:
  fn mcp__{name}(input_json_ptr: u64) -> u64
     Input:  JSON matching input_schema
     Output: MCP tool result JSON
     Host enables:  query_graph, fs_read*, http_get*

IF surfaces.mcp_resources declared:
  fn mcp__{name}(uri_ptr: u64) -> u64
     Input:  resolved URI string
     Output: resource content (text or blob)
     Host enables:  query_graph, fs_read*

IF surfaces.lsp.completion_providers declared:
  fn lsp__complete_{name}(context_json_ptr: u64) -> u64
     Input:  { "uri": "...", "position": {...}, "entity_kind": "..." }
     Output: { "items": [...] }
     Host enables:  query_graph (read-only)

IF surfaces.lsp.hover_providers declared:
  fn lsp__hover_{name}(context_json_ptr: u64) -> u64
     Input:  { "uri": "...", "position": {...}, "entity_kind": "..." }
     Output: { "contents": "..." }
     Host enables:  query_graph (read-only)
```

#### CLI Dispatch

```
User types: specforge collect rust --junit-xml results.xml

1. CLI parser sees unknown subcommand "collect"
2. Scans extension manifests (JSON only, no Wasm loaded)
3. Finds @specforge/rust declares command "collect"
4. Builds clap Args from manifest's args schema
5. Parses remaining args with generated clap command
6. Loads @specforge/rust Wasm module
7. Calls cmd__collect({ args: { "junit-xml": "results.xml" }, ... })
8. Returns exit code
```

#### MCP Auto-Promotion

Every CLI command is automatically available as an MCP tool:
```
CLI command "collect" in @specforge/rust
  → MCP tool "specforge.rust.collect"
  → Input schema auto-derived from command args
  → Output: { "stdout": "...", "stderr": "...", "exit_code": 0 }
```

Extension-declared MCP tools are additional — they can have richer schemas and return structured JSON instead of stdout/stderr.

#### Pros
- Manifest-only discovery (no Wasm loading at startup)
- Excellent cold-start performance for CLI
- Full JSON Schema for MCP tools (AI agent friendly)
- Clear separation of compile-time and runtime contributions
- Granular per-contribution sandbox
- Matches VS Code's proven model

#### Cons
- Manifest becomes large for extensions with many surface contributions
- Duplicate information (command args in manifest AND Wasm code must agree)
- Manifest schema versioning becomes more complex
- Static declarations can't express dynamic capabilities (e.g., "I contribute one command per configured target")

---

### Option B: Runtime Registration via Host Functions (Terraform Model)

**Championed by**: Expert #3 (partially)

#### Mechanism

During `specforge_init()`, extensions call new host functions to register surface contributions:

```
New host functions:
  specforge.register_command(json)    → register CLI command
  specforge.register_tool(json)       → register MCP tool
  specforge.register_resource(json)   → register MCP resource
  specforge.register_prompt(json)     → register MCP prompt
  specforge.register_lsp_provider(json) → register LSP capability
```

#### Wasm Code (Rust PDK Example)

```rust
#[specforge::init]
fn init() {
    specforge::register_command(Command {
        id: "collect",
        title: "Collect Rust test results",
        args: vec![
            Arg::path("junit-xml").required(),
            Arg::path("out").default("specforge-report.json"),
        ],
    });

    specforge::register_tool(Tool {
        name: "collect_rust_results",
        description: "Parse JUnit XML and return test-to-entity mappings",
        input_schema: json!({
            "type": "object",
            "properties": {
                "junit_xml_path": { "type": "string" }
            },
            "required": ["junit_xml_path"]
        }),
    });
}
```

#### Discovery Requires Loading

```
CLI startup:
1. Load ALL extension Wasm modules (~50ms each)
2. Call specforge_init() on each
3. Collect registered commands/tools/resources
4. Build CLI dispatch table
5. NOW parse user's command
```

#### Pros
- Dynamic: registration can be conditional (e.g., based on config passed at init)
- Single source of truth (code IS the declaration)
- No manifest/code drift
- Supports computed contributions (e.g., one tool per configured language)

#### Cons
- **Kills CLI cold start** — must load ALL Wasm modules before parsing first command
- Every surface invocation loads every extension (even unrelated ones)
- Host function explosion (6 existing + 5 new = 11)
- Order-dependent registration (extension A registers tool X, extension B wants to reference it)
- Cannot discover surface contributions without executing untrusted code
- Not compatible with `specforge extensions` listing tools without loading Wasm

---

### Option C: Hybrid — Static Manifest with Dynamic Override (Recommended Synthesis)

**Championed by**: Experts #1, #5, #7, #10

Combines Option A's static discovery with Option B's dynamic flexibility.

#### Manifest (Static Baseline)

Same as Option A — `surfaces` key in manifest declares all contributions statically.

#### Runtime Override (Optional)

During `specforge_init()`, an extension MAY call `specforge.override_surfaces(json)` to:
- **Modify** a declared contribution (e.g., add args based on config)
- **Disable** a declared contribution (e.g., if a required binary isn't installed)
- **NOT add** entirely new contributions (must be in manifest)

```
Lifecycle:
1. Read manifest.json (fast, no Wasm)        → static surface index
2. User invokes a surface item               → load Wasm
3. Call specforge_init()                      → optional override_surfaces()
4. Dispatch to export                        → execute contribution
```

#### Override Example

```rust
#[specforge::init]
fn init(config: &Config) {
    // The manifest declares "collect" command.
    // At runtime, check if nextest is available and adjust.
    if !which::which("cargo-nextest").is_ok() {
        specforge::override_surfaces(json!({
            "commands": {
                "collect": {
                    "args": {
                        "format": {
                            "values": ["json"],  // remove "nextest" option
                            "default": "json"
                        }
                    }
                }
            }
        }));
    }
}
```

#### Pros
- Fast cold start (static manifest for discovery)
- Dynamic flexibility when needed
- Single override host function (not 5 new registration functions)
- Manifest is always the source of truth for discovery
- Runtime can only narrow, not expand (security)

#### Cons
- Two sources of truth (manifest + runtime override) — which one is correct?
- Override semantics are complex (deep merge)
- Extensions might over-rely on overrides, making manifests misleading
- Added complexity over pure Option A for marginal benefit

---

### Option D: Sidecar Description Files (Unix / kubectl Model)

**Championed by**: Expert #2

#### Mechanism

Each extension ships sidecar JSON files alongside the Wasm binary:

```
@specforge/rust/
  manifest.json         # existing extension manifest (no surface info)
  rust.wasm             # Wasm binary
  surfaces/
    cli.json            # CLI command definitions
    mcp.json            # MCP tool/resource definitions
    lsp.json            # LSP provider definitions
```

#### Sidecar Format (cli.json)

```json
{
  "commands": [
    {
      "id": "collect",
      "title": "Collect Rust test results",
      "export": "cmd__collect",
      "args": [
        { "name": "junit-xml", "type": "path", "required": true },
        { "name": "out", "type": "path", "default": "specforge-report.json" }
      ]
    }
  ]
}
```

#### Discovery

```
CLI startup:
1. Scan extension directories
2. Check for surfaces/cli.json in each
3. Parse only cli.json files (skip manifest.json, skip Wasm)
4. Build dispatch table from collected commands
5. Parse user's command
6. Load only the matching extension's Wasm

MCP startup:
1. Scan extension directories
2. Check for surfaces/mcp.json in each
3. Parse, aggregate tools/resources/prompts
4. Register with MCP server

LSP startup:
1. Scan extension directories
2. Check for surfaces/lsp.json in each
3. Parse, register providers
```

#### Pros
- **Surface-specific discovery** — CLI startup only reads cli.json files
- Even faster cold start than Option A (smaller files to parse)
- Clear file-per-surface separation
- Easy to inspect/debug (just read the JSON files)
- No manifest bloat — surfaces are separate files
- Can be generated by build tools (e.g., `specforge extension build` generates sidecar files from code annotations)

#### Cons
- Multiple files to maintain (manifest.json + N sidecar files)
- Distribution must include sidecar files (registry, npm, etc.)
- Harder to validate consistency between manifest and sidecars
- No single source of truth for "what does this extension do?"
- More filesystem I/O at startup (reading N directories × M sidecar files)
- Breaks the single-manifest model from RES-23

---

### Option E: Capability Negotiation (LSP-Inspired Model)

**Championed by**: Expert #4

#### Mechanism

The host sends a `negotiate_surfaces` call to the Wasm module. The module responds with its supported surfaces.

```
1. Host loads Wasm module
2. Host calls: specforge_negotiate(capabilities_json) -> surfaces_json
   Input:  { "cli": true, "mcp": true, "lsp": { "completion": true, "hover": true } }
   Output: { "cli": { "commands": [...] }, "mcp": { "tools": [...] }, "lsp": { ... } }
3. Host registers returned surface contributions
```

#### Pros
- Module can adapt to host capabilities (e.g., skip LSP contributions if host is CLI-only)
- Single negotiation call instead of multiple registration calls
- Host tells module what surfaces are available
- Module responds with what it supports — bilateral agreement

#### Cons
- **Requires loading Wasm for discovery** (same problem as Option B)
- Two-phase loading: negotiate first, invoke later
- Complex protocol for simple declaration
- Over-engineered for the common case
- Negotiation must happen at startup (latency)
- Not cacheable — negotiation result could depend on runtime state

---

### Option F: Auto-Promotion Only (Minimal Model)

**Championed by**: (none — presented as baseline for comparison)

#### Mechanism

No new contribution types. Every existing contribution is automatically promoted to all surfaces:

```
Validator "jira_key_format"
  → CLI: (runs during compile, no separate command)
  → MCP: (diagnostic results available via specforge.compile)
  → LSP: diagnostic provider (auto, already works via emit_diagnostic)

Ref scheme "jira"
  → CLI: specforge resolve jira:PROJ-42 (auto)
  → MCP tool: specforge.resolve_ref (auto, with scheme param)
  → LSP: hover on jira:PROJ-42 → resolved URL (auto)
```

#### What Cannot Be Expressed

- `specforge collect rust` — not a validator, not a ref scheme. It's an **adapter** (RES-17), which has no contribution type.
- A custom MCP tool that queries the graph in an extension-specific way (e.g., "show Jira coverage dashboard")
- A custom LSP code action (e.g., "Extract scenario from this Given/When/Then block")
- MCP resources with custom URI templates
- MCP prompts for guided workflows

#### Pros
- Zero new API surface
- No manifest changes
- No new Wasm exports
- Simplest possible implementation
- Works for 60% of cases

#### Cons
- Cannot express extension-specific commands (the `collect` use case)
- Cannot express custom MCP tools beyond auto-promoted ones
- Cannot express custom LSP features
- Extensions feel like second-class citizens (can't do what core can do)
- The 40% of cases it can't handle are the most valuable (adapters, custom tooling)

---

## Comparison Matrix

| Criterion | A: Static Manifest | B: Runtime Reg | C: Hybrid | D: Sidecar | E: Negotiation | F: Auto-Promote |
|---|---|---|---|---|---|---|
| CLI cold start | **<10ms** | ~200ms+ | **<10ms** | **<5ms** | ~200ms+ | **0ms** |
| Discovery without Wasm | **Yes** | No | **Yes** | **Yes** | No | N/A |
| Dynamic contributions | No | **Yes** | Partial | No | **Yes** | No |
| Single source of truth | Manifest | Code | Mixed | Split | Code | Implicit |
| New host functions | 0 | 5 | 1 | 0 | 1 | 0 |
| Manifest complexity | Medium | Low | Medium | **Low** (split) | Low | **None** |
| Custom CLI commands | **Yes** | **Yes** | **Yes** | **Yes** | **Yes** | No |
| Custom MCP tools | **Yes** | **Yes** | **Yes** | **Yes** | **Yes** | No |
| Custom LSP features | **Yes** | **Yes** | **Yes** | **Yes** | **Yes** | No |
| DX (author effort) | Medium | Low | Medium | Medium | Medium | **None** |
| Security (sandboxable) | **Yes** | Partial | **Yes** | **Yes** | Partial | **Yes** |
| Consistent with RES-23 | **Yes** | No (new reg model) | **Yes** | No (breaks single manifest) | No (new protocol) | **Yes** |
| VS Code precedent | **Yes** | Terraform-like | VS Code hybrid | kubectl-like | LSP-like | Implicit |

---

## Expert Consensus

### Round 1: Eliminate Clearly Inferior Options

- **Option B (Runtime Registration)**: Rejected 9-1. CLI cold-start penalty is disqualifying. Loading all Wasm to discover surfaces violates the <100ms startup requirement.
- **Option E (Capability Negotiation)**: Rejected 8-2. Same cold-start problem as B, plus added protocol complexity. Over-engineered for the problem.
- **Option F (Auto-Promote Only)**: Rejected 7-3. The 40% gap (adapters, custom tools) is precisely where the most valuable use cases live. `specforge collect rust` is the first proof-of-concept and it can't be expressed.

### Round 2: Evaluate Remaining Options

- **Option D (Sidecar)**: 4 votes. Fast cold start. But breaks the single-manifest model that RES-23 carefully established. Multiple files to maintain. 6 against — "fixing a performance problem that doesn't exist yet."
- **Option C (Hybrid)**: 3 votes. Pragmatic. But "two sources of truth" concern raised by 7 experts. Override semantics are a complexity trap. The 10% of cases needing dynamic override don't justify the complexity.
- **Option A (Static Manifest)**: 8 votes. Best alignment with RES-23. Proven at scale (VS Code). Fast cold start. Single source of truth. The "can't do dynamic contributions" limitation is acceptable — extensions that need dynamic behavior can update their manifest at build time.

### Final Vote

| Option | Votes | Status |
|---|---|---|
| A: Static Manifest + Lazy Loading | **8/10** | **RECOMMENDED** |
| D: Sidecar Files | 1/10 | Viable alternative |
| C: Hybrid | 1/10 | Over-engineered |
| B: Runtime Registration | 0/10 | Rejected |
| E: Capability Negotiation | 0/10 | Rejected |
| F: Auto-Promote Only | 0/10 | Insufficient |

### Key Modifiers to Option A

The 8-vote consensus includes these refinements from the minority positions:

1. **From Option F**: Auto-promotion as a **complement**, not alternative. Every CLI command is automatically an MCP tool. Every ref scheme automatically gets a resolve tool and LSP hover. Auto-promotion is the floor, explicit surface contributions are the ceiling.

2. **From Option D**: The `specforge extension build` step should **produce** manifest surface entries from code annotations when possible. This reduces manual manifest editing while keeping the manifest as the single source of truth.

3. **From Option C**: One narrow escape hatch: the `specforge_init()` export MAY return a JSON object with `disabled_surfaces: ["command:collect"]` to conditionally disable declared contributions at runtime. It cannot add or modify — only disable.

---

## Recommended Architecture: Option A with Modifiers

### 1. Manifest Schema Extension

The `surfaces` key is added as a sibling to `contributes`:

```json
{
  "extension": "@specforge/rust",
  "manifest_version": "2",
  "version": "1.0.0",
  "wasm": "rust.wasm",

  "contributes": {
    "collectors": [{ "name": "rust", "description": "Parse cargo-nextest JUnit XML into specforge-report.json" }]
  },

  "surfaces": {
    "commands": [
      {
        "id": "collect",
        "title": "Collect Rust test results",
        "description": "Parse cargo-nextest JUnit XML and emit specforge-report.json",
        "category": "test",
        "export": "cmd__collect",
        "args": [
          { "name": "junit-xml", "type": "path", "required": true, "description": "Path to JUnit XML file" },
          { "name": "out", "type": "path", "required": false, "default": "specforge-report.json" }
        ]
      },
    ],

    "mcp_tools": [
      {
        "name": "collect_rust_results",
        "description": "Parse JUnit XML and return test-to-entity mappings as structured JSON",
        "category": "test",
        "export": "mcp__collect_results",
        "input_schema": {
          "type": "object",
          "properties": {
            "junit_xml_path": { "type": "string", "description": "Path to JUnit XML" },
            "entity_filter": { "type": "array", "items": { "type": "string" }, "description": "Entity IDs to filter" }
          },
          "required": ["junit_xml_path"]
        }
      },
      {
        "name": "entity_coverage",
        "description": "Get test coverage status for one or all entities",
        "category": "test",
        "export": "mcp__entity_coverage",
        "input_schema": {
          "type": "object",
          "properties": {
            "entity_id": { "type": "string", "description": "Optional specific entity" }
          }
        }
      }
    ],

    "mcp_resources": [
      {
        "uri_template": "specforge://rust/coverage",
        "name": "Rust test coverage summary",
        "description": "Overall coverage of testable entities with passing Rust tests",
        "export": "mcp__coverage_resource",
        "mime_type": "application/json"
      }
    ],

    "lsp_providers": {
      "completion": [
        {
          "id": "verify_kinds",
          "entity_kinds": ["behavior", "invariant"],
          "field_contexts": ["verify"],
          "export": "lsp__complete_verify",
          "description": "Suggest Rust-specific verify kinds"
        }
      ],
      "hover": [
        {
          "id": "coverage_hover",
          "entity_kinds": ["behavior", "invariant", "event", "constraint"],
          "export": "lsp__hover_coverage",
          "description": "Show test coverage status on hover"
        }
      ],
      "code_actions": []
    }
  },

  "sandbox": {
    "max_memory_bytes": 268435456,
    "commands": {
      "collect": { "fs_read": true, "fs_write": true }
    },
    "mcp_tools": {
      "collect_rust_results": { "fs_read": true },
      "entity_coverage": {}
    },
    "lsp_providers": {}
  }
}
```

### 2. Surface Contribution Types

| Surface Type | Key | Export Prefix | Description |
|---|---|---|---|
| CLI command | `surfaces.commands[]` | `cmd__` | Extension-contributed CLI subcommands |
| MCP tool | `surfaces.mcp_tools[]` | `mcp__` | Callable MCP tools with JSON Schema |
| MCP resource | `surfaces.mcp_resources[]` | `mcp__` | Readable MCP resources with URI templates |
| MCP prompt | `surfaces.mcp_prompts[]` | `mcp__` | Templated MCP prompts |
| LSP completion | `surfaces.lsp_providers.completion[]` | `lsp__complete_` | Completion item providers |
| LSP hover | `surfaces.lsp_providers.hover[]` | `lsp__hover_` | Hover documentation providers |
| LSP code action | `surfaces.lsp_providers.code_actions[]` | `lsp__action_` | Quick fix and refactoring actions |
| LSP diagnostic | `surfaces.lsp_providers.diagnostics[]` | `lsp__diagnose_` | Pull diagnostic providers |

### 3. CLI Command Routing

Commands are namespaced under the extension's short name (scope stripped):

```
Extension: @specforge/rust, command id: "collect"
  → specforge collect rust [args]
  → specforge rust collect [args]    (alternative: extension-first)

Extension: @specforge/gh, command id: "sync"
  → specforge sync gh [args]

Extension: @specforge/jira, command id: "import"
  → specforge import jira [args]
```

**Disambiguation**: If two extensions contribute the same command ID, fully qualified invocation is required:
```
specforge --extension @specforge/rust collect [args]
```

**Recommended pattern**: `specforge <verb> <extension-short-name> [args]` — action-first, consistent with `specforge collect rust`.

### 4. MCP Auto-Promotion Rules

Every CLI command is **automatically** an MCP tool with the name `specforge.{extension_short}.{command_id}`:

```
CLI: specforge collect rust --junit-xml results.xml
MCP: { tool: "specforge.rust.collect", input: { "junit_xml": "results.xml" } }
```

Auto-promoted tools wrap CLI output:
```json
{
  "content": [
    {
      "type": "text",
      "text": "Collected 42 test results for 15 entities..."
    }
  ],
  "meta": {
    "exit_code": 0,
    "source": "auto-promoted-cli"
  }
}
```

Extension-declared MCP tools (`surfaces.mcp_tools`) are **in addition** to auto-promoted ones. They provide richer structured output:

```json
{
  "content": [
    {
      "type": "text",
      "text": "{\"entities\": [{\"id\": \"auth_login\", \"tests\": 3, \"passing\": 2, \"failing\": 1}]}"
    }
  ]
}
```

### 5. LSP Provider Merging

LSP contributions are **additive providers**, not replacements:

```
User hovers over "behavior auth_login { ... }"

1. Core LSP: provides entity documentation (from spec)
2. @specforge/rust (lsp__hover_coverage): provides "Tests: 3/5 passing"
3. @specforge/jira (lsp__hover_jira): provides "Linked to PROJ-42 (In Progress)"

Merged hover result:
  ## auth_login
  > User authenticates via login form

  **Rust Coverage**: 3/5 tests passing
  **Jira**: PROJ-42 — In Progress
```

Provider ordering: core first, then extensions in load order. Each provider returns its section. The host concatenates.

### 6. Surface Permission Model

Extending RES-23's per-call-site sandbox:

```
                     fs_read  fs_write  network  graph_read  register_*
                     -------  --------  -------  ----------  ----------
cmd__{id}()            *        *         *         OK          -
mcp__{name}()          *        *         *         OK          -
mcp__{resource}()      *        -         *         OK          -
lsp__complete_*()      -        -         -         OK          -
lsp__hover_*()         -        -         -         OK          -
lsp__action_*()        -        **        -         OK          -
lsp__diagnose_*()      -        -         -         OK          -

*  = per-contribution sandbox (declared in manifest)
** = code actions can write to spec files only (sandbox enforced)
```

LSP providers are **read-only** by default. Code actions can request fs_write but only within the spec root.

### 7. Pipeline Mapping

Updated from RES-23 Expert #6's pipeline phases:

```
COMPILE-TIME PLANE (RES-23 contributes):
  PHASE 1: PARSE ---------> query_extensions
  PHASE 2: REGISTER ------> entities, edges, enhancements
  PHASE 3: BUILD GRAPH ----> (internal)
  PHASE 4: RESOLVE -------> ref_schemes
  PHASE 5: VALIDATE ------> validators
  PHASE 6: EXPORT --------> (Graph Protocol, JSON, DOT)

RUNTIME PLANE (RES-24 surfaces):
  PHASE 7: CLI DISPATCH ---> commands
  PHASE 8: MCP DISPATCH ---> mcp_tools, mcp_resources, mcp_prompts
  PHASE 9: LSP DISPATCH ---> lsp_providers (completion, hover, code_actions, diagnostics)

AUTO-PROMOTION (cross-plane):
  ref_schemes → MCP tool (specforge.resolve.{scheme}) [auto]
  ref_schemes → LSP hover (resolve on hover) [auto]
  commands    → MCP tool (specforge.{pkg}.{cmd}) [auto]
```

### 8. Validation Codes

| Code | Severity | Description |
|---|---|---|
| E026 | Error | Surface export missing — manifest declares `surfaces.commands[].export: "cmd__collect"` but Wasm has no such export |
| E027 | Error | MCP tool input_schema is invalid JSON Schema |
| E028 | Error | CLI command arg type is unknown |
| E029 | Error | LSP provider references unknown entity_kind |
| E030 | Error | Two extensions contribute same command ID without disambiguation |
| W028 | Warning | Surface contribution declared but extension has no Wasm binary (stub extension) |
| W029 | Warning | MCP tool has no description (AI agents need descriptions) |
| W030 | Warning | CLI command has no args (probably missing args declaration) |
| I008 | Info | Auto-promoted CLI command conflicts with explicit MCP tool name (explicit wins) |

### 9. Concrete Example: @specforge/gh

A GitHub provider extension contributing to all three surfaces:

```json
{
  "extension": "@specforge/gh",
  "manifest_version": "2",
  "version": "1.0.0",
  "wasm": "gh.wasm",

  "contributes": {
    "ref_schemes": [
      { "scheme": "gh", "kinds": ["issue", "pr", "discussion", "release"] }
    ],
    "validators": [
      { "name": "gh_ref_reachable", "description": "Validates GitHub refs are reachable via API" }
    ]
  },

  "surfaces": {
    "commands": [
      {
        "id": "sync",
        "title": "Sync spec entities with GitHub issues",
        "category": "sync",
        "export": "cmd__sync",
        "args": [
          { "name": "direction", "type": "enum", "values": ["push", "pull", "both"], "default": "both" },
          { "name": "dry-run", "type": "bool", "default": "true" }
        ]
      },
      {
        "id": "link",
        "title": "Link a spec entity to a GitHub issue",
        "category": "ref",
        "export": "cmd__link",
        "args": [
          { "name": "entity", "type": "string", "required": true },
          { "name": "issue", "type": "string", "required": true }
        ]
      }
    ],

    "mcp_tools": [
      {
        "name": "resolve_github_ref",
        "description": "Resolve a gh: reference to its full URL and current status",
        "category": "ref",
        "export": "mcp__resolve_ref",
        "input_schema": {
          "type": "object",
          "properties": {
            "ref": { "type": "string", "description": "GitHub ref (e.g., gh.issue:42)" },
            "include_body": { "type": "boolean", "default": false }
          },
          "required": ["ref"]
        }
      },
      {
        "name": "github_coverage",
        "description": "Show which spec entities are linked to GitHub issues",
        "category": "traceability",
        "export": "mcp__coverage",
        "input_schema": {
          "type": "object",
          "properties": {
            "unlinked_only": { "type": "boolean", "default": false }
          }
        }
      }
    ],

    "mcp_resources": [
      {
        "uri_template": "specforge://gh/issue/{number}",
        "name": "GitHub issue details",
        "export": "mcp__issue_resource",
        "mime_type": "application/json"
      }
    ],

    "lsp_providers": {
      "completion": [
        {
          "id": "gh_ref_completion",
          "entity_kinds": ["*"],
          "field_contexts": ["ref"],
          "export": "lsp__complete_gh_ref",
          "description": "Auto-complete GitHub issue/PR numbers"
        }
      ],
      "hover": [
        {
          "id": "gh_ref_hover",
          "entity_kinds": ["*"],
          "export": "lsp__hover_gh_ref",
          "description": "Show GitHub issue/PR details on hover"
        }
      ],
      "code_actions": [
        {
          "id": "create_gh_issue",
          "entity_kinds": ["behavior", "feature"],
          "title": "Create GitHub issue from entity",
          "export": "lsp__action_create_issue",
          "kind": "refactor"
        }
      ]
    }
  },

  "sandbox": {
    "max_memory_bytes": 134217728,
    "commands": {
      "sync": { "fs_read": true, "fs_write": true, "network": ["api.github.com"] },
      "link": { "fs_read": true, "fs_write": true }
    },
    "mcp_tools": {
      "resolve_github_ref": { "network": ["api.github.com"] },
      "github_coverage": {}
    },
    "lsp_providers": {
      "gh_ref_completion": { "network": ["api.github.com"] },
      "gh_ref_hover": { "network": ["api.github.com"] }
    }
  },

  "peer_dependencies": {}
}
```

**Result**: From one extension, users get:
- CLI: `specforge sync gh`, `specforge link gh`
- MCP tools: `specforge.gh.sync`, `specforge.gh.link` (auto-promoted) + `resolve_github_ref`, `github_coverage` (explicit)
- MCP resources: `specforge://gh/issue/{number}`
- LSP: GitHub ref completion, hover, and "Create issue" code action
- Compile-time: `gh:` ref scheme validation

### 10. Effort vs. Impact

| Item | Effort | Impact | Phase |
|---|---|---|---|
| Manifest schema extension (`surfaces` key) | Low | High | 1 |
| CLI dynamic command dispatch from manifest | Medium | **Critical** | 1 |
| Wasm export protocol for `cmd__` prefix | Low | High | 1 |
| MCP server with tool aggregation | High | **Critical** | 1 |
| MCP auto-promotion (CLI → MCP tools) | Medium | High | 1 |
| Per-contribution sandbox enforcement | Medium | High | 1 |
| LSP provider merging framework | Medium | Medium | 2 |
| LSP completion contribution dispatch | Medium | Medium | 2 |
| LSP hover contribution dispatch | Low | Medium | 2 |
| LSP code action contribution dispatch | Medium | Medium | 2 |
| MCP resource serving | Low | Medium | 2 |
| MCP prompt contributions | Low | Low | 3 |
| `specforge extension init` surface scaffolding | Low | Medium | 2 |
| Validation codes E026-E030, W028-W030, I008 | Medium | Medium | 2 |
| Registry surface indexing | Low | Low | 3 |

### 11. Phased Delivery

**Phase 1: CLI + MCP (ship together)**
- `surfaces.commands` in manifest
- CLI dynamic dispatch from manifest
- `cmd__` Wasm exports
- MCP server with built-in tools
- Auto-promotion: CLI commands → MCP tools
- `surfaces.mcp_tools` in manifest
- Per-contribution sandbox for commands and tools

**Phase 2: LSP + Rich MCP**
- `surfaces.lsp_providers` in manifest
- LSP completion/hover/code action dispatch
- `surfaces.mcp_resources` in manifest
- Provider merging framework
- Surface validation codes

**Phase 3: Ecosystem**
- `surfaces.mcp_prompts`
- Registry surface indexing
- `specforge extension init` surface scaffolding
- Surface-specific test helpers in PDK

---

## Cross-References

- **RES-23**: Contribution-based extension model (graph contributions). This document extends with surface contributions.
- **RES-17**: Rust plugin design. The `specforge collect rust` command is the primary motivating example for CLI surface contributions.
- **RES-18**: AI agent token economics. MCP tool count and description quality directly impact token budgets.
- **RES-11a**: Core compiler architecture. Pipeline phases 1-6.
- **RES-16**: Test execution integration. `specforge collect` pattern.
- `docs/extension-model.md`: User-facing extension documentation (needs update).
- `crates/runtime/wasm/src/manifest.rs`: `ExtensionManifest` and `ExtensionContributions` types (add `surfaces` field).
- `crates/runtime/wasm/src/host_functions.rs`: Host function dispatch (add surface-aware permission gating).
- `crates/surfaces/cli/src/main.rs`: CLI entrypoint (add dynamic command registration).
