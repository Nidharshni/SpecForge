# A mutation's preview is a fact of its outcome

**Status:** accepted (2026-10-19, architecture round 16, plan 05). Amends [ADR 0022](0022-a-mutation-reports-what-it-wrote.md)
D6 and [ADR 0052](0052-an-operations-outcome-is-one-document-both-surfaces-print.md) D4 and D8.
Amended by round 17 plan 05 (2026-10-19): rename runs through `rename::run`.

"Did this mutation only preview, and what did it write?" was answered at 18 places: seven CLI `files_written` sites
under four rules, eight `MutationArgs::previews` impls and three MCP handler lines that read `dry_run` again. The
marker was spelled three ways (`dry_run`, `action: "planned"`, `check_only`) in two shapes (always present, or present
only when true), and the add's document had none. The surfaces disagreed:

- `specforge add|remove --dry-run --format json` printed `"files_written": []`; MCP's preview leaves the key out, and
  the CLI's own doc comment said it was left out.
- A real typed edit that changed nothing left `files_written` out on the CLI (it read like a preview); MCP sent `[]`.
- A dry run of an extension already present printed, on the CLI, the bytes of a real add: ADR 0052 D4 had dropped the
  add's `dry_run` because the `planned` tag states it, which fails for an entry nothing would change.

## Decision

- **P1. The request asks, the outcome says.** Every operation that can preview takes it in its request (`dry_run`;
  format's `Mode`; `edit::run`'s argument) and its outcome answers `specforge_ops::Wrote` (`Previewed`, or `Ran(Writes)`),
  copied from the request, never from what the operation found. Operations that cannot preview (init, a rollback, an
  inference step) answer `Writes`.
- **P2. One `files_written` rule, `Wrote::named`:** absent from a preview, every file a run wrote otherwise, `[]` when
  none, on both surfaces. The CLI's `Written` is built only from it (amends ADR 0022 D6, which named no rule for a CLI
  dry run, to D7's).
- **P3. MCP's arguments still decide first** (ADR 0022 M2 kept): `Written` carries the outcome's fact, and debug builds
  assert it agrees with `MutationArgs::previews`. A failure has no outcome, so the arguments are all it has.
- **P4. One marker.** Every document of a mutation that can preview says `dry_run: bool`, always present (amends ADR
  0052 D4, and D8's `check_only`). The add's per-entry `action: "planned"` stays: it says what the add would do for that
  entry. Remove's `action` and format's `check_only` are gone.
- **P5. Typed edits have one document**, `EditDocument`, beside `edit::run` (ADR 0052 D1 extended to edits); a run with an
  empty plan is applied and answers `Ran([])`.

Rejected: the outcome alone over MCP (a failure has no outcome; ADR 0022's rejected `Mutated::new(preview, ..)`); a
CLI-only `Written::of(document, dry_run, writes)` (fixes the CLI rules, leaves the add's missing marker and two edit
documents); `preview` as the key (no caller types it); a stored `wrote` on every outcome (it replaces `writes` at ~25
test sites for an invariant the request copy already makes true).

## Consequences

CLI `--format json`: `add --dry-run` and `remove --dry-run` no longer print `files_written`; the edit verbs print
`"files_written": []` for a run that changed nothing; `add` gains `dry_run`, `remove` loses `action` and gains
`dry_run`, `format`'s `check_only` is `dry_run`. MCP: the same document changes (and `specforge.edit` and
`specforge.rename` always send `dry_run`); `files_written` over MCP does not change. `update` still writes no
`files_written`.

What would reopen it: a mutation whose preview its request cannot state; a client that needs to tell "would write
nothing" from "previewed" on a failure.

**Amended by round 17 plan 05 (2026-10-19):** P1's operations include rename: `rename::run(nav, old_id, new_id,
dry_run, caller)` answers `Renamed { plan, wrote }`, and `Wrote::unless_preview` is crate-private to ops, so no
surface decides a preview. Round 16 had left rename's handler calling it (plan 05 D5, a file-ownership call).
