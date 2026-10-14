---
id: RES-23
kind: research
title: "Contribution-Based Extension Model — 10-Expert Redesign of the Extension Architecture"
status: active
date: 2026-03-03
depends_on: [RES-11a, RES-22]
priority: critical
tags: [wasm, extensions, architecture, naming, extensions]
---

# RES-23: Contribution-Based Extension Model

## Executive Summary

The current extension model classifies every Wasm extension as exactly one of three roles via `PluginKind::Plugin | Provider | Generator`. This forces artificial extension splitting: a Jira integration requires three separate extensions with version sync nightmares, duplicated internals, and self-referential peer dependencies.

A **10-expert panel** converged on replacing the role-based taxonomy with a **contribution-based model** inspired by VS Code's `contributes` manifest key. A single extension declares what it contributes — entities, edges, ref schemes, validators — at the granularity of individual items, not coarse roles. The host dispatches to the right pipeline phase based on declared contributions.

This document also establishes:
- The **new naming convention** for the entire extension system, replacing "Wasm plugin" terminology with "extension" terminology
- The **extension source resolution** model with registry, local path, and git sources
- The **version pinning** strategy with semver ranges and a lock file
- The **per-call-site sandbox** model for least-privilege security

---

## Problem Statement

### 1. The `PluginKind` Enum Forces Single-Role Extensions

```rust
// Current: crates/runtime/wasm/src/manifest.rs:69-76
pub enum PluginKind {
    Plugin,     // entities + edges + validation
    Provider,   // ref schemes + URL resolution
}
```

A `@specforge/jira` extension naturally provides both concerns:
- **Entities**: `epic`, `story`, `task`, `sprint` (plugin role)
- **Ref resolution**: `jira:PROJ-42` → validate against Jira API (provider role)

The current model forces this into three extensions or an arbitrary choice of one role.

### 2. Duplicated Structs

Two manifest types coexist with overlapping concerns:
- `WasmPluginManifest` in `crates/runtime/wasm/src/manifest.rs` — full manifest with sandbox, peer deps, lifecycle
- `PluginManifestV2` in `crates/kernel/common/src/field_registry.rs` — lightweight subset for enhancement registration

These should be one type.

### 3. Naming Inconsistency

The term "plugin" is overloaded:
- The `plugins` key in `specforge.json` lists all extensions (including those that are providers)
- `PluginKind::Plugin` is one specific role
- `WasmPluginManifest` describes all extensions, not just plugins
- The `specforge-wasm` crate prefixes everything with `Wasm` even though that's an implementation detail

### 4. No Version Control

Extensions have no version pinning, no source specifiers, and no lock file. Builds are not reproducible.

---

## 10-Expert Panel

### Expert #1 — Language Runtime Designer

**Prior art**: Terraform providers register N resource types + N data sources in one binary. VSCode extensions contribute commands, languages, debuggers, themes in one `.vsix`. Kubernetes operators register CRDs, controllers, webhooks in one binary. None use a `kind` enum.

**Verdict**: Replace role taxonomy with contribution declarations. An extension says what it contributes, not what it is.

### Expert #2 — Extension Manager Architect

**Principle**: The extension is the unit of distribution. The contribution is the unit of functionality.

**Prior art**: npm extensions contain bins, libs, types, browser bundles — no `kind: "library"` field. Cargo crates have `[[bin]]`, `[lib]`, features — multiple artifact types from one source.

**Verdict**: Separate distribution identity (extension name, version) from functional identity (what it contributes). One extension version covers all its contributions atomically. Add version pinning with semver ranges, source specifiers, and a lock file.

### Expert #3 — Wasm/Extism Expert

**Key insight**: One `.wasm` module = one linear memory = one sandbox boundary. Multiple contributions share state naturally (Jira API client initialized once, used by entity registration, ref resolution, and generation).

**Verdict**: Use namespaced Wasm exports (`entities__register`, `refs__resolve`, `validators__run`). The host calls only the exports that correspond to declared contributions. Missing export for declared contribution = load-time error `E020`.

### Expert #4 — Developer Experience

**Principle**: An extension that only adds one ref scheme should be 20 lines of code, not 200.

**Verdict**: No boilerplate for unused contribution types. Authors implement only the exports they declare. The `specforge extension init` scaffolding asks "what will your extension contribute?" and generates only the necessary code.

### Expert #5 — Security Architect

**Key insight**: The OS concept of "capabilities" means granting minimum necessary permissions per operation, not per identity. Apply this per call-site, not per extension.

**Verdict**: Same `.wasm` module gets different host function permissions depending on which export the host is calling. When the host calls `refs__resolve()`, it wires up `http_get` but blocks `emit_diagnostic`. Enforcement at the host dispatch level. Lock file integrity hashes verify `.wasm` binaries haven't been tampered with.

### Expert #6 — Compiler Pipeline Architect

**Key insight**: The compilation pipeline has phases (parse → register → build graph → resolve → validate → export). Contributions plug into specific phases. The contribution types ARE the extension points — they're not arbitrary categories but pipeline insertion points.

