# The lock plan answers what a project already has

**Status:** accepted (2026-10-17, architecture round 14, plan 06). Amends [ADR 0065](0065-every-extension-change-is-one-lock-plan-change.md)
D1 (the per-ask outcomes) and [ADR 0126](0126-a-lock-plan-writes-the-specforge-json-entries-it-judged.md) D1 (the per-ask
`appended`). D1's "an entry enables it" is a named entry naming it ([ADR 0160](0160-what-an-extension-entry-enables-is-the-loads-decision.md)).

`add` decided "already there" before it asked its lock plan: a builtin already enabled, a local install or a registry
package whose lock entry, module on disk and `specforge.json` entry all matched. Only the rest reached the plan, which
decided the builtin half again over the same entries and answered `Asked.appended`, always `true` on add's path.
The install half's rule lived in `Installed::verified`, documented as "what already installed means for `add`". An early
return in `add` was the only thing that kept a no-op add from being judged.

A dry run and a real add agree today (reproduced on the CLI and over MCP), so this is a decision about where the rule
lives, not a fix.

## Decision

- **D1. The plan answers each ask `Changes` or `Unchanged`.** `enable` answers `Unchanged` when an entry of the
  `specforge.json` the plan opened enables the name; `install` when the lock the plan opened pins the package with the
  same hash and kind of source, its module on disk is the one pinned (`Installed::verified`) and an entry enables it;
  `fetch` returns the locked install, asking no registry, for a registry package pinned at that version under the same
  conditions. An unchanged ask stages and appends nothing. An operation asks for every specifier it names; none decides
  "already there" itself.
- **D2. A change that changes nothing is neither judged nor written.** No staged install, appended or dropped entry,
  uninstall or created `specforge.json`: the plan runs no closure and no peer rule, resolves no file, compares no
  `specforge.json` and opens no installed change; it answers.

## Rejected

- `fetch` returning a second result type the operation matches (the operation decides again).
- The plan answering an install's whole report (`Wanted`'s fields): every verb's answer grows for add's printing.
- Judging a plan that changes nothing: an add of what the project has could refuse E028, `outside_root` or
  `file_changed`.

## Consequences

`Asked.appended` goes (`Asked` is `Changes { peers_enabled }` or `Unchanged { locked }`); `add::step`,
`already_present`, `Step` and `specforge_ops::extension::enabled_builtins` go; `Installed::verified`'s one caller is the
lock plan. No surface answer changes.

## What would reopen it

An operation that must re-install what the lock already pins (a `--force` reinstall); a lock entry whose sameness needs
more than its hash, version and source kind (a signature the lock records and the module must still carry).
