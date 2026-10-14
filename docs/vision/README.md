# The SpecForge Manifesto

## Why SpecForge Exists

AI agents are the new consumers of human intent. But intent is trapped in prose — scattered across docs, comments, tickets, and tribal knowledge. No amount of prompt engineering fixes a structural problem.

SpecForge compiles human intent into a validated, typed entity graph. **The graph is the product.** Not the compiler. Not the DSL. The graph — the structured, validated, machine-readable representation of what your team means — is what makes AI agents reliable.

---

## The Structural Gap

Every paradigm shift in computing created a structured layer between humans and machines:

- **Relational databases** got SQL — a language to describe and query data.
- **Microservices** got Protocol Buffers — a schema to describe messages.
- **Cloud infrastructure** got HCL (Terraform) — a language to describe resources.
- **REST APIs** got OpenAPI — a schema to describe endpoints.

Each of these followed the same pattern. A new category of machine capability emerged. People initially used prose to communicate with it — writing deployment runbooks, API guides, database documentation. Then someone built a structured format, and an ecosystem formed around it. The structured format won because machines cannot reliably consume prose. They need schemas, types, validated references.

```
  Paradigm                Structured Layer        Became...
 ┌─────────────────┐     ┌──────────────────┐
 │ Databases       │ ──> │ SQL              │ ──> Universal query language
 ├─────────────────┤     ├──────────────────┤
 │ Microservices   │ ──> │ Protocol Buffers │ ──> Schema standard
 ├─────────────────┤     ├──────────────────┤
 │ Infrastructure  │ ──> │ HCL (Terraform)  │ ──> IaC standard
 ├─────────────────┤     ├──────────────────┤
 │ REST APIs       │ ──> │ OpenAPI          │ ──> API contract standard
 ├─────────────────┤     ├──────────────────┤
 │ AI Agents       │ ──> │ ???              │ ──> ???
 └─────────────────┘     └──────────────────┘
                                ▲
                                │
                            SpecForge
```

The AI agent paradigm has no structured layer.

Today, agents read prose. They scan README files, architecture docs, Confluence pages, CLAUDE.md files, and code comments. Then they guess what you meant. When they guess wrong — which is often — humans correct them, and the cycle repeats. This is not a tooling problem or a model quality problem. It is a missing layer.

Consider what happens when a coding agent needs to add an authentication endpoint. Without structured context, it reads 15-30 files, consumes 40,000-80,000 tokens on discovery alone, and still misses the constraint that all auth endpoints must use the rate-limiting port. With a validated entity graph, it reads the behavior, its ports, the relevant constraints, and the linked invariants — in 9,000 tokens, with nothing missing.

This is not unique to coding:

- A **compliance agent** auditing your GDPR controls reads scattered policy documents and hopes it finds every relevant control. With a graph, every regulation links to its controls, every control links to its evidence, and gaps are visible immediately.
- A **PM agent** writing a status report scans Jira, Slack, and meeting notes. With a graph, it reads structured milestone entities with their linked deliverables and feature status — no scanning required.
- A **documentation agent** producing API docs parses source code and guesses at intent. With a graph, it reads endpoint entities, their schemas, auth requirements, and rate limits — all validated at compile time.
- A **security agent** assessing risk reads architecture docs that may be months out of date. With a graph, it reads the current port surface, constraint entities, and failure modes — and the compiler guarantees they are consistent.

Every agent that works from prose is guessing. Every agent that works from a validated graph knows.

Prose is not structure. A context file is not a compiled graph. A CLAUDE.md is not a specification.

SpecForge is the missing layer: a specification language and compiler that produces a typed entity graph — the structured context that AI agents need to do their jobs correctly.

---

## How It Works

The core loop is simple:

1. **You write `.spec` files** — a concise, structured DSL describing the entities that matter to your work. What those entities are depends on the extensions you install. Software teams describe behaviors and invariants. Compliance teams describe regulations and controls. Design teams describe atoms and organisms. Any domain, one syntax.

2. **The compiler builds a typed entity graph** — resolving references across files, validating constraints, detecting orphans and contradictions, and producing a rich graph with nodes, edges, and metadata. Between structural parsing and semantic validation, extensions may inject body parsers that interpret entity body content using extension-defined grammars. This is not linting. It is compilation. The graph has semantic meaning that the raw text does not.

3. **AI agents consume the graph** — via the Graph Protocol, a standardized JSON schema that any agent framework can read. A coding agent reads it and generates correct code. A compliance agent reads it and produces audit trails. A PM agent reads it and writes accurate status reports. Same graph, different consumers, correct results.

```
 ┌─────────────────┐    ┌──────────────────────────┐    ┌────────────────┐
 │   .spec files   │    │   SpecForge Compiler     │    │ Graph Protocol │
 │                 │    │                          │    │    (JSON)      │
 │  Any domain     │──> │  Parse (tree-sitter)     │──> │                │──> Any AI Agent
 │  vocabulary     │    │  Resolve references      │    │  Nodes + edges │
 │  from extensions│    │  Validate constraints    │    │  Validation    │
 │                 │    │  Build typed graph        │    │  Traceability  │
 └─────────────────┘    └──────────────────────────┘    └────────────────┘
```

