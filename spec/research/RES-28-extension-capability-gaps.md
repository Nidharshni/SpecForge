---
id: RES-28
kind: research
title: "Extension Capability Gaps — Vision-to-Architecture Alignment Audit"
status: active
date: 2026-03-06
depends_on: [RES-23, RES-24, RES-26, RES-18]
priority: critical
tags: [extensions, vision, gaps, mcp, agents, traceability, graph-protocol, registry]
---

# RES-28: Extension Capability Gaps

## Executive Summary

A systematic audit of the vision (`docs/vision/README.md`, `docs/vision/principles.md`, `docs/vision/north-star.md`) against the current extension architecture (RES-23, RES-24, RES-26) reveals **7 capability gaps** — features the vision commits to that the extension model cannot currently deliver.

The gaps cluster around **Principle 3 (Agents are first-class consumers)**, which accounts for 3 of the 7. This is not a coincidence. RES-23 designed the extension model around the **compilation pipeline** (parse, register, build, resolve, validate, export). RES-24 began addressing **runtime surfaces** (CLI, MCP, LSP) but remains research. The vision puts agents at the center; the architecture puts the compiler at the center. The gaps are the delta between these two worldviews.

```
  VISION CENTER OF GRAVITY              ARCHITECTURE CENTER OF GRAVITY
 ┌──────────────────────────┐          ┌──────────────────────────┐
 │                          │          │                          │
 │      AI Agents           │          │      Compiler            │
 │      (consume graph,     │          │      (parse, resolve,    │
 │       validate plans,    │          │       validate, export)  │
 │       query live,        │          │                          │
 │       stream updates)    │          │      Extensions plug     │
 │                          │          │      into pipeline       │
 │      Extensions serve    │          │      phases              │
 │      agents + humans     │          │                          │
 │      equally             │          │      Agents consume      │
 │                          │          │      output post-hoc     │
 └──────────────────────────┘          └──────────────────────────┘
         ▲                                       ▲
         │ WHERE WE'RE GOING                     │ WHERE WE ARE
```

This document catalogs each gap, traces it to the violated principle, proposes a concrete fix with manifest schema changes, and estimates implementation scope. Gaps are ordered by vision-criticality, not implementation ease.

---

## Gap Inventory

| # | Gap | Violated Principle(s) | Severity | RES Dependency |
|---|-----|----------------------|----------|----------------|
| 1 | No MCP Server | P3 (agents first), P6 (graph = standard) | **Critical** | RES-24 |
| 2 | No multi-resolution graph queries | P3 (agents first) | **Critical** | RES-18 |
| 3 | No formal collector contribution type | P5 (traceability loop) | **High** | RES-17 |
| 4 | No plan validation | P3 (agents first) | **High** | RES-18 |
| 5 | No extension registry API | H2 (ecosystem), NS (not a walled garden) | **High** | RES-23 |
| 6 | No Graph Protocol schema extension mechanism | P6 (graph = standard), P3 (agents first) | **High** | RES-26 |
| 7 | No incremental graph delta events | P3 (agents first), P8 (seconds to value) | **Medium** | RES-24 |

---

## Gap 1: No MCP Server

### Violated Principles

- **P3:** "Every output format, every error message, every graph query is designed for machines as much as for humans."
- **P6:** "The Graph Protocol is the standard, not the compiler."
- **North Star H2:** "The Graph Protocol is consumed by multiple agent frameworks."

### Problem

SpecForge has no MCP server. AI agents — the vision's primary consumer — have no interactive interface to the graph. The only path today is:

```
specforge export --format=json | pbcopy   (paste into agent context)
```

This is batch-mode, non-interactive, and cannot serve the live query workflows that agents need. An agent cannot ask "what are the constraints on auth_login?" without consuming the entire graph.

MCP (Model Context Protocol) is the standard agent-to-tool interface in 2026. Every major agent framework supports it. RES-24 identifies MCP as the **superset surface** — CLI + LSP capabilities should be promotable to MCP tools. But RES-24 remains research with no implementation path.

### What This Blocks

```
┌─────────────────────────────────────────────────────────────────────┐
│  WITHOUT MCP SERVER                                                 │
│                                                                     │
│  Agent cannot:                                                      │
│  - Query specific entities by ID with depth control                 │
│  - Validate a plan against the graph interactively                  │
│  - Get coverage status for a behavior                               │
│  - Browse available entity kinds and their fields                   │
│  - Receive live diagnostics after a spec change                     │
│  - Use extension-contributed tools (collect, resolve, etc.)         │
│                                                                     │
│  Agent must:                                                        │
│  - Parse the entire exported graph (expensive)                      │
│  - Re-export after every change (slow)                              │
│  - Have no knowledge of what queries are possible                   │
└─────────────────────────────────────────────────────────────────────┘
```

### Proposed Architecture

