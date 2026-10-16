# A session's inputs are one value

**Status:** accepted (2026-10-07)

ADR 0014 made the project session the one judge of what a changed path is. What the session
depends on, though, was rebuilt in five places from the same three helpers: the stamp taken at open,
the stamp taken before the checks, the classifier, `watch_roots` and the LSP's `file_watchers`, which
reached into `Environment::inputs` and `Environment::named_files` to do it. The copies drifted. The
LSP never watched the directory of a missing referenced file (the classifier counts a file created
there, since it changes E016's suggestion) and registered `spec/../docs/guide.md`, a glob no client
matches against the path it reports. Watch dropped any input directory that did not exist yet. Both
adapters re-derived what to watch only after an environment reload, so an edit that named a new file
outside the watched directories was never followed. Opening a session read `specforge.json` three
times, the first time inside the extension runtime and before any stamp, so a config written while
the runtime loaded was recorded as already read: the session served the old extensions and a
spurious E028 until the next unrelated reload, through watch and MCP alike (`ensure_fresh` saw
nothing stale). `ProjectSession` carried two hidden modes, `Origin` and `owns_runtime`, in thirteen
and five places.

## Decisions

- **D1. One value.** `specforge_project::SessionInputs` holds the spec root and `exclude`, the
  environment inputs (config, lock, each loaded module) and the check inputs (build cache, each file
  a `file_reference` field or a `file_exists` rule names, each missing such file's directory).
  `ProjectSession::inputs` returns it. `classify`, `changes`, `watch_roots` and `watched` answer from
  it, and the session stamps and discovers through it. `Environment::{inputs, check_inputs,
  named_files, referenced_files}`, `EnvironmentInputs`, the classifier and the session's own
  `classify`/`changes`/`watch_roots` are gone.
  *(ADR 0073: `classify` and `SessionInputs::changes` are gone; `ProjectSession::changes(paths)` is
  the stamp comparison narrowed by the paths. The value also holds each symlinked source's
  target.)*
  *(ADR 0106: the session stamps and discovers through its read ledger, `specforge_project::ledger`, which owns this value and the stamps.)*
  *(ADR 0055: complete. Import resolution reads no file, so a `.spec` file discovery does not find is no
  input of any kind.)*
- **D2. When it changes.** It is made when the environment loads and renewed each time the checks
  run. The checks' reads define the check inputs, so an update that skips the checks keeps the
  previous set. `Update::inputs_changed` says the value changed. Watch re-arms and the LSP registers
  its watchers again after any such update (amends ADR 0014 D9's "after a reload").
- **D3. One read, stamps first.** An environment load stamps `specforge.json`, reads it once, stamps
  the lock and every module, then builds the extension runtime and the environment from that one
  read (`RuntimeSource`: built per load from the config read, or fixed). A file written during the
  load is seen by the next `stale()`. The lock is still read by the runtime and by the environment,
  both after its stamp. `owns_runtime` is gone.
  *(Round 6, plan 06: `Environment::from_read` reads the lock and runs the extension load, then
  `Environment::assemble` builds the environment from those parts; ADR 0047 "An environment
  assembles from its loaded parts".)*
- **D4. Spelling.** Classification compares canonical paths. Watch roots are canonical (what the file
  watcher reports). The LSP's watchers name each input as the checks read it (OS resolution of
  `..`), re-spelled under the project root as the editor gave it, absolute outside it.
  *(ADR 0073: a source path is compared by its directory's canonical form and its own name, so a
  symlink is keyed by itself.)*
- **D5. Detached.** A detached session's inputs are empty: a `.spec` path is a buffer source keyed
  by itself, nothing is watched, stamped or stale. `Origin` is gone. ADR 0025 D4 stands.
- **D6. Missing directories.** An input directory outside the root that does not exist is watched
  from its nearest existing ancestor, non-recursively. A path created there on the way to it is that
  input's change (a check input's or a module's; the environment's for the spec root). The project
  root itself must exist.
- **D7. Watch catches up at start.** After arming its watchers, watch brings the session up to date
  before it reports ready, as it already did after a re-arm.

## Consequences

- A new input kind is one change in `inputs.rs`. Watch, the LSP and the stamps follow it.
- The LSP's registered globs change spelling (`{root}/docs/guide.md`, not `{root}/spec/../docs/guide.md`)
  and gain `{dir}/*` for a missing referenced file's directory (user-visible).
- Watch's `ready` reports disk as of when its watchers were armed (user-visible).
- One-shot compiles (`CompiledProject`, the CLI) still read `specforge.json` in the runtime and in
  the environment. They keep no stamp, so a disagreement cannot outlive the command.
- Amends ADR 0014 D9 and its paragraph on `watch_roots`.
