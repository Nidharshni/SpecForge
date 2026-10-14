# Formal Methods Integration: Expert Analysis Summary

> [!CAUTION]
> **SUPERSEDED by RES-27** — "CORE (8 entities)" diagram outdated per zero-entity core (RES-26). Code generation integration outdated per `specforge gen` deprecation. Entity model (`contract`/`refinement`/`process`) rejected — formal methods use inline syntax on existing entities.
>
> **What changed:**
> - "CORE (8 entities)" → zero-entity core (RES-26); all entities from extensions
> - `contract`/`refinement`/`process` entities → rejected; inline `requires`/`ensures`/`maintains`/`sync` blocks on existing entities
> - `specforge gen` / codegen integration → deprecated; AI agents consume entity graph directly
> - Separate `@specforge/formal-analysis` plugin → formal methods syntax is part of `@specforge/software`

**Expert**: Expert 10 — Plugin & Extension System Designer
**Date**: March 4, 2026
**Status**: superseded
**Research Input**: Design by Contract, B-Method, CSP (195KB of research)

---

## Executive Summary

After analyzing three formal methods (Design by Contract, B-Method, CSP) and SpecForge's extension model, I recommend implementing formal analysis as a **standard plugin** using the existing plugin infrastructure. No new extension type is needed.

**Key decision**: `@specforge/formal-analysis` plugin with `contributes.validators: true`

---

## Documents Created

This analysis produced four comprehensive documents:

1. **`formal-methods-plugin-design.md`** (30KB)
   - Complete design specification
   - Host function proposals
   - Plugin architecture
   - Validation rules
   - Performance analysis
   - Implementation roadmap

2. **`formal-methods-quick-reference.md`** (15KB)
   - TL;DR for quick lookup
   - Core decisions
   - Example usage
   - Integration points

3. **`formal-methods-architecture.md`** (20KB)
   - Visual diagrams (ASCII art)
   - Extension model position
   - Host function flows
   - Query extension architecture
   - Data flow diagrams

4. **`formal-methods-implementation-spec.md`** (25KB)
   - Ready-to-implement specification
   - Manifest schema
   - Entity schemas
   - Validation rules (V001-V007)
   - Query extensions (Rhai)
   - Code generation integration
   - Implementation checklist

**Total**: 90KB of design documentation

---

## Core Recommendations

### 1. Extension Type: Plugin (NOT a New Type)

**Decision**: Use existing plugin model with `contributes.validators: true`

**Rationale**:
- Formal analysis IS validation
- Plugins already support entity registration + custom validators
- Reuses all existing infrastructure (manifests, CLI, host functions, load order)
- Conceptually simple: "a plugin that validates"

**Rejected alternative**: New "Analyzer" extension type (high implementation cost, no benefits)

### 2. Three New Entities

| Entity | Testable | Source | Purpose |
|--------|----------|--------|---------|
| `contract` | ✅ Yes | DbC | Preconditions, postconditions, invariants |
| `refinement` | ❌ No | B-Method | Abstract → concrete implementation proofs |
| `process` | ✅ Yes | CSP | Concurrent process definitions |

**Testability**: Only `contract` and `process` emit test scaffolding (refinement is structural).

### 3. Query Extensions (Key Innovation)

Instead of adding many host functions (`query_event_flow`, `check_refinement`, etc.), use **query extensions** — Rhai scripts bundled with the plugin that run host-side (Rust).

**Benefits**:
- **Performance**: Graph algorithms run at native Rust speed, not Wasm speed
- **Composability**: Multiple plugins contribute query extensions without namespace collisions
- **Type safety**: Rhai scripts validated at plugin load time
- **LSP-friendly**: Query extensions available in both CLI and LSP contexts

**New host function**: `specforge.invoke_query(name, args) -> result`

**Example query extension** (`queries/find_deadlocks.rhai`):
```rhai
fn query(graph, args) {
    let edges = graph.edges_of_type("composes_with");
    return graph.find_cycles(edges);
}
```

