# SpecForge

**Compile human intent into a validated, typed entity graph that AI agents can actually consume.**

Intent is trapped in prose — scattered across docs, comments, tickets, and tribal knowledge. AI agents waste most of their token budget rediscovering what should have been specified. SpecForge gives them a structured, validated, machine-readable representation of what your team means.

**The graph is the product.** Not the compiler, not the DSL — the typed graph, exported as an open JSON schema (the Graph Protocol), is what makes agents reliable. SpecForge is *not* a code generator and *not* a test framework: it provides context, agents produce output. The compiler never executes anything; test-runner extensions declare how your tests run, and `specforge collect` runs them with your consent to record which entities they prove.

## Quick Example

```spec
spec "payments" {
  version "0.1.0"
}

type user "User account" {
  status draft
  verify "rejects an empty email"
}

behavior authenticate_user "Authenticate a user with credentials" {
  status   draft
  category "auth"
  contract "Given valid credentials, returns an auth token"
  produces [user_logged_in]

  verify "rejects invalid password"
  verify "returns token on success"
}

event user_logged_in "User successfully logged in" {
  payload user
  verify "is emitted once per successful login"
}
```

```bash
specforge init --extensions @specforge/software   # scaffold a project
specforge check       # validate all .spec files and report diagnostics
specforge export      # emit the typed graph for an agent to consume
```

## Install

SpecForge is a single self-contained binary. Pick one:

```bash
# macOS / Linux — prebuilt binary, checksum-verified
curl -fsSL https://raw.githubusercontent.com/leaderiop/SpecForge/main/install.sh | sh

# Homebrew (once the tap is published)
brew install leaderiop/tap/specforge

# Prebuilt binary via cargo-binstall
cargo binstall --git https://github.com/leaderiop/SpecForge specforge-cli

# From source (stable Rust toolchain)
git clone https://github.com/leaderiop/SpecForge && cd SpecForge
cargo install --path crates/specforge-cli     # installs `specforge` into ~/.cargo/bin
```