```
PHASE 1: PARSE ---------> query_extensions
PHASE 2: REGISTER ------> entities, edges, enhancements
PHASE 3: BUILD GRAPH ----> (internal, no extension point)
PHASE 4: RESOLVE -------> ref_schemes
PHASE 5: VALIDATE ------> validators
PHASE 6: EXPORT --------> (Graph Protocol, JSON, DOT — internal, no extension point)
PHASE 7: REPORT --------> reporters        (future)
PHASE 8: SYNC ----------> syncers          (future)
```

**Verdict**: Contribution types map 1:1 to pipeline extension points. Adding a new extension point = adding a new contribution key. No schema migration needed.

### Expert #7 — API/ABI Design

**Prior art**: VSCode's `contributes` object with well-known keys scales to 40,000+ extensions.

**Verdict**: Use a `contributes` object in the manifest with typed keys. Each key is a pipeline extension point. Schema validation per contribution type. IDE completion for known keys. Naturally extensible — add new keys without breaking existing ones.

### Expert #8 — Registry / Distribution

**Key insight**: Contribution-level indexing beats extension-level search. Reproducible builds require a lock file.

**Verdict**: The registry indexes contributions, not extensions. Search "who provides ref_scheme `jira`?" not "which extensions are providers?". Conflict detection becomes automatic at the contribution level. `specforge.lock` pins exact versions and `.wasm` integrity hashes.

### Expert #9 — AI Agent Integration

**Key insight**: Agents need flat queries: "what entity types exist?", "what validators run?". The contribution model makes this a simple aggregation across all loaded extensions.

**Verdict**: Future `mcp_tools` contribution type lets extensions expose MCP tools directly to AI agents. The flat contribution model makes discovery trivial.

### Expert #10 — Ecosystem Strategist

**Key insight**: Low barrier to entry drives ecosystem adoption. An extension with just one ref scheme (20 lines of Rust) should be as easy to publish as a full-featured integration.

**Verdict**: The contribution model creates a marketplace of capabilities, not a marketplace of extensions. Three source types (registry, local, git) let authors iterate locally, share via git, and publish to registry when ready.

---

## 1. Contribution Model

### 1.1 Manifest Schema

The `kind` field is removed. Replaced by `contributes` with well-known keys:

```json
{
  "extension": "@specforge/jira",
  "manifest_version": "2",
  "version": "1.0.0",
  "wasm": "jira.wasm",
  "description": "Full Jira integration for SpecForge",

  "contributes": {
    "entities": [
      {
        "name": "epic",
        "testable": false,
        "fields": [
          { "name": "jira_key", "type": "string", "required": true },
          { "name": "stories", "type": "reference_list", "required": false }
        ],
        "reference_targets": { "stories": "story" }
      },
      {
        "name": "story",
        "testable": false,
        "fields": [
          { "name": "jira_key", "type": "string", "required": true },
          { "name": "points", "type": "integer", "required": false },
          { "name": "sprint", "type": "reference", "required": false }
        ]
      }
    ],

    "edges": [
      {
        "label": "belongs_to_sprint",
        "source_kind": "story",
        "target_kind": "sprint",
        "soft": false
      }
    ],

    "enhancements": [
      {
        "target_entity": "behavior",
        "field_name": "jira_ticket",
        "field_type": { "kind": "string" },
        "required": false,
        "description": "Link behavior to a Jira ticket key"
      }
    ],

    "ref_schemes": [
      {
        "scheme": "jira",
        "kinds": ["issue", "epic", "story", "board", "sprint"]
      }
    ],

    "validators": [
      {
        "name": "jira_key_format",
        "description": "Validates jira_key fields match PROJ-NNN pattern"
      }
    ],

    "query_extensions": [
      { "path": "queries/jira-highlights.scm" }
    ]
  },

  "sandbox": {
    "max_memory_bytes": 134217728,
    "max_fuel": 1000000,
    "refs": {
      "allowed_domains": ["*.atlassian.net"]
    }
  },

  "peer_dependencies": {
    "@specforge/product": ">=0.1.0"
  }
}
```

### 1.2 Field Migration from WasmPluginManifest

| Current field | New location |
|---|---|
| `kind: "plugin"` | Removed. Inferred from `contributes` keys. |
| `entity_kinds: [...]` | `contributes.entities: [...]` |
| `dynamic_edge_types: [...]` | `contributes.edges: [...]` |
| `enhancements: [...]` | `contributes.enhancements: [...]` |
| `provider: { schemes, kinds }` | `contributes.ref_schemes: [...]` |
| `query_extensions: [...]` | `contributes.query_extensions: [...]` |
| `sandbox: { flat }` | `sandbox: { per-contribution-type scoping }` |

### 1.3 Contribution Keys