```
┌─────────────────────────────────────────────────────────────────────┐
│                      SpecForge MCP Server                           │
│                                                                     │
│  BUILT-IN TOOLS (core, always available):                           │
│  ┌──────────────────┐ ┌──────────────────┐ ┌────────────────────┐  │
│  │ specforge.compile │ │ specforge.query  │ │ specforge.trace    │  │
│  │ Compile project,  │ │ Query entity by  │ │ Validate agent     │  │
│  │ return diagnostics│ │ ID + depth + fmt │ │ plan against graph │  │
│  └──────────────────┘ └──────────────────┘ └────────────────────┘  │
│  ┌──────────────────┐ ┌──────────────────┐ ┌────────────────────┐  │
│  │ specforge.schema │ │ specforge.search │ │ specforge.coverage │  │
│  │ Describe entity  │ │ Find entities by │ │ Get test coverage  │  │
│  │ kinds and fields │ │ kind, field, ref │ │ for entity/project │  │
│  └──────────────────┘ └──────────────────┘ └────────────────────┘  │
│                                                                     │
│  BUILT-IN RESOURCES (core):                                         │
│  ┌───────────────────────────────────────────────────────────────┐  │
│  │ specforge://graph                 Full graph (JSON)           │  │
│  │ specforge://graph/{entity_id}     Single entity + neighbors   │  │
│  │ specforge://diagnostics           Current diagnostics         │  │
│  │ specforge://schema                Entity kind schemas         │  │
│  └───────────────────────────────────────────────────────────────┘  │
│                                                                     │
│  EXTENSION-CONTRIBUTED (from surfaces.mcp_tools, per RES-24):       │
│  ┌──────────────────┐ ┌──────────────────┐ ┌────────────────────┐  │
│  │ rust.collect     │ │ gh.resolve_ref   │ │ product.roadmap    │  │
│  │ (@specforge/rust)│ │ (@specforge/gh)  │ │ (@specforge/prod)  │  │
│  └──────────────────┘ └──────────────────┘ └────────────────────┘  │
│                                                                     │
│  BUILT-IN PROMPTS:                                                  │
│  ┌───────────────────────────────────────────────────────────────┐  │
│  │ specforge://prompts/implement     "Implement {entity_id}"     │  │
│  │ specforge://prompts/review        "Review coverage for ..."   │  │
│  │ specforge://prompts/trace         "Trace plan against graph"  │  │
│  └───────────────────────────────────────────────────────────────┘  │
│                                                                     │
├────────────────────── TRANSPORT ────────────────────────────────────┤
│  stdio (default)  │  SSE (remote)  │  streamable-http (future)     │
└─────────────────────────────────────────────────────────────────────┘
```

### Tool Definitions

#### specforge.compile

```json
{
  "name": "specforge.compile",
  "description": "Compile the project's .spec files and return diagnostics",
  "inputSchema": {
    "type": "object",
    "properties": {
      "spec_root": { "type": "string", "description": "Path to spec root (default: auto-detect)" },
      "severity_filter": { "type": "string", "enum": ["error", "warning", "info"], "default": "warning" }
    }
  }
}
```

#### specforge.query

```json
{
  "name": "specforge.query",
  "description": "Query the spec graph for entities, relationships, and context",
  "inputSchema": {
    "type": "object",
    "properties": {
      "entity_id": { "type": "string", "description": "Entity ID to query" },
      "kind": { "type": "string", "description": "Filter by entity kind" },
      "depth": { "type": "integer", "default": 1, "description": "Neighborhood depth (0=entity only, 1=direct refs, 2=two-hop)" },
      "format": { "type": "string", "enum": ["full", "context", "brief"], "default": "context" },
      "view": { "type": "string", "description": "Named view from extension (see Gap 2)" },
      "include_coverage": { "type": "boolean", "default": false }
    },
    "required": ["entity_id"]
  }
}
```

#### specforge.trace

```json
{
  "name": "specforge.trace",
  "description": "Validate an agent's plan against the spec graph, identifying missing entities, unmet constraints, and coverage gaps",
  "inputSchema": {
    "type": "object",
    "properties": {
      "plan": {
        "type": "object",
        "properties": {
          "task": { "type": "string" },
          "touches": { "type": "array", "items": { "type": "string" } },
          "creates": { "type": "array", "items": { "type": "string" } },
          "modifies": { "type": "array", "items": { "type": "string" } }
        },
        "required": ["task"]
      }
    },
    "required": ["plan"]
  }
}
```

#### specforge.schema

```json
{
  "name": "specforge.schema",
  "description": "Describe installed entity kinds, their fields, edge types, and validation rules",
  "inputSchema": {
    "type": "object",
    "properties": {
      "kind": { "type": "string", "description": "Specific entity kind (omit for all)" },
      "include_edges": { "type": "boolean", "default": true },
      "include_validation_rules": { "type": "boolean", "default": false }
    }
  }
}
```

#### specforge.search

```json
{
  "name": "specforge.search",
  "description": "Search entities by kind, field value, reference target, or text match",
  "inputSchema": {
    "type": "object",
    "properties": {
      "kind": { "type": "string" },
      "field": { "type": "string", "description": "Field name to filter on" },
      "value": { "type": "string", "description": "Field value to match (substring)" },
      "references": { "type": "string", "description": "Find entities referencing this ID" },
      "text": { "type": "string", "description": "Free-text search across all string fields" },
      "limit": { "type": "integer", "default": 20 }
    }
  }
}
```

#### specforge.coverage

```json
{
  "name": "specforge.coverage",
  "description": "Get test coverage status for entities",
  "inputSchema": {
    "type": "object",
    "properties": {
      "entity_id": { "type": "string", "description": "Specific entity (omit for project summary)" },
      "kind": { "type": "string", "description": "Filter by entity kind" },
      "status_filter": { "type": "string", "enum": ["covered", "uncovered", "partial", "all"], "default": "all" }
    }
  }
}
```

### Token Economics (per RES-18)

```
Built-in tool definitions:    ~6 tools x 200 tokens  = 1,200 tokens
Extension tools (5 exts):     ~10 tools x 200 tokens = 2,000 tokens
Resources:                    ~5 resources x 100 tok  =   500 tokens
Prompts:                      ~3 prompts x 100 tok    =   300 tokens
                                                       ─────────────
Total MCP overhead per turn:                           ~4,000 tokens

At 128K context: ~3% overhead (acceptable)
At 32K context:  ~12% overhead (acceptable with tool filtering)
At 8K context:   ~50% overhead (requires lazy tool listing)
```

Tool filtering mitigation: `tools/list` supports `category` filter. Default: only project-relevant tools (from installed extensions). Agent can request `category: "all"` when exploring.

### Implementation Scope

