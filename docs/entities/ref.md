# ref

> **Module:** `core`

## Purpose

A `ref` declares an **external reference** — a typed link to a resource outside the spec graph such as a GitHub issue, Jira ticket, Figma design, or Mermaid diagram. Refs are the bridge between the compiler-checked spec world and the unstructured external world.

It answers: **"What external resource is this connected to?"**

Refs do not carry behavioral contracts or guarantees. They are metadata anchors: any entity can link to refs via the `refs` field, and the compiler validates that the ref target format is well-formed and the provider scheme is recognized. Inline `[gh.issue:42]` syntax makes refs usable directly in prose fields.

## ID Pattern

```
scheme.kind:identifier
```

Examples: `gh.issue:42`, `jira.epic:PROJ-123`, `mermaid:user-flow`, `figma.frame:abc123`, `gh.pr:187`

Note: Refs use a scheme-based ID pattern rather than a plain identifier. The scheme identifies the provider (e.g., `gh` for GitHub), the kind identifies the resource type within that provider (e.g., `issue`, `pr`), and the identifier is the provider-specific locator. Some providers have only a scheme and identifier with no kind (e.g., `mermaid:user-flow`).

## Syntax

### One-Line Declaration

Most refs are declared inline in a single line:

```spec
ref gh.issue:42 "Track login timeout bug"
ref jira.epic:PROJ-123 "Q2 auth overhaul"
ref mermaid:user-flow "User registration sequence"
```

### Block Syntax (Rare)

For refs that carry provider-specific metadata:

```spec
ref gh.issue:42 "Track login timeout bug" {
  labels   ["bug", "auth"]
  priority high
}
```

Block syntax is rarely needed — most refs are one-liners. Provider-specific fields inside the block are validated by the provider, not the core compiler.

### Inline References

Refs can be referenced inline in any prose field using bracket syntax:

```spec
behavior create_user "Create User" {
  contract """
    When a valid CreateUserCommand is received,
    the system MUST create a user record.
    See [gh.issue:42] for the original requirements.
  """

  refs [gh.issue:42, jira.epic:PROJ-123]
}
```

## Fields

### Required

| Field | Type | Description |
|-------|------|-------------|
| `scheme` | identifier | The provider scheme (e.g., `gh`, `jira`, `figma`, `mermaid`). Part of the ID, not a separate field. |
| `identifier` | string | The provider-specific resource locator (e.g., `42`, `PROJ-123`). Part of the ID, not a separate field. |

### Optional

| Field | Type | Description |
|-------|------|-------------|
| `title` | string | Human-readable description (the string after the ref ID). |
| Provider-specific fields | varies | Any fields inside a block are validated by the provider, not the core compiler. |

## Relationships

### Incoming edges

None. No extension declares an edge type that targets a `ref`; an entity's `refs` field names it without creating an edge.

### Outgoing edges

None. Refs are leaf nodes — they represent external resources and do not reference other spec entities.

## Validation Rules

| Code | Rule |
|------|------|
| E002 | No two refs may share the same ID across all `.spec` files. |
| W012 | **Unreferenced ref** — declared but never referenced by any entity's `refs` field. |
| I005 | **Unknown provider scheme** — once `specforge.json` configures a provider, a ref whose scheme no configured provider registered. The ref is stored but not validated. |

## Design Guidance

### Good Refs

Refs should:
- **Link to actionable resources** — issues, tickets, designs, diagrams — not arbitrary URLs
- **Use the scheme-based ID** — `gh.issue:42` is better than a raw URL because it's compiler-aware and provider-validated
- **Be discoverable** — use inline `[gh.issue:42]` in contract text so readers can navigate to the resource

### When to Use Refs

Use refs when:
- A behavior implements a requirement from an external tracker (GitHub issue, Jira ticket)
- A feature is tied to a design in Figma or a diagram in Mermaid
- An invariant was identified during an incident tracked in PagerDuty
- A decision references an RFC or external specification

### When NOT to Use Refs

Skip refs for:
- Internal cross-references between spec entities — use entity identifiers directly (`[data_persistence]`)
- One-off URLs mentioned in prose — just include them as text
- Resources that change frequently — refs are declarations, not live links

### Ref vs. Entity Reference

| Ref | Entity Reference |
|-----|-----------------|
| Points to an external resource | Points to a spec entity |
| `[gh.issue:42]` | `[data_persistence]` |
| Validated by the provider | Validated by the core compiler |
| Scheme-based ID | Named identifier |
| Declared with `ref` block | Declared with `invariant`, `behavior`, etc. |

## Related Entities

Refs declare no edge types and no extension declares an edge type that targets one: entities reference a ref through their `refs` field, which creates no graph edge.

## Examples

### Simple Issue Reference

```spec
ref gh.issue:42 "Track login timeout bug"
```

### Jira Epic

```spec
ref jira.epic:PROJ-123 "Q2 Authentication Overhaul"
```

### Mermaid Diagram

```spec
ref mermaid:user-flow "User registration sequence diagram"
```

### Behavior with Refs

```spec
behavior create_user "Create User" {
  invariants [data_persistence, email_uniqueness]

  contract """
    When a valid CreateUserCommand is received,
    the system MUST create a user record with unique email
    and MUST return Result<User, DuplicateEmailError>.
    See [gh.issue:42] for the original requirements.
  """

  refs [gh.issue:42, jira.epic:PROJ-123]

  verify unit "insert user, retrieve by ID, assert equal"
}
```

### Block Syntax with Metadata

```spec
ref gh.issue:99 "Performance regression in search" {
  labels   ["performance", "search"]
  priority critical
  assignee "alice"
}
```

### Multiple Providers

```spec
// GitHub for issues and PRs
ref gh.issue:42 "Login timeout bug"
ref gh.pr:187 "Fix login timeout"

// Jira for epics and stories
ref jira.epic:PROJ-123 "Q2 Auth Overhaul"
ref jira.story:PROJ-456 "Implement SSO"

// Figma for designs
ref figma.frame:abc123 "Login page redesign"

// Mermaid for inline diagrams
ref mermaid:auth-flow "Authentication sequence diagram"
```