```
v1.0 (ship with initial release):
  entities          Entity kinds with fields and testability
  edges             Edge types between entity kinds
  enhancements      Additional fields on existing entity kinds
  ref_schemes       Reference scheme handlers (resolve + validate)
  validators        Custom validation passes
  query_extensions  Tree-sitter .scm query patterns
  grammars          Extension-defined grammars for entity body content
  body_parsers      Body parser functions dispatched per entity kind

Future (additive, no schema migration):
  importers         Import from external formats (Jira export, Notion)
  reporters         Custom test report formats
  syncers           Bidirectional sync with external tools
  mcp_tools         MCP tool definitions exposed to AI agents
  code_actions      LSP code action contributions
  completions       LSP completion contributions
  hover_providers   LSP hover information contributions
```

Adding a new contribution type requires:
1. Define the manifest schema for the new key
2. Define the Wasm exports it requires
3. Add host-side dispatch logic

No changes to existing contribution types. No manifest version bump needed (additive).

### 1.4 Conflict Detection

Conflict detection applies per contribution item:

| Conflict | Code | Resolution |
|---|---|---|
| Two extensions contribute same ref scheme | E021 | Disable one via `enable` |
| Two extensions contribute same entity kind | E022 | Hard error at load time — author must rename |
| Two extensions contribute same edge label | E023 | Disable one via `enable` |
| Two extensions contribute same validator name | E025 | Disable one via `enable` |
| Two extensions enhance same (entity, field) | E017 | Existing `enhancement_policy` |

---

## 2. Extension Source Resolution

### 2.1 Three Source Types

```
REGISTRY (default):
  @scope/name             latest stable from registry
  @scope/name@1.2.0       exact version
  @scope/name@^1.0.0      semver range (compatible updates)
  @scope/name@>=1.2,<2.0  bounded range

LOCAL PATH (starts with . or /):
  ./extensions/my-plugin            relative to project root
  /opt/specforge/extensions/custom  absolute path

GIT (git: prefix, any host):
  git:https://github.com/org/repo             default branch
  git:https://github.com/org/repo#main        specific branch
  git:https://github.com/org/repo#v1.2.0      specific tag
  git:https://github.com/org/repo#a1b2c3d     specific commit
  git:https://gitlab.com/org/repo#develop      works with any host
  git:ssh://git@bitbucket.org/org/repo         SSH protocol
```

### 2.2 Specifier Parsing

```
Input: "@specforge/jira@^1.0.0"

  1. Starts with "./" or "/" ?
     YES --> Source::Local { path }
     NO  --> continue

  2. Starts with "git:" ?
     YES --> strip prefix, split on "#"
             Source::Git { url, git_ref }
     NO  --> continue

  3. Default: registry
     Split on last "@" after scope prefix
     "@specforge/jira@^1.0.0"
       name = "@specforge/jira"
       version_req = "^1.0.0"
     Source::Registry { name, version_req }


Input: "./extensions/my-plugin"
  --> Source::Local { path: "./extensions/my-plugin" }

Input: "git:https://github.com/org/repo#v1.2.0"
  --> Source::Git { url: "https://github.com/org/repo", git_ref: "v1.2.0" }

Input: "@specforge/product"
  --> Source::Registry { name: "@specforge/product", version_req: "*" }
```

### 2.3 specforge.json Config

String shorthand and object form are both supported:

```json
{
  "name": "my-project",
  "version": "1.0",

  "extensions": [
    "@specforge/product@^1.0.0",

    "./extensions/my-plugin",

    "git:https://github.com/myorg/ddd-plugin#main",

    {
      "extension": "@specforge/jira",
      "version": "^1.0.0",
      "config": { "instance": "myteam.atlassian.net" }
    },

    {
      "git": "https://github.com/myorg/custom",
      "ref": "feature-branch",
      "enable": { "entities": ["threat", "mitigation"] }
    },

    {
      "path": "./extensions/internal"
    }
  ]
}
```

**Granular contribution toggle** (object form only):

```json
{
  "extension": "@specforge/jira",
  "version": "^1.0.0",
  "enable": {
    "entities": true,
    "ref_schemes": true
  }
}
```

**Per-item granularity** (most specific):

```json
{
  "extension": "@specforge/jira",
  "version": "^1.0.0",
  "enable": {
    "entities": ["epic", "story"],
    "ref_schemes": true
  }
}
```

### 2.4 Lock File: specforge.lock

Reproducible builds require pinning exact resolved versions and verifying binary integrity.

```json
{
  "lockfile_version": 1,
  "extensions": {
    "@specforge/product": {
      "version": "1.2.0",
      "source": "registry",
      "integrity": "sha256:a1b2c3d4e5f6...",
      "wasm_hash": "sha256:f6e5d4c3b2a1..."
    },
    "@specforge/jira": {
      "version": "1.0.0",
      "source": "registry",
      "integrity": "sha256:...",
      "wasm_hash": "sha256:...",
      "peer_dependencies": {
        "@specforge/product": ">=1.0.0"
      }
    },
    "git:https://github.com/myorg/ddd-plugin": {
      "version": "0.3.0",
      "source": "git:https://github.com/myorg/ddd-plugin#main",
      "resolved_ref": "a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0",
      "wasm_hash": "sha256:..."
    },
    "./extensions/my-plugin": {
      "version": "0.0.0-local",
      "source": "local",
      "wasm_hash": "sha256:..."
    }
  }
}
```