| Component | Effort |
|-----------|--------|
| MCP server binary (`specforge-mcp`) | 2-3 weeks |
| Built-in tools (compile, query, search, coverage, schema, trace) | 2-3 weeks |
| Extension tool dispatch (from `surfaces.mcp_tools`) | 1-2 weeks |
| Resource and prompt support | 1 week |
| Transport (stdio + SSE) | 1 week |
| **Total** | **7-10 weeks** |

### Relationship to RES-24

RES-24 designed the surface contribution model for CLI, MCP, and LSP. This gap focuses specifically on the MCP server implementation and built-in tool definitions. RES-24's `surfaces.mcp_tools` manifest key provides the extension mechanism; this gap provides the host infrastructure.

---

## Gap 2: No Multi-Resolution Graph Queries

### Violated Principles

- **P3:** "Multi-resolution queries that let agents request exactly the context slice they need."
- **RES-18:** "Selective context loading replaces exploration — 70-90% token reduction."

### Problem

The graph can be exported as a monolithic JSON blob. There is no mechanism for:

1. **Depth-controlled queries** — "give me auth_login and its direct neighbors only"
2. **Format-controlled output** — "brief" (entity + title), "context" (entity + fields + 1-hop), "full" (complete subgraph)
3. **Extension-contributed views** — named projections of the graph optimized for specific agent workflows

Without multi-resolution queries, every agent interaction pays the token cost of the full graph. RES-18 quantifies this: agents waste 40-60% of their budget on exploration. Selective context loading is the primary mechanism SpecForge uses to eliminate that waste.

### What Exists Today

```
┌─────────────────────────────────────────────────────────────────────┐
│  TODAY                                                              │
│                                                                     │
│  specforge export --format=json    →  Full graph, all entities,     │
│                                       all edges, all metadata       │
│                                       (5,000-50,000+ tokens)        │
│                                                                     │
│  specforge export --format=dot     →  DOT graph (visual, not       │
│                                       agent-consumable)              │
│                                                                     │
│  specforge export --format=markdown→  Human-readable, not           │
│                                       structured for agents         │
│                                                                     │
│  No depth control. No entity filtering. No format variants.         │
│  No extension-contributed views.                                    │
└─────────────────────────────────────────────────────────────────────┘
```

### Proposed Architecture

#### Core Query Engine

The core provides a generic subgraph extraction algorithm. Extensions contribute named views that configure the extraction.

```
┌─────────────────────────────────────────────────────────────────────┐
│  QUERY PIPELINE                                                     │
│                                                                     │
│  1. SEED       Select starting entities (by ID, kind, or search)    │
│  2. EXPAND     Walk edges to depth N (configurable per edge type)   │
│  3. FILTER     Apply view-specific inclusion/exclusion rules        │
│  4. FORMAT     Serialize to requested format (brief/context/full)   │
│  5. BUDGET     Truncate if result exceeds token budget hint         │
│                                                                     │
│  Steps 1-2, 4-5: CORE (domain-agnostic graph traversal)            │
│  Step 3: EXTENSION-CONTRIBUTED (domain-specific view logic)         │
└─────────────────────────────────────────────────────────────────────┘
```

#### Format Tiers

| Format | Content | Token Cost (typical) | Use Case |
|--------|---------|---------------------|----------|
| `brief` | Entity ID, kind, title | ~50 tokens/entity | Listings, overviews |
| `context` | + fields, 1-hop refs (IDs only) | ~200 tokens/entity | Agent working context |
| `full` | + all edges, all metadata, N-hop | ~500 tokens/entity | Deep analysis |

#### Extension-Contributed Views

A new `graph_views` contribution type lets extensions declare named projections:

```json
{
  "extension": "@specforge/product",
  "contributes": {
    "graph_views": [
      {
        "name": "roadmap-status",
        "description": "Roadmap phases with linked deliverables and coverage percentages",
        "seed_kinds": ["roadmap"],
        "expand_edges": ["tracks_deliverable", "implements"],
        "max_depth": 2,
        "include_coverage": true,
        "format_override": "context"
      },
      {
        "name": "feature-tree",
        "description": "Feature hierarchy with implementing behaviors and their test status",
        "seed_kinds": ["feature"],
        "expand_edges": ["implements", "uses_port", "enforced_by"],
        "max_depth": 3,
        "include_coverage": true
      }
    ]
  }
}
```

```json
{
  "extension": "@specforge/compliance",
  "contributes": {
    "graph_views": [
      {
        "name": "audit-trail",
        "description": "Regulations with controls, evidence, and audit findings",
        "seed_kinds": ["regulation"],
        "expand_edges": ["controls", "evidence", "findings"],
        "max_depth": 3,
        "include_coverage": true
      },
      {
        "name": "gap-analysis",
        "description": "Regulations with missing or incomplete controls",
        "seed_kinds": ["regulation"],
        "expand_edges": ["controls"],
        "filter": "missing_outgoing_edges",
        "max_depth": 2
      }
    ]
  }
}
```

#### CLI and MCP Integration

```bash
# CLI
specforge query auth_login --depth=2 --format=context
specforge query --kind=behavior --format=brief
specforge query --view=roadmap-status
specforge query --view=audit-trail --format=full

# MCP (specforge.query tool)
{ "entity_id": "auth_login", "depth": 2, "format": "context" }
{ "kind": "behavior", "format": "brief" }
{ "view": "roadmap-status" }
```

### Manifest Schema Addition

Add `graph_views` to the `contributes` object (compile-time contribution, not a surface):

```
contributes:
  entities: [...]          # existing (RES-23)
  edges: [...]             # existing (RES-23)
  enhancements: [...]      # existing (RES-23)
  ref_schemes: [...]       # existing (RES-23)
  validators: [...]        # existing (RES-23)
  query_extensions: [...]  # existing (RES-23)
  graph_views: [...]       # NEW — named graph projections
```

