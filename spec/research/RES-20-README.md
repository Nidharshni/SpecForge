# RES-20: Type System Evolution — Research Index

> [!NOTE]
> **ID collision:** RES-20 covers two unrelated research efforts — this type system evolution AND the [LSP Implementation Review](./RES-20-lsp-implementation-review.md). The LSP review should be cited as RES-20-LSP.

> **Note (RES-26):** Phantom type markers in Layer 0 assume static entity kinds. Under zero-entity core (Principle 2), entity kinds are extension-defined and dynamic via `KindRegistry` + `InternedId`. The phantom type patterns **must be redesigned** for `InternedId`-based kind registration before implementation — see RES-26 for the migration path.

**Research Date:** March 4, 2026
**Expert:** Type System Architect (Expert 6)
**Status:** active
**Total Lines:** 3,700 lines of analysis + 170 KB of supporting research

---

## Overview

This research proposes a comprehensive evolution of SpecForge's type system, adding five layers of type-level reasoning inspired by three formal methods traditions: Design by Contract (Eiffel, Dafny), B-Method (formal refinement), and CSP (process algebra).

**Key Insight:** SpecForge can become the **first specification language optimized for AI agent consumption** that combines refinement types, typed behavioral contracts, and event payload typing.

---

## Document Structure

### Core Research Documents (RES-20)

1. **[RES-20-executive-summary.md](RES-20-executive-summary.md)** (279 lines, 10 KB)
   - **Start here** — Quick overview of problem, solution, roadmap
   - Suitable for stakeholder review and decision-making
   - 5-minute read

2. **[RES-20-type-system-evolution.md](RES-20-type-system-evolution.md)** (1,836 lines, 58 KB)
   - **Main research document** — Comprehensive analysis
   - Five type system layers with detailed examples
   - Type checking algorithms and error catalog
   - Implementation roadmap (v2.0 → v3.0)
   - 30-45 minute read

3. **[RES-20-type-system-architecture.md](RES-20-type-system-architecture.md)** (340 lines, 21 KB)
   - **Architecture diagrams** — Visual overview of system layers
   - Type checking pipeline flowcharts
   - Performance budgets and adoption strategy
   - Comparison matrices with related systems
   - 15-minute read

4. **[RES-20-syntax-reference.md](RES-20-syntax-reference.md)** (1,003 lines, 27 KB)
   - **DSL syntax guide** — Concrete examples for each feature
   - Grammar extensions (EBNF)
   - Complete error message catalog
   - Migration checklist
   - Reference document (consult as needed)

### Supporting Research (Background)

5. **[RES-25-sources/research-design-by-contract.md](RES-25-sources/research-design-by-contract.md)** (60 KB)
   - Comprehensive study of Design by Contract (DbC)
   - Eiffel, Dafny, F*, SPARK history and implementations
   - Precondition/postcondition semantics
   - Liskov Substitution Principle analysis

6. **[RES-25-sources/research-b-method.md](RES-25-sources/research-b-method.md)** (76 KB)
   - B-Method and Event-B formal refinement
   - Set-theoretic types and invariants
   - Substitution rules and proof obligations
   - Industrial applications (Paris Métro Line 14)

7. **[RES-25-sources/research-csp.md](RES-25-sources/research-csp.md)** (68 KB)
   - Communicating Sequential Processes (CSP)
   - Channel typing and process algebra
   - FDR model checker and refinement verification
   - Influence on Go, Erlang, Rust async

**Total Supporting Research:** 204 KB, ~6,000 lines

---

## Reading Paths

### For Decision Makers (10 minutes)

1. Read: **RES-20-executive-summary.md**
2. Skim: **RES-20-type-system-architecture.md** (diagrams only)

**Outcome:** Understand the proposal, benefits, and roadmap.

### For Architects (1 hour)

1. Read: **RES-20-executive-summary.md**
2. Read: **RES-20-type-system-evolution.md** (focus: sections 1-7, 9-12)
3. Skim: **RES-20-syntax-reference.md** (examples)

**Outcome:** Understand technical design, trade-offs, and implementation approach.

### For Implementers (2-3 hours)

1. Read: **RES-20-executive-summary.md**
2. Read: **RES-20-type-system-evolution.md** (full)
3. Read: **RES-20-syntax-reference.md** (full)
4. Read: **RES-20-type-system-architecture.md** (type checker details)