**Lock file behavior**:

| Command | Behavior |
|---|---|
| `specforge compile` (lock exists) | Use locked versions. Verify `wasm_hash` after download. |
| `specforge compile` (no lock) | Resolve all extensions. Write `specforge.lock`. Warn: "commit this file". |
| `specforge update` | Re-resolve all extensions within version ranges. Update lock. Show diff. |
| `specforge update @specforge/jira` | Re-resolve one extension only. Update its lock entry. |
| `specforge add @specforge/jira@^1.0.0` | Add to `specforge.json` + resolve + update lock. |
| `specforge remove @specforge/jira` | Remove from `specforge.json` + update lock. |

### 2.5 Extension Cache

```
~/.specforge/cache/extensions/
  @specforge/
    product/
      1.2.0/
        manifest.json
        product.wasm
      1.3.0/
        manifest.json
        product.wasm
  git/
    https---github.com-myorg-ddd-plugin/
      a1b2c3d4/
        manifest.json
        ddd.wasm

AOT cache (existing, unchanged):
  .specforge/cache/{wasm_sha256}_{runtime_ver}.aot
```

Local path extensions are never cached — they are read directly from the filesystem. The `wasm_hash` in the lock file detects when a local extension has changed since last compile.

---

## 3. Wasm Export Protocol

### 3.1 Namespaced Exports

Exports are namespaced by contribution category. The host calls only exports matching declared contributions:

```
ALWAYS REQUIRED:
  fn specforge_init() -> i32
     Called once on module load. Return 0 = success.

IF contributes.entities OR contributes.edges OR contributes.enhancements:
  fn entities__register()
     Host enables:  register_entity, register_edge, emit_diagnostic
     Host blocks:   http_get

IF contributes.ref_schemes:
  fn refs__resolve(input_json_ptr: u64) -> u64
  fn refs__validate(input_json_ptr: u64) -> u64
     Host enables:  http_get (scoped), emit_diagnostic
     Host blocks:   register_entity, register_edge

IF contributes.validators:
  fn validators__run()
     Host enables:  query_graph, emit_diagnostic
     Host blocks:   http_get, register_entity, register_edge
```

Missing export for a declared contribution = `E020` error at load time.
Extra export for an undeclared contribution = ignored (safe).

### 3.2 Per-Call-Site Permission Matrix

```
                     register  register  emit     query   http
                     _entity   _edge     _diag    _graph  _get
                     --------  --------  -------  ------  ------
specforge_init()        -         -        OK       -       -
entities__register()   OK        OK        OK       -       -
refs__resolve()         -         -        OK       -      OK*
refs__validate()        -         -        OK       -      OK*
validators__run()       -         -        OK      OK       -

*  scoped to sandbox.refs.allowed_domains
```

### 3.3 Per-Contribution Sandbox

The flat `SandboxPolicy` becomes scoped:

```json
{
  "sandbox": {
    "max_memory_bytes": 134217728,
    "max_fuel": 1000000,

    "refs": {
      "allowed_domains": ["*.atlassian.net", "api.atlassian.com"],
      "allow_http": true
    }
  }
}
```

`max_memory_bytes` and `max_fuel` apply to the Wasm module globally (per-module constraints). Network and filesystem permissions are scoped to the contribution type that uses them.

---

## 4. Naming Convention

### 4.1 Rust Struct Renames

| Current | New | Location |
|---|---|---|
| `WasmPluginManifest` | `ExtensionManifest` | `crates/runtime/wasm/src/manifest.rs` |
| `PluginManifestV2` | Removed (merged into `ExtensionManifest`) | `crates/kernel/common/src/field_registry.rs` |
| `PluginKind` | Removed (inferred from `contributes`) | `crates/runtime/wasm/src/manifest.rs` |
| `PluginLifecycleState` | `ExtensionLifecycleState` | `crates/runtime/wasm/src/manifest.rs` |
| `WasmEntityKind` | `ContributedEntity` | `crates/runtime/wasm/src/manifest.rs` |
| `WasmEntityField` | `ContributedField` | `crates/runtime/wasm/src/manifest.rs` |
| `WasmProviderConfig` | Removed (inline in `ContributedRefScheme`) | `crates/runtime/wasm/src/manifest.rs` |
| `WasmRuntime` | `ExtensionRuntime` | `crates/runtime/wasm/src/runtime.rs` |
| `WasmError` | `ExtensionError` | `crates/runtime/wasm/src/error.rs` |
| `WarmInstancePool` | `WarmExtensionPool` | `crates/runtime/wasm/src/warm.rs` |
| `LoadedPlugin` | `LoadedExtension` | `crates/runtime/wasm/src/loader.rs` |
| `load_wasm_module` | `load_extension_module` | `crates/runtime/wasm/src/loader.rs` |
| `discover_wasm_plugins` | `discover_extensions` | `crates/runtime/wasm/src/discover.rs` |
| `HostContext.in_initialize` | `HostContext.active_phase` | `crates/runtime/wasm/src/host_functions.rs` |