### 4. Seven Validation Rules

| Code | Severity | Check |
|------|----------|-------|
| V001 | Error | Contract expression is empty |
| V002 | Error | Refinement cycle detected |
| V003 | Warning | Event not in process alphabet |
| V004 | Warning | Contract not enforced |
| V005 | Error | Deadlock detected (CSP) |
| V006 | Warning | Refinement lacks proof obligation |
| V007 | Info | Formal analysis skipped (missing plugin) |

### 5. Performance Strategy

| Analysis | LSP (Real-Time) | CLI (Batch) |
|----------|----------------|-------------|
| Contract validation | ✅ < 50ms | ✅ Fast |
| Refinement DAG check | ✅ < 50ms | ✅ Fast |
| Event flow tracing | ✅ < 100ms | ✅ Fast |
| Simple deadlock | ✅ < 100ms | ✅ Fast |
| Advanced deadlock | ❌ Skip | ⚠️ 10s timeout |

**Strategy**: Fast checks on every keystroke, medium checks on save, expensive checks only in CLI.

### 6. Code Generation Integration

Generators query formal entities via graph edges and emit:
- **Runtime checks** (TypeScript: `throw ContractViolation`, Rust: `debug_assert!`)
- **Documentation** (Rust: doc comments with contract expressions)
- **Test scaffolding** (for testable entities: `contract`, `process`)

**Key insight**: No direct plugin-to-plugin communication needed — generators read the graph.

---

## Implementation Roadmap

| Phase | Duration | Deliverable |
|-------|----------|-------------|
| 1. MVP | 2 weeks | Entity registration + V001-V004 validators |
| 2. Query extensions | 2 weeks | `invoke_query` host function + Rhai queries |
| 3. Advanced validation | 3 weeks | V005 deadlock + custom rules + LSP integration |
| 4. Codegen | 2 weeks | Contract emission in TypeScript/Rust |
| 5. Proof tooling | 4 weeks | External prover integration (optional) |

**Total**: 9 weeks (core) + 4 weeks (proof tooling if needed)

---

## Key Insights from Research

### Design by Contract (Bertrand Meyer, 1988)

- Contracts as first-class entities (not comments)
- Three types: preconditions, postconditions, invariants
- Inheritance: subclass contracts must be compatible (Liskov substitution)
- Runtime vs static: SpecForge uses contracts for **documentation + validation hints** (not mechanically verified)

**SpecForge application**:
- `contract` entity with `type: precondition | postcondition | invariant`
- `enforced_by` field links contracts to behaviors
- AI agents use contract expressions as structured context for implementation

### B-Method (Jean-Raymond Abrial, 1996)

- Stepwise refinement: abstract machines → concrete implementations
- Proof obligations: each refinement step generates obligations
- Industrial success: Paris Métro Line 14 (driverless train) verified with B-Method

**SpecForge application**:
- `refinement` entity with `abstract` + `concrete` behavior references
- `proof_obligation` field (free-form text, not mechanically checked)
- V002 validator ensures refinements form a DAG (no cycles)

### CSP (Tony Hoare, 1978)

- Process algebra for concurrent systems
- Processes communicate via synchronous message passing
- Deadlock detection: CSP allows reasoning about liveness properties
- Trace semantics: processes defined by observable behavior

**SpecForge application**:
- `process` entity with `alphabet` (valid events) + `definition` (CSP expression)
- `traces` field links processes to implementing behaviors
- V005 validator detects deadlocks in process composition

---

## Why This Design Works

1. **Composable**: Users opt in by installing the plugin (no mandatory formal verification)
2. **Performant**: Query extensions run in Rust, not Wasm (low overhead)
3. **Consistent**: Reuses existing plugin infrastructure (no new concepts)
4. **Extensible**: Custom rules via Rhai scripts (domain-specific analyses)
5. **Practical**: Integrates with code generation and test traceability
6. **Progressive**: Start with documentation, add mechanical verification later

