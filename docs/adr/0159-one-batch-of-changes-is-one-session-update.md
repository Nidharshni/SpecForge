# One batch of changes is one session update

**Status:** accepted (2026-10-19, architecture round 16, plan 02). Amends [ADR 0014](0014-the-session-keeps-itself-current.md) (the intro's "`apply` runs the one rebuild a batch needs"), [ADR 0100](0100-a-session-takes-a-change-from-disk-only-as-changes.md) D1 (the key door) and [ADR 0136](0136-every-session-update-goes-through-one-bracket.md) D5. Keeps ADR 0136 D1-D4 and ADR 0073 D7.

`ProjectSession::apply` was an if/else ladder: an environment change reloads, else changed sources are reread, else
the check inputs are rechecked. Only the first kind a batch named ran. Until round 14 that was safe, because the
sources branch always ran every check, which renewed every check input's stamp. ADR 0136 D3 (commit `41d21edf`) made a
source that reads as the session holds it change nothing and run no check, and left the ladder as it was. A batch that
named such a source and a changed check input (a `touch` plus a created file a `file_exists` rule names) took the
sources branch, ran nothing and dropped the check input: `apply` answered `None`, the stale diagnostic stayed, MCP
fixed it one request late, and watch and the LSP stayed wrong until another event reached that path. A source relinked
to its own text dropped it too while answering `Some`.

## Decision

- **D1.** `apply` makes at most one update for everything a `Changes` names. A changed environment input reloads the
  environment, which reads every source and runs every check, and covers the rest.
- **D2.** Otherwise the named sources are reread and staged (ADR 0136 D3), and the checks run once when a read
  applied, a check input is named, or an earlier update skipped them. A check input is never dropped because a source
  in the same batch read as the session holds it.
- **D3.** The private doors `reread` and `recheck` are gone. Whether the update changed anything is the bracket's
  `changed_nothing`, in one place (ADR 0136 D5).

Rejected: falling through to `recheck` when `reread` answers `None` (misses the relinked case, keeps three decision
sites); two updates, sources then checks (runs the checks twice, two events); marking the checks skipped to force them
(`skip_checks` empties the report and claims a state that did not happen).

## Consequences

A batch of an unchanged source and a changed check input now yields one `Checks` update: `specforge watch` prints a
`rechecked` line, MCP bumps its generation, the LSP publishes. No surface changes code.

## What would reopen it

A change kind that cannot be applied inside one bracket; a check input whose change must not re-run every check
(per-rule rechecks).
