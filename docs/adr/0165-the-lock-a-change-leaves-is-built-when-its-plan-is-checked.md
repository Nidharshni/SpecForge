# The lock a change leaves is built when its plan is checked

**Status:** accepted (2026-10-19, architecture round 17, plan 01). Amends [ADR 0041](0041-peer-requirements-are-one-module.md)
(B4), [ADR 0065](0065-every-extension-change-is-one-lock-plan-change.md) (D1),
[ADR 0134](0134-add-and-init-ask-their-lock-plan-through-the-names-module.md) (D2) and
[ADR 0028](0028-installed-extensions-are-one-module.md) (D4, as amended in round 16).

## Context

A lock plan was opened with `installs: bool`, computed by its operation from the request in a different file than the
asks that needed it. That one flag decided three things: whether an unreadable lock refused at the open, whether
`install` could stage (else it panicked on an `expect`), and whether the commit opened and bounded the lock
(`changes_lock = lock.is_some()`). The third was a live defect: an `add` whose installs were all already present
writes only `specforge.json`, yet MCP refused it `invalid_input` for a `specforge.lock` that links outside the project,
a lock it never writes, while the same add without the no-op specifier succeeded. ADR 0138 had already made an ask the
project has stage nothing, so "the flag is true" no longer meant "something is staged".

## Decisions

**D1. The plan holds no lock.** The lock a change leaves is built when the plan is checked (`LockPlan::view`), from the
lock as read, each staged install's entry put in place of its package's previous one and each uninstalled package's
dropped; a lock that can't be read refuses there (E033). A change that stages neither accepts any lock, judges the lock
as the compile read it, and opens no installed change. `install` is total: no field can be `None`.

**D2. The early refusal is asked by whoever reads the request**, before it reads a file or asks a registry: `Names::ask`
(through `LockPlan::can_install`) for specifiers that install, `updating` for `update`, `removing` for an uninstall, and
`remove`'s selection for a row only the lock could tell. No constructor takes a flag about the lock. The refusal order
is unchanged: config, lock, request.

**D3. The commit writes and bounds the lock if and only if the plan staged or uninstalled.** A preview stages registry
packages with no module, so its bound check still covers the lock.

## Rejected

- `names` opening the plan: ADR 0134's rejected item stays rejected; it is a second plan type for one bit, and D1
  leaves no panic for it to guard.
- Keeping the staged lock and making `install` fallible: two fixes (the E033 and `changes_lock`) for one cause.
- Routing `remove`'s two "can't tell" refusals through the open: they write no lock. ADR 0028's round-16 rejection keeps
  its first reason (a name with nothing under it opens no plan); its second ("a readable lock would open a change that
  writes nothing") is gone with the flag.
- Moving each verb's config rule into the plan: the rules differ on purpose and `add`'s refusal order would change.
- A `runtime` held by the plan for `names`: an `Option` and an `expect`, or the field round 16 plan 10 deleted.

## Consequences

MCP `add_extension` of a present install beside another change, over a `specforge.lock` linked outside the project,
answers instead of `invalid_input`. `LockPlan::{adding, initializing}` take no `installs` and cannot fail;
`Names::installs` and `LockPlan::lock` are gone; `LockPlan::view` returns a `Result`.

## What would reopen it

An operation that must stage into the lock between its asks and the check; a lock the judgement must read without the
operation having parsed it.
