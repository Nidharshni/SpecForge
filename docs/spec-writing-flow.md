# Spec Writing Flow

Top-down approach: start from what you ship, zoom into implementation details.

---

## Overview

```
Phase 0          Phase 1           Phase 2          Phase 3
FOUNDATION  ───► WHAT WE SHIP ───► WHAT USERS DO ──► WHAT THE SYSTEM DOES
spec             deliverable        journey           feature
term                                                  behavior
                                                      type

     Phase 4           Phase 5          Phase 6          Phase 7
───► COMMUNICATION ───► GUARANTEES ────► CODE ──────────► GOVERNANCE & TIMELINE
     event              invariant        module           decision
     port                                                 constraint
                                                          failure_mode
                                                          milestone
```

Cross-cutting: `ref` — attach external references at any phase.

---

## Phase 0 — Foundation

> *"What is this project? What do we call things?"*

### Entities
- **`spec`** — project identity, extensions, providers, personas, surfaces
- **`term`** — ubiquitous language definitions for the team

### Key questions
1. What is the project name and version?
2. Which extensions do we need? (`@specforge/product`, `@specforge/governance`, `@specforge/software`, `@specforge/formal`)
4. Who are the personas? (e.g., `developer`, `admin`, `end-user`)
5. What surfaces do they interact with? (e.g., `web`, `cli`, `api`, `mobile`)
6. What are the core domain terms the team must agree on?

### Checkpoint
- [ ] `spec` block compiles with name, version, extensions
- [ ] Term entities cover every domain term the team uses differently than plain English
- [ ] Personas and surfaces are declared — journeys will reference them

---

## Phase 1 — What We Ship

> *"What artifacts leave our hands and reach users?"*

### Entity
- **`deliverable`** (author-chosen identifier) — apps, services, CLIs, browser extensions, libraries

### Key questions
1. What are the distinct shippable artifacts?
2. What type is each? (`webapp`, `api`, `cli`, `library`, `mobile`, `extension`)
3. Who is each deliverable for? (→ link to personas from Phase 0)

### How to write
- One deliverable per shippable artifact
- Leave `journeys` empty for now — you'll fill them in Phase 2
- Leave `modules` empty — you'll fill them in Phase 6

### Checkpoint
- [ ] Every artifact the team ships has a `DLV` entry
- [ ] No overlap — each artifact is distinct
- [ ] Deliverable types match the actual deployment targets

---

## Phase 2 — What Users Can Do

> *"For each persona + surface, what are the end-to-end flows?"*

### Entity
- **`journey`** (`identifier`) — a user-facing flow (persona x surface -> features)

### Key questions
1. For each persona, what are the key journeys?
2. On which surface does each journey happen?
3. What is the happy-path flow? (describe steps)
4. Which deliverable bundles this journey?

### How to write
- One journey per distinct user flow
- Describe the `flow` in plain-language steps
- Leave `features` as placeholder IDs — you'll define them in Phase 3
- Back-link: update deliverables to `bundles` these journeys

### Checkpoint
- [ ] Every persona has at least one journey
- [ ] Every deliverable bundles at least one journey (avoids W043)
- [ ] Flows are concrete enough that a designer could wireframe from them

---

## Phase 3 — What the System Does

> *"What functionality supports those user flows? What are the contracts? What data shapes exist?"*

### Entities
- **`feature`** (`identifier`) — a user-facing value unit composed of behaviors
- **`behavior`** (author-chosen identifier) — atomic system contract
- **`type`** — domain data shapes (structs, unions, enums)

### Key questions — Features
1. Break each journey into discrete features
2. What problem does each feature solve?
3. What is the solution approach?

### Key questions — Behaviors
1. For each feature, what are the atomic contracts?
2. What does the contract guarantee? (use RFC 2119: MUST, SHOULD, MAY)
3. How would you verify each behavior?

### Key questions — Types
1. What data structures does the domain need?
2. What are the fields, which are required vs optional?
3. Are there discriminated unions (tagged types)?

### How to write
1. Start with features — decompose each journey into 2-5 features
2. For each feature, define its behaviors — these are the atomic units
3. As behaviors mention data shapes, define types
4. Back-link: update journeys to `traces_to` these features

### Checkpoint
- [ ] Every journey traces to at least one feature
- [ ] Every feature has at least one behavior (avoids W001 for unreferenced behaviors)
- [ ] Every behavior has a `contract` with RFC 2119 keywords
- [ ] Every behavior has a `verify` statement (avoids W004)
- [ ] Types cover all data shapes mentioned in behavior contracts

---

## Phase 4 — Communication

> *"How do components talk to each other? What events flow through the system?"*

### Entities
- **`event`** (author-chosen identifier) — domain events announced by the system
- **`port`** — interface contracts between components

### Key questions — Events
1. Which behaviors produce side effects that other parts care about?
2. What data does each event carry? (→ link to types from Phase 3)
3. Which behaviors consume each event?

### Key questions — Ports
1. What are the inbound interfaces? (APIs others call into)
2. What are the outbound interfaces? (external systems we call)
3. What methods does each port expose?
4. What category? (`persistence`, `messaging`, `http`, `auth`, etc.)

### How to write
1. Scan behaviors for produce/consume patterns → create events
2. Scan behaviors for external dependencies → create outbound ports
3. Scan behaviors for public interfaces → create inbound ports
4. On each behavior, list the types and ports it uses in `types` and `ports`

### Checkpoint
- [ ] Every event is produced by at least one behavior, in its `produces` (avoids W007)
- [ ] Every relation is written on the field that owns it (avoids E079)
- [ ] Ports cover all integration points
- [ ] No behavior references a non-existent port or type (avoids E001)

---

## Phase 5 — Guarantees

> *"What must ALWAYS be true, no matter what?"*