---

## Architecture Highlights

### Extension Model Position

```
┌─────────────────────────────────────────────────────────────────┐
│                    SPECFORGE CORE (8 entities)                   │
│  spec · invariant · behavior · feature · event · type · port · ref│
├──────────────────────┬──────────────────┬───────────────────────┤
│   PLUGINS (entities) │ PROVIDERS (refs) │ GENERATORS (output)   │
│                      │                  │                       │
│  @specforge/product  │  @specforge/gh   │  @specforge/gen-ts    │
│  @specforge/         │  @specforge/jira │  @specforge/gen-rust  │
│    governance        │                  │                       │
│                      │                  │                       │
│  @specforge/         │                  │  ◄────────────────────┤
│    formal-analysis   │                  │  Queries contracts    │
│    (NEW PLUGIN)      │                  │  Emits runtime checks │
└──────────────────────┴──────────────────┴───────────────────────┘
```

**Position**: `@specforge/formal-analysis` is a plugin (left column), not a new extension type.

### Query Extension Flow

```
Wasm Plugin (Rust)
  │ invoke_query("find_deadlocks", {})
  ▼
Extism Host (Rust)
  │ 1. Lookup "find_deadlocks" in query_registry
  │ 2. Load queries/find_deadlocks.rhai
  │ 3. Create Rhai engine with graph helpers
  │ 4. Execute Rhai script (runs in Rust, not Wasm)
  │ 5. Return result as JSON
  ▼
Wasm Plugin (Rust)
  │ Parse result, emit diagnostics if cycles found
  ▼
Host collects diagnostics
```

**Performance win**: Graph traversal (BFS, DFS, cycle detection) runs at native Rust speed.

---

## Example Usage

### Install Plugin

```bash
specforge add @specforge/formal-analysis
```

### Declare Entities

```spec
contract auth_login_precond {
  title "Login requires credentials"
  type precondition
  expression "username != null && password != null"
  enforced_by [auth_login]

  verify unit "rejects null credentials" {
    path "tests/auth_test.rs::auth_login_precond__rejects_null"
  }
}

refinement payment_flow_refinement {
  abstract payment_process_abstract
  concrete payment_process_impl
  proof_obligation "preserve_balance_invariant"
}

process auth_service {
  alphabet ["login", "logout", "refresh_token"]
  definition "login -> (logout -> STOP | refresh_token -> auth_service)"
  traces [auth_login, auth_logout, token_refresh]
}
```

### Custom Validation Rules

```rhai
// .specforge/queries/custom_payment_rule.rhai

fn validate(graph) {
    let payment_behaviors = graph.filter_nodes(#{
        kind: "behavior",
        name_contains: "payment"
    });

    for behavior in payment_behaviors {
        let contracts = graph.incoming_edges(behavior.id, "guards");
        if !contracts.any(|c| c.field("type") == "invariant") {
            emit_diagnostic("warning", "CUSTOM001",
                `Payment behavior '${behavior.id}' missing invariant`,
                behavior.span
            );
        }
    }
}
```

### Generated Code

**Rust**:
```rust
impl AuthService {
    /// # Precondition
    /// `username != null && password != null`
    pub fn auth_login(&self, username: &str, password: &str) -> Result<Token> {
        debug_assert!(!username.is_empty() && !password.is_empty(),
            "Contract violated: auth_login_precond");
        // ... implementation
    }
}
```

**TypeScript**:
```typescript
export function auth_login(username: string, password: string): Token {
  // @specforge-contract: auth_login_precond
  if (username == null || password == null) {
    throw new ContractViolation("username != null && password != null");
  }
  // ... implementation
}
```

---

## Open Questions (Answered)

### Q1: Should formal analysis be a 4th extension type?

**Answer**: No. Use existing plugin model.

