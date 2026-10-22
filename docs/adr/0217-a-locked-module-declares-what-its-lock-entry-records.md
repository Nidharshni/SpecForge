# A locked module declares what its lock entry records

**Status:** accepted (2026-10-22, architecture round 24, plan 05). Extends [ADR 0041](0041-peer-requirements-are-one-module.md) B1 (an installed extension by the version and peers its pinned module declares) and [ADR 0181](0181-the-lock-plan-predicts-the-peer-set-by-the-extension-loads-walk.md) D3; amends [ADR 0028](0028-installed-extensions-are-one-module.md) D3 (the load's admit) and [ADR 0138](0138-the-lock-plan-answers-what-a-project-already-has.md) D1 ("already has it").

## Context

- A `specforge.lock` entry records the module's SHA-256 and a copy of its version and peers. The load compared the module with
  its entry by hash and by name only, so nothing compared the copy with the module.
- The lock plan's peer gate judged that copy (`ProjectPeers::of`, the locked arm) while `check`, `doctor`, `extensions`, the
  LSP and MCP judged what the module declares. A lock edited by hand, or merged badly, steered `add`, `init`, `update` and
  `remove` in both directions: it refused a change `check` accepts (an entry recording a peer the module never declared) and
  accepted one `check` then failed with E027 (an entry recording fewer peers).
- A reinstall did not repair an entry whose peers alone were wrong: `LockPlan::unchanged` compared the version, the hash and the
  source, not the peers, so the add answered "already installed" and kept them. E070's own remedy looped.
- ADR 0041 named this as what would reopen B1: "a peer whose installed module may declare other peers than its lock entry records".

## Decisions

- **D1. One rule, `LockFileEntry::declares`, read by the load's admit.** A module loads under a lock entry only when it declares
  the entry's name, then its version, then its peers, as text and in order. A disagreeing entry is E070 everywhere the load
  runs (`LoadProblem::NotWhatItsEntryRecords`, beside `NotItsLockEntry`); the suggestion names the reinstall at the version the
  module declares (`Installed::reinstall_at`).
- **D2. The prediction reads a locked member's declaration through the walk.** `Declarations::locked` reads what the module a
  locked entry pins declares (the pinned bytes through the plan's disk, the declaration through its runtime, cached, seeded from
  what the compile loaded) and the walk's admit holds it to the entry. An entry that disagrees, a module that is missing or
  changed, and one that is no component are no member, as the load refuses them. A staged pin and every name under
  `Modules::Locked` still answer by the entry: the pin was built from a declaration, and the two sets `differing` compares are
  built from the same entries.
- **D3. "The project already has it" means the load admits it.** `LockPlan::unchanged` also asks `declares` over the module's
  declaration, so an add of a module whose entry lies is an install that rewrites the entry.
- **D4. E070 widens; no new code.** Its explanation covers another version or other peers. Peers compare as an ordered list of
  exact text, and the version as text: every install writes what the module declared, so nothing normalises.
- **D5. An existing disagreeing lock is refused, not read** (U2 a). A lock is the project's integrity record; the load does not
  write, so the remedy is one command, `specforge add <path>` or `specforge add <name>@<declared version>`.

## Rejected

- Admit compares only, and the gate keeps the copy: the gate would judge a member the load refuses (ADR 0181 D3 broken for that
  entry).
- The prediction reads the module only: the version copy stays trusted by `update`, the R014 pin, the reinstall hint and the
  listing, and `check` stays clean on a lying lock.
- Loading by what the module declares with a warning (a new code, and bytes trusted over the integrity record), or rewriting the
  entry on `update` or on any lock write (a write path in the load).
- Normalising peers before comparing: harmless strictness instead of a second rule.

## Consequences

- A `specforge.lock` written by `add`, `update` or `init` agrees with its modules and is unaffected. A hand-edited or badly merged
  entry is E070 until `specforge add` rewrites it. So is an entry written before peers were recorded, if its module declares
  peers.
- A plan check loads each enabled, unstaged locked module once more, under the candidate name and cached, as it already does
  for every enabled builtin. A registry entry whose version alone was edited is E070; the re-fetch of the declared version is
  not byte-pinned (R014 protects bytes at the entry's version), as ADR 0201 already says of "another version".
- No component declares peers except the builtins, so a CLI or MCP test cannot fabricate a locked requirer any more: those run in
  process (`crates/app/ops/tests/operations/locked_requirers.rs`).
- `specforge-installed`: `Declared`, `LockFileEntry::declares`, `LoadProblem::NotWhatItsEntryRecords`, `Installed::reinstall_at`;
  `Installed::verified` is replaced by `Installed::pinned_module`; the testing helpers take the runtime that serves the
  extension. `WasmRuntime` is `Any`, so a test can ask a shared runtime whether it is the in-process one.

## What would reopen it

- A lock that must record something its module cannot declare (a resolver choosing versions).
- A module that may be loaded under a lock entry it does not match.
