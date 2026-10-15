# The follow loop is written once, over a Follower seam

**Status:** accepted (2026-10-15, round 10, plan 08). Amends ADR 0035 D2 (the entry condition) and reopens its
rejected "session-driven loop over a watcher trait".

After an update that moved the session's inputs, a watching surface moves its watchers, catches up on what changed
while they moved, and repeats while that moves the inputs again, at most eight times. The rule was written twice:
`SessionWatch::follow` in watch, and `Reaction::follow` with `sync_watchers` in the LSP. The copies had drifted:

- After a refused registration, watch remembers the refused set and stops; the LSP fell back to the static
  watchers and so asked the editor for the same refused set again on every round (eight times in one reaction when
  each catch-up reloaded the environment), and dropped the refusal's reason.
- The LSP entered the loop after `environment || inputs_changed`, watch after `inputs_changed`; after a refusal an
  environment reload with unchanged inputs made the LSP ask again.

`incremental.spec` says a refusal is reported and the session still catches up, and ADR 0035 says once per set.

## Decision

- D1. `specforge_watch::Following<W>` is the loop, over a `Follower` trait (`wanted`, `watch`, `refused`,
  `catch_up`) generic over the watched value: watch's `Vec<WatchRoot>`, the LSP's `Vec<FileSystemWatcher>`. The cap
  of eight is private to `follow.rs`. Each surface implements `Follower` over its existing port (`Watchers`,
  `Editor`).
- D2. The refusal rule is watch's: a set refused once is not asked for again in the same follow, the refusal is
  reported, and the session still catches up. What is watched after a refusal is the surface's (`Follower::refused`
  returns it): watch keeps what was watched, the LSP falls back to the static watchers. The memory is per follow;
  the next update that moves the inputs asks again.
- D3. The LSP reports a refusal as a `window/logMessage` at WARNING, sent after it registers the static watchers.
- D4. The one entry condition is `Update::inputs_changed`. `Applied::rewatch` and ADR 0035 D2's "(or loaded the
  environment)" go.

## Consequences

`specforge_watch` gains `Follower` and `Following`; `specforge_lsp::changes::Applied::rewatch` is removed. The LSP
logs a WARNING when its editor refuses the project's watchers, no longer re-asks a refused set, and sends no
registration after a reload that leaves the inputs as they were. The loop is tested without a session or a client.

## Rejected

- Fixing the LSP in place: it keeps two copies of a rule that already drifted once.
- Closures for `wanted` and `catch_up`: both borrow the session, one mutably.
- `Watchers<W>` implemented by `Editor`: ADR 0043 rejected it, and it cannot express the LSP's fallback or its
  different catch-up. `Watchers` stays watch's.
- The loop as a method of `ProjectSession`: `Watchers` would move down a crate.
- Refusal memory per session: watch documents that the next batch tries again.

## What would reopen it

A surface whose watchers cannot answer synchronously, or a third catch-up shape.