Graph views are **declarative** — the core query engine interprets them. No Wasm export needed. Extensions describe what to include; the core does the traversal and formatting.

For **complex views** that require computation beyond declarative configuration, extensions can implement an optional `view__{name}` Wasm export that receives the subgraph and transforms it before formatting.

### Implementation Scope

| Component | Effort |
|-----------|--------|
| Core query engine (seed, expand, filter, format, budget) | 2-3 weeks |
| `graph_views` manifest schema and registry | 1 week |
| CLI integration (`specforge query`) | 1 week |
| MCP integration (`specforge.query` tool) | included in Gap 1 |
| Wasm view exports (optional, for complex views) | 1 week |
| **Total** | **5-6 weeks** |

---

## Gap 3: No Formal Collector Contribution Type

### Violated Principles

- **P5:** "Specs link to tests. Tests produce results. Results feed back into the graph. This is the mechanism that makes AI agents self-correcting."
- **North Star:** "Traceability makes SpecForge compound in value over time."

### Problem

The traceability feedback loop has a missing formal contract. Test report consumption (`specforge collect`) exists as ad-hoc CLI commands contributed by extensions via `surfaces.commands`. But there is no **typed contribution** that formally models:

1. What input formats a collector supports
2. What output it produces (always `specforge-report.json`)
3. How entity-to-test mappings work
4. How the collected report feeds back into the graph

This matters because the traceability loop is THE compounding value mechanism. Without a formal type, the collect step is a black box that each extension implements differently, with no guarantees about the output contract.

```
  THE TRACEABILITY LOOP (PRINCIPLE 5)
 ┌──────────────────────────────────────────────────────────────────────┐
 │                                                                      │
 │  .spec files ──> Compile ──> Graph ──> Agent ──> Code + Tests       │
 │       ▲                                              │               │
 │       │                                              ▼               │
 │       │         ┌─────────────────────┐        Test Runner           │
 │       │         │ specforge-report.json│             │               │
 │       └──────── │ (coverage + results) │ <──────────┘               │
 │                 └──────────┬──────────┘                              │
 │                            │                                         │
 │                    ┌───────┴────────┐                                │
 │                    │   COLLECTOR    │  <── THIS IS THE GAP           │
 │                    │               │                                  │
 │                    │  Input:  ???   │  No formal contract             │
 │                    │  Output: ???   │  No schema validation           │
 │                    │  Mapping: ???  │  No entity mapping rules        │
 │                    └───────────────┘                                  │
 └──────────────────────────────────────────────────────────────────────┘
```

### Proposed Design

#### Collector Contribution Type

Add `collectors` to the `contributes` object:

```json
{
  "extension": "@specforge/rust",
  "contributes": {
    "collectors": [
      {
        "name": "rust",
        "description": "Collect Rust test results from cargo-nextest JUnit XML",
        "input_formats": ["junit-xml"],
        "auto_detect": {
          "file_patterns": ["target/nextest/*/junit.xml", "test-results.xml"],
          "env_vars": ["CARGO_TARGET_DIR"]
        },
        "entity_mapping": {
          "strategies": [
            { "priority": 1, "type": "tests_field", "description": "Explicit tests field in .spec entity" },
            { "priority": 2, "type": "proc_macro", "description": "#[specforge::test(\"entity_id\")] annotation" },
            { "priority": 3, "type": "naming_convention", "description": "{entity_id}__{test_slug} pattern" }
          ]
        },
        "export": "collect__rust",
        "output_schema": "specforge-report/v1"
      }
    ]
  }
}
```

```json
{
  "extension": "@specforge/cucumber",
  "contributes": {
    "collectors": [
      {
        "name": "cucumber",
        "description": "Collect Cucumber/Gherkin test results",
        "input_formats": ["cucumber-json", "cucumber-ndjson"],
        "auto_detect": {
          "file_patterns": ["reports/cucumber.json", "cucumber-report.json"]
        },
        "entity_mapping": {
          "strategies": [
            { "priority": 1, "type": "gherkin_field", "description": "gherkin field in .spec points to .feature file" },
            { "priority": 2, "type": "tag_annotation", "description": "@specforge:entity_id scenario tag" }
          ]
        },
        "export": "collect__cucumber",
        "output_schema": "specforge-report/v1"
      }
    ]
  }
}
```

#### Wasm Export Contract

```
fn collect__{name}(input_json_ptr: u64) -> u64

Input:  {
  "report_path": "/path/to/junit.xml",
  "format": "junit-xml",
  "entity_ids": ["auth_login", "auth_logout", ...],
  "mapping_config": { ... }
}

Output: {
  "schema": "specforge-report/v1",
  "entries": [
    {
      "entity_id": "auth_login",
      "test_id": "tests::auth::test_login_valid",
      "status": "pass",
      "duration_ms": 42,
      "source": "target/nextest/default/junit.xml:23"
    }
  ],
  "unmapped_tests": [...],
  "stats": { "total": 150, "mapped": 130, "unmapped": 20 }
}
```

#### CLI Dispatch

```bash
# Auto-detect collector from installed extensions
specforge collect                         # scans for known file patterns

# Explicit collector
specforge collect rust --junit-xml target/nextest/default/junit.xml

# Explicit collector with format
specforge collect cucumber --format=cucumber-json reports/cucumber.json
```

The `specforge collect` command with no arguments iterates over installed extensions' `collectors` contributions, runs each `auto_detect` pattern, and dispatches to the first matching collector. This is the **zero-config traceability** path.

### Implementation Scope

| Component | Effort |
|-----------|--------|
| `collectors` manifest schema | 0.5 week |
| Core `collect` dispatcher (auto-detect + explicit) | 1-2 weeks |
| `specforge-report/v1` output schema validation | 1 week |
| Report ingestion back into graph (coverage overlay) | 1-2 weeks |
| **Total** | **3-5 weeks** |