### Entity
- **`invariant`** (author-chosen identifier) — runtime guarantees

### Key questions
1. What must never be violated? (data integrity, security, consistency)
2. Which component enforces each invariant?
3. What is the risk level if an invariant is broken? (`low`, `medium`, `high`)

### How to write
1. Review all behaviors — which ones have implicit guarantees?
2. Extract those guarantees as explicit invariants
3. On each behavior that enforces an invariant, list it in the behavior's `invariants` (an invariant lists no behaviors; `trace <invariant>` shows them upstream)

### Checkpoint
- [ ] Every high-risk invariant is referenced by at least one behavior (avoids W003)
- [ ] Every invariant is listed in the `invariants` of at least one behavior
- [ ] High-risk invariants should get failure_modes later (Phase 7)

---

## Phase 6 — Code Organization

> *"How is the code structured? Which packages implement what?"*

### Entity
- **`module`** (`identifier`) — code packages

### Key questions
1. What packages/crates/modules does the codebase have?
2. Which features does each module implement?
3. Which ports does each module define?
4. What are the inter-module dependencies?
5. Which deliverable is each module built into?

### How to write
1. Map your actual (or planned) code packages to module entities
2. Link each module to the features it implements (`provides`)
3. Link each module to the ports it defines (`defines_port`)
4. Declare `depends_on` between modules
5. Back-link: update deliverables with `built_from` module references

### Checkpoint
- [ ] No circular module dependencies (avoids E007)
- [ ] Every feature is provided by at least one module
- [ ] Every module is referenced by at least one deliverable (avoids W009)
- [ ] Every deliverable's journeys are reachable via its modules (avoids W008)

---

## Phase 7 — Governance & Timeline

> *"Why did we make these choices? What are the quality limits? What can go wrong? When does it ship?"*

### Entities
- **`decision`** (`ADR-{n}`) — architectural decision records
- **`constraint`** (author-chosen identifier) — non-functional requirements
- **`failure_mode`** (author-chosen identifier) — FMEA risk analysis
- **`milestone`** (`identifier`) — delivery timeline phases

### Key questions — Decisions
1. What significant architectural choices were made?
2. What was the context? What alternatives were considered?
3. What are the consequences?

### Key questions — Constraints
1. What are the performance requirements?
2. What are the security requirements?
3. What are the compliance requirements?
4. What behaviors do they affect?

### Key questions — Failure Modes
1. For each high-risk invariant, what can go wrong?
2. How severe is the failure? (1-10)
3. How likely is it? (1-10)
4. How detectable is it? (1-10)
5. What is the mitigation?

### Key questions — Milestones
1. What are the delivery phases?
2. What features/deliverables ship in each phase?
3. What are the acceptance criteria per phase?

### How to write
1. ADRs: one per significant choice, link invariants they protect
2. Constraints: group by category (performance, security, etc.)
3. Failure modes: one per high-risk invariant, calculate RPN = severity × occurrence × detection
4. Milestones: phases with status (`proposed` -> `accepted` -> `delivered`)
5. Back-link: behaviors `shaped_by` decisions, constraints `constrains` behaviors

### Checkpoint
- [ ] Every significant choice has an ADR
- [ ] Every high-risk invariant has a failure_mode (avoids W005)
- [ ] RPN = severity × occurrence × detection (not checked by the compiler)
- [ ] Milestone phases cover all deliverables
- [ ] No stale proposals older than 30 days (not checked by the compiler)

---

## Cross-Cutting: External References

> *"What tickets, designs, and external docs are linked?"*

### Entity
- **`ref`** (`scheme.kind:identifier`) — external references

### When to add
- Anytime during any phase
- When a behavior maps to a GitHub issue: `ref gh.issue:42`
- When a capability maps to a Figma design: `ref figma.file:abc123`
- When a decision references an RFC: `ref gh.discussion:7`

---

## Iteration Pattern

The top-down flow is not strictly one-pass. Expect this loop:

```
 ┌──────────────────────────────────────────────────┐
 │  Phase 0 → 1 → 2 → 3 → 4 → 5 → 6 → 7          │
 │     │                                  │          │
 │     │         DISCOVERY LOOP           │          │
 │     │  ◄────────────────────────────── │          │
 │     │  (defining behaviors reveals     │          │
 │     │   new terms → update terms,      │          │
 │     │   new journeys → update          │          │
 │     │   deliverables, etc.)            │          │
 └──────────────────────────────────────────────────┘
```

**First pass**: sketch everything with placeholder references.
**Second pass**: fill in references, resolve IDs, close gaps.
**Third pass**: validate — run `specforge check` and fix all errors/warnings.

---

## Validation Summary

After completing all phases, the spec should have zero errors:

| Check | What it catches |
|-------|----------------|
| No dangling refs (E001) | Every ID reference resolves |
| No duplicate IDs (E002) | Each entity ID is unique |
| No import cycles (W113, a warning) | File imports form a DAG |
| No circular modules (E007) | Module deps form a DAG |
| Valid failure-mode ratings (E061) | Severity, occurrence and detection are recognized values |
| Resolved references (E003) | Journey personas and channels are declared |
| Relations on their owning side (E079) | A relation is written on the field that owns it, never on its other side |

And minimal warnings:

| Check | What it catches |
|-------|----------------|
| No unreferenced behaviors (W001) | Every behavior belongs to a feature |
| No unreferenced features (W002) | Every feature belongs to a journey |
| No unreferenced events (W007) | Every event has consumers |
| No unreferenced modules (W009) | Every module belongs to a deliverable |
| No unreferenced journeys (W042) | Every journey belongs to a deliverable |
| Verified behaviors (W004) | Every behavior has a verify statement |
| Mitigated risks (W005) | High-risk invariants have failure modes |
