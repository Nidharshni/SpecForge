# A compiled project's root answers rooted or detached

**Status:** accepted (2026-10-16, architecture round 12, plan 10). Amends [ADR 0047](0047-the-compiled-project-is-the-sessions-core.md) D4 (`root()` is the one answer; the project root is the environment's) and [ADR 0108](0108-a-project-view-comes-only-from-a-compiled-project.md) D4 (an assembled project is rooted at a directory, not a project root).

"Is this project on disk, and where?" had eight spellings in five crates. Two accessors of one value disagreed:
`CompiledProject::root()` and `CompiledProject::project_root()`. For an in-memory test project
(`testing::Assembled::compiled()`), `root()` was `Some(dir)` and `project_root()` was `None`. The disagreement was
latent: the only production readers of `project_root()` were two lines in MCP, and they only see sessions opened from
disk.

The review asked for one fact that every accessor derives from. There are two facts:

- **F1: is the compiled project rooted or detached, and at which directory?** Stored as `on_disk`, read through
  `CompiledProject::root()` (ADR 0047 D1, D4).
- **F2: which project root was the environment loaded at?** `Environment::project_root()` (ADR 0053 D1: "A session
  keeps the root it was opened at"). It is read only to reopen a session and to hand an operation a `ProjectRoot`.

Folding F2 into F1 fails on the fixture, for two reasons:

- A `ProjectRoot` is located by one rule (`ProjectRoot::of`: the path must exist, walk up to the first
  `specforge.json`, symlinks resolved), and there is no constructor that takes a directory as given. The fixture is
  rooted at directories that rule never returns: `/p` (which does not exist) and a subdirectory of a configured
  project. Production cannot root at either (`check --path /p` is `file_not_found`; `doctor --path proj/sub` roots at
  `proj`).
- ADR 0047 D4 has the fixture's detached project share the rooted project's environment. If the environment owned F1,
  the fixture would need a second environment, and `Environment` is not `Clone` (`RegistryBuild` is not).

## Decision

- **D1. `CompiledProject::root()` is the one answer to rooted or detached** (F1, stored as `on_disk`). `None` means
  detached: no import is resolved, no report, schema record or anchors file is read, and an operation that needs the
  disk refuses with `no_project`. For every compiled project production makes, `root()` equals
  `environment().project_root()`'s directory.
- **D2. `Environment::project_root()` is where the environment was loaded** (F2). `CompiledProject::project_root` is
  gone; MCP names the fact it wants, `session.project().environment().project_root()`.
- **D3. An assembled project is rooted at a directory, never a project root.** Its environment was loaded from none:
  `environment().project_root()` is `None` while `root()` is `Some(dir)`. Nothing that needs a `ProjectRoot` reaches it.
- **D4. The session, the LSP and I002 ask F1.** `ProjectSession::{reload_environment, apply}` ask the compiled project,
  not the ledger; the LSP's go-to-definition on a `use` path asks `view().root()`, not whether the spec root is an
  empty path; I002's unreachable "no project" wording (an empty installed root) is deleted. The ledger's own gates
  stay (whether it stamps is its concern).

## Rejected

- **The environment owns `Option<ProjectRoot>` and `on_disk` goes** (the card). It needs a kernel constructor that
  takes a directory as given, against ADR 0053 D1, and a second environment for `Assembled::detached()`.
- **The compiled project owns the root and the environment loses it.** `of(root, env)` and `unread(root, env)` would
  take a root that must agree with `env.installed.root()`: the "`root` parameter beside `installed` (two values that
  must agree)" ADR 0047's amendment rejected.
- **Repairing `project_root()`** (read `env.project_root()` without the `on_disk` gate). It would agree for the
  fixture, but a compiled project would still carry two accessors for two facts.
- **`Environment::root()` as `Option<&Path>`.** Every production reader runs on a rooted project; `Option` adds an
  `unwrap_or` at each and gives the fixture's environment a new split.

## Consequences

- Two stored facts remain for a session's project. They agree by construction in `read`, `detached` and `unread`, and
  `tests/rooted.rs` pins it.
- `specforge-project`: `CompiledProject::project_root()` is removed (pre-release; no shim). No reply, diagnostic or
  exit code changes.

## What would reopen it

An `Environment` that is `Clone`, together with a `ProjectRoot` taken as given: then the environment owning the root
is cheap. Or an operation unit-tested through `Assembled` that needs a `&ProjectRoot`.