---

## Gap 4: No Plan Validation

### Violated Principles

- **P3:** "Agents are first-class consumers."
- **P5:** "When an agent can see that a behavior is specified, partially implemented, and has one failing test — it can fix the failing test."
- **RES-18:** "Plan validation — `specforge trace --plan plan.json`"

### Problem

Agents generate implementation plans before writing code. Today, those plans are unvalidated against the spec graph. An agent might plan to modify `auth_login` without knowing that `rate_limit_constraint` applies to it, or create a new `auth_logout` behavior without linking the `session_expired` event.

Plan validation is the bridge between "agent reads graph" (passive consumption) and "agent acts on graph" (active validation). Without it, the graph is reference material. With it, the graph is a guardrail.

### Proposed Design

#### Plan Schema

```json
{
  "$schema": "specforge-plan/v1",
  "task": "Add logout functionality",
  "agent": "claude-code",
  "entities": {
    "touches": ["auth_login", "session_port"],
    "creates": [
      {
        "id": "auth_logout",
        "kind": "behavior",
        "implements": ["user_authentication"],
        "produces": ["session_expired"],
        "uses_port": ["session_port"]
      }
    ],
    "modifies": [
      {
        "id": "auth_login",
        "changes": ["adds session_created event production"]
      }
    ]
  },
  "tests": {
    "creates": ["test_logout_clears_session", "test_logout_emits_event"],
    "maps_to": { "auth_logout": ["test_logout_clears_session", "test_logout_emits_event"] }
  }
}
```

#### Trace Output

```json
{
  "plan_valid": false,
  "findings": [
    {
      "severity": "error",
      "code": "T001",
      "message": "Plan touches auth_login but does not account for constraint rate_limit_all_auth",
      "entity": "auth_login",
      "missing_ref": "rate_limit_all_auth",
      "edge": "constrained_by"
    },
    {
      "severity": "warning",
      "code": "T002",
      "message": "New behavior auth_logout uses session_port but does not reference invariant session_must_be_active",
      "entity": "auth_logout",
      "suggestion": "Add invariant reference: invariants [session_must_be_active]"
    },
    {
      "severity": "info",
      "code": "T003",
      "message": "Plan creates auth_logout with 2 tests — coverage is adequate",
      "entity": "auth_logout"
    }
  ],
  "graph_impact": {
    "new_nodes": 1,
    "new_edges": 4,
    "affected_existing_nodes": 2
  }
}
```

#### Core vs Extension Responsibility

```
┌─────────────────────────────────────────────────────────────────────┐
│  CORE TRACE ENGINE (domain-agnostic)                                │
│                                                                     │
│  - For each "touches" entity: find all incoming/outgoing edges      │
│    and check the plan accounts for them                             │
│  - For each "creates" entity: validate references exist and         │
│    required fields per KindRegistry are present                     │
│  - For each "modifies" entity: check existing constraints and       │
│    invariants still hold after the described change                 │
│  - Coverage check: verify testable entities have test mappings      │
│                                                                     │
├─────────────────────────────────────────────────────────────────────┤
│  EXTENSION PLAN VALIDATORS (domain-specific, optional)              │
│                                                                     │
│  contributes.plan_validators:                                       │
│  - @specforge/software: "behaviors implementing a feature must      │
│    cover all verify declarations"                                   │
│  - @specforge/compliance: "new controls must reference a            │
│    regulation and have evidence"                                    │
│  - @specforge/governance: "decisions referenced by plan must be     │
│    in 'accepted' status"                                            │
│                                                                     │
│  Wasm export: fn plan__validate(plan_json, graph_json) -> findings  │
└─────────────────────────────────────────────────────────────────────┘
```

#### Manifest Addition

```json
{
  "extension": "@specforge/software",
  "contributes": {
    "plan_validators": [
      {
        "name": "verify_coverage",
        "description": "Ensure planned behaviors have verify blocks for all declared test kinds",
        "export": "plan__verify_coverage"
      }
    ]
  }
}
```

### Interface Points

- **CLI:** `specforge trace plan.json` — validate and print findings
- **MCP:** `specforge.trace` tool (defined in Gap 1)
- **CI:** Exit code non-zero if plan has error-severity findings

### Implementation Scope

| Component | Effort |
|-----------|--------|
| Plan schema (`specforge-plan/v1`) | 0.5 week |
| Core trace engine (graph impact analysis) | 2-3 weeks |
| `plan_validators` contribution type | 1 week |
| CLI command (`specforge trace`) | 0.5 week |
| MCP tool integration | included in Gap 1 |
| **Total** | **4-5 weeks** |

---

## Gap 5: No Extension Registry API

### Violated Principles

- **North Star H2:** "Community extension authors build vocabularies for domains the core team never imagined. The extension registry grows organically."
- **North Star "Not a walled garden":** "Extensions are open. The registry is open. Community contributions are first-class citizens."

### Problem

RES-23 defines three source types (registry `@scope/name`, local `./path`, git `git:url#ref`). The local and git sources work today. The registry source (`@scope/name`) has no implementation target — there is no registry API, no publish endpoint, no search capability.

Without a registry:
- Extension discovery relies on word-of-mouth or GitHub search
- `specforge add @specforge/compliance` has no resolution target
- Version pinning in `specforge.lock` cannot verify against a source of truth
- The H2 vision of "20+ community extensions" has no distribution mechanism

### Proposed Design

#### Registry API Contract

The registry is specified as an **HTTP API contract** that any implementation can fulfill. SpecForge does not mandate a specific registry host — organizations can run private registries.

