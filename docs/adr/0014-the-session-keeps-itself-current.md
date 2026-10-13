# The project session keeps itself current; MCP calls name their target

**Status:** accepted (2026-10-05)

Amended by ADR 0024 (round-5 amendment): the reaches are Served and AnyProject; Unscoped and NewProject are targets of their own, and WritesAnyProject is gone.

Which project an MCP call served, and whether that project was current with disk, was decided in a
dozen places. Each handler resolved its `path` against a public `McpState::project_root`, picked one
of three staleness predicates (`use_cached || diagnostics.is_empty()`, `node_count() == 0`,
`loaded_at.is_none()`) and called a full environment reload when it saw fit; the dispatcher reloaded
the served project after any mutation, whatever project the mutation wrote; the router refreshed
only `tools/call` and `resources/read`, and only when `specforge watch` had written a newer
`.specforge/graph.json` (C9-07). Watch and the LSP each hard-coded "an environment change" as
`specforge.json` or any `*.wasm`. The confirmed results: `rename` with another project's `path`
planned on the served graph, edited the served project and then served the other one;
`remove_extension` checked the served project's dependents; `rename` planned on a stale graph and
left a reference dangling; `prompts/get` served a stale graph, and without watch every read did;
`validate use_cached` recompiled every project with no diagnostics; watch never reloaded on
`specforge.lock` but did on `cargo build` output, and kept its old spec root after `spec_root`
changed.

Two modules now own these questions (architecture plan 01).

## The session knows what it is built from

`ProjectSession` (`specforge-project`) classifies any changed path (`classify`, `changes`): a
`.spec` file discovery finds under the spec root is a **source**, keyed relative to it;
`specforge.json`, `specforge.lock` and the module of every extension the config enables that is not
built in are **environment inputs**; `specforge-cache.json` (when check-phase passes read it) and the
files `file_reference` fields name are **check inputs**; anything else changes nothing. `apply`
runs the one rebuild a batch needs: an environment reload, an update of the changed sources, or a
re-check. Its inputs name the directories a watcher must watch (`inputs().watch_roots()`, ADR 0030).
It can also bring itself up to
date without any watcher: `stale` compares what it last read (size and modification time, stamped
before each read) with disk, and `ensure_fresh` applies exactly that.

## Decisions

- **D1. Freshness is the session's, on demand.** Not watch's marker, not an MCP-side watcher:
  `ensure_fresh` is exact at the request boundary, needs no other process, and costs a directory
  walk and a stat per input (6.5 ms on an unchanged 1 000-file project in a test build), far below
  the full reload validate, analyze, doctor and collect paid per call. Every MCP read sees disk as of
  the request, with or without `specforge watch`.
- **D2. MCP adopts the incremental update.** ADR 0006 makes an update's diagnostics equal a fresh
  compile's, so MCP reloads the environment only when an environment input changed. In a debug build
  every MCP update is verified against a cold rebuild, and a divergence is a debug assertion. This
  amends CONTEXT.md's "every fresh compile … is an environment reload". *(ADR 0035: every session
  verifies in a debug build; MCP asserts in `applied`.)*
- **D3. `.specforge/graph.json` is gone**, writer and reader: nothing else read it, and it was not a
  graph. Watch no longer creates it; a stale one is ignored. This supersedes C9-07 (`cacded2f`).
- **D4. A mutation with another project's `path` acts entirely on that project.** It is planned,
  checked, written and reported against a project compiled for the call (`OtherProject`), recompiled
  for the returned diagnostics; *(ADR 0047: the other project is a session opened for the call and brought up
  to date after the write, not compiled again.)* the served session is never touched or reloaded.
- **D5. A `path` while nothing is served is adopted** by every tool that takes one (and `init`
  serves the project it created), so "a project is served" never depends on which tool came first.
  *(Round 6 plan 12: only a project is adopted: a `path` naming a directory that holds neither
  `specforge.json` nor `specforge.spec` is read for that call as another project, and nothing becomes served.)*
  *(Round 7 plan 01: a project is a directory holding `specforge.json`; `specforge.spec` marks nothing,
  ADR 0053 D2.)*
