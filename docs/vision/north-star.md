# North Star

This is not a roadmap. There are no dates here. This is a description of the world we are building toward — the destination that guides every decision along the way.

---

## The World We Are Building

Every serious project has a `spec/` directory. Teams write `.spec` files the way they write `package.json` or `Cargo.toml` — not as documentation, but as compiled infrastructure. The spec files are checked into version control, validated in CI, and consumed by every AI agent that touches the project.

AI agents of all kinds read the Graph Protocol before performing any task. A coding agent reads it before generating code. A compliance agent reads it before producing an audit trail. A PM agent reads it before writing a status report. A security agent reads it before assessing risk. A documentation agent reads it before producing API docs. The graph is the shared language between human intent and machine action.

Extensions exist for domains nobody at SpecForge imagined. A maritime logistics team has `@specforge/shipping` and models container routes, port schedules, and customs declarations. A clinical research group has `@specforge/clinical-trials` and models protocols, endpoints, and patient cohorts. A game studio has `@specforge/game-design` and models quest systems, dialogue trees, and progression mechanics. None of these were planned by anyone at SpecForge. They emerged because the compiler is domain-agnostic and the extension format is open. The ecosystem sustains itself.

When someone asks "how does this AI agent know what to do?", the answer is always the same: it reads the graph.

---

## Three Horizons

These are maturity stages, not calendar milestones. Each horizon builds on the last. Progress through them is measured by adoption patterns and ecosystem health, not dates.

```
  H1: The Compiler         H2: The Ecosystem        H3: The Standard
 ┌────────────────────┐  ┌────────────────────┐  ┌────────────────────┐
 │                    │  │                    │  │                    │
 │  Individual tool   │  │  Platform with     │  │  Open industry     │
 │  Local value       │─>│  20+ extensions      │─>│  standard          │
 │  One user, one     │  │  Org-wide value    │  │  Multiple          │
 │  project           │  │  Multiple agents   │  │  compilers         │
 │                    │  │                    │  │                    │
 └────────────────────┘  └────────────────────┘  └────────────────────┘
  Competes with:          Competes with:          Competes with:
  Inertia                 Fragmentation           Proprietary lock-in
```

### H1: The Compiler

SpecForge is an individual tool that provides local value. A developer installs it, writes a few `.spec` files, runs `specforge check`, and feeds the graph to an AI agent. The agent produces better results on the first attempt. The value is immediate, personal, and measurable: fewer corrections, less context waste, higher first-pass accuracy.

The user does not need their team to adopt SpecForge. They do not need organizational buy-in. One person, one project, one spec file — and the difference is visible. This is how all enduring developer tools begin: with individual value that spreads through demonstrated results.

At this stage, SpecForge competes with inertia — the habit of dumping prose into context files and hoping agents figure it out. The product wins by being obviously better within fifteen minutes of first use.

### H2: The Ecosystem

SpecForge is a platform with twenty or more domain extensions. Teams adopt it across their organizations. The Graph Protocol is consumed by multiple agent frameworks — not just coding assistants, but PM tools, compliance platforms, documentation generators, and infrastructure agents.

Providers bridge SpecForge to external systems: issue trackers, design tools, CI pipelines, observability platforms. The spec graph becomes the connective tissue across tools that previously operated in isolation. Engineering managers, compliance leads, and product directors see cross-cutting traceability from a single source.

Community extension authors build vocabularies for domains the core team never imagined. The extension registry grows organically. First-party extensions are the minority. The ecosystem is self-sustaining.

At this stage, SpecForge competes with fragmentation — the reality that every team invents its own context format and every agent framework invents its own schema. The platform wins by being the one graph protocol that works everywhere.

### H3: The Standard

The Graph Protocol is an open industry standard. Multiple compilers produce it. Multiple agent platforms consume it natively. SpecForge is the reference implementation — the most complete, the most trusted, the most extensible — but the standard exists independently of any single tool.

Organizations choose SpecForge the way they choose Terraform: because the ecosystem is deepest, the extensions are richest, and the tooling is most mature. But they are not locked in. The Graph Protocol is an open format. Alternative implementations exist and are welcomed.

At this stage, SpecForge competes with proprietary lock-in — the risk that a dominant AI platform dictates its own context format and creates a walled garden. The standard wins by being open, composable, and vendor-neutral. If every agent platform agrees on the graph schema, the entire industry benefits — including SpecForge.

---

## The Measure of Success

One metric captures everything:

**What percentage of AI agent tasks produce correct results on first attempt when the agent has access to a SpecForge graph?**

Today, without structured context, that number is roughly 30%. Agents guess, hallucinate, miss constraints, and require human correction on most non-trivial tasks.

With a validated spec graph, we believe that number reaches 70-85%. The agent knows the entities, their relationships, the constraints, the test status, and the traceability links. It does not need to discover anything. It reads the graph and acts.

This metric applies to all agent types, all domains, all task complexities. Coding tasks. Compliance audits. Status reports. Documentation. Security reviews. The graph improves all of them because the problem is always the same: agents need structured context, not prose.

If SpecForge reaches 100,000 projects and that number has not moved, we have failed — regardless of adoption. If SpecForge reaches 1,000 projects and that number has moved dramatically, we are on the right path. The metric is task accuracy, not vanity metrics.

---

## What We Refuse To Become

These are forward-looking rejections. As SpecForge grows, pressure will mount to become each of these things. Each pressure will come with reasonable-sounding arguments. We will resist all of them.

**A code generator.** SpecForge provides context. Agents produce output. These are fundamentally different jobs. The moment SpecForge generates code, it competes with every AI coding tool instead of empowering all of them. The entire value of the Graph Protocol depends on SpecForge being neutral infrastructure, not an opinionated output producer. Extension renderers that emit spec-layer diagnostics — coverage reports, traceability matrices, validation summaries — are not output in this sense. They serve the compiler's own feedback loop, helping teams understand their graph. They are never application code, never deployed artifacts, never domain output.

**A test framework.** SpecForge traces tests and consumes results. How tests are written and run belongs to test runners — pytest, cargo test, vitest, JUnit. SpecForge belongs to the specification layer. The compiler never executes anything; each runner has its own extension that declares the one command that runs it, which `specforge collect` runs with the user's consent ([ADR 0002](../adr/0002-test-runner-extensions.md)). Consuming the runner's report closes the traceability loop without coupling the core to any specific test framework.

**Enterprise-only.** A solo developer with no manager must choose SpecForge freely and find it valuable. Enterprise features emerge from individual adoption, not the reverse. If the free tier is not compelling on its own, the product is broken at its foundation. Enterprise value is a consequence of widespread adoption, never a prerequisite for it.

**Software-only.** The zero-entity core exists precisely because SpecForge must serve compliance teams, design teams, data teams, research teams, legal teams, and domains that do not exist yet. The compiler will never grow a hardcoded vocabulary for one industry. Every domain gets equal treatment through the extension system.

**A proprietary format.** The Graph Protocol is an open schema. Anyone can produce it. Anyone can consume it. The spec format is documented and the compiler is open source. Proprietary lock-in would destroy the network effect that makes the standard valuable. The graph schema becomes more valuable as more tools produce and consume it — and that only works if it is open.

**A walled garden.** Extensions are open. The registry is open. Community contributions are first-class citizens, not second-class add-ons. If the ecosystem cannot thrive without SpecForge's explicit permission, the architecture is wrong. The measure of a good platform is that it enables things its creators never imagined.
