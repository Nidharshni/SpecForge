# Installed extensions are one module

**Status:** accepted (2026-10-07)
**Amended by [ADR 0172](0172-the-extension-a-path-names-is-read-by-one-module.md) (round 18):** D7: a path is read by `LocalComponent::read` for add, init, publish and extension validate alike; a `.wasm` file that can't be read is E040 (was E054).
D4, D6 and D10 amended (round 16): an unreadable lock is refused once, by the lock plan's open, with the lock's own remedy.
D4 amended by [ADR 0128](0128-one-project-change-over-held-files.md): the change places modules and writes the lock; the project files of its last step are a project change that puts back its own files.
The round-7 D3's 'watched module paths (`Installed::modules`)' is replaced by [ADR 0114](0114-environment-inputs-are-recorded-as-read.md) (`Installed` reads through the disk it is handed).

How a `specforge.json` entry becomes a loaded extension, and how `add`, `update` and `remove` change
what is installed, had no owner. `.specforge/extensions` was derived in four places; each compile read
`specforge.json` and `specforge.lock` twice; the load policy (lock entry, hash pin, `.wasm` file entry
rules) ran inside the wasmtime adapter and parked its results on the `WasmRuntime` port
(`load_failure`, `file_entry_extension`), so it could only be tested with wasmtime; the hash and the
compile read the file separately. `update` put back what it changed, `add` and `remove` did not: a
failed add left a binary its lock refused, a failed remove a lock entry without a binary, and `add`
replaced a lock it could not read with one holding only the new entry. Doctor reported a changed
binary twice, with two remedies, one of which `add` answered "already installed".

## Decision

- **D1. One crate, `specforge-installed`.** It owns the layout (`.specforge/extensions/<name>/extension.wasm`,
  `specforge.lock`), the lock, the pin, the extension load and changes to what is installed.
  `specforge-wasm` keeps the runtime port, extension calls and the declaration loader;
  `specforge-component` is the wasmtime adapter and the builtin blobs; its loaders that put builtins into
  a runtime outside the extension load are test support (feature `testing`, D7). The crate takes builtins as a
  value and the engine as `&dyn WasmRuntime`, so it depends on neither.
- **D2. `Installed` is a project's installed extensions, its lock read once.** The environment holds
  it (`Environment::installed`); `add` and `update`, which run without a compile, read it with
  `Installed::at`. Nothing else derives an installed path.
- **D3. The environment loads its extensions through `Installed::load`.** Every environment, with
  any runtime, loads through the same policy: a builtin from its bytes, an installed extension from
  its module when its SHA-256 is the pinned one (E070 otherwise, W149 when none is pinned) and it
  declares the extension its entry names, a `.wasm` file entry from its file under the name it
  declares. The bytes hashed are the bytes compiled. Each entry's outcome is a typed `LoadFailure`
  with one diagnostic; the runtime keeps no load state, and the port is `load(name, bytes)`, `rename`,
  `unload`, `call_export`, `apply_limits`. Tests that serve an extension in process install it on disk.
- **D4. One change, all or nothing.** `Installed::change()` stages installs and uninstalls;
  `commit_with` places or moves aside each module (renames under `.specforge/extensions/.staging`),
  writes the lock once, then runs the `specforge.json` edit, and on any failure puts back every file.
  `add`, `update` and `remove` use it. A failed change reports what it could not put back (normally
  nothing) as its error's writes (ADR 0022). A change over an unreadable lock is refused (E033).
- **D5. "Installed" means verified.** `add` treats an extension as already installed only when its
  module is the pinned one, so the reinstall doctor and E070 name repairs it. Doctor reports a
  missing or changed binary once, with that command.
- **D6. Codes.** E070 is a binary that is not the one its lock entry pins; E033 keeps the lock
  file's own problems; W149 is an entry that pins no hash; W119, never reported, is retired.

## Consequences

- A failed `add` or `remove` writes nothing; MCP's `files_written` and the CLI's `wrote:` lines for it
  are empty unless the rollback itself failed.
- `check` reports an unreadable lock once (E033) when an enabled installed extension needs it, before
  the E028 of each such extension; a project that enables none still does not report it.
- A tampered or replaced binary is E070 everywhere it was E033.
- Lock files are byte-identical; the lock entry's source is typed in memory only (`LockSource`), and
  so is where an operation says an extension comes from (`Origin::Installed { source: LockSource }`).
- `specforge-installed` is a path dependency, not a workspace dependency: the root manifest's
  `[workspace.dependencies]` table is an input of the builtin blobs.

## Amendment (2026-10-08): a candidate's declaration is read through the caller's runtime