- **D6. Path normalization.** Canonical, then the nearest enclosing project (`specforge.json` or
  `specforge.spec`), else the directory itself; one that does not exist is `file_not_found` on
  `path`. A path inside the served project is the served project. `init`'s path is used as given and
  may not lie inside the served project. *(Round 7 plan 01: the rule is `ProjectRoot::of`, made canonical for
  MCP (`canonicalized`); it is the one every surface uses, ADR 0053.)*
  *(Round 6 plan 12: the server's own root (`projectRoot`, else the path `specforge mcp` was started
  with) is read by the first step of this rule only: the nearest enclosing project, the root included;
  with none, nothing is served. It used to be served as given, so a directory with no project, or a
  project's subdirectory, became a project of its own with default configuration, and init beneath it
  was refused as inside the current project. The served root now always holds a project, so init's
  refusal is about a real one.)*
- **D7. One refusal for "no project".** With nothing served and no `path`, `Call::project` is
  `precondition_failed`; analyze no longer runs over an empty graph with a `NoRuntime`. Tools that
  answer usefully without a project (list, stats, coverage and the other reads of the whole
  project, over the empty session; a read that names a file or an entity is the same
  `precondition_failed`, ADR 0025) keep reading the session.
- **D8. Environment inputs** are `specforge.json`, `specforge.lock` and each loaded module path,
  even outside the root; builtin blobs are in the binary. A `.wasm` no extension loads changes
  nothing. Check inputs re-run the checks without re-parsing.
- **D9. One classifier for watch, the LSP and MCP.** The watcher reports whole paths; watch re-arms
  its watchers from `inputs().watch_roots()` after any update that changes the session's inputs, not
  only a reload (and catches up with `ensure_fresh`); the LSP classifies inside the update, while it
  holds the session, and registers its `didChangeWatchedFiles` watchers from the session's inputs,
  again after any update that changed them (amended by ADR 0030). *(ADR 0035: each then catches up on
  what changed while its watchers moved, and the LSP's follow is one reaction every change goes
  through.)*
  *(ADR 0073: there is no classifier; a batch of paths narrows the session's stale comparison, and a
  directory reaches every source and input under it.)*
- **D10. No public root.** `McpState::project_root()` is the session's root. MCP serves only a
  project opened from disk (amended by ADR 0025). An `initialize` root that does not exist serves
  nothing.
- **D11. `use_cached`** (validate, analyze, doctor) skips bringing the served project up to date,
  whatever its diagnostics. With nothing served there is nothing cached, so a call with a `path`
  adopts and compiles it. ADR 0004 D3-d's "compiles fresh" now reads "brought up to date with disk,
  with a fresh compile's diagnostics" (ADR 0006 guarantees the equality).
- **D12. Which methods refresh**: `tools/call` (per its tool's target), `resources/read`,
  `prompts/get` and the three list methods, since an environment change changes the extension tools
  listed. `resources/templates/list`, `ping` and subscriptions do not.

## The MCP call target

`specforge_mcp::target` resolves each call before its handler runs. Every tool, resource and prompt
entry declares a `TargetSpec`: its reach (`Unscoped`, `Served`, `AnyProject`, `WritesAnyProject`,
`NewProject`) and freshness (`Fresh`, or `FreshUnlessCached` for validate, analyze and doctor).
`resolve` turns the call's `path` and that spec into one `CallTarget` (`Served`, `Other`, `New`,
`Unscoped`, `NoProject`), and handlers read their project through `Call::project()` as a
`ProjectRef` (root, spec root, environment, graph, runtime, `view()`, `diagnostics()`), unable to
tell the served session from a project compiled for the call *(ADR 0047: from another project's session)*. A mutation's handler returns what it
wrote (ADR 0022); `mutation::refresh`, called by the dispatcher alone, brings the target up to date
after any call that wrote files, succeeded or not, with exactly what changed.

## Consequences

- ADR 0006 is the mechanism; nothing conflicts with it. ADR 0004 D4-c's `ttlMs: 0` stays correct:
  "any request may recompile" reads "any request brings the project up to date".
- `specforge watch` reports a new `rechecked` event (same fields as `rebuilt`) for a check input,
  and reloads on a lock change.
- Racy files: a file modified within 2 s of being stamped also records its content hash, compared
  when its stamp is unchanged, so a same-second same-length rewrite is still seen and an unchanged
  file is not reported. A file with a future modification time stays racy (hashed per request)
  until the clock passes it.
- Rollback is one line: MCP's `ensure_fresh` can call `reload_environment` whenever `stale()` is not
  empty, without touching the API.
