# What a CLI command says beside its document is said once

**Status:** accepted (2026-10-16, architecture round 12, plan 09). Amends [ADR 0029](0029-operations-decide-their-failure-and-verdict.md)
D3a and D4 (review), [ADR 0052](0052-an-operations-outcome-is-one-document-both-surfaces-print.md) D8 and
[ADR 0111](0111-every-reader-of-the-versioned-schema-reports-its-notices.md) D2.

## Context

Every CLI command decided by itself what it writes on stderr beside its document. The command line had 62 production
stderr writes. Under `--format json`, seven of them said a fact the document on stdout already carried, and two wrote
prose for a person to a JSON reader's stderr; four modules gated a person's line by hand, each in its own spelling.

Three written rules disagreed. ADR 0029 D3a: "under `--json` nothing goes to stderr" (written for analyze's gate).
ADR 0052 D8: the same for format. ADR 0111 D2: "the CLI prints the notices on stderr before the document". The one
helper, `Terminal::diagnostics`, printed "whatever the output format".

Reproduced against the binary:

- analyze: a stray test record is in `stray_records` and is W097 on stderr.
- check: `note: --lint pedantic …` and `note: specforge-cache.json not written …` on a JSON reader's stderr.
- search: a failed registry is R005 in `diagnostics` and again on stderr, against "reported once".
- explore: I020 is in `notices` and again on stderr.
- migrate: hook failures, W054 and the restore lines are in the document and on stderr.

Each duplicate arrived when a document gained a field and the CLI's stderr loop for the same fact stayed.

## Decisions

- **D1. One rule.** A command's stdout is its document, its human rendering, or a fixed format. Beside it, on stderr,
  a **note** (prose for a person) is written under human output only. A **notice** (a coded diagnostic) is written
  once: in the document when the document carries it; else on stderr, as `severity[CODE]: message`, before the
  document, under every format. A fixed-format command (export, schema, model, query, outline) has no field for one,
  so its notices are on stderr. Refusals keep ADR 0029 D5.
- **D2. One owner.** `specforge_cli::notes::Notes` (`beside`, `warning`, `carried`, `person`) is the only writer of a
  line beside a document. `Terminal::diagnostics` is gone; the terminal is streams, colour and asking.
- **D3. The three sentences are cases of D1.** ADR 0029 D3a holds for analyze because every fact analyze reports is in
  its document. ADR 0052 D8 holds for format for the same reason. ADR 0111 D2 holds for the readers of the versioned
  schema because their documents carry no notices (ADR 0111 D1); MCP carries them in `_meta.diagnostics`. Explore's
  I020, search's failed registries and migrate's rollback are in their documents, so they leave stderr under JSON.
- **D4. Notices no document carries.** The registry configuration's W140, I003, W155 and W156 and publish's
  declaration warnings are written beside the document on every registry command. Collect's unremembered approval and
  export's unwritten schema cache are uncoded warnings, on stderr under every format.
- **D5. Review refuses as a read view.** Every refusal exits 1. `Exit::Unjudged` from a refusal is stats' and analyze's
  (ADR 0029 D4's list). `query --include-coverage` already exited 1 on the same unreadable report.
- **D6. Not covered.** `watch` and `mcp` stream; extension commands keep ADR 0011's contract; a human rendering that
  is itself on stderr (check, format, migrate's summary) is a rendering, not a note.

## Rejected

- Everything in the document: fixed formats cannot hold it, check's bare array would become an object, and every
  document would gain a CLI-only key the parity harness must skip.
- Notices always on stderr: drops keys from documents MCP shares (ADR 0052 D2), and takes I020 from the explore
  prompt, which has no `_meta`.
- A runner for the one-shot reads: the sixteen compile-and-refuse blocks differ in format handling, pre-steps and
  verdicts; the runner's interface would be as wide as its body.
- The notes as methods on `Terminal`: the terminal would learn the output format.

## What would reopen it

A document that must carry a notice another surface keeps in `_meta`, or a consumer that reads stderr under
`--format json` as structured output.
