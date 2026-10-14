# Architecture

How SpecForge is built. For what it is and why it exists, start with the [README](README.md).

## Zero-domain-knowledge core + extensions

The compiler is a **pure typed-graph engine**. It knows how to parse `keyword name { fields }` blocks, resolve references, detect orphans and cycles, and emit a validated graph — but it carries **no domain vocabulary**. Every entity kind, edge type, and validation rule comes from an extension. If a new domain required a compiler change, the architecture would have failed.

Nine extensions ship as builtins, embedded in the binary. Enable one with `specforge add @specforge/<name>` (or `specforge init --extensions ...`); it is recorded in `specforge.json`, with nothing to download:

| Extension | Entity kinds | Purpose |
|-----------|-------------|---------|
| **`@specforge/software`** | behavior · invariant · event · type · port | The universal specification chain: contracts, guarantees, data, interfaces. |
| **`@specforge/product`** | journey · deliverable · milestone · module · term · feature · persona · channel · release | Product planning, roadmaps, and ubiquitous language. |
| **`@specforge/governance`** | decision · constraint · failure_mode | Architecture decisions, non-functional requirements, FMEA risk. |
| **`@specforge/formal`** | property · axiom · protocol · refinement · process | Formal methods: temporal properties, specification layering, event-graph linting. Enhances software entities. |
| **`@specforge/testing`** | — | Test vocabulary: which kinds accept `verify` obligations and of which kinds (W004/W009). Enabled with `@specforge/software`; test-runner extensions build on it ([ADR 0002](docs/adr/0002-test-runner-extensions.md)). |
| **`@specforge/cargo-test`** | — | Rust test runner: `specforge collect` runs `cargo test` (after you approve the command once per project) and records which entities the `#[specforge_test]`-annotated tests prove. Enabled by `init` in a Cargo project; `specforge add` also enables `@specforge/testing`. |
| **`@specforge/vitest`** | — | vitest runner: `specforge collect` runs the project's own vitest (after you approve the command once per project) and records which entities tests name in their `meta.specforge`. Enabled by `init` in a vitest project; `specforge add` also enables `@specforge/testing`. |
| **`@specforge/rust`** | — | Source analyzer used by inference: maps Rust code to spec entities. |
| **`@specforge/typescript`** | — | Source analyzer used by inference: maps TypeScript/JavaScript code to spec entities. |

Anyone can author and publish their own extension ([Extension Protocol](docs/extension-protocol.md)); the open Graph Protocol is the moat, not the implementation. The full catalog is in the [extension inventory](docs/extension-inventory.md).

## Components

- **Parser** — Tree-sitter grammar that parses any `keyword name { ... }` block generically, with error recovery (collects multiple diagnostics, never fails fast).
- **Graph** — typed entity graph over interned symbols (custom node/edge indexes), with cycle detection and subgraph queries. Reference resolution is one shared code path used by the CLI, LSP, and watch mode.
- **Extension runtime** — every extension (including the builtins) is a WIT-typed wasip2 component executed through a single wasmtime component engine, with deterministic per-extension fuel limits. No native tier: builtins and third-party extensions are the same kind of plugin. The builtin blobs are vendored under `extensions/<name>/wasm/` and embedded in the binary. A change to anything a blob compiles from is re-vendored with `cargo run -p xtask --bin revendor` ([docs/agents/revendor.md](docs/agents/revendor.md)); `cargo gate` names the blobs a change feeds, and CI's bootstrap job rebuilds every blob from a cold clone and compares it with the vendored bytes.
- **Surfaces** — CLI (`specforge-cli`), LSP (`specforge-lsp`), and MCP (`specforge-mcp`) all consume the same graph.

The implementation is a Rust workspace (edition 2024) under [`crates/`](crates/).

> Building the CLI uses the committed builtin blobs, so it needs no wasm target. Rebuilding the builtins after changing an extension needs `rustup target add wasm32-wasip2`.

## The graph is the product

The typed graph, exported as an open JSON schema (the Graph Protocol, `specforge schema`), is what agents consume. The compiler and the DSL are one way to produce it. Agents read it through:

- **Multi-resolution queries** — `specforge query <id> --depth N` exposes the graph at multiple zoom levels.
- **Agent-optimized exports** — `specforge export --format=context|brief|graph|dot`.
- **Stable, open schema** — `specforge schema` emits the Graph Protocol (JSON Schema draft 2020-12).
- **MCP server** — `specforge mcp` exposes the graph and tools to agents over JSON-RPC.
- **Plan validation** — trace tests and entities end to end (Intent → Linkage → Proof).

See the [entity model](docs/entity-model.md) for the full design, and the [vision](docs/vision/README.md) for the reasons behind it.
