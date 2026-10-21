# Every extension change is one lock-plan change

**Status:** accepted (2026-10-13, architecture round 8, plan 02). Amends ADR 0041 (amendments A5, B1), ADR 0028 (D4, D8,
D9, D10) and ADR 0045 (D2).
**D3 amended by [ADR 0201](0201-a-locked-package-is-fetched-to-the-pin-its-lock-entry-records.md) (round 22):** a dry run refuses a locked version served as other bytes (R014), from the manifest's SHA-256.
**D5 amended by [ADR 0180](0180-the-extension-load-records-where-each-entry-loads-from.md) (round 19):** `Origin::of` is `Resolution::origin`, and the inventory reads the recorded origin.

ADR 0041 made the lock plan the one way `add`, `update` and `init` install, and its round-7 amendment made it judge the
project peer set. Each operation still drove the plan's protocol itself: which constructor, staging then enabling,
`close()` before `appended()`, a refusal walked back to a specifier through `required_by`, `peers_enabled` rebuilt from
the appended entries and the config edit. Five of the ten commits that changed both the plan and `add` since 2026-10-01
changed `add` only because a plan rule changed what its callers call. Three changes went around the plan:

- `remove` judged its own set (every lock entry, enabled or not) and opened the change itself, so it refused to remove a
  builtin that `add` and `check` accepted beside a locked extension no entry enables.
- `add`'s dry run answered `planned` before any plan existed: "planned" for an add the real run refused, and no
  `peers_enabled`.
- `init` judged one plan, dropped it, judged a second in `apply`, rolled back by hand, and read its specifiers with its
  own mapping (`extension_not_found` where `add` answers E054, R-RES-003, E063, E064).

## Decisions

- **D1. An operation asks; the plan stages, judges once, then previews or commits.** `LockPlan` opens per verb
  (`adding`, `updating`, `initializing`, `removing`) and takes what an operation asks for, each charged to an `Ask` (its
  position among the operation's requests): `install`, `enable`, `disable`, `uninstall`, and `fetch` from the plan's
  registry. `check()` closes over required builtins (and `init`'s companions), judges the project peer set as the change
  leaves it, and loads what it fetched; `Checked::finish` writes or, for a preview, only answers, with per-ask outcomes
  (`appended`, `peers_enabled`). Refusals name their ask. The plan keeps its own staged lock (`LockFile::put` and
  `without`, the rule `Change` writes by) and builds the change when it commits.
- **D2. Remove judges the project peer set.** A removal is a lock-plan change: removing a member another member requires
  as a non-optional peer is E027 ("removing '<name>' breaks '<dependent>': ..."), unless forced. A locked extension no
  entry enables is not a member and keeps nothing from being removed (ADR 0041 B1's reason, for removals). Supersedes
  ADR 0028 D8's "which loaded or locked extensions require a name" and D10's last sentence; `Inventory::dependents` is
  gone.
- **D3. A dry run is the plan checked and not committed.** `add`'s and `remove`'s dry runs open the plan with
  `Purpose::Preview`: the same staging, closure and judgement, then no write, no pin, no download. A refusal is the real
  run's refusal. A registry package is judged by the declaration it was published with, read without its binary through
  the port's `published` (ADR 0045 D2 as amended): the declaration the real gate judges. What only the binary or its
  signature can fail (integrity, R-TRUST-*, E028, R-TRUST-004) is not previewed. `specforge add` and `specforge remove`
  take `--dry-run`.
- **D4. Init is one change.** `init::init` reads its specifiers through `extension::specified` (add's codes; a registry
  package E063, a git source E064), opens one plan, and commits its modules, lock, `specforge.json`, `.gitignore` and
  starter as one change. `Change` puts back every file its last step names and removes the directories it created (ADR
  0028 D4 as amended); init's hand rollback, `init::plan` and `init::apply` are gone. The commit order is modules, lock,
  `specforge.json`, `.gitignore`, starter, pins.
- **D5. One origin.** `Origin::of(resolution, at)` maps the load's resolution (`Installed::resolve`) to where an
  extension comes from; the peer set's members and the inventory's rows read it. A row's origin is that of the entry the
  load loaded it by (ADR 0028 D9 amended: a `.wasm` file entry that loaded beside a named entry that did not is
  `file:`).
- **D6. A specifier's syntax decides its source.** A builtin's name; `git+...`; a path ending in `.wasm` or starting with
  `./`, `../`, `/`; a package reference when it starts with `@`; anything else is a local path. Registry packages are
  always `@scope/name` (ADR 0036), so nothing a registry serves is lost, and `parse` stays pure. `update <name>` of a
  name the lock does not hold is refused before any registry is asked.

## Rejected

- Remove judging every lock entry (the old contract): add and remove would disagree on one project.
- A full fetch in a dry run: it downloads, would ask a key-change question for a preview, and its peer verdict is the
  manifest's anyway.
- An unjudged `planned` for registry packages: today's lie, kept for them.
- `init::plan` as a preview: no surface previews an init.
- One public `Opening` struct for every operation: each caller would learn every other's fields.

## Consequences

- `specforge remove` of an extension only a disabled lock entry requires succeeds; its E027 wording changes.
- MCP `add_extension` with `dry_run` answers the add's refusal (`isError`), reports `peers_enabled` on `planned`, and
  asks a registry for a registry package's manifest (showing the registry configuration's diagnostics).
- `specforge add --dry-run`, `specforge remove --dry-run`.
- `init --extensions` refuses with add's codes; a registry package is E063, a git source E064.
- `specforge update <name>` of a name the lock does not hold exits 1 (`extension_not_found`, E072).
- `specforge add ext` (a crate directory without `./`) installs it.
- The listing's `source` of a `.wasm` file entry that loaded beside a failed named entry is `file:<path>`.

## What would reopen it

An operation that must judge something other than the project peer set (a builtin's version change); a registry
protocol whose manifest is not the declaration the gate judges; a surface that previews an init.

**Amended by ADR 0098 (2026-10-15):** D4: `init` also claims its specifiers through add's rule (`extension::claims`): two specifiers that add one extension differently are refused with `extension_conflict`.

**Amended by ADR 0105 (2026-10-16):** D4 names `extension::specified`; init now reads its specifiers, as add does, through `extension::names`, and neither splits a string on commas.

**Amended by ADR 0126 (2026-10-16):** D1: `Checked::finish` writes the entries the plan judged (`file_changed` when the file changed since it was read).

**Amended by ADR 0138 (2026-10-17):** D1: the per-ask outcomes are `Changes { peers_enabled }` or `Unchanged { locked }` (no `appended`); an ask the project already has stages nothing, and a plan that changes nothing is not judged.

**Amended by ADR 0134 (2026-10-17):** D1: add and init ask through `extension::names`, which charges each specifier's ask to its position.

**Amended by ADR 0137 (2026-10-17):** D1: the plan is opened with the operation's `Caller`; `Checked::finish` takes no argument and bounds every write to the plan's project.

**Amended (2026-10-19, architecture round 16, plan 10):** D1: a removal's plan is opened with the package it uninstalls (`LockPlan::removing`); it is not asked of an open plan.

**Amended by [ADR 0165](0165-the-lock-a-change-leaves-is-built-when-its-plan-is-checked.md) (2026-10-19):** D1: the plan keeps no staged lock; the lock the change leaves is built when the plan is checked (`LockFile::put` and `without`, the rule `Change` writes by).

**Amended by [ADR 0168](0168-the-lock-plan-answers-an-ask-when-it-is-asked.md) (2026-10-19):** D1: each ask is answered when it is asked; `Checked::finish` answers the builtins each ask enabled.
