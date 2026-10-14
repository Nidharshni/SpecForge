# One reaction to a session update; one debounce rule

**Status:** accepted (2026-10-08)

What a project session is built from depends on its sources, not only on its environment: a
`file_reference` field or a `file_exists` rule names files, and both the watch roots and the LSP's
watcher globs include them. Watch and the LSP each decided for themselves when to look again at what
they watch, and both looked only after an environment reload (ADR 0014 D9). ADR 0030 made the session
say when its inputs changed (`Update::inputs_changed`) and both followed it, but neither caught up on
what changed while its watchers moved: watch did, the LSP never did, so a file written between an edit
that named it and the client acknowledging the new registration stayed unseen until something else
changed it. The reaction itself was written in each surface: the LSP repeated the follow-up and the
semantic-token refresh in five handlers and its reparse worker, and the watch loop lived in the CLI
binary, testable only by spawning it. The debounce rule was written twice, once in `specforge-watch`
on a std channel and once in the LSP on tokio. A session verified its updates in a debug build whichever
surface held it (ADR 0032), but each surface reported a divergence its own way, and MCP asserted in
`ensure_fresh` only.

## Decisions

- **D1. An update says whether its watchers must follow, and how it diverged.** `Update::inputs_changed`
  (ADR 0030) is the fact that what the session is built from moved; "the environment loaded again"
  stays `UpdateKind::Environment`; `Update::divergence()` is how the incremental graph differs from a
  cold rebuild, when it was verified and does. No separate reaction type restates them.
- **D2. A watching surface follows, then catches up.** After an update that changed the inputs (or
  loaded the environment), watch re-arms on `inputs().watch_roots()` and the LSP re-registers its
  globs. Each then brings the session up to date with disk (`ensure_fresh`; the LSP leaves out an open
  document whose file exists, because its buffer is the truth), and repeats while the catch-up moves
  the inputs again, at most eight times. MCP watches nothing and ignores it. This amends ADR 0014 D9:
  the catch-up is part of following.
  *(ADR 0046: the session leaves out every file an editor buffer holds, whether or not it exists; the LSP's
  catch-up is `ProjectSession::stale` as it is.)*
- **D3. Each surface reports a divergence where it reports.** Watch prints it in its event, the LSP
  logs it at ERROR, MCP debug-asserts in `McpState::applied`, the one place every update of the served
  project passes. A session-level assertion was rejected: it would crash watch before its event says
  what diverged, and kill the LSP's worker task silently. (Whether a session verifies is ADR 0032's.)
- **D4. One debounce rule.** `specforge_watch::Coalescer` holds it and reads no clock: a change
  restarts a 50 ms window, and the batch is the sorted set of changes. `Debouncer::coalesce` runs it on
  a std channel (watch's) and `Debouncer::coalesce_async` on a tokio one (the LSP's reparse worker,
  feature `tokio`). The spec's "configurable" window and `watch_debounce_ms` were never built and
  leave the spec.
- **D5. `specforge-watch` keeps a session current.** This amends ADR 0006 ("`specforge-watch` is the
  file watcher and its debounce"). It is the file watcher, the debounce rule, and `SessionWatch`, the
  loop `specforge watch` runs: apply a batch, follow, catch up, name the changed paths. Its seam to the
  file system is `Watchers`, with `Notify` (one `SpecWatcher` per root, every path they report debounced as one
  stream, so one burst across roots is one batch) in production and a recorder in
  its tests. The CLI renders `SessionWatch`'s events. The crate now depends on `specforge-project`.
  Deleting it would put the loop back in a binary and leave the LSP's debounce rule without a home, so
  it stays.
- **D6. The LSP has one reaction.** `Reaction::react` applies a change, publishes, follows the
  session when its inputs moved (registering the globs, then `Change::CatchUp`), and last asks the client
  to refresh its highlighting. The reparse worker and every handler that changes the session call it
  and nothing else; none decides these steps for itself.

## Consequences

- A file a source begins to name is watched by watch and registered by the LSP, and a file written while
  the watchers move is seen: a watch event can be followed by a catch-up event after such an edit
  (user-visible).
- `specforge_lsp::DEBOUNCE_WINDOW` is gone; the LSP builds its debouncer from
  `specforge_watch::DEFAULT_DEBOUNCE_WINDOW`.
- The watch loop's re-arm, catch-up and naming are tested without a process
  (`crates/session/watch/tests/session_watch.rs`), and the debounce rule without a clock
  (`crates/session/watch/tests/coalesce.rs`).
- A failed move of the watchers is reported once per set of roots, and the session still catches up.

## Rejected

- **Each surface compares its own watch set after every update**: the rule twice, and choosing when
  to compare was the bug.
- **A session-driven loop over a watcher trait**: the LSP's registration is an async request
  answered by the client after the blocking update, and MCP has no watcher.
- **Deleting `specforge-watch`** (review 2026-10-07 c11): about 140 lines moved into the CLI,
  concentrating nothing.
- **A configurable window**: it would change on a reload for both surfaces, and nothing asked for it.

## What would reopen it

A third watching surface, or a client protocol that lets the LSP watch directories instead of files
(then its catch-up could use watch's roots).