```
┌─────────────────────────────────────────────────────────────────────┐
│  REGISTRY API v1                                                    │
│                                                                     │
│  GET  /v1/extensions/{scope}/{name}                                 │
│       → Extension metadata, versions, download URLs                 │
│                                                                     │
│  GET  /v1/extensions/{scope}/{name}/{version}                       │
│       → Specific version metadata + wasm download URL               │
│                                                                     │
│  GET  /v1/search?q={query}&kind={contribution_type}                 │
│       → Search by name, description, or contribution type           │
│                                                                     │
│  PUT  /v1/extensions/{scope}/{name}/{version}                       │
│       → Publish (authenticated, scoped to @scope owner)             │
│                                                                     │
│  GET  /v1/extensions/{scope}/{name}/{version}/wasm                  │
│       → Download .wasm binary                                       │
│                                                                     │
│  GET  /v1/extensions/{scope}/{name}/{version}/manifest              │
│       → Download manifest.json                                      │
└─────────────────────────────────────────────────────────────────────┘
```

#### Extension Metadata Response

```json
{
  "extension": "@specforge/compliance",
  "description": "Regulatory compliance vocabulary for SpecForge",
  "latest": "1.2.0",
  "versions": ["1.0.0", "1.1.0", "1.2.0"],
  "contributes_summary": {
    "entities": ["regulation", "control", "evidence", "audit_finding"],
    "edges": ["controls", "evidences", "findings"],
    "ref_schemes": [],
    "validators": ["control_requires_regulation", "evidence_requires_control"],
    "graph_views": ["audit-trail", "gap-analysis"],
    "collectors": []
  },
  "peer_dependencies": {},
  "downloads": 1250,
  "published_at": "2026-02-15T10:00:00Z",
  "wasm_size_bytes": 245760,
  "sha256": "abc123..."
}
```

#### Search by Contribution Type

```bash
# Find all extensions that contribute entity kinds
specforge search --contributes=entities

# Find extensions that provide a collector for JUnit
specforge search --contributes=collectors --query=junit

# Find extensions with MCP tools for GitHub
specforge search --contributes=mcp_tools --query=github
```

This directly implements RES-23 Expert #8's verdict: "The registry indexes contributions, not extensions."

#### Configuration

```json
// specforge.json
{
  "registries": {
    "default": "https://registry.specforge.dev/v1",
    "internal": "https://specforge-registry.acme.corp/v1"
  }
}
```

Multiple registries resolve in order. Scoped packages (`@acme/*`) can be routed to specific registries.

#### Trust Model

```
┌─────────────────────────────────────────────────────────────────────┐
│  TRUST LEVELS                                                       │
│                                                                     │
│  1. VERIFIED (@specforge/* scope)                                   │
│     - Published by the SpecForge project                            │
│     - Signed with project key                                       │
│     - Audited source code                                           │
│                                                                     │
│  2. COMMUNITY (@author/* scope)                                     │
│     - Published by authenticated community members                  │
│     - SHA256 integrity verified via specforge.lock                  │
│     - No source audit guarantee                                     │
│                                                                     │
│  3. LOCAL (./path)                                                  │
│     - Developer's own extension, no registry involved               │
│     - Full trust (it's your code)                                   │
│                                                                     │
│  4. GIT (git:url#ref)                                               │
│     - Source-verifiable via commit hash                              │
│     - Pinned in specforge.lock with hash                            │
└─────────────────────────────────────────────────────────────────────┘
```

### Implementation Scope

| Component | Effort |
|-----------|--------|
| Registry API specification (OpenAPI) | 1 week |
| Client-side resolution (`specforge add @scope/name`) | 2 weeks |
| `specforge search` CLI command | 1 week |
| `specforge extension publish` integration | 1-2 weeks |
| Registry server (reference implementation) | 3-4 weeks |
| **Total** | **8-10 weeks** |

Note: The registry server is a separate project. The client-side spec and integration (4-5 weeks) can ship before the server is built. Local and git sources work independently.

---

## Gap 6: No Graph Protocol Schema Extension Mechanism

### Violated Principles

- **P6:** "The Graph Protocol is SpecForge's equivalent [of SQL, OpenAPI, Protobuf]. It defines the structure of the entity graph."
- **P3:** "Structured JSON with deterministic ordering. Stable schemas that do not break between versions."
- **North Star H3:** "The Graph Protocol is an open industry standard. Multiple compilers produce it. Multiple agent platforms consume it natively."

### Problem

When extensions add entity kinds with custom fields, those fields appear in the Graph Protocol JSON output as opaque key-value pairs. An agent consuming the graph has no way to know:

1. **What entity kinds exist** — without reading the manifest
2. **What fields each kind has** — without trial and error
3. **What types those fields are** — string? reference list? enum?
4. **Which fields are references** — and to what target kinds?
5. **Which extension contributed each kind** — for attribution and trust

The Graph Protocol output is not **self-describing**. An agent receiving a graph from an unknown project must either have prior knowledge of the installed extensions or treat all fields as opaque strings.

This directly blocks the H3 vision where "multiple agent platforms consume [the graph] natively." Native consumption requires schema introspection.

### Current Output (no schema)

```json
{
  "format_version": "1.0",
  "nodes": [
    {
      "id": "auth_login",
      "kind": "behavior",
      "title": "User Login",
      "fields": {
        "contract": "The system MUST validate credentials...",
        "implements": ["user_authentication"],
        "produces": ["login_succeeded", "login_failed"],
        "uses_port": ["auth_port"]
      }
    }
  ],
  "edges": [
    { "source": "auth_login", "target": "user_authentication", "label": "implements" }
  ]
}
```

An agent sees `"kind": "behavior"` but has no schema for what a behavior IS. It sees `"implements"` but doesn't know it's a reference list to features.

### Proposed Output (with schema section)