**Outcome:** Ready to implement type checker phases.

### For Researchers (Deep Dive)

1. Read all RES-20 documents (above)
2. Read: **research-design-by-contract.md**
3. Read: **research-b-method.md**
4. Read: **research-csp.md**

**Outcome:** Full context on formal methods foundations.

---

## Key Proposals

### 1. Refinement Types on Entity Fields

**Problem:** Can't express value constraints like "amount must be positive"

**Solution:**

```spec
type Money {
  amount   number { > 0 }
  currency string { in ["USD", "EUR", "GBP"] }
}
```

**Impact:** Compile-time validation of value constraints.

### 2. Channel Typing for Events

**Problem:** Event consumers might expect fields not provided by producers

**Solution:**

```spec
event user_created {
  payload { userId string, email string }
  consumers [send_notification]
}

behavior send_notification {
  consumes user_created { userId, email }  // Validated at compile time
}
```

**Impact:** Catch payload mismatches before runtime.

### 3. Contract Type Compatibility

**Problem:** Can't verify if behavior A can substitute behavior B

**Solution:**

```spec
behavior base_operation {
  contract {
    requires input != ""
    ensures result != ""
  }
}

behavior enhanced_operation extends base_operation {
  contract {
    requires input != ""            // SAME (OK)
    ensures result != ""
    ensures result.length > 10      // STRONGER (OK)
  }
}
```

**Impact:** Enforce Liskov Substitution Principle at compile time.

### 4. Port Interface Subtyping

**Problem:** Can't validate if port A can replace port B

**Solution:**

```spec
port BaseRepository {
  method findById(id: string) -> Result<Entity, NotFoundError>
}

port UserRepository extends BaseRepository {
  method findById(id: string) -> Result<User, NotFoundError>  // Covariant return (OK)
}
```

**Impact:** Apply variance rules to port methods.

### 5. Phantom Types for Entity IDs

**Problem:** Compiler allows passing BehaviorId where TypeId is expected

**Solution (internal Rust implementation):**

```rust
pub type BehaviorId = EntityId<BehaviorMarker>;
pub type TypeDefId = EntityId<TypeDefMarker>;

// This won't compile:
graph.add_edge(type_id, behavior_id, EdgeType::Implements);
// ERROR: TypeDefMarker doesn't implement ValidEdgeSource<BehaviorMarker, Implements>
```

**Impact:** Catch entity kind mismatches at Rust compile time.

---

## Implementation Roadmap

| Version | Milestone | Features | Timeline |
|---------|-----------|----------|----------|
| **v2.0** | Foundation | Parse refinements/contracts (no validation) | 3 months |
| **v2.1** | Phantom Types | EntityId\<K\>, compile-time safety | +1 month |
| **v2.2** | Refinement Checker | Validate refinements + contracts | +2 months |
| **v2.3** | Event Typing | Payload compatibility checking | +1 month |
| **v2.4** | Contract Compat | Liskov Substitution validation | +2 months |
| **v2.5** | Port Subtyping | Variance rules for ports | +1 month |
| **v2.6** | Set Constraints | B-Method style predicates | +2 months |
| **v3.0** | SMT Verification | Z3 integration (optional) | +6 months |

**Total:** ~18 months from v2.0 to v3.0 (with v2.6 at 12 months)

> **Vision alignment (Principle 8 — Seconds to value):** This roadmap is entirely additive. v1.0 functionality is unaffected — `init`, `check`, and `export` continue to work in seconds throughout. All type system features are opt-in; existing users who never write a refinement annotation or contract get the same fast experience. The 18-month timeline applies only to progressive type-checking capabilities, not to the core compile-and-export loop.

---

## Error Codes Introduced

> **Note:** These codes are reserved for the type system evolution roadmap (v2.2–v3.0)
> and do not conflict with current core or extension allocations. They use the E041+
> future range per the glossary allocation table (glossary.spec).

| Code | Severity | Name | Phase |
|------|----------|------|-------|
| E041 | Error | Event payload field missing | v2.3 |
| E042 | Error | Refinement type violation | v2.2 |
| E043 | Error | Precondition strengthened | v2.4 |
| E044 | Error | Postcondition weakened | v2.4 |
| E045 | Error | Parameter not contravariant | v2.5 |
| E046 | Error | Return not covariant | v2.5 |
| E047 | Error | Set membership violation | v2.6 |
| E048 | Error | Subset violation | v2.6 |
| E049 | Error | SMT verification failed | v3.0 |
| W058 | Warning | Possible refinement violation | v2.2 |
| W059 | Warning | Unknown refinement operator | v2.2 |
| W060 | Warning | SMT verification timeout | v3.0 |
| I018 | Info | Consumer payload projection | v2.3 |
| I019 | Info | Set constraint optimization hint | v2.6 |

