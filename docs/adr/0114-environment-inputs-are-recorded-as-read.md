# Environment inputs are recorded as read

**Status:** accepted (2026-10-16, architecture round 12, plan 03). Reopens
[ADR 0107](0107-the-checks-read-through-the-disk-they-are-given.md)'s rejected option "Module reads
(`Installed::load`) through the port" and meets [ADR 0106](0106-a-project-session-reads-the-disk-through-one-ledger.md)'s
reopen condition ("a reader port into `specforge-installed`"). Amends
[ADR 0030](0030-a-sessions-inputs-are-one-value.md) D3 and D4, ADR 0106 D1, ADR 0107 D1 and D4, and
[ADR 0028](0028-installed-extensions-are-one-module.md) D3 (round 7 amendment).
D3 is completed for sources by [ADR 0129](0129-a-source-read-is-a-recorded-read.md): a source is read through the same `RunDisk`.

## Context

A project session worked out its environment inputs (`specforge.json`, `specforge.lock`, each extension module) before
anything was read (`Installed::unread(root).modules(..)`), stamped that prediction (`Reads::before_load`), and let
`specforge-installed` do the real reads through `std::fs`. The prediction recorded the path, not what the read followed.
Three symlink rules lived side by side:

- sources: a symlinked source is keyed by its own name, its target is a `Link` input (ADR 0073 D5);
- check inputs: each hop of a link is recorded and stamped (ADR 0107);
- environment inputs: none. `watch_roots` took the link's directory, and `watched()` resolved the link to its target.

Reproduced against a debug build:

- `specforge watch` misses an edit behind a symlinked `.wasm` entry, `specforge.json` or `specforge.lock` (`check` sees
  each);
- a link pointed elsewhere (`ln -sfn`) reaches nothing in `changes`, for a module and for a check input, in watch and in
  the LSP, because every input is keyed by its resolved path.

Round 11 declined the port on "no drift: both go through `Installed::resolve`". The paths do agree. What drifts is what
the read follows, which is the drift ADR 0107's Context already names for check inputs.

## Decision

- **D1.** `Installed` reads `specforge.lock` and every module through a `specforge_registry::Disk` it is handed
  (`Installed::read(root, disk)`, `Installed::load(.., disk)`). `Installed::at` is `read(root, &PlainDisk)`.
- **D2.** A session's environment inputs are its environment load's recorded reads: the config, the lock and each module
  read, through the ledger's `RunDisk`, each stamped at its read, each link hop recorded. What the load does not read is
  no input. `Reads::before_load` and `Installed::modules` are gone.
- **D3.** Every input is keyed by its own name in its canonical directory, and each hop it follows is an input of its own.
  A batch path reaches an input recorded at or under it, compared both resolved and as spelled. This extends ADR 0073 D5's
  source rule to environment and check inputs.
- **D4.** `PlainDisk` lives in `specforge_registry::disk`, next to the port: the one `std::fs` call of that crate's `src`
  outside tests. The three adapters (`RunDisk`, `PlainDisk`, `MemoryDisk`) serve the environment load too.
- **D5.** Behaviour changes: (1) watch watches the target directory of a symlinked config, lock or module; (2) a config,
  lock, module or check input that is a link and is pointed elsewhere reloads or re-runs the checks, in watch and the
  LSP; (3) the LSP registers a symlinked input as itself and as each hop; (4) with no runtime no module is an input, and
  an entry with no lock entry has no module input; (5) a module written after the runtime is built but before the load
  reads it is what the session read, not stale.

## Rejected

- Canonicalising before `parent()` in `watch_roots` alone: it leaves the retarget defect and the prediction.
- The port in `specforge-common`: ADR 0107's reason holds, and its reopen condition (a lower layer needs the port) is not
  met. `specforge-installed` is at the runtime layer, above the registry.
- A private file-system adapter in `specforge-installed`: a second plain adapter.
- `read_link` on the port for tests: `MemoryDisk` models no link, and the link recording is tested on tempdir symlinks.

## What would reopen it

An environment read the port cannot express (a module directory listing, metadata), or a lower layer that needs the
port.
