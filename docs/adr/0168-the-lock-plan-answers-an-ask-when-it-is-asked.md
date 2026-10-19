# The lock plan answers an ask when it is asked

**Status:** accepted (2026-10-19, architecture round 17, plan 04). Amends [ADR 0138](0138-the-lock-plan-answers-what-a-project-already-has.md)
D1 (sameness includes the version; `Unchanged` carries no version; the answer is returned), [ADR 0134](0134-add-and-init-ask-their-lock-plan-through-the-names-module.md)
D1 (each `Answer` carries what the plan answered) and [ADR 0065](0065-every-extension-change-is-one-lock-plan-change.md)
D1 (per-ask outcomes: the answer at ask time, the peers from `Done`).

## Context

The lock plan decided "changes or unchanged" when an ask was made, stored it as a placeholder
(`Asked::Changes { peers_enabled: vec![] }`), and `finish` rewrote every placeholder with the peers the change
enabled. The only reader, `Done::asked(Ask)`, panicked on an ask that was never made (a duplicate's position), a panic
a test pinned as intended.

The "unchanged" half also answered wrongly. Since round 14 an install was "already there" when the lock pinned the same
bytes, whatever version was asked. A registry that serves the locked bytes under a newer version was therefore never
loaded, so ADR 0012's "the binary declares what was published" (E028) was skipped: `update` reported
`@sdk/greet 0.1.0 -> 0.2.0` applied (and emitted `extension_upgrade_completed`) while the lock stayed at 0.1.0, and
`add @sdk/greet@0.2.0` answered `already_present 0.1.0`. The same lie with other bytes was refused E028.

## Decisions

- **D1. An install is the project's already only at the version asked:** the lock pins the same version, SHA-256 and
  kind of source, its module on disk is the one pinned, and a named entry enables it. A fetched install the lock pins
  under another version is staged and loaded like any other.
- **D2. The plan answers an ask when it is asked:** `install` and `enable` return `Changes` or `Unchanged`; the plan
  keeps no answers; `names` hands each back in its `Answer`.
- **D3. A finished plan answers what finishing decides:** the builtins it enabled for each ask
  (`Done::peers_enabled`, total: none for an ask that enabled none), its writes and its count.
- **D4. A refused plan has two readers:** one decisive refusal for a change reported as one (`add`, `init`, `remove`),
  every refused ask for one reported per row (`update`). Unchanged.

Rejected:
- A plan generic over each caller's payload returning finished rows: 9 impl blocks gain a type parameter, and the
  verbs' reports still hold rows that are no asks (add's duplicates, update's up-to-date and not-from-registry rows).
- The plan returning the verbs' rows, unasked ones included (ADR 0138's rejected "every verb's answer grows").
- Peers keyed by the extension's name: two keys for one ask.
- `Done::asked` returning `Option`: it moves the `expect` into `add`.
- `update` reading an unchanged upgrade as up to date: it hides a registry serving one binary under two versions.

## Consequences

`Asked` is `Changes | Unchanged` and lives in `lock_plan`; `Done::asked` and the plan's `asked` are gone. `add` and
`update` refuse E028 where they answered `already_present` or reported an update they did not write. A dry run of that
case answers `Planned` where the real add refuses, as for any binary that does not declare what was published
(ADR 0065 D3: a preview loads no binary). `add`'s `Wanted` is a `Package` and one match.

What would reopen it: an operation that must read an ask's answer only after the plan is checked (a later judgement
that turns a `Changes` into an `Unchanged`); a report whose rows are all asks, for every verb.