Unchanged (correct as-is):
- `load_manifest` — still loads a manifest
- `validate_peer_dependencies` — still validates peer deps
- `topological_sort` — still sorts
- `SandboxPolicy` — still a sandbox policy (gains scoped fields)
- `AotCache` — still an AOT cache
- `QueryExtension` — still a query extension

### 4.2 Crate-Level Naming

| Crate | Change | Rationale |
|---|---|---|
| `specforge-wasm` | Keep name | The crate IS the Wasm runtime. Internal naming is correct. |
| `specforge-common` | Remove `PluginManifestV2` export, keep contribution types | Domain types (`ContributedEntity`, `FieldEnhancement`, etc.) stay here. Full `ExtensionManifest` lives in `specforge-wasm`. |

### 4.3 specforge.json Schema

| Current key | New key | Transition |
|---|---|---|
| `plugins: [...]` | `extensions: [...]` | Deprecated alias (W027 warning, auto-merge) |
| `providers: { alias: {...} }` | Inline in `extensions` with `config` | Deprecated alias (W027 warning, auto-merge) |

### 4.4 CLI Commands

| Current | New |
|---|---|
| `specforge plugin init` | `specforge extension init` |
| `specforge plugin build` | `specforge extension build` |
| `specforge plugin test` | `specforge extension test` |
| `specforge plugin publish` | `specforge extension publish` |
| `specforge plugins` | `specforge extensions` |
| — | `specforge extensions outdated` (new) |
| — | `specforge add <specifier>` (new) |
| — | `specforge remove <extension>` (new) |
| — | `specforge update [extension]` (new) |

### 4.5 ADR Renames

| Current | New |
|---|---|
| `wasm_extism_plugin_runtime` | `wasm_extism_extension_runtime` |
| `wasm_peer_dependencies` | `extension_peer_dependencies` |

---

## 5. Rust Type Definitions

### 5.1 ExtensionManifest

```rust
/// The sidecar manifest for a SpecForge extension (`manifest.json`).
///
/// An extension contributes to one or more extension points of the
/// compilation pipeline: entities, edges, ref schemes, validators,
/// query extensions, etc.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtensionManifest {
    /// Extension name (e.g., `@specforge/jira`).
    pub extension: String,
    /// Manifest schema version.
    #[serde(default = "default_manifest_version")]
    pub manifest_version: String,
    /// Path to the `.wasm` binary, relative to the manifest file.
    pub wasm: String,
    /// Human-readable description.
    #[serde(default)]
    pub description: String,
    /// Extension version (semver).
    #[serde(default = "default_version")]
    pub version: String,

    /// What this extension contributes to the host.
    #[serde(default)]
    pub contributes: ExtensionContributions,

    /// Sandbox policy (global limits + per-contribution scoping).
    #[serde(default)]
    pub sandbox: SandboxPolicy,

    /// Peer dependencies on other extensions (extension name → semver range).
    #[serde(default)]
    pub peer_dependencies: HashMap<String, String>,

    /// Resolved absolute path to the manifest file (set at load time).
    #[serde(skip)]
    pub manifest_path: PathBuf,
    /// Resolved absolute path to the `.wasm` binary (set at load time).
    #[serde(skip)]
    pub wasm_path: PathBuf,
}
```

### 5.2 ExtensionContributions

```rust
/// Everything an extension contributes to the host.
///
/// All fields are optional — an extension only implements the contributions
/// it declares. Empty vectors mean "no contributions of this type".
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ExtensionContributions {
    #[serde(default)]
    pub entities: Vec<ContributedEntity>,

    #[serde(default)]
    pub edges: Vec<DynamicEdgeType>,

    #[serde(default)]
    pub enhancements: Vec<FieldEnhancement>,

    #[serde(default)]
    pub ref_schemes: Vec<ContributedRefScheme>,

    #[serde(default)]
    pub validators: Vec<ContributedValidator>,

    #[serde(default)]
    pub query_extensions: Vec<QueryExtension>,
}

impl ExtensionContributions {
    /// Returns true if this extension has no contributions at all.
    pub fn is_empty(&self) -> bool {
        self.entities.is_empty()
            && self.edges.is_empty()
            && self.enhancements.is_empty()
            && self.ref_schemes.is_empty()
            && self.validators.is_empty()
            && self.query_extensions.is_empty()
    }

    /// Needs entities__register export.
    pub fn has_entity_contributions(&self) -> bool {
        !self.entities.is_empty()
            || !self.edges.is_empty()
            || !self.enhancements.is_empty()
    }

    /// Needs refs__resolve and refs__validate exports.
    pub fn has_ref_contributions(&self) -> bool {
        !self.ref_schemes.is_empty()
    }

    /// Needs validators__run export.
    pub fn has_validator_contributions(&self) -> bool {
        !self.validators.is_empty()
    }

}
```

