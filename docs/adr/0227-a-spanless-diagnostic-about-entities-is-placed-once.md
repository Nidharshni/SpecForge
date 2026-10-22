# A spanless diagnostic about entities is placed once

**Status:** accepted (2026-10-23, architecture round 25, plan 07). Reopens the last sentence of [ADR 0016](0016-navigation-is-one-module.md) D7: the compiled project locates a spanless diagnostic about entities at the first one's block, and the LSP relates the others.

## Context

- W061 (a reference cycle) and W148 (a custom rule's function failing on entities) have no span of their own; their data names the entities they are about.
- The LSP alone placed them, at the first entity's name. CLI `check`, MCP `validate`, `specforge://diagnostics`, `watch --format json` and `inspect` showed them with no file, line or column.
- A pass diagnostic naming an entity was located for every surface by a second rule in the pass runtime, at the entity's block, and every rule diagnostic about an entity (W045, I048, ...) already sits at its block. The LSP's name was the outlier.

```text
BEFORE  LSP place_at_subjects (name)   pass runtime or_else(span_of) (block)   reported(): nothing
AFTER   Diagnostic::placed_at_subject  <- CompiledProject::reported(), pass runtime; LSP converts
```

## Decisions

- **D1.** One rule, `Diagnostic::placed_at_subject` in `specforge-common` beside `DiagnosticData::entities`: a diagnostic without a span sits at the span of the first entity its data names that the caller knows. A diagnostic with a span, without data, or naming no known entity is unchanged.
- **D2.** It is applied once where the compiled project writes its report (`CompiledProject::reported`, which every surface reads), with each entity's block, and by the pass runtime. The graph build keeps W061 spanless.
- **D3.** The place is the block, as pass and rule diagnostics already have.
- **D4.** The LSP only converts; it keeps related information at each other entity's name (`also_about`).
- **D5.** A moved cycle is a removed and an added W061 in a diagnostics delta, like any located diagnostic that moves.

## Rejected

- The first entity's name: the name rule is navigation's (app layer), so the session would need its own copy, and rule diagnostics would still sit at blocks.
- Each producer giving its span: three places, and the next spanless producer repeats the bug.
- The rule in `specforge-ops` beside `navigate::subjects`: the LSP, MCP state and the session's delta read the compiled project directly, below ops.
- Dropping the LSP's related information: it is the jump to the other cycle members, and is conversion, not placement.

## Consequences

- Breaking: W061 and W148 carry `span`, `file`, `line` and `column` in CLI `check`, `collect`/`watch --format json`, MCP `validate`, `specforge://diagnostics`, `inspect` and `diagnosticsChanged`; the LSP range of W061 is the block; the human `check` report draws W061 as a located report.
- `surface_parity` no longer re-derives the LSP's placement.
- No new code, no re-vendor.

## What would reopen it

- A surface that needs a name location for every diagnostic about an entity, rules included.