**Rationale**: Formal analysis is validation. Plugins already support validation. No new type needed.

### Q2: What new host functions are needed?

**Answer**: Only one: `specforge.invoke_query(name, args) -> result`

**Rationale**: Query extensions (Rhai scripts) provide domain-specific graph queries without polluting the host function namespace.

### Q3: Should refinement be testable?

**Answer**: No (start with `testable: false`).

**Rationale**: Refinement is a structural property verified by the plugin. If users request equivalence tests later, add `testable: true` in v0.2.0.

### Q4: Should we parse contract expressions?

**Answer**: No (start with free-form strings).

**Rationale**: Lower barrier to entry. Users can write contracts in natural language. Add optional expression DSL in v0.6.0 if mechanical verification is added.

### Q5: Should we support multiple formal plugins?

**Answer**: Yes (via load-order priority).

**Rationale**: Users might want both `@specforge/formal-analysis` and `@community/tla-plus`. If entity names collide, use manifest priority + emit `W023`.

---

## Success Criteria

### MVP (Phase 1)

- [ ] Plugin installs via `specforge add @specforge/formal-analysis`
- [ ] Contract, refinement, process entities parse correctly
- [ ] V001-V004 validators emit diagnostics
- [ ] Example `.spec` files compile without errors
- [ ] Documentation published

### Phase 2 (Query Extensions)

- [ ] `invoke_query` host function works
- [ ] Rhai query extensions execute
- [ ] V005 deadlock detection works
- [ ] Query performance < 100ms for medium graphs (100 nodes)

### Phase 3 (Advanced Validation)

- [ ] Custom rules load from `.specforge/queries/`
- [ ] LSP integration: fast checks on keystroke, slow checks skipped
- [ ] Fuel limits prevent runaway analyses

### Phase 4 (Codegen)

- [ ] TypeScript generator emits runtime checks
- [ ] Rust generator emits debug assertions
- [ ] Test scaffolding generated for contracts/processes
- [ ] Examples work end-to-end

---

## Related Files

### Research Input (195KB)

- `/Users/u1070457/Projects/Perso/specforge/research-design-by-contract.md` (60.5KB)
- `/Users/u1070457/Projects/Perso/specforge/research-b-method.md` (75.7KB)
- `/Users/u1070457/Projects/Perso/specforge/research-csp.md` (67.9KB)

### Design Output (90KB)

- `/Users/u1070457/Projects/Perso/specforge/formal-methods-plugin-design.md` (30KB)
- `/Users/u1070457/Projects/Perso/specforge/formal-methods-quick-reference.md` (15KB)
- `/Users/u1070457/Projects/Perso/specforge/formal-methods-architecture.md` (20KB)
- `/Users/u1070457/Projects/Perso/specforge/formal-methods-implementation-spec.md` (25KB)

### Existing Documentation

- `/Users/u1070457/Projects/Perso/specforge/docs/extension-model.md`
- `/Users/u1070457/Projects/Perso/specforge/crates/runtime/wasm/src/host_functions.rs`
- `/Users/u1070457/Projects/Perso/specforge/crates/pipeline/graph/src/spec_graph.rs`

---

## Next Steps

1. **Review** these four documents with the SpecForge team
2. **Decide** on query extensions vs specialized host functions (recommend query extensions)
3. **Prioritize** phases (MVP → query extensions → advanced validation → codegen)
4. **Allocate** 9 weeks for core implementation (13 weeks if proof tooling is needed)
5. **Implement** Phase 1 (MVP) — entity registration + basic validators

---

## Key Takeaway

Formal methods integration should be a **plugin**, not a new extension type. This design:
- Reuses existing infrastructure (low implementation cost)
- Maintains conceptual clarity (plugins validate)
- Provides excellent performance (query extensions in Rust)
- Enables progressive enhancement (start with documentation, add mechanical verification later)

The plugin model is the right choice.

---

**End of Summary**
