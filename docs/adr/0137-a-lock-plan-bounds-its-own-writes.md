# A lock plan bounds its own writes

**Status:** accepted (2026-10-17, architecture round 14, plan 05). Amends [ADR 0065](0065-every-extension-change-is-one-lock-plan-change.md) Amended by [ADR 0144](0144-a-project-file-is-written-by-replacing-its-name.md) (a project file is written by replacing its name).
D1 (`Checked::finish`), [ADR 0104](0104-containment-is-applied-where-the-operation-opens-the-file.md) D2 (who applies the
containment to lock-plan writes) and [ADR 0128](0128-one-project-change-over-held-files.md) D4 (where the `Also` files
enter). Builds on [ADR 0124](0124-what-the-caller-lends-an-operation-is-one-value.md).

Every lock-plan write was checked against a containment in `Checked::finish(root, also, containment)`, but the plan did
not choose that containment: each of the four operations passed one. add and init narrowed it to their own project
(`.narrowed(dir)`), update and remove passed the caller's bound as it was. Every operation also passed a `root` the plan
already held as `installed.root()`. Nothing escaped: no surface lends update or remove a bound wider than their
project. But the rule "a change bounds its writes to its own project" (ADR 0092 D8) was decided at four call sites, three
commits in one day edited all four together, and no test held the narrowing at any of them. Reproduced: MCP init refuses a
`.gitignore` linked to a sibling inside the server's root, and only init's own narrowing refuses it.

## Decision

- **D1.** A lock plan is opened with the operation's `Caller`. Its fetches ask the caller's `Asking` (`Fetching` carries
  the registry and `allow_unsigned` only), and its commit reads the caller's containment.
- **D2.** The commit bounds its writes to its own project. It resolves the project root through the lent bound (a root
  outside the bound is `outside_root`; the bound is never widened), narrows the bound to that root, and resolves every
  file it writes through the narrowed bound (`also`, `specforge.json`, the lock, each module), before anything is
  written, a preview included. `Checked::finish` takes no argument.
- **D3.** Files an operation writes beside the plan's (init's `.gitignore` and starter) are handed to the checked plan
  (`Checked::writing`), because the starter is chosen from the entries the plan judged. The commit order of ADR 0128 D4
  and ADR 0059 D2 is unchanged.
- **D4.** `fetch` returns the install, and the operation charges it to its ask. Resolution stays before the claims
  (ADR 0098).

## Rejected

- `LockPlan::initializing(.., also)`: the starter needs the judged entries and the templates named after the plan opens.
- `finish(&Containment)` narrowed inside: half the caller at open, half at finish.
- `Containment::narrowed` returning `Result`: a public change for one internal caller.
- `fetch(ask, ..)` with a `Fetched` type: 02's `record` needs the `Install` between the two calls.

## Consequences

No CLI, MCP or LSP output changes: the CLI is unbounded, MCP lends add and remove the project root, and init's
narrowing was already in place. `specforge_ops` `update` and `remove` lent a bound wider than their project now refuse
`outside_root` for a write between the two. `Containment::narrowed` keeps two callers: the lock plan and init's runner
probe. ADR 0124 is not reopened: the caller still lends one bound, and the plan derives a narrower one for its own writes.

## What would reopen it

A lock plan that must write outside its project (a shared module cache, a workspace lock); a surface that lends an
operation a bound that does not hold the project root.
