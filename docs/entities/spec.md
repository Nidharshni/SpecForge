# spec

> **Module:** `core`

## Purpose

The `spec` block is the **project root configuration** — a singleton that declares the project identity, format version, and optional configuration for test coverage and code generation.

Every SpecForge project has exactly one `spec` block, written in any source file (by convention `specforge.spec` under the spec root); it is a graph entity: the project's configuration is `specforge.json`. It answers: **"What project is this?"**

## ID Pattern

Singleton — no ID. There is exactly one `spec` block per project.

## Syntax

```spec
spec "my-service" {
  version "1.0"

  extensions [
    "@specforge/product",
    "@specforge/governance",
  ]

  providers {
    gh "work" {
      extension "@specforge/gh"
      repo    "myorg/my-service"
    }
  }

  gen typescript {
    out       "src/generated/"
    result    "hex-di"
    readonly  true
    naming    "camelCase"
    tests     "@specforge/vitest"
  }
}
```

## Fields

### Required

| Field | Type | Description |
|-------|------|-------------|
| `name` | string | Project name (the string after `spec`). Used in generated output headers. |
| `version` | string | Format version of the `.spec` files. The compiler checks compatibility on startup. |

### Optional

| Field | Type | Description |
|-------|------|-------------|
| `namespace` | string | Optional namespace for cross-project references (e.g., `"@auth-service"`). |
| `display_prefix` | string | Optional prefix for human-readable reports (e.g., `"MS"`). Does not affect entity IDs. |
| `extensions` | string list | Installed extension packages (e.g., `"@specforge/product"`, `"@specforge/governance"`). |
| `providers` | block | *(planned — not yet implemented)* Provider configurations for external platform integrations. See [extension-model.md](../extension-model.md). |
| `persona` | sub-block(s) | Persona definitions. Validates that every `persona` in a `journey` block matches a defined persona. |
| `surface` | sub-block(s) | Surface definitions. Validates that every `surface` in a `journey` block matches a defined surface. |
| `gen` | block(s) | *(planned — not yet implemented)* Code generation configuration per target language. |

There is no coverage configuration: `specforge analyze coverage` reads the
project's `specforge-report.json` (or `--test-results <path>`), and
`--min N` is the proof gate. Nor is there a list of test directories: each
collector extension (`@specforge/cargo-test`, `@specforge/vitest`) knows
where its runner's tests are.

### Persona Sub-Block

Persona definitions validate that every `persona` referenced in a `journey` block matches a defined persona.

| Field | Type | Description |
|-------|------|-------------|
| `name` | identifier | Persona identifier (the keyword after `persona`). Used in `journey` blocks. |
| `title` | string | Human-readable persona name (the string after the identifier). |
| `description` | string | Optional description of the persona's role and responsibilities. |

### Surface Sub-Block

Surface definitions validate that every `surface` referenced in a `journey` block matches a defined surface.

| Field | Type | Description |
|-------|------|-------------|
| `name` | identifier | Surface identifier (the keyword after `surface`). Used in `journey` blocks. |
| `title` | string | Human-readable surface name (the string after the identifier). |
| `type` | string | Optional surface type (e.g., `webapp`, `terminal`, `mobile`, `api`). |

### Extensions Field

The `extensions` field is a string list of installed extension packages:

```spec
extensions [
  "@specforge/product",
  "@specforge/governance",
]
```

Extensions extend the entity model with new block types, edge types, and validation rules. See [extension-model.md](../extension-model.md) for details.

### Providers Sub-Block (planned — not yet implemented)

The `providers` block configures external platform integrations for `ref` entity validation and URL resolution:

```spec
providers {
  gh "work" {
    extension "@specforge/gh"
    repo    "myorg/my-service"
  }

  jira "project" {
    extension "@specforge/jira"
    project "PROJ"
    server  "https://myorg.atlassian.net"
  }
}
```

Each provider entry has:

| Field | Type | Description |
|-------|------|-------------|
| `alias` | identifier | Instance name (the string after the scheme). Supports multiple instances of the same provider. |
| `extension` | string | The provider extension package to use (e.g., `"@specforge/gh"`). |
| Provider-specific fields | varies | Configuration fields defined by the provider (e.g., `repo`, `project`, `server`). |

See [extension-model.md](../extension-model.md) for the full provider model.

### Gen Sub-Block (per language, planned — not yet implemented)

| Field | Type | Description |
|-------|------|-------------|
| `out` | string | Output directory for generated files. |
| `result` | string | Result type library to use (e.g., `"hex-di"`, `"neverthrow"`, `"result"`). |
| `readonly` | boolean | Generate `readonly` fields in output types. |
| `naming` | string | Naming convention: `"camelCase"`, `"snake_case"`, `"PascalCase"`. |
| `tests` | string | Test plugin package name for generating test stubs. |
| `module` | string | Module path prefix (Go-specific). |
| `frozen` | boolean | Generate frozen/immutable types (Python-specific). |

## Relationships

The `spec` block does not participate in the traceability chain. It is configuration, not a traced entity.

It provides project-level configuration including optional `namespace` for cross-project references and `display_prefix` for report formatting.

## Validation Rules

| Code | Rule |
|------|------|
| E002 | Exactly one `spec` block must exist across all `.spec` files in the project. |
| — | The `version` must be a valid semver-compatible version string. |
| I003 | If the project `version` is older than the compiler version, emit info diagnostic. |

## File Location

The spec root is `spec_root` in `specforge.json` (else the project root); a `specforge.spec` file marks nothing.

## Related Entities

The `spec` block does not participate in the traceability graph. It is root configuration. No entities reference it, and it references no entities.

## Examples

### Minimal

```spec
spec "my-api" {
  version "1.0"
}
```

### With Persona and Surface Definitions

```spec
spec "my-service" {
  version "1.0"

  persona admin       "System Administrator"
  persona developer   "API Integrator"
  persona viewer      "Read-Only User"

  surface web  "Web Dashboard"
  surface cli  "Command Line"
  surface api  "REST API"
}
```

### With Providers (planned — not yet implemented)

```spec
spec "my-service" {
  version "1.0"

  extensions [
    "@specforge/product",
  ]

  providers {
    gh "main" {
      extension "@specforge/gh"
      repo    "myorg/my-service"
    }

    gh "shared" {
      extension "@specforge/gh"
      repo    "myorg/shared-libs"
    }

    jira "backend" {
      extension "@specforge/jira"
      project "BACK"
      server  "https://myorg.atlassian.net"
    }
  }
}
```

### Full Configuration

```spec
spec "healthcare-platform" {
  version "1.0"

  namespace      "@healthcare"
  display_prefix "HP"

  extensions [
    "@specforge/product",
    "@specforge/governance",
  ]

  providers {
    gh "platform" {
      extension "@specforge/gh"
      repo    "healthorg/platform"
    }

    jira "clinical" {
      extension "@specforge/jira"
      project "CLIN"
      server  "https://healthorg.atlassian.net"
    }
  }

  persona clinician  "Healthcare Provider"
  persona patient    "Patient User"
  persona admin      "System Administrator"

  surface web     "Web Portal"
  surface mobile  "Mobile App"
  surface api     "HL7 FHIR API"

  gen typescript {
    out       "packages/shared/src/generated/"
    result    "hex-di"
    readonly  true
    naming    "camelCase"
    tests     "@specforge/vitest"
  }

  gen python {
    out       "services/ml/src/generated/"
    result    "result"
    frozen    true
    naming    "snake_case"
    tests     "@specforge/pytest"
  }
}
```