---

## Benefits Summary

### For Developers

- Earlier error detection (compile time vs. runtime)
- Better IDE support (LSP refinement warnings)
- Self-documenting types (machine-readable specs)
- Refactoring confidence (type checker validates)

### For AI Agents

- Structured contracts (LLMs reason about require/ensure)
- Explicit constraints (refinements guide AI agents)
- Typed traceability (follow edges through graph)
- Test oracles (refinements become assertions)

### For Teams

- Design clarity (refinements force precision)
- Contract enforcement (compiler validates)
- Safe evolution (structural typing)
- Cross-service verification (event typing)

---

## Comparison to Existing Systems

**SpecForge v2 is unique:**

| Feature | Liquid Haskell | Dafny | Eiffel | TLA+ | CSP | SpecForge |
|---------|---------------|-------|--------|------|-----|-----------|
| Refinement types | ✅ | ✅ | ❌ | ✅ | ❌ | ✅ |
| Behavioral contracts | ❌ | ✅ | ✅ | ✅ | ❌ | ✅ |
| Channel typing | ❌ | ❌ | ❌ | ❌ | ✅ | ✅ |
| Port subtyping | ✅ | ✅ | ✅ | ❌ | ❌ | ✅ |
| **AI-optimized** | ❌ | ❌ | ❌ | ❌ | ❌ | **✅** |

**First specification language combining all four mechanisms for AI agent consumption.**

---

## Open Questions for Stakeholders

1. **Syntax Preferences:**
   - `requires`/`ensures` vs. `pre`/`post` for contracts?
   - `{ > 0 }` vs. `where amount > 0` for refinements?

2. **SMT Integration:**
   - Always Z3? Or pluggable (CVC5, etc.)?
   - Default timeout: 5s per entity?

3. **Performance:**
   - 100ms type-check budget realistic for 10K entities?
   - Should type checking be skippable in CI?

4. **Error Messages:**
   - Show SMT solver output? Or abstract it?
   - LSP quick-fix suggestions?

5. **Adoption:**
   - Opt-in sufficient? Or per-file feature flags?
   - Should v1 specs emit "consider upgrading" hints?

---

## Next Steps

1. **Review** — Stakeholder feedback on RES-20 documents
2. **ADR** — Create `ADR-TypeSystemEvolution` with final decisions
3. **Prototype** — Implement v2.0 syntax parsing in feature branch
4. **Validate** — Test DSL ergonomics with real specs
5. **Iterate** — Refine based on feedback
6. **Ship** — Incremental releases (v2.0 → v2.6 → v3.0)

---

## File Manifest

```
spec/research/
├── RES-20-README.md                      (this file)
├── RES-20-executive-summary.md           (279 lines, 10 KB)
├── RES-20-type-system-evolution.md       (1,836 lines, 58 KB)
├── RES-20-type-system-architecture.md    (340 lines, 21 KB)
└── RES-20-syntax-reference.md            (1,003 lines, 27 KB)

Supporting research (root):
├── research-design-by-contract.md        (60 KB)
├── research-b-method.md                  (76 KB)
└── research-csp.md                       (68 KB)
```

**Total Research Output:** 3,700 lines (120 KB) + 204 KB supporting research = 324 KB

---

## Citation

When referencing this research in ADRs, issues, or documentation:

```markdown
See RES-20: Type System Evolution (March 2026)
- Executive Summary: spec/research/RES-20-executive-summary.md
- Full Analysis: spec/research/RES-20-type-system-evolution.md
```

---

## Acknowledgments

This research synthesizes insights from:

- **Bertrand Meyer** — Design by Contract (Eiffel)
- **Jean-Raymond Abrial** — B-Method and Event-B
- **Tony Hoare** — Communicating Sequential Processes (CSP)
- **Ranjit Jhala** — Liquid Types and SMT-based refinements
- **Rustan Leino** — Dafny and program verification

---

**Status:** active
**Contact:** Type System Architect (Expert 6)
**Date:** March 4, 2026
