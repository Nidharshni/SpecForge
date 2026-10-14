# Principles

These are the non-negotiable beliefs that govern how SpecForge is built. They sit above product strategy and technical architecture. When a decision is ambiguous, these principles resolve it.

Product decisions (what to build, for whom, in what order) flow from these principles. Technical decisions (what architecture, what trade-offs, what constraints) flow from these principles. If a product feature or technical choice contradicts a principle, the principle wins and the decision is revisited.

Each principle includes a concrete **test** — a question or measurement that tells us whether we are living up to it. Principles without tests are aspirations. These are commitments.

---

## 1. Structure is a spectrum, not a binary.

You do not need to model your entire domain before SpecForge provides value. One entity in a `.spec` file is better than zero. Ten entities are better than one. A hundred entities with full traceability is transformative.

A single behavior with two verify declarations already gives an AI agent more to work with than a page of prose. A small spec graph with twenty entities already eliminates the discovery phase of most agent tasks. Progressive adoption is not a compromise — it is the design. Start where the pain is. Expand as the value compounds.

The corollary: SpecForge must never require comprehensive coverage to be useful. If the first spec file does not improve agent output, the product has failed at its most basic level.

**The test:** A team with five spec files and fifty unstructured docs should still see measurable improvement in agent accuracy compared to having zero spec files.

## 2. The compiler knows nothing about your domain.

Zero built-in entity types. The compiler is a typed-graph engine: it parses, resolves, validates, and exports. All domain vocabulary — every keyword, every edge type, every validation rule — comes from extensions that you install.

This is not a limitation. It is the architectural decision that makes SpecForge useful to domains its creators never anticipated. A maritime logistics company can model shipping routes. A clinical research group can model trial protocols. A game studio can model quest systems. None of these require changes to the compiler.

When we are tempted to hardcode domain knowledge into core, we write an extension instead. Every time. The compiler earns its code by being domain-agnostic.

**The test:** If a line of code in the compiler references a specific domain concept — "behavior", "endpoint", "regulation" — it is in the wrong place. Core can only reference structural concepts: "entity", "edge", "reference", "field", "grammar injection", "body parser dispatch".

## 3. Agents are first-class consumers.

Every output format, every error message, every graph query is designed for machines as much as for humans. This means structured JSON with deterministic ordering. Stable schemas that do not break between versions. Multi-resolution queries that let agents request exactly the context slice they need. Token-efficient representations that fit in constrained context windows.

We measure output quality not just by human readability but by agent task-completion rates. If a human can read the output but an agent cannot parse it reliably, it is a bug. The Graph Protocol is the primary interface, not the terminal output.

**The test:** Every new output format and every schema change is evaluated against the question: "Does this improve or degrade agent task-completion rates?" Human aesthetics are secondary to machine parseability.

## 4. Validation is the value.

Without compilation, a `.spec` file is just a text file with syntax. What separates SpecForge from a fancy comment format is the compiler.

The compiler catches dangling references — an entity points to something that does not exist. It catches orphaned entities — something exists but nothing points to it. It catches missing coverage — a testable entity has no test declarations. It catches circular dependencies, duplicate identifiers, invalid field values, and contradictions between declared and actual structure.

Every error caught at compile time is a round-trip saved with an AI agent. Every warning surfaced in the editor is a mistake prevented before it reaches production. The gap between "text file with syntax" and "validated graph" is where all the value lives.

**The test:** If someone asks "why not just use markdown with conventions?", the answer is always the same: markdown does not catch dangling references, does not detect orphans, does not validate graph consistency. The compiler is the difference.

## 5. Traceability is a feedback loop, not a report.

Specs link to tests. Tests produce results. Results feed back into the graph. This is not a compliance feature. It is the mechanism that makes AI agents self-correcting.

When an agent can see that a behavior is specified, partially implemented, and has one failing test — it can fix the failing test. When an agent can see that a deliverable depends on three features, two of which are fully covered and one which has zero tests — it can focus its effort. Without traceability, the agent starts from scratch every time.

Traceability makes SpecForge compound in value over time. Each test result that feeds back into the graph makes the next agent task more accurate. This is not a compliance checkbox. It is the core value loop.

```
     ┌──────────┐       ┌──────────┐       ┌──────────┐
     │  Specs   │ ────> │  Tests   │ ────> │ Results  │
     │ (.spec)  │       │ (runner) │       │ (report) │
     └──────────┘       └──────────┘       └────┬─────┘
          ▲                                      │
          │         ┌──────────┐                 │
          └──────── │  Agent   │ <───────────────┘
                    │ (reads   │
                    │  graph)  │
                    └──────────┘
```

**The test:** A project using SpecForge for six months with continuous test feedback should produce measurably better agent results than the same project on day one — not because the specs changed, but because the traceability data accumulated.

## 6. The graph protocol is the standard, not the compiler.

Ten compilers producing the same graph schema is success. A hundred agent frameworks consuming the same graph schema is success. SpecForge being one of many tools in that ecosystem — as long as the Graph Protocol is the shared standard — is success.

We invest in the stability, openness, and documentation of the Graph Protocol above all else. The schema is versioned. Breaking changes require migration paths. The specification is public. Anyone can implement a producer or consumer.

If someone builds a better compiler, the ecosystem wins and SpecForge wins. The standard is the moat, not the implementation.

**The test:** When evaluating any feature, we ask: "Does this strengthen the Graph Protocol's position as a shared standard, or does it create lock-in to our specific compiler?" If it creates lock-in, we redesign it.

## 7. Extensions over built-ins, always.

When in doubt, make it an extension. When it feels like it should be in core, make it an extension anyway. The compiler stays small. The ecosystem grows.

A clean extension manifest format that lets the community build fifty domain vocabularies is worth infinitely more than fifty hardcoded entity types. Each hardcoded type is a decision that limits who can use SpecForge. Each extension is an invitation for a new domain to join.

Core earns its code by being domain-agnostic. If a line of code in the compiler references a specific domain concept, it is in the wrong place.

**The test:** Before adding anything to core, we ask: "Could this be an extension instead?" If the answer is yes — even if it would be slightly less convenient — it becomes an extension.

## 8. Seconds to value, not days.

Install to first validated output in under sixty seconds. No accounts. No configuration ceremony. No mandatory YAML. No onboarding wizard. No sign-up form.

The CLI works with zero configuration on any project. `specforge init` creates a starter file. `specforge check` validates it. `specforge export` produces a graph. Three commands, sixty seconds, immediate value.

Every additional feature is opt-in complexity, never mandatory overhead. Extensions are opt-in. Providers are opt-in. Cloud features are opt-in. The free, local, zero-config experience is sacred. If a new user cannot run `specforge check` within a minute of installing, something is wrong — and fixing it is the highest priority.

**The test:** Time the experience from `brew install specforge` to seeing validated output. If it exceeds sixty seconds, treat it as a P0 bug.
