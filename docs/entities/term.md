# term

> **Module:** `@specforge/product`

## Purpose

A `term` declares a **structured vocabulary definition** — an individual entity that establishes precise domain vocabulary. Each term defines a word or phrase, its precise meaning, aliases, and cross-references to the entities that use it.

It answers: **"What does this term mean?"**

Ambiguous terminology is the #1 source of specification bugs. When one behavior says "committed write" and another says "persisted mutation," term entities make explicit whether these are the same thing. They prevent the drift between how different team members (or different parts of the spec) use the same words.

## ID Pattern

```
identifier
```

Examples: `committed_write`, `eventual_consistency`, `idempotency_key`

## Syntax

```spec
term committed_write "Committed Write" {
  definition """
    A write operation that has been acknowledged to the caller
    AND durably persisted to the primary datastore. The write
    is guaranteed to survive process restarts.
  """
  aliases    ["persisted mutation", "durable write"]
  see_also   [data_persistence, create_user]
}

term session "Session" {
  definition """
    A materialized, mutable view of the in-memory graph
    scoped to a single compilation unit. Not an HTTP session.
  """
  context "This term is specific to the SpecForge compiler internals. See use_event_sourcing."
  see_also [use_event_sourcing]
}

term invariant_violation "Invariant Violation" {
  definition """
    A state where a runtime guarantee no longer holds.
    An invariant violation means the system is fundamentally broken,
    not merely degraded or slow.
  """
  aliases ["broken invariant"]
  see_also [data_persistence, email_uniqueness, audit_integrity]
}
```

## Fields

### Required

| Field | Type | Description |
|-------|------|-------------|
| `definition` | string or triple-string | The precise meaning of the term. Should be clear enough that two readers arrive at the same understanding. |

### Optional

| Field | Type | Description |
|-------|------|-------------|
| `title` | string | Human-readable canonical form of the term (the string after the entity ID). This is the preferred form. |
| `aliases` | string list | Alternative forms of this term that mean the same thing. Listed so the team knows they are synonyms. |
| `context` | string | Optional disambiguation or usage notes. Use when a term has a specific meaning in this project that differs from common usage. |
| `see_also` | reference list | Cross-references to entities (invariants, behaviors, decisions, etc.) that use or define this term. |

## Relationships

Term entities participate in the graph via two edge types. `TermReferencesRelatedTerm` (term→term, from the `see_also` field) is **navigation-only** — it models cross-references between terms for discovery and browsing, but carries no traceability semantics, and enables tools to build term relationship maps. `TermBelongsToModule` (term→module, from the `module` field) places a term in a module's bounded context.

| To | Edge Type | Meaning |
|----|-----------|---------|
| `module` | `TermBelongsToModule` | "This term belongs to this module" (via `module` field) |
| `term` | `TermReferencesRelatedTerm` | "This term cross-references these terms" (via `see_also` field) |

Both edges are declared by `@specforge/product`.

Terms are documentation aids. Their edges support navigation and consistency checking (e.g., unreferenced term detection via I010), not dependency tracking or coverage computation.

## Validation Rules

| Code | Level | Rule |
|------|-------|------|
| I010 | info | Term not referenced by any other term via `see_also` (unreferenced term). |

## Design Guidance

### When to Add a Term

Add a term when:
- A word has a specific meaning in your project that differs from common usage
- Two team members might interpret a term differently
- A term appears in multiple behaviors or invariants
- The term is jargon that new team members won't know

### When NOT to Add a Term

Skip terms for:
- Standard programming concepts (e.g., "function", "variable", "database")
- Terms whose meaning is obvious from context
- Implementation details that don't appear in spec entities

### Writing Good Definitions

Definitions should be:
- **Self-contained** — understandable without reading other definitions
- **Precise** — two readers arrive at the same understanding
- **Concise** — as short as possible while being unambiguous
- **Example-grounded** — include a concrete example when the concept is abstract

### Aliases

Aliases capture the different ways people say the same thing:

```spec
term behavior_def "Behavior" {
  definition "A behavioral contract specifying what the system does in a specific situation."
  aliases    ["behavioral contract", "behavior spec", "BEH"]
}
```

This prevents confusion when someone says "behavioral contract" in a meeting and someone else says "behavior spec" — the term confirms they mean the same thing.

> **Alias uniqueness:** Keep aliases unique across all terms (case-insensitive), and don't reuse another term's entity ID as an alias, so the same word never resolves to two definitions. The compiler doesn't check this.

