# A read's notices reach the reply through one seam

**Status:** accepted (2026-10-16, architecture round 13, plan 08). Amends [ADR 0111](0111-every-reader-of-the-versioned-schema-reports-its-notices.md)
D1 (the outcomes that carry the notices) and D2 (how the surfaces put them), [ADR 0120](0120-what-a-cli-command-says-beside-its-document-is-said-once.md)
D2 (`Notes` gains `noticed`) and [ADR 0015](0015-read-views-are-operations-over-the-project-view.md) (the read views'
outcome types `QueryOutcome`, `Listing`, `SearchOutcome` and `ModelOutcome` are gone).

## Context

Ten read outcomes in `specforge-ops` carried a `pub notices` field (the schema notices I016 and W053 of ADR 0111, and
I020), and each of the 18 surface sites that turns one into a reply had to remember to forward it by hand: 12 MCP
`.with_diagnostics(x.notices)` or `.with_notices(x.notices)` calls and 6 CLI `Notes::beside` or `carried` calls. A site
that forgets compiles, answers and is silent.

History shows the cost. Round 11's plan 08 fixed three omissions in three tickets (`7b42913c`, `70b230d2`, `daf5f79e`);
each added a notices field to an outcome and then edited every surface by hand. The entities resource dropped I020 by
design until `5e1f43cf`, and render answered with `Ok(reply.into())`, silent, before `7b42913c`.

The hunt for a live drop found none: over a corrupt record and a breaking change, every reader that names the schema
version carried its notices on every CLI command and every MCP tool and resource. Every silence is one ADR 0111
decided: a refusal carries none (D1), the trace prompt and extension commands report nothing (D6). This decision closes
a seam before plans 09 and 12 rewrite render and trace; it does not fix a bug.

## Decision

- **D1. A read that notices returns `Noticed<T>`** (`specforge_ops::Noticed`): `export`, `export_recorded`, `query`,
  `list`, `search`, `explore`, `trace`, `schema`, `json_schema` and `model`. Its value is reached only with its notices
  (`map`, `try_map`) or by `into_value_and_notices`. It is not a document: it derives no `Serialize`.
- **D2. Three envelopes open it.** MCP's `Answer::noticed` (tool results) and `ResourceText::noticed` (resource reads)
  put the notices in `_meta.diagnostics`; the CLI's `Notes::noticed` writes them on stderr before the document, under
  every format, and hands back the value. Explore's JSON document carries its notices through
  `Noticed<Exploration>::document`. A scan test per surface allows `into_value_and_notices` only in these, and in the
  trace prompt, which reports nothing (ADR 0111 D6).
- **D3. Only a read makes notices of the versioning.** `VersionedSchema::notices` is crate-private;
  `specforge://schema` is the schema read view's document, byte for byte.
- **D4. A read that notices nothing returns its value.** No outcome gains an empty `Noticed`.
- **D5. Refusals carry none.** ADR 0111 D1 stands: a refused read says no notices, except `export_recorded`, which
  reports them whether the export succeeds or fails.
- **D6. Not covered.** The registry configuration's notices, publish's declaration warnings and login's (ADR 0120 D4)
  stay on `Notes::beside` and `Answer::with_diagnostics`; extension commands and prompts (ADR 0111 D6).

## Rejected

- A trait over the outcome types: it names the field and prevents no drop, since their bodies are public and a handler
  reads `outcome.document` without calling it.
- Doing nothing and extending the harnesses: a drop is then found only where a harness looks.
- `Noticed` in `specforge_common`: only ops reads notice, and only ops can make `VersionedSchema::notices`
  crate-private and still produce them.
- `impl From<Noticed<R>> for Answer<R>`: it would sit beside `From<R> for Answer<R>`; an associated function says what
  it does.

## Consequences

Six one-field outcome types go (`Exported`, `QueryOutcome`, `PublishedSchema`, `ModelOutcome`, `Listing`,
`SearchOutcome`) and four `notices` fields. A read that starts noticing changes its return type, and the compiler lists
every surface that must route it. The scan tests do not guard a closure that smuggles the value out through `try_map`'s
`Err`; review catches that.

## What would reopen it

A reader that must carry notices on its refusal (the MCP export refused over a corrupt record does not say why its
version is 1.0.0); a third surface that reads these outcomes; notices that are not a read's moving onto the same seam.