```json
{
  "format_version": "2.0",

  "schema": {
    "extensions": [
      {
        "name": "@specforge/software",
        "version": "1.0.0"
      },
      {
        "name": "@specforge/governance",
        "version": "1.0.0"
      }
    ],
    "entity_kinds": {
      "behavior": {
        "extension": "@specforge/software",
        "testable": true,
        "singleton": false,
        "fields": {
          "contract": { "type": "string", "required": false },
          "implements": { "type": "reference_list", "target_kind": "feature" },
          "produces": { "type": "reference_list", "target_kind": "event" },
          "consumes": { "type": "reference_list", "target_kind": "event" },
          "uses_type": { "type": "reference_list", "target_kind": "type" },
          "uses_port": { "type": "reference_list", "target_kind": "port" },
          "tests": { "type": "string_list" }
        }
      },
      "decision": {
        "extension": "@specforge/governance",
        "testable": false,
        "singleton": false,
        "fields": {
          "status": { "type": "enum", "values": ["proposed", "accepted", "deprecated", "superseded"] },
          "context": { "type": "string" },
          "consequences": { "type": "string" }
        }
      }
    },
    "edge_types": {
      "implements": { "source_kind": "behavior", "target_kind": "feature", "extension": "@specforge/software" },
      "produces": { "source_kind": "behavior", "target_kind": "event", "extension": "@specforge/software" },
      "constrained_by": { "source_kind": "*", "target_kind": "constraint", "extension": "@specforge/governance" }
    }
  },

  "nodes": [ ... ],
  "edges": [ ... ],
  "diagnostics": [ ... ],
  "coverage": { ... }
}
```

### Agent Benefits

```
┌─────────────────────────────────────────────────────────────────────┐
│  WITH SELF-DESCRIBING GRAPH                                         │
│                                                                     │
│  Agent can:                                                         │
│  - Enumerate all entity kinds without prior knowledge               │
│  - Discover field types (reference vs string vs enum)               │
│  - Follow reference edges with type safety                          │
│  - Filter entities by kind with confidence in field availability    │
│  - Attribute entity kinds to extensions (trust/provenance)          │
│  - Validate its own plan against the schema                         │
│  - Generate correct spec files without seeing examples              │
│                                                                     │
│  Agent cannot (today):                                              │
│  - Know what "implements" means without guessing                    │
│  - Distinguish reference fields from string fields                  │
│  - Know which fields are required vs optional                       │
│  - Know which entities are testable                                 │
└─────────────────────────────────────────────────────────────────────┘
```

### Schema Generation

The schema section is generated automatically from the populated KindRegistry and FieldRegistry after extension loading. No additional extension work required — manifests already declare everything needed. The schema section is the **serialized form of the registries**.

### MCP Resource

The schema is also exposed as an MCP resource (`specforge://schema`) and via the `specforge.schema` tool (Gap 1), so agents can query it independently of a full graph export.

### Implementation Scope

| Component | Effort |
|-----------|--------|
| Schema section generation from registries | 1-2 weeks |
| Graph Protocol v2 schema definition (JSON Schema) | 1 week |
| Export engine integration | 0.5 week |
| MCP resource (`specforge://schema`) | included in Gap 1 |
| **Total** | **2-4 weeks** |

This is the **lowest-effort, highest-impact** gap. It makes every graph export immediately more useful to every agent.

---

## Gap 7: No Incremental Graph Delta Events

### Violated Principles

- **P3:** "Agents are first-class consumers" (live workflows, not just batch)
- **P8:** "Seconds to value" (recompiling the full graph on every edit is slow)

### Problem

In watch mode (`specforge watch`) and the LSP server, the compiler recompiles on file changes. But there is no mechanism for:

1. Extensions to receive **delta events** (what changed) instead of re-running against the full graph
2. MCP clients to receive **streaming updates** when the graph changes
3. The LSP to notify extensions of **incremental graph mutations**

Today, every file change triggers a full recompile, full re-validation (including all extension validators), and full re-export. For small edits, this wastes most of the computation.

For MCP in particular, an agent working with a live project should be notified when the graph changes (a developer edits a .spec file), rather than polling or re-querying.

### Proposed Design

#### Graph Delta Event

```json
{
  "type": "graph_delta",
  "timestamp": "2026-03-06T14:30:00Z",
  "changes": {
    "added_nodes": [
      { "id": "auth_logout", "kind": "behavior", "file": "spec/behaviors/auth.spec", "line": 45 }
    ],
    "removed_nodes": [],
    "modified_nodes": [
      {
        "id": "auth_login",
        "changed_fields": ["contract"],
        "file": "spec/behaviors/auth.spec",
        "line": 12
      }
    ],
    "added_edges": [
      { "source": "auth_logout", "target": "user_authentication", "label": "implements" }
    ],
    "removed_edges": [],
    "affected_files": ["spec/behaviors/auth.spec"]
  },
  "diagnostics_delta": {
    "added": [],
    "removed": [{ "code": "W001", "entity": "auth_logout" }]
  }
}
```

#### Extension Subscription

Extensions that want incremental updates declare it in the manifest:

```json
{
  "extension": "@specforge/product",
  "contributes": {
    "validators": [
      {
        "name": "roadmap_coverage",
        "incremental": true,
        "watches_kinds": ["behavior", "feature", "deliverable"],
        "export": "validate__roadmap_coverage"
      }
    ]
  }
}
```