The compiler is the bottleneck that guarantees quality. Everything that enters the graph has been validated. Everything that exits the graph is structured, typed, and consistent. Agents never receive contradictory or incomplete context — because the compiler would have caught it.

---

## Zero Domain Knowledge in Core

This is the single most important belief in the project.

SpecForge's compiler has **zero built-in entity types**. It does not know what "behavior" means. It does not know what "endpoint" means. It does not know what "regulation" means. It does not know what "atom" means in a design system, or what "hypothesis" means in a research context.

The compiler is a pure typed-graph engine: it parses blocks, resolves references, builds graphs, validates constraints, and exports results. That is all.

All domain vocabulary comes from **extensions** that you install based on your work. Shipping today: `@specforge/product`, `@specforge/governance`, `@specforge/software`, and `@specforge/formal`. The domains below are illustrative of the model:

- A software team installs `@specforge/software` and gets behavior, invariant, feature, event, type, port.
- A compliance team would install a future `@specforge/compliance` extension to get regulation, control, evidence, audit.
- A design systems team would install a future `@specforge/atomic-design` extension to get atom, molecule, organism, template, page.
- A data team would install a future `@specforge/data-pipeline` extension to get source, transform, sink, schedule.
- An API team would install a future `@specforge/api-design` extension to get endpoint, schema, operation.
- A business strategy team would install a future `@specforge/business-model` extension to get value_proposition, customer_segment, channel.

You combine extensions freely. A regulated fintech team might use `@specforge/software` + `@specforge/compliance` + `@specforge/api-design`. A product design team might use `@specforge/product` + `@specforge/atomic-design`. Each combination shapes the graph that agents consume.

This is exactly how Terraform works. Terraform's core has zero infrastructure knowledge — it knows how to parse HCL, manage state, and execute providers. All cloud concepts (EC2 instances, S3 buckets, Kubernetes pods) come from provider plugins. SpecForge's core has zero domain knowledge — it knows how to parse `.spec` files, build graphs, and run validation. All domain concepts come from extensions.

```
 ┌────────────────────────────────────────────────────────┐
 │                   SpecForge Core                       │
 │              (zero domain knowledge)                   │
 │                                                        │
 │  ┌────────────┐ ┌───────────┐ ┌────────────┐         │
 │  │ Tree-sitter│ │   Graph   │ │ Validation │         │
 │  │   Parser   │ │   Engine  │ │   Engine   │         │
 │  └────────────┘ └───────────┘ └────────────┘         │
 │  ┌────────────┐ ┌───────────┐ ┌────────────┐         │
 │  │ Reference  │ │   Export  │ │    LSP     │         │
 │  │  Resolver  │ │   Engine  │ │ Framework  │         │
 │  └────────────┘ └───────────┘ └────────────┘         │
 ├────────────────────────────────────────────────────────┤
 │           Extensions (ALL domain knowledge)            │
 │                                                        │
 │  shipping today: product, governance, software, formal │
 │                                                        │
 │  illustrative futures (not yet built):                 │
 │  @specforge/software    @specforge/compliance          │
 │  @specforge/product     @specforge/api-design          │
 │  @specforge/governance  @specforge/atomic-design       │
 │  @specforge/data-pipeline   ...any domain...           │
 └────────────────────────────────────────────────────────┘
```

This is not a convenience feature. It is a conviction. SpecForge is domain-agnostic because it must serve domains its creators never imagined.

A team at a maritime logistics company should be able to write `@specforge/shipping` and model container routes, port schedules, and customs declarations — without anyone at SpecForge having anticipated that use case. A clinical research group should be able to write `@specforge/clinical-trials` and model protocols, patient cohorts, and outcome measures. A game studio should be able to write `@specforge/game-design` and model quest systems, progression mechanics, and dialogue trees. The compiler does not need to change for any of these. Only a new extension needs to exist.

The test of this conviction is simple: if a new domain requires a change to the compiler rather than a new extension, the architecture has failed.

---

## The Graph Protocol Is the Product

The compiler is an implementation detail. The DSL is a user interface. The real product is the **Graph Protocol** — the JSON schema that AI agents consume.

This distinction matters enormously.

If someone builds a better compiler that produces the same graph, SpecForge wins. If someone writes a GUI that generates `.spec` files, SpecForge wins. If an AI agent produces `.spec` files from a conversation, SpecForge wins. If a Figma plugin exports design tokens as Graph Protocol JSON, SpecForge wins. The value is in the graph schema that agents agree to read — not in any particular tool that produces it.

This is how standards work. SQL's value was never in Oracle or PostgreSQL. OpenAPI's value was never in Swagger UI. Protocol Buffers' value was never in protoc. The value was in the shared schema that an ecosystem of tools could produce and consume independently.

