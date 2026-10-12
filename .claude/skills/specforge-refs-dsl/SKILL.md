---
name: specforge-refs-dsl
description: "Write ref external reference declarations in .spec DSL files. Supports one-line (ref gh.issue:42 \"Title\") and block syntax with provider-specific metadata. Also covers inline refs in entity refs fields. Use when linking spec entities to external resources like GitHub issues, Jira tickets, Figma designs, or diagrams."
---

# SpecForge Refs DSL

Rules and conventions for authoring **`ref` declarations** in `.spec` files. Refs are typed links to external resources -- the bridge between the compiler-checked spec world and external issue trackers, design tools, and documentation.

## When to Use

- Linking spec entities to GitHub issues, PRs, or discussions
- Linking to Jira tickets, epics, or stories
- Referencing Figma designs or Mermaid diagrams
- Declaring external references that multiple entities share
- Adding provider-specific metadata to external links

## Block Syntax

### One-Line Declaration (most common)

```spec
ref gh.issue:42 "Track login timeout bug"
ref jira.epic:PROJ-123 "Q2 auth overhaul"
ref mermaid:user-flow "User registration sequence"
```

### Block Syntax (rare -- for metadata)

```spec
ref gh.issue:42 "Track login timeout bug" {
  labels   ["bug", "auth"]
  priority high
}
```

### Inline References (in entity `refs` fields)

```spec
behavior create_user "Create User" {
  contract "..."
  refs [gh.issue:42, jira.epic:PROJ-123]
}
```

### Inline References in Prose

```spec
contract """
  See [gh.issue:42] for the original requirements.
"""
```

## ID Pattern

```
scheme.kind:identifier
```

- `scheme` -- provider name (e.g., `gh`, `jira`, `figma`, `mermaid`)
- `kind` -- resource type within provider (e.g., `issue`, `pr`, `epic`, `frame`)
- `identifier` -- provider-specific locator (e.g., `42`, `PROJ-123`, `abc123`)

Some providers have only scheme and identifier (no kind): `mermaid:user-flow`.

## Fields Reference

### Required

| Field | Type | Description |
|-------|------|-------------|
| `scheme` | identifier | Provider scheme (part of the ID, not a separate field). |
| `identifier` | string | Provider-specific resource locator (part of the ID). |

### Optional

| Field | Type | Description |
|-------|------|-------------|
| `title` | string | Human-readable description (string after the ref ID). |
| Provider-specific fields | varies | Any fields inside a block are validated by the provider. |

## Relationships

### Incoming edges

| From | Edge Type | Meaning |
|------|-----------|---------|
| any entity | `links_to` | Entity references this external resource (via `refs` field) |

### Outgoing edges

None. Refs are leaf nodes -- they do not reference other spec entities.

## Writing Rules

1. **Use scheme-based IDs** -- `gh.issue:42` not raw URLs, because they are compiler-aware and provider-validated.
2. **One-line for most refs** -- block syntax is only needed for provider-specific metadata.
3. **Declare refs at the file level** -- then reference them in entity `refs` fields.
4. **Use inline `[gh.issue:42]` in prose** -- for in-context navigation.
5. **Configure providers in `specforge.json` (`providers`)** -- refs using `gh.*` require a `gh` provider configuration.
6. **Ref IDs are globally unique** -- no two refs share the same scheme.kind:identifier.

## Validation Rules

| Code | Rule |
|------|------|
| E002 | No duplicate ref IDs across all `.spec` files. |
| E011 | Invalid ref target format -- provider validates identifier pattern. |
| E012 | Unknown provider kind -- kind not registered by the provider. |
| W012 | Orphan ref -- declared but never referenced by any entity's `refs` field. |
| I005 | Unknown provider scheme -- scheme not registered by any installed provider. |

## Examples

### Simple Refs

```spec
ref gh.issue:42 "Track login timeout bug"
ref jira.epic:PROJ-123 "Q2 Authentication Overhaul"
ref mermaid:user-flow "User registration sequence diagram"
ref figma.frame:abc123 "Login page redesign"
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
```

### Behavior with Refs

```spec
behavior create_user "Create User" {
  invariants [data_persistence, email_uniqueness]

  contract """
    When a valid CreateUserCommand is received,
    the system MUST create a user record with unique email.
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

## What NOT to Do

- Do not use raw URLs instead of scheme-based IDs -- use `gh.issue:42` not `https://github.com/...`
- Do not use refs for internal cross-references between spec entities -- use entity IDs directly
- Do not declare refs without referencing them from at least one entity -- or `W012` orphan warning fires
- Do not use schemes without configuring the provider in `specforge.json` (`providers`) -- unknown schemes emit `I005`
- Do not put the kind in the identifier -- `gh.issue:42` not `gh:issue-42`