D1 put the engine behind `&dyn WasmRuntime` for the extension load, but every operation that read an
extension the project does not load yet — `add` and `update` (`Declared::of`), the builtin-peer lookup,
`init`'s starter template, `publish` and `extension validate` — built `ComponentRuntime::new()` itself:
uncached (a builtin compiled from scratch on every `add`), impossible to replace in a test (their
tests read vendored `.wasm` blobs), and three ways to read a declaration. `init` read a local file
three times and enabled a builtin without the builtins it requires (E027 on the first `check`), and the
peer lookup swallowed a load failure.

- **D7. A candidate's declaration is read through the runtime the operation is given.**
  `specforge_ops::extension::candidate` is the one module that reads an extension the project does
  not load yet: a builtin from its embedded binary, a module, or a `.wasm` file (E054 when it can't be
  read), each loaded under the candidate name and unloaded (`declaration_of`). A binary to install must
  load (E028) and must not declare a builtin's name (`extension_conflict`). `add`, `update`, `init` and
  `publish` take `&dyn WasmRuntime` from their surface, as `migrate`, `analyze` and `collect` do: the
  CLI passes `ComponentRuntime::with_user_cache()`, MCP the call's runtime (the project's, else the
  host's, else one of its own), tests `InProcessRuntime`. Ops never constructs an adapter;
  `specforge-component` is a dev-dependency of `specforge-ops` (wasmtime still reaches it through
  `specforge-project`, whose `builtins()` and `RuntimeSource::project()` are the host's).
  - Enabling a builtin enables the builtins it requires — its non-optional peers that are builtins,
    and theirs, dependencies first — in `add` and `init` alike, each read once; a builtin that does
    not load refuses the operation (E028), and a required builtin peer the embedded one does not
    satisfy, by the one peer satisfaction rule, refuses it (E027). Nothing is written either way.
  - `init`'s plan keeps the local files it read, and `apply` installs them without reading them again.
  - Rejected: a fresh runtime per MCP call (uncached, and it bypasses a host runtime tests inject); a
    static table of builtin peers (a second source for what the binary declares, ADR 0012); loading a
    builtin under its own name to read it (it would replace and unload one the served session loaded).

**What would reopen it:** two processes changing one project's extensions at once (the invariant's
"concurrent install and uninstall are serialized" is not enforced: an advisory lock on the staging
directory would be the place), or a crash between moving a module aside and placing the new one
needing automatic recovery (today the next change sweeps the staging directory and doctor reports
the missing binary).
A runtime that cannot read a candidate without disturbing what it has loaded (D7 relies on the
candidate name being loaded and unloaded in place).

## Amendment (2026-10-11): installs stage through the lock plan

D4's change is unchanged, but `add`, `update` and `init` no longer open it themselves: they stage installs
into the lock plan (`specforge_ops::extension::lock_plan`, ADR 0041 amendment 2026-10-11), which owns the
change, judges the peers once, loads what it fetched, and commits only a checked plan. `init`'s local files
are one change (they were one per file, each committed before the next was judged). `remove` still opens
the change directly.

## Amendment (2026-10-11): the project's extensions are one inventory

D3 gave the extension load one owner, but "which extensions does this project have" still had
three. `extension::list`, `doctor::diagnose` and `extension::remove` each joined what the entries
enabled, the lock pinned and the compile loaded, each with its own version precedence, and named a
source by `Origin::of` (ADR 0015 M6) except doctor's lock-only rows, which printed the lock entry's
source directly. Doctor left out an enabled entry that neither loaded nor is locked and said nothing
about status; `remove` refused that same entry ("not installed") although the listing listed it;
and `Origin::of` itself disagreed with D3's load: it named a builtin by a stray lock entry, and took
"file" from whichever entry came first where the load always takes a named entry before a `.wasm`
file entry.

- **D8. One inventory.** `specforge_ops::extension::inventory(&view)` is every extension the project
  has (enabled by a `specforge.json` entry, pinned in the lock the compile read, or loaded), one row
  per name, sorted: its origin, status (loaded, not loaded, not configured), the entries that enable
  it, its declaration, its lock entry and why its entries did not load. It is pure over the project
  view (no file is read; binary health stays `Installed::health`). The extensions listing, doctor and
  remove read it and nothing else to know the project's extensions; it also answers which loaded or
  locked extensions require a name (remove's E027) and what a `remove` argument names (a `.wasm` file
  entry by its text, path or declared name; else an extension by name; ambiguous when a file entry
  and another entry both match).
- **D9. The origin follows the load.** A row's origin is a `.wasm` file only when no named entry
  enables the name; else a builtin for a builtin's name; else the lock entry's source; else unknown,
  the order D3's load tries them. Its version is the loaded declaration's; else, for an installed
  extension, the locked one; else, for one of unknown origin, what a legacy `name@version` entry
  writes; else none.
