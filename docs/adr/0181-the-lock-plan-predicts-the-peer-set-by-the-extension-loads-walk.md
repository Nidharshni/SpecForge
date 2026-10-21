# The lock plan predicts the peer set by the extension load's walk

**Status:** accepted (2026-10-21, architecture round 19, plan 04). Amends [ADR 0041](0041-peer-requirements-are-one-module.md) (B1: the project as it is answers a locked extension by its module on disk), [ADR 0160](0160-what-an-extension-entry-enables-is-the-loads-decision.md) (D2: the set it asks about is the load's) and [ADR 0028](0028-installed-extensions-are-one-module.md) (D3: the load's walk is one function).

## Context

- `Installed::load` and `ProjectPeers::of` (the lock plan's prediction of the project peer set) each walk a project's
  entries: load order, skip `Again`, admit a `.wasm` file entry against "the names loaded so far", sort back to entry
  order. Order (`Resolution::in_load_order`) and admit rules (`Resolved::admit`) are already single; the loop around them
  and its definition of "loaded so far" are not.
- The two disagree. The load counts a name only when its entry loaded; the prediction counts every locked entry. With a
  locked extension whose module is missing and a `.wasm` file entry declaring its name, the load loads the file while the
  plan thinks the lock entry wins, so `add` skips ADR 0160 D2's refusal and swaps one E028 for another (reproduced,
  04-D1).
- Twice (568d4664 and 9e47f1a6, b70fee00) a change to the load's walk stopped at the load; the prediction kept its own
  admit call and its own `std::fs` read of a file entry (`Module::read`, which hashed bytes nobody read).
- The prediction's `std::fs` read follows a link or a `../` path out of a bounded MCP root, and so does the bounded
  server's own environment load (ADR 0145 D3). No MCP request writes a `.wasm` file entry; the path is the project
  owner's text. Not a live defect.

## Decision

- **D1.** `specforge-installed` owns the walk once: `Resolution::walk(read, settle)`. Load order, each entry's declared
  name admitted against the names the entries tried before it were admitted under, the outcomes in entry order. The
  extension load and the lock plan's peer set are its two callers; each supplies only how a resolved entry's declaration
  is read.
- **D2.** The prediction reads `.wasm` file entries and modules through a `Disk`, `PlainDisk` in production: the disk the
  environment load reads through (ADR 0145 D3). `Module::read` is gone. Bounding file entries over MCP would be one
  decision for the load and the prediction together (U2, left as ADR 0145 D3 decided).
- **D3.** The peer set of the project as it is (ADR 0160 D2's question) answers a locked entry by its module on disk
  (`Installed::verified`), as the load does. The set a change leaves answers it by its lock entry (ADR 0041 B1): its
  staged modules are not written yet.

## Rejected

- **A `Source` trait** for the two callers: a named type with two methods for two callers that hold no state worth naming.
- **Bounding only the prediction** by the caller's containment: a new drift, since the load reads outside.
- **Caching sets across `close` and `check`**: each run after the first reads nothing new; invalidation would cost more.
- **An "entry stops loading" rule** (before against after): ADR 0160 rejected it already.
- **Every set by module** (the after-set too): needs an injectable disk on every lock-plan constructor and 29 fixtures
  with modules; left as a follow-up if a broken peer is ever reproduced (04-D2).

## Consequences

- `add`, `add_extension` and `init` refuse (`extension_conflict`) a name a `.wasm` file entry loads because the installed
  module of that name is missing or changed. The E028 for a missing module suggests `specforge add`; with such a file
  entry, that add is now refused with "remove that entry first": two steps instead of a silent swap.
- The project-as-it-is set reads and hashes every enabled locked module once per enabling check.
- Rust: `specforge-installed` gains `Resolution::walk`, loses `Module::read` and the public `FileEntry::admit` and
  `Resolution::in_load_order`; `Installed::verified` takes a `Disk`.

## Reopen when

A third caller of the walk needs another notion of "loaded", or ADR 0145 D3 is reopened for file entries.

**Amended (2026-10-21, round 19 follow-up, 04-D2):** D3: the set a change leaves also answers a locked entry by its
module on disk, except one the change stages, which it answers by its staged pin (its module is written at commit).
Reproduced: with a locked peer whose module was gone, an install requiring it passed the plan and `check` then reported
E027; it is now refused with that E027. "Every set by module" is no longer rejected. The plan reads modules through the
disk its `Declarations` holds, so no constructor changed; the lock-plan unit fixtures write the modules their locks pin.
`differing` (what a removal touches) still compares the two sets by lock entry.
