# add and init ask their lock plan through the names module

**Status:** accepted (2026-10-17, architecture round 14, plan 02). Amends [ADR 0105](0105-one-reader-for-the-extensions-a-change-names.md)
D1 (add's `apply` goes; the reader asks the opened plan) and [ADR 0065](0065-every-extension-change-is-one-lock-plan-change.md)
D1 (add and init ask through `extension::names`). Amended by [ADR 0189](0189-init-checks-and-lays-out-its-starter-with-the-registry-build-of-the-extensions-it-enables.md) (D1: a local file's answer carries its declaration, not only its starter template). Keeps [ADR 0098](0098-one-claim-rule-for-the-extensions-a-change-names.md)
D1 and its rejected option c. Builds on [ADR 0138](0138-the-lock-plan-answers-what-a-project-already-has.md), under which
"already there" is the plan's answer, so no operation filters its asks.

`add` and `init` read their specifiers through one module (`extension::names`), but each then re-dispatched the `Named`
it got back onto the lock plan in its own loop and mapped the plan's refusal back to a specifier with the same four
lines. The commit history shows the loop changed in nine commits since 2026-10-11, four times as an add commit followed
minutes later by the matching init commit. The rule "open the plan before any local file is read" was a comment each
caller kept by hand, and init kept an `unreachable!` where the plan already refuses a fetch with no registry. No
behaviour differed between the two loops (reproduced).

## Decision

- **D1. `names` asks.** `Names::ask` takes the plan the operation opened, names each specifier (read, claimed), asks the
  plan for each under its position (enable, install, fetch then install; a duplicate asks nothing), checks it once, and
  names the decisive refusal by its specifier among several. It returns the checked plan and one answer per specifier:
  a duplicate of which, the builtin enabled, or what the operation recorded of the install (its own step, called before
  the plan takes the install) with the starter a local file declares. No filter: "already there" is the plan's answer, and
  a change that changes nothing is neither judged nor written (ADR 0138).
- **D2. The plan is opened before anything is read.** The operation opens its plan for its verb (ADR 0065's
  constructors) and hands it to `ask`, the only way to read the files a change names; a lock that cannot be read refuses
  first.
- **D3. What stays with each operation.** add: its config, the registry step, the dry run's purpose, what it records of
  an install (`Wanted`) and its outcomes, and `finish`. init: its validations, the E063 step, the companions, the starter
  and `finish`.
- **D4. The reader holds no registry.** A package's version is the caller's step (ADR 0105 D1); its fetch is an ask of
  the plan, from the registry the operation opened the plan with; a plan opened without one refuses E063.

## Rejected

- A step per `Named` variant: each caller still dispatches.
- Packages left to add: two loops again (ADR 0105 rejected it).
- The loop in the lock plan: the plan knows asks, not specifiers (ADR 0098 D1 b's reason).
- `names` opening the plan: ADR 0065's rejected shared `Opening`.
- `ask` returning a `Refusal<F>`: `names` would learn each operation's field type for one `.about`.
- `names` carrying add's install report itself: init would carry four fields it never reads.

## Consequences

No behaviour change. `Named`, `name_each` and `Names::named` are private to `names`; init's `unreachable!` is the plan's
E063; the next rule about how a specifier becomes an ask lands once.

## What would reopen it

An operation that must act between its asks and the check beyond what the plan reads at check time (then `ask` returns
the unchecked plan); an operation whose specifiers are asked of something other than a lock plan.

**Amended by [ADR 0165](0165-the-lock-a-change-leaves-is-built-when-its-plan-is-checked.md) (2026-10-19):** D2: the operation still opens its plan and hands it to `ask`; `ask` asks it, before it reads a file, whether it can install (`LockPlan::can_install`), so a lock that cannot be read refuses first. The rejected "`names` opening the plan" stands.

**Amended by [ADR 0168](0168-the-lock-plan-answers-an-ask-when-it-is-asked.md) (2026-10-19):** D1: each `Answer` names what the plan answered its ask; the builtins it enabled are the finished plan's, by the same position.