Prebuilt archives for macOS, Linux and Windows are on the [Releases page](https://github.com/leaderiop/SpecForge/releases). Formal proofs (`@specforge/formal`) additionally need the [`z3`](https://github.com/Z3Prover/z3) solver on your `PATH`; everything else works without it.

The builtin extensions are embedded in the binary; no extra toolchain is needed to install it.

### Connect an agent (MCP)

`specforge mcp <project>` serves the graph over MCP (JSON-RPC on stdio). Point your client at an absolute project path:

```bash
claude mcp add specforge -- specforge mcp /path/to/your/project
```

```json
{
  "mcpServers": {
    "specforge": { "command": "specforge", "args": ["mcp", "/path/to/your/project"] }
  }
}
```

## Zero-Domain-Knowledge Core + Extensions

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

Start with one extension and a single spec file — that already improves agent output. Add more as your project grows. Anyone can author and publish their own extension; the open Graph Protocol is the moat, not the implementation.

## Why SpecForge? — The AI Agent Cost Problem

AI coding agents spend most of their token budget on exploration and disambiguation — reading files, guessing requirements, making wrong assumptions, and reworking failed attempts. SpecForge replaces that exploration with a structured query:

| Without SpecForge | With SpecForge |
|-------------------|----------------|
| Agent reads 20–50 files to reconstruct intent | Agent queries the spec graph in a few KB |
| Agent guesses requirements, asks questions | Contracts, types, and ports are explicit |
| 30–50% of tasks need rework | Rework drops sharply on first attempt |

The goal: lift AI agent first-attempt accuracy from ~30% (prose) toward 70–85% (graph). The most expensive token is the one spent discovering what should have been specified.

> See **[RES-18: AI Agent Token Economics](spec/research/RES-18-ai-agent-token-economics.md)** for the full analysis with industry data and citations.

## Agents Are First-Class Consumers

- **Multi-resolution queries** — `specforge query <id> --depth N` exposes the graph at multiple zoom levels.
- **Agent-optimized exports** — `specforge export --format=context|brief|graph|dot`.
- **Stable, open schema** — `specforge schema` emits the Graph Protocol (JSON Schema draft 2020-12).
- **MCP server** — `specforge mcp` exposes the graph and tools to agents over JSON-RPC.
- **Plan validation** — trace tests and entities end to end (Intent → Linkage → Proof).

## CLI Commands

```bash
# Project lifecycle
specforge init                       # scaffold a new project
specforge check                      # validate .spec files
specforge check --strict             # promote warnings to errors
specforge check --severity error     # show only errors (the exit code still counts everything)
specforge check --cache              # record statuses in specforge-cache.json (history rules)

# Graph access (for agents and humans)
specforge export --format=context    # token-efficient context for an agent
specforge query <id> --depth 2       # multi-resolution neighborhood query
specforge query <id> --format context --include-coverage   # an agent's slice with coverage
specforge trace <id>                 # traceability chain for an entity
specforge schema                     # emit the Graph Protocol schema
specforge model                      # render the logical data model
specforge outline                    # render the extension architecture
specforge stats                      # project statistics
specforge explore [<id>]             # starting points, hubs and unconnected entities
specforge review [<id>]              # coverage gaps around an entity (or the whole project)
specforge infer-guide [<kind>]       # what to look for in code to write a kind's entities

# Extensions & registry
specforge extensions                 # list enabled builtins and installed extensions
specforge add @specforge/product     # enable a builtin, or install @scope/name@version from a registry
specforge add <spec> <spec>...       # several at once: one change, all or nothing
specforge remove <name>              # disable a builtin, drop an entry, or uninstall an extension
specforge search <query>             # search registries
specforge publish                    # publish an extension to a registry
# There is no built-in registry: list yours under "registries" in specforge.json
# (see docs/registry-trust.md); without one, add/update/search/publish fail with E063.

# Tooling
specforge format                     # format .spec files
specforge collect                    # run the test runners (with your approval) and record what they prove
specforge doctor                     # health-check installed extensions
specforge mcp                        # start the MCP server (stdio)
specforge explain E001               # explain a diagnostic code
```

## Configuration

Projects are configured via **`specforge.json`** (like `tsconfig.json`):

```json
{
  "name": "payments",
  "version": "0.1.0",
  "spec_root": "spec",
  "extensions": ["@specforge/software"]
}
```

`specforge init --extensions @specforge/software` creates this for you; `specforge add` / `specforge remove` edit the `extensions` list afterwards.

## Architecture

- **Parser** — Tree-sitter grammar that parses any `keyword name { ... }` block generically, with error recovery (collects multiple diagnostics, never fails fast).
- **Graph** — typed entity graph over interned symbols (custom node/edge indexes), with cycle detection and subgraph queries. Reference resolution is one shared code path used by the CLI, LSP, and watch mode.
- **Extension runtime** — every extension (including the builtins) is a WIT-typed wasip2 component executed through a single wasmtime component engine, with deterministic per-extension fuel limits. No native tier: builtins and third-party extensions are the same kind of plugin. The builtin blobs are vendored under `extensions/<name>/wasm/` and embedded in the binary; after changing an extension or the SDK, rebuild and re-vendor them with `cargo run -p xtask --bin build-builtins -- --install` (CI's `--check` fails when vendored blobs drift from their sources). The blob is built in a staged workspace, so `--install` from any checkout gives the same bytes; `--verify` checks it.
- **Surfaces** — CLI (`specforge-cli`), LSP (`specforge-lsp`), and MCP (`specforge-mcp`) all consume the same graph.

The implementation is a Rust workspace (edition 2024) under [`crates/`](crates/).

> Building the CLI uses the committed builtin blobs, so it needs no wasm target. Rebuilding the builtins after changing an extension needs `rustup target add wasm32-wasip2`.

## Documentation

- **[Authoring Tutorial](docs/guides/authoring-spec-files.md)** — zero-to-hero guide to writing `.spec` files, with a complete [worked example](examples/todo-app/). Companions: [cookbook](docs/guides/spec-cookbook.md), [best practices](docs/guides/spec-best-practices.md), [troubleshooting](docs/guides/spec-troubleshooting.md).
- **[Vision](vision/README.md)** — the manifesto, [principles](vision/principles.md), and [north star](vision/north-star.md). The source of truth for every product decision.
- **[Documentation Hub](docs/README.md)** — entity reference tables, traceability chain, validation codes.
- **[Rust Test Tracing](docs/guides/rust-test-tracing.md)** — link Rust tests to the entities they prove with `#[specforge_test]`, then `specforge collect` and `specforge analyze coverage`.
- **[vitest Test Tracing](docs/guides/vitest-test-tracing.md)** — link vitest tests to the entities they prove through test metadata, then `specforge collect`.
- **[Formal Verification](docs/guides/formal-verification.md)** — SMT-proven specs: declare machine-checkable bounds and claims, get contradictions and counterexamples with exact locations.
- **[Entity Model](docs/entity-model.md)** — full architecture: core engine, extensions, edges, validation rules.
- **[Quick Reference](docs/quick-reference.md)** — single-page cheat sheet.
- **[Extension Protocol](docs/extension-protocol.md)** — how to author and publish your own extension.

## Business Plan

A comprehensive business plan lives in **[business/](business/README.md)**.

## Research Specs

| ID | Title |
|----|-------|
| [RES-11a](spec/research/RES-11a-spec-dsl-core-compiler.md) | Core compiler architecture |
| [RES-18](spec/research/RES-18-ai-agent-token-economics.md) | AI agent token economics — cost reduction analysis |
| [RES-19](spec/research/RES-19-market-position-success-estimation.md) | Market position & success estimation |
| [RES-25](spec/research/RES-25-formal-methods-integration.md) | Formal methods integration |
| [RES-26](spec/research/RES-26-zero-entity-core-architecture.md) | Zero-entity core architecture |
| [RES-27](spec/research/RES-27-software-eng-entity-redesign.md) | Software engineering entity redesign |

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or [MIT license](LICENSE-MIT) at your option.