- **D10. Doctor lists the inventory; remove acts on a row.** Doctor's extensions are the listing's
  rows, in its order, each with its status (`McpDoctorExtension.status`; `version` optional), and its
  load findings come from the rows. `remove` uninstalls a row with a lock entry (its directory named
  by the lock entry's package name, validated when the lock was read, ADR 0036) and otherwise drops
  the row's `specforge.json` entries, so an entry that does not load (E028) is removed by its name.
  An argument that names nothing is `extension_not_found`, or E072 when it is no package name; an
  entry of unknown origin under an unreadable lock is still refused, since whether it is installed
  is not known. The E027 dependents check runs only when the removed extension is loaded or locked.

Rejected: a port for where the rows come from (one adapter; the project view is already the seam,
with a compiled and an in-memory adapter); rows carrying binary health (`health()` hashes every
module: the listing and remove must not pay for it); one row per entry (the listing and doctor list
extensions by name, and remove's ambiguity rule needs the entries of one name together).

Consequences: `specforge doctor` and `specforge.doctor` list every extension the listing lists, by
name, with `status`; their load findings are in the environment's order, entry by entry (amended by ADR 0054). A `.wasm` file entry that did
not load has no version. A builtin is `builtin` even when a stray lock entry names it, and an
extension a named entry loads is never `file:` because a refused `.wasm` file entry declares the same
name. `specforge remove` and `specforge.remove_extension` remove an entry the project cannot load,
by its name.

## Amendment (2026-10-12): the change's last step

**Amended by ADR 0059:** `commit_with(config, last)` runs `last` after the lock: the `specforge.json` edit and
the user's publisher-key pins. A failure there puts every file back; `config` is changed when its bytes are.

## Amendment (round 7): one entry rule; the lock plan enables required builtins

- **D3:** which extension each `specforge.json` entry names is decided once, `Installed::resolve` (a
  builtin's name before a lock entry; `.wasm` file entries after every named entry; a file declaring a
  name already loaded refused). The extension load, the watched module paths (`Installed::modules`) and
  the lock plan's project peer set read it (ADR 0041, amendment round 7).
  **Amended by ADR 0160 (2026-10-19):** the lock plan's other questions (append, unchanged, drops, companions) read the
  same rule: a named entry by its text, a `.wasm` file entry by what its component declares.
- **D7**, first bullet: enabling the builtins an extension requires is the lock plan's (ADR 0041 B3),
  for a builtin, a local file and a registry package alike, in `add` and `init`; `candidate::
  required_builtins` is gone. A builtin that does not load still refuses the operation (E028).

## Amendment (round 8)

- **D4:** `Change` also puts back every file its last step names and removes the directories it created; a change that
  stages nothing writes no lock; `remove` no longer opens the change itself (all four operations go through the lock
  plan).
- **D8/D10:** the inventory no longer answers which extensions require a name (remove asks the lock plan).
- **D9:** a row's origin is that of the entry the load loaded it by (`Origin::of` over `Installed::resolve`), else of
  its first entry in load order (ADR 0065 D5).

**Amended by ADR 0126 (2026-10-16):** D4's last step writes the `specforge.json` entries the lock plan judged, after checking the file still holds what the plan opened with.

## Amendment (round 16): an unreadable lock is refused once

`update` read the lock itself and refused one that can't be read with its own wording ("Run `specforge add`
first", which refuses the same lock); `remove` refused it as `extension_not_found`, about its argument; the lock
plan's open refused it with a third remedy, and the lock's own diagnostic named a fourth, `specforge install`, a
command that does not exist.

- **D4:** a change over an unreadable lock is refused once: `lock_plan::writable_lock`, which the plan's open asks
  whenever the change may write the lock, is E033, `schema_mismatch`, the lock's own message and remedy, about no
  field of the request (ADR 0072), before anything is read, asked or written. `update` opens its plan before it
  reads an entry; each verb words only its own empty case (`update` over no lock file).
- **D6:** the lock's E033 carries one remedy whatever made it unreadable: "fix specforge.lock, or delete it and
  install the extensions again with `specforge add`". `check`, doctor and every refused change name it.
- **D10:** the refusal of an entry of unknown origin under an unreadable lock, and of a name the project has
  nothing under, is that same refusal.

Rejected: routing `remove` through the plan's open with a widened `uninstalls` (a name with nothing under it opens
no plan, and a readable lock would open a change that writes nothing); keeping `extension_not_found` with the
lock's remedy as its suggestion (the code would still say the extension is missing when nobody can tell).
Consequences: `specforge remove` and `specforge.remove_extension` answer E033 / `schema_mismatch` there (was
`extension_not_found`, about `name`); `specforge update` answers it with the hint (was ". Run `specforge add`
first." in the message, kind `precondition_failed`).

**Amended by [ADR 0165](0165-the-lock-a-change-leaves-is-built-when-its-plan-is-checked.md) (2026-10-19):** D4 (round 16): `writable_lock` is asked when a plan that staged is checked, and before anything is read by `update`'s open, an uninstall's open, `Names::ask` for specifiers that install and a removal only the lock could answer.