The Graph Protocol is SpecForge's equivalent. It defines the structure of the entity graph: nodes with types and fields, edges with labels and directionality, validation results, coverage status, traceability links. Any tool that produces this schema is part of the ecosystem. Any agent that reads this schema benefits from the entire ecosystem.

When a coding agent, a PM agent, a compliance agent, and a documentation agent all read the same graph and produce correct results on first attempt — across different frameworks, different organizations, different domains — that is the product working. The network effect is in the schema, not the compiler.

Every new agent that learns to read the Graph Protocol makes every existing `.spec` file more valuable. Every new `.spec` file makes the Graph Protocol more worth learning. This is the flywheel: adoption of the schema drives adoption of the tools, which drives adoption of the schema.

```
  Producers                Standard                 Consumers
 ┌──────────────┐                              ┌──────────────┐
 │ specforge CLI│──┐                      ┌──│ Coding Agent │
 ├──────────────┤  │  ┌────────────────┐  │  ├──────────────┤
 │ GUI editor   │──┼─>│ Graph Protocol │─>┼──│ PM Agent     │
 ├──────────────┤  │  │    (JSON)      │  │  ├──────────────┤
 │ AI generator │──┤  └────────────────┘  ├──│ Compliance   │
 ├──────────────┤  │                      │  ├──────────────┤
 │ Figma plugin │──┘                      └──│ Security     │
 └──────────────┘                              └──────────────┘
```

---

## What SpecForge Is NOT

**Not a code generator.** SpecForge does not produce application code, configuration, documentation, or any domain output artifact. It provides structured context so that agents — and humans — can do their jobs better. The moment SpecForge generates code, it competes with every AI coding tool instead of empowering all of them. Extension renderers may emit spec-layer diagnostic artifacts — coverage reports, traceability matrices, validation dashboards — because these are part of the compiler's own feedback loop, not domain output. The distinction is clear: if an artifact helps you understand the graph, it belongs to SpecForge; if an artifact is consumed by end users or deployed to production, it belongs to an agent.

**Not a test framework.** SpecForge doesn't define how tests are written or run; cargo test, vitest and pytest do. It closes the traceability loop by consuming their results. The compiler never executes anything. A test-runner extension declares the command that runs its runner, and `specforge collect` runs that command on the extension's behalf, only after the user approves it, then hands the report to the extension's pure mapping ([ADR 0002](../adr/0002-test-runner-extensions.md)). SpecForge belongs to the specification layer.

**Not documentation.** `.spec` files are compiler-checked contracts. They have validation errors, warnings, typed references, and graph semantics. They break the build when they are wrong. Documentation drifts; specifications are validated.

**Not limited to software.** Any domain that benefits from structured knowledge and AI agents can use SpecForge. Compliance, design systems, data pipelines, API contracts, curriculum design, legal contracts, business models, scientific research, infrastructure operations — if your domain has entities with relationships and you want AI agents to understand them, SpecForge can model it.

**Not an AI agent itself.** SpecForge does not use AI to do its work. It is deterministic infrastructure — a compiler, written in Rust, with no model calls and no probabilistic behavior. It makes AI agents better the way a database makes applications better: by providing a structured, queryable, reliable layer beneath them.

---

## Who This Is For

SpecForge is for anyone whose work involves structured intent that AI agents should understand.

**Software engineers** who want AI coding agents to produce correct code on the first pass. Today, agents guess from scattered context and hallucinate missing relationships. With a spec graph, the agent knows which behaviors implement which features, which ports are required, which invariants must hold, and which tests already exist.

**Compliance officers** who need traceable links between regulations, controls, evidence, and audit findings. Today, traceability lives in spreadsheets that drift from reality. With a spec graph, a compliance agent generates audit trails automatically — and the graph validates that every control maps to a regulation and every regulation has evidence.

**API designers** who maintain contracts across multiple services. Today, API documentation is a separate artifact that falls out of sync. With a spec graph, endpoints, schemas, and operations are validated entities with typed references — and any agent can produce consistent client code, docs, or tests from the same graph.

**Design system architects** who maintain component hierarchies across teams. Today, the relationship between atoms, molecules, and organisms lives in someone's head or a Figma file. With a spec graph, a design agent understands the full hierarchy and can ensure consistency from token to template.

**Data engineers** who need to trace lineage from source to sink. Today, pipeline documentation is prose that nobody trusts. With a spec graph, lineage is a validated edge in a typed graph — and any agent can assess migration impact against it.

**Product managers** who want AI agents to produce accurate status reports. Today, agents scan Jira and guess at progress. With a spec graph, the agent reads structured roadmaps, deliverables, and feature specs — and produces reports that match reality.

**Anyone** who has watched an AI agent hallucinate because it was working from prose instead of structure.

The common thread is not the domain. It is the pattern: you have structured knowledge, AI agents need to consume it, and prose is not good enough.

---

*SpecForge compiles human intent into a validated graph. The graph is the product. Everything else follows from that.*