### 5.3 Contributed Types

```rust
/// An entity kind contributed by an extension.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContributedEntity {
    pub name: String,
    #[serde(default)]
    pub testable: bool,
    #[serde(default)]
    pub fields: Vec<ContributedField>,
    #[serde(default)]
    pub reference_targets: HashMap<String, String>,
}

/// A field on a contributed entity kind.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContributedField {
    pub name: String,
    #[serde(rename = "type")]
    pub field_type: String,
    #[serde(default)]
    pub required: bool,
}

/// A ref scheme contributed by an extension.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContributedRefScheme {
    /// Scheme prefix (e.g., `"jira"`).
    pub scheme: String,
    /// Valid ref kinds for this scheme.
    #[serde(default)]
    pub kinds: Vec<String>,
}

/// A validation pass contributed by an extension.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContributedValidator {
    pub name: String,
    #[serde(default)]
    pub description: String,
}

```

### 5.4 Extension Source Types

```rust
/// An extension entry in specforge.json `extensions` array.
/// Supports string shorthand and object form.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ExtensionSpecifier {
    /// String shorthand: "@scope/name@version", "./path", "git:url#ref"
    Short(String),
    /// Object form with explicit fields.
    Full(ExtensionSpecifierFull),
}

/// Object form of an extension specifier.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtensionSpecifierFull {
    /// Registry extension name (e.g., "@specforge/jira").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extension: Option<String>,

    /// Semver version requirement (e.g., "^1.0.0"). Registry source only.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,

    /// Git repository URL. Implies git source.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub git: Option<String>,

    /// Branch, tag, or commit hash. Used with `git` source.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "ref")]
    pub git_ref: Option<String>,

    /// Local filesystem path. Implies local source.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,

    /// Contribution toggle (disable specific contributions).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enable: Option<ContributionToggle>,

    /// Extension-specific configuration passed to the Wasm module at init.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub config: Option<serde_json::Value>,
}

/// Resolved source after parsing a specifier.
#[derive(Debug, Clone)]
pub enum ResolvedSource {
    Registry {
        name: String,
        version_req: semver::VersionReq,
    },
    Local {
        path: PathBuf,
    },
    Git {
        url: String,
        git_ref: Option<String>,
    },
}

/// Contribution toggle — enable/disable specific contributions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContributionToggle {
    #[serde(default)]
    pub entities: Option<ContributionFilter>,
    #[serde(default)]
    pub edges: Option<ContributionFilter>,
    #[serde(default)]
    pub enhancements: Option<ContributionFilter>,
    #[serde(default)]
    pub ref_schemes: Option<ContributionFilter>,
    #[serde(default)]
    pub validators: Option<ContributionFilter>,
    #[serde(default)]
    pub query_extensions: Option<ContributionFilter>,
}

/// Filter for enabling/disabling contributions.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ContributionFilter {
    /// Enable or disable all contributions of this type.
    All(bool),
    /// Enable only specific named contributions.
    Named(Vec<String>),
}
```

### 5.5 SandboxPolicy (revised)

```rust
/// Sandbox policy for an extension.
///
/// Global limits (memory, fuel) apply to the Wasm module.
/// Scoped limits apply per contribution type during dispatch.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxPolicy {
    #[serde(default = "default_max_memory")]
    pub max_memory_bytes: u64,
    #[serde(default)]
    pub max_fuel: u64,

    /// Permissions scoped to ref scheme contributions.
    #[serde(default)]
    pub refs: RefSandbox,

}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RefSandbox {
    #[serde(default)]
    pub allowed_domains: Vec<String>,
    #[serde(default)]
    pub allow_http: bool,
}
```

### 5.6 Lock File Types

```rust
/// Contents of specforge.lock
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LockFile {
    pub lockfile_version: u32,
    pub extensions: HashMap<String, LockedExtension>,
}

/// A resolved and pinned extension entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LockedExtension {
    /// Exact resolved version.
    pub version: String,
    /// Resolved source description.
    pub source: String,
    /// SHA-256 hash of the extension archive (for registry/git sources).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub integrity: Option<String>,
    /// SHA-256 hash of the .wasm binary.
    pub wasm_hash: String,
    /// Resolved git commit (for git sources).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolved_ref: Option<String>,
    /// Locked peer dependencies.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub peer_dependencies: HashMap<String, String>,
}
```

### 5.7 ExtensionLifecycleState

```rust
/// Lifecycle states for a loaded extension.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExtensionLifecycleState {
    /// Manifest loaded, binary not yet loaded.
    Discovered,
    /// Binary loaded into memory, not yet initialized.
    Loading,
    /// Extension has been initialized (contributions registered).
    Initialized,
    /// Extension is ready for validate calls.
    Ready,
    /// Extension encountered an error.
    Failed,
}
```

