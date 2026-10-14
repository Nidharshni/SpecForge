# Extension management stays a module group of specforge-ops

**Status:** accepted (2026-10-14, structure round, plan 09).

Extension management is the largest group of operations in `specforge-ops`: add, update and remove, the lock plan,
the inventory, listings, publish, and the commands extensions declare. At the tip this plan started from
(`1c33fb27`), the group is `crates/app/ops/src/extension/` (10,143 lines with `publish.rs`, 11,539 with
`command.rs`; about 58 % of them inline tests). Plan 06 of this round (ops module groups, ADR 0086) groups the
operations by what they are and moves these paths, so the paths in this record are as at `1c33fb27`. We measured
whether the group should be its own crate, and prototyped the extraction.

## Measurements

Measured at `0113232f` (the round's base; the crates have since moved, the code has not), Apple M3 Pro, dev
profile, incremental, three runs each. A run flips a string literal in a non-test function body, then times
`cargo build -p <package> --timings`.

| A body-only edit in ... then `cargo build -p ...` | Today | Extracted (prototype) |
|---|---|---|
| `extension/add.rs`, specforge-cli | 4.9-7.6 s | 2.3-2.4 s |
| `analyze.rs`, specforge-cli | 3.1-7.8 s | 3.1-3.2 s |
| `extension/add.rs`, specforge-lsp | 2.2-4.3 s | 0.2 s (nothing to build) |
| `analyze.rs`, specforge-lsp | 2.2-3.1 s | 2.3 s |

The `specforge-ops` unit is 0.8-2.5 s of each rebuild. In a full build it is 24.9 s of 2,593 s of compile time
(1.0 %). The LSP links the same 259 packages with or without the extraction, because the extension code uses no
dependency that the rest of ops does not also use. The runs were taken under load from other agents (load average
20-228), so the wall times are noisy; the conclusion needs only that the largest possible saving is bounded by
the `specforge-ops` unit plus the LSP's two units, a few seconds.

## Decision

The extension operations stay a module group of `specforge-ops`. No `specforge-ops-extension` crate exists.

## Why

- **The saving is small.** At most about 3 s per extension edit, on rebuilds that already take 2-8 s, and nothing
  for edits elsewhere in ops.
- **A crate that follows the group's dependencies gives the LSP nothing.** Analysis (doctor reads the inventory)
  and authoring (init installs through the lock plan) sit above extension management. The read views the LSP uses
  share a crate with them, so that crate would depend on the extension crate, and the LSP would still link it. The
  split would also make the lock plan's protocol (`LockPlan`, `Install`, `Ask`, `Also`), crate-private today, the
  public interface of a crate. ADR 0065 took that protocol away from the operations.
- **The crate that does save time breaks the inventory.** Moving `init` into the extension crate, and the
  inventory below it (doctor needs it), puts one concept in two crates (ADR 0028's one extension inventory) and
  reverses where plan 06 places both.
- **The boundary would be shallow.** Without the crate, its callers (CLI and MCP) lose nothing; the only thing the
  boundary buys is build parallelism.

## Rejected

- The extension group as a crate over a new base crate, with read, analysis and authoring above it.
- An extension crate holding the operations plus `init` and `publish`, with the inventory left in ops.

## What would reopen it

- The extension code needs a dependency the rest of ops does not use (an archive format, an HTTP client, a git
  library). The LSP would then link it, which is ADR 0010's reason to split. Re-check with `rg -l '<crate>::'`
  inside and outside the extension group.
- `cargo build -p specforge-lsp` after a body-only edit in the extension group takes over 10 s on an idle machine.
  Re-measure as above: three runs, `CARGO_INCREMENTAL` unset.
- A consumer that needs the extension operations and must not link the rest of ops.