### TermReferencesRelatedTerm Edge Scope

The `see_also` field accepts `EntityId[]` — references to any entity kind — and all references pass E001 resolution. However, only **term-to-term** references produce `TermReferencesRelatedTerm` graph edges. References to non-term entities (behaviors, invariants, decisions, etc.) are documentation-only: they validate at compile time but create no graph edge.

This is by design. Terms are navigation aids for vocabulary consistency, not dependency-tracking nodes. Cross-kind edges from terms would pollute dependency analysis, cycle detection, and graph traversal with non-structural relationships.

```spec
term committed_write "Committed Write" {
  definition "A write that has been acknowledged and durably persisted."
  see_also [data_persistence, create_user]
  //        ^^^^^^^^^^^^^^^^^  ^^^^^^^^^^^
  //        term → TermReferencesRelatedTerm edge    behavior → documentation-only (no edge)
}
```

### Context Field

Use `context` for disambiguation:

```spec
term session_def "Session" {
  definition "A materialized, mutable view of the in-memory graph."
  context    "In SpecForge, 'session' does NOT refer to HTTP sessions or user sessions."
}
```

## File Location

Terms can live in any `.spec` file, but by convention they should be in `terms.spec` at the spec root or in a `governance/terms.spec` file.

## Related Entities

Terms participate in the graph via the navigation-only `TermReferencesRelatedTerm` edge (term→term, from the `see_also` field), which carries no traceability semantics, and place themselves in a module with `TermBelongsToModule`. The `refs` field links to external references (e.g., `[gh.issue:42]`) but creates no graph edge.

## Examples

### Minimal Terms

```spec
term spec_file "Spec" {
  definition "A .spec file containing entity declarations in the SpecForge DSL."
}

term namespace_def "Namespace" {
  definition "A logical grouping scope used to organize entities within a project."
  see_also [spec_root]
}
```

### Domain-Specific Terms

```spec
term committed_write "Committed Write" {
  definition """
    A write operation that has been acknowledged to the caller
    AND durably persisted to the primary datastore. The write
    is guaranteed to survive process restarts.
  """
  aliases ["persisted mutation", "durable write", "acknowledged write"]
  see     [data_persistence, create_user]
}

term eventual_consistency "Eventual Consistency" {
  definition """
    A consistency model where reads may return stale data
    for a bounded period after a write. All replicas will
    converge to the same state within the consistency window.
  """
  context "Our read models are eventually consistent with a 5-second window. See event_sourcing_audit."
  see_also [event_sourcing_audit]
}

term idempotency_key "Idempotency Key" {
  definition """
    A client-provided unique identifier attached to a mutating request.
    If the same key is sent twice, the system MUST return the same result
    without performing the operation again.
  """
  aliases ["idempotency token", "request ID"]
  see_also [place_order, idempotent_orders]
}

term rpn "RPN" {
  definition """
    Risk Priority Number. Calculated as Severity x Occurrence x Detection
    in an FMEA analysis. Higher RPN = higher risk priority.
  """
  aliases ["risk priority number"]
  see_also [write_loss, email_race]
}

term unreferenced "Unreferenced" {
  definition """
    An entity that exists in the spec but is not referenced by any
    higher-level entity. An unreferenced behavior is not in any feature.
    An unreferenced feature is not in any journey.
  """
  context "Unreferenced entities emit compiler warnings (W001, W002). They are not errors — they may be work in progress."
}
```

### Technical Terms

```spec
term port_def "Port" {
  definition """
    An interface boundary between the domain and the outside world.
    Inbound ports define what the system offers; outbound ports
    define what the system requires.
  """
  aliases   ["interface", "boundary"]
  context   "In SpecForge, 'port' specifically means a hexagonal architecture port, not a network port."
  see_also  [hexagonal_arch]
}

term adapter "Adapter" {
  definition """
    A concrete implementation of a port. Adapters connect the domain
    to real infrastructure: databases, APIs, message brokers.
  """
  aliases   ["implementation", "driver"]
  context   "Adapters are hand-written code, not generated. Ports are generated; adapters implement them."
}

term result_type "Result Type" {
  definition """
    A type that represents either success or failure with typed errors.
    All port methods return Result types instead of throwing exceptions.
  """
  aliases   ["ResultAsync", "Result monad"]
  context   "TypeScript uses ResultAsync, Python uses Result[T, E], Go uses (T, error)."
  see_also  [result_types]
}
```