When `incremental: true`, the host calls the validator with the delta instead of the full graph. The validator can maintain internal state across invocations (warm engine, per Gap 1's `warm_wasm_engine_instance`). If the extension does not declare `incremental`, it receives the full graph on every change (backward compatible).

#### MCP Streaming

The MCP server supports notifications for graph changes:

```
MCP Client ←── notification ─── SpecForge MCP Server
                                      │
                 { method: "notifications/graph_changed",
                   params: { delta: { ... } } }
```

Agents opt in by calling a `specforge.subscribe` tool or by connecting with streaming transport (SSE / streamable-http).

#### LSP Integration

The LSP server already recompiles on file changes and pushes diagnostics. With deltas, it additionally pushes:
- Updated semantic tokens for changed entities
- Updated code lens (coverage changes)
- Workspace edit suggestions for newly orphaned entities

### Implementation Scope

| Component | Effort |
|-----------|--------|
| Graph diff algorithm (previous graph vs new graph) | 2-3 weeks |
| Delta event schema | 0.5 week |
| Extension incremental validator dispatch | 1-2 weeks |
| MCP notification support | 1 week |
| LSP delta integration | 1-2 weeks |
| **Total** | **6-8 weeks** |

Note: This is the lowest-priority gap because the full-recompile path works correctly — it's a performance and UX optimization, not a capability gap. Prioritize after Gaps 1-6.

---

## Implementation Priority

Ordering by vision-criticality weighted against effort:

```
┌─────┬────────────────────────────────────┬──────────┬──────────┬───────────┐
│ Ord │ Gap                                │ Severity │ Effort   │ Rationale │
├─────┼────────────────────────────────────┼──────────┼──────────┼───────────┤
│  1  │ Gap 6: Self-describing graph       │ High     │ 2-4 wk  │ Lowest    │
│     │        schema                      │          │          │ effort,   │
│     │                                    │          │          │ highest   │
│     │                                    │          │          │ ROI —     │
│     │                                    │          │          │ every     │
│     │                                    │          │          │ agent     │
│     │                                    │          │          │ benefits  │
├─────┼────────────────────────────────────┼──────────┼──────────┼───────────┤
│  2  │ Gap 2: Multi-resolution queries    │ Critical │ 5-6 wk  │ Core      │
│     │                                    │          │          │ agent     │
│     │                                    │          │          │ value     │
│     │                                    │          │          │ prop.     │
│     │                                    │          │          │ Blocks    │
│     │                                    │          │          │ token     │
│     │                                    │          │          │ savings   │
├─────┼────────────────────────────────────┼──────────┼──────────┼───────────┤
│  3  │ Gap 1: MCP server                  │ Critical │ 7-10 wk │ Primary   │
│     │                                    │          │          │ agent     │
│     │                                    │          │          │ interface │
│     │                                    │          │          │ (depends  │
│     │                                    │          │          │ on Gap 2) │
├─────┼────────────────────────────────────┼──────────┼──────────┼───────────┤
│  4  │ Gap 3: Collector contribution      │ High     │ 3-5 wk  │ Closes    │
│     │                                    │          │          │ the P5    │
│     │                                    │          │          │ loop      │
│     │                                    │          │          │ formally  │
├─────┼────────────────────────────────────┼──────────┼──────────┼───────────┤
│  5  │ Gap 4: Plan validation             │ High     │ 4-5 wk  │ Agent     │
│     │                                    │          │          │ guardrail │
│     │                                    │          │          │ (depends  │
│     │                                    │          │          │ on Gap 2) │
├─────┼────────────────────────────────────┼──────────┼──────────┼───────────┤
│  6  │ Gap 5: Extension registry          │ High     │ 8-10 wk │ Blocks    │
│     │                                    │          │          │ H2 but    │
│     │                                    │          │          │ not H1.   │
│     │                                    │          │          │ Spec now, │
│     │                                    │          │          │ build     │
│     │                                    │          │          │ later     │
├─────┼────────────────────────────────────┼──────────┼──────────┼───────────┤
│  7  │ Gap 7: Incremental deltas          │ Medium   │ 6-8 wk  │ Perf      │
│     │                                    │          │          │ optim,    │
│     │                                    │          │          │ not       │
│     │                                    │          │          │ blocking  │
└─────┴────────────────────────────────────┴──────────┴──────────┴───────────┘

Total estimated effort: 36-48 weeks (overlapping; parallel tracks possible)
```

### Parallel Tracks

```
TRACK A (Agent Interface):     Gap 6 → Gap 2 → Gap 1 → Gap 4
                               [2wk]   [5wk]   [7wk]   [4wk]   = 18 weeks serial

TRACK B (Traceability):        Gap 3 → (feeds into Gap 1 MCP tools)
                               [4wk]                             = 4 weeks serial

TRACK C (Ecosystem):           Gap 5 (spec only: 2wk now, build later)
                               [2wk]                             = 2 weeks serial

TRACK D (Performance):         Gap 7 (after Gaps 1-6)
                               [7wk]                             = 7 weeks serial

Parallel execution:            ~18 weeks (Track A dominates critical path)
```

---

## Manifest Schema Summary

All 7 gaps together add the following to the extension manifest:

```
EXISTING (RES-23):                    NEW (this research):
contributes:                          contributes:
  entities: [...]                       entities: [...]
  edges: [...]                          edges: [...]
  enhancements: [...]                   enhancements: [...]
  ref_schemes: [...]                    ref_schemes: [...]
  validators: [...]                     validators: [...] + incremental flag (Gap 7)
  query_extensions: [...]               query_extensions: [...]
                                        graph_views: [...]        ← Gap 2
                                        collectors: [...]         ← Gap 3
                                        plan_validators: [...]    ← Gap 4

EXISTING (RES-24):                    UNCHANGED:
surfaces:                             surfaces:
  commands: [...]                       commands: [...]
  mcp_tools: [...]                      mcp_tools: [...]
  mcp_resources: [...]                  mcp_resources: [...]
  lsp: { ... }                          lsp: { ... }
```

The Graph Protocol output gains a `schema` section (Gap 6). The MCP server (Gap 1) and registry API (Gap 5) are infrastructure, not manifest changes. The incremental delta (Gap 7) adds an `incremental` flag to existing validator contributions.

**Three new contribution keys. One new output section. Two new infrastructure components. One new flag.** That is the total delta to close all 7 gaps.

---

*RES-28. Vision-to-architecture alignment audit. 7 gaps, 3 new contribution types, 2 infrastructure components.*