---

## 6. Host Dispatch Logic

```rust
/// In ExtensionRuntime (replaces WasmRuntime):

pub fn initialize_extension(
    &mut self,
    pkg: &LoadedExtension,
) -> Result<(), ExtensionError> {
    // Always call specforge_init
    self.call_export(pkg, "specforge_init")?;

    // Entity contributions
    if pkg.manifest.contributes.has_entity_contributions() {
        self.set_phase(DispatchPhase::EntityRegistration);
        self.call_export(pkg, "entities__register")?;
    }

    // Ref scheme registration (runtime init for API clients)
    if pkg.manifest.contributes.has_ref_contributions() {
        self.set_phase(DispatchPhase::RefRegistration);
        if pkg.has_export("refs__init") {
            self.call_export(pkg, "refs__init")?;
        }
    }

    pkg.set_state(ExtensionLifecycleState::Initialized);
    Ok(())
}

pub fn validate_all(
    &mut self,
    graph_json: &str,
) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    for pkg in &self.extensions {
        if pkg.manifest.contributes.has_validator_contributions() {
            self.set_phase(DispatchPhase::Validation);
            let diags = self.call_export(pkg, "validators__run");
            diagnostics.extend(diags);
        }
    }
    diagnostics
}

pub fn resolve_ref(
    &mut self,
    scheme: &str,
    target: &str,
) -> Result<RefResolution, ExtensionError> {
    let pkg = self.find_extension_for_scheme(scheme)?;
    self.set_phase(DispatchPhase::RefResolution);
    self.call_export_with_arg(pkg, "refs__resolve", target)
}

```

---

## 7. Backward Compatibility

### 7.1 Manifest v1 → v2

Manifests with a `kind` field (v1) are auto-migrated:

```rust
impl ExtensionManifest {
    fn migrate_v1(v1: &V1Manifest) -> Self {
        let mut contributes = ExtensionContributions::default();
        match v1.kind.as_str() {
            "plugin" => {
                contributes.entities = v1.entity_kinds.into();
                contributes.edges = v1.dynamic_edge_types.into();
                contributes.enhancements = v1.enhancements.into();
            }
            "provider" => {
                if let Some(p) = &v1.provider {
                    contributes.ref_schemes = p.schemes.iter()
                        .map(|s| ContributedRefScheme {
                            scheme: s.clone(),
                            kinds: p.kinds.get(s).cloned()
                                .unwrap_or_default(),
                        })
                        .collect();
                }
            }
            _ => {}
        }
        // ... map sandbox, peer_dependencies, etc.
    }
}
```

A deprecation warning `W026` is emitted for v1 manifests.

### 7.2 specforge.json `plugins` → `extensions`

The `plugins` key is accepted as an alias. The compiler:
1. Merges entries into the `extensions` resolution list
2. Emits deprecation warning `W027`

The `providers` key entries are converted:

```json
// Old:
{
  "providers": {
    "github": { "extension": "@specforge/gh", "repo": "org/repo" }
  }
}

// Equivalent:
{
  "extensions": [
    { "extension": "@specforge/gh", "config": { "repo": "org/repo" } }
  ]
}
```

---

## 8. Validation Codes

| Code | Severity | Description |
|---|---|---|
| E020 | Error | Missing Wasm export for declared contribution |
| E021 | Error | Ref scheme conflict (two extensions, same scheme) |
| E022 | Error | Entity kind conflict (two extensions, same kind name) |
| E023 | Error | Edge type conflict (two extensions, same label) |
| E025 | Error | Validator name conflict (two extensions, same name) |
| E026 | Error | Lock file integrity mismatch (`wasm_hash` changed) |
| E027 | Error | Extension version not found (registry) or ref not found (git) |
| W026 | Warning | Manifest v1 format deprecated (has `kind` field) |
| W027 | Warning | `plugins`/`providers` config keys deprecated, use `extensions` |
| W028 | Warning | Local extension `.wasm` hash differs from lock file |
| I008 | Info | Extension contributes to N extension points |

---

## 9. Concrete Example

### @specforge/jira — Single Extension, Full Integration

**manifest.json**:

```json
{
  "extension": "@specforge/jira",
  "manifest_version": "2",
  "version": "1.0.0",
  "wasm": "jira.wasm",

  "contributes": {
    "entities": [
      { "name": "epic",   "testable": false, "fields": [...] },
      { "name": "story",  "testable": false, "fields": [...] },
      { "name": "task",   "testable": true,  "fields": [...] },
      { "name": "sprint", "testable": false, "fields": [...] }
    ],
    "edges": [
      { "label": "belongs_to_sprint", "source_kind": "story", "target_kind": "sprint", "soft": false },
      { "label": "subtask_of", "source_kind": "task", "target_kind": "story", "soft": false }
    ],
    "enhancements": [
      { "target_entity": "behavior", "field_name": "jira_ticket", "field_type": { "kind": "string" } }
    ],
    "ref_schemes": [
      { "scheme": "jira", "kinds": ["issue", "epic", "story", "board"] }
    ],
    "validators": [
      { "name": "jira_key_format" }
    ],
    "query_extensions": [
      { "path": "queries/jira-highlights.scm" }
    ]
  },

  "sandbox": {
    "refs": { "allowed_domains": ["*.atlassian.net"], "allow_http": true }
  },

  "peer_dependencies": { "@specforge/product": ">=0.1.0" }
}
```

**specforge.json**:

```json
{
  "name": "my-project",
  "version": "1.0",
  "extensions": [
    "@specforge/product@^1.0.0",
    "@specforge/governance@^1.0.0",
    {
      "extension": "@specforge/jira",
      "version": "^1.0.0",
      "config": { "instance": "myteam.atlassian.net" }
    }
  ]
}
```

**.spec file usage**:

```
// Contributed entities
epic onboarding_flow "User Onboarding" {
  jira_key "PROJ-42"
  stories [signup_story, profile_story]
}

story signup_story "Sign Up Flow" {
  jira_key "PROJ-43"
  sprint sprint_q1
  points 5
}

// Contributed ref scheme
ref jira:PROJ-42 "Onboarding Epic"

// Contributed enhancement on core entity
behavior validate_signup {
  contract "User can sign up with email"
  jira_ticket "PROJ-43"
  verify unit "test_signup"
}
```

### Progressive Enhancement

```
@community/jira-lite v0.1.0:        (start small)
  contributes:
    ref_schemes: [{ scheme: "jira" }]

@community/jira-lite v0.5.0:        (add entities)
  contributes:
    entities: [epic, story]
    ref_schemes: [{ scheme: "jira" }]

@community/jira-lite v1.0.0:        (full integration)
  contributes:
    entities: [epic, story, task, sprint]
    edges: [belongs_to_sprint]
    ref_schemes: [{ scheme: "jira" }]
    validators: [{ name: "jira_key_format" }]

Same extension. Same name. Additive evolution.
```

---

## 10. Effort vs Impact

| Change | Effort | Impact |
|---|---|---|
| `ExtensionManifest` + `ExtensionContributions` structs | Medium | Foundation for everything else |
| Remove `PluginKind`, add `contributes` parsing | Low | Unblocks multi-role extensions |
| Per-call-site host function permissions | Medium | Security improvement |
| Extension source resolution (registry, local, git) | Medium | Enables version control |
| Lock file (`specforge.lock`) | Medium | Reproducible builds |
| `specforge.json` `extensions` key | Low | UX improvement |
| Backward compat (v1 manifest, `plugins` alias) | Low | No ecosystem breakage |
| Rename `Wasm*` → `Extension*` across crate | Medium | Naming consistency |
| New validation codes E020-E027, W026-W028 | Low | Clear diagnostics |
| CLI `specforge extension*` + `add`/`remove`/`update` | Medium | Extension management UX |

---

## 11. Recommendations

1. **Adopt the contribution model.** Replace `PluginKind` with `ExtensionContributions`. This is the critical architectural change.

2. **Apply the rename table.** The naming convention in section 4 is comprehensive. Apply it in a single coordinated PR.

3. **Implement the three source types.** Registry, local, and git cover all distribution needs without unnecessary complexity.

4. **Ship the lock file from day one.** Reproducible builds are table stakes. `specforge.lock` should be committed to version control.

5. **Implement per-call-site permissions.** This is a security win that comes naturally from the contribution dispatch model.

6. **Merge `PluginManifestV2` into `ExtensionManifest`.** Eliminate the dual-struct problem. `specforge-common` exports contribution types; `specforge-wasm` owns the full manifest.

7. **Keep `specforge-wasm` crate name.** The crate IS the Wasm runtime. Internal Wasm-specific types are fine. Only the public-facing types drop the `Wasm` prefix.

8. **Reserve future contribution keys.** Document `importers`, `reporters`, `syncers`, `mcp_tools` as planned. This signals extensibility to extension authors.

---

## Cross-References

- **RES-11a** — Core compiler architecture, pipeline phases
- **RES-22** — Tree-sitter + Wasm highlighting, query extensions
- `spec/behaviors/wasm.spec` — Behavioral specifications for the extension runtime
- `spec/features/wasm.spec` — Feature specifications for the extension model
- `spec/invariants/wasm.spec` — Runtime invariants for sandbox and isolation
- `spec/governance/decisions.spec` — ADRs `wasm_extism_plugin_runtime`, `wasm_peer_dependencies`
- `crates/runtime/wasm/src/manifest.rs` — Current `WasmPluginManifest` (to be replaced)
- `crates/kernel/common/src/field_registry.rs` — Current `FieldRegistry` + `PluginManifestV2`
- `schema/specforge.schema.json` — Current config schema (to be updated)
