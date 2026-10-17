# Reopening a project in place is one session operation

**Status:** accepted (2026-10-16, architecture round 11, plan 06). Amends ADR 0043 D7, ADR 0046 D3 and ADR 0047 D6. Amended by [ADR 0136](0136-every-session-update-goes-through-one-bracket.md) (D2: the skipped-checks flag is the compiled project's, so a reopen's new project starts without it).

Three places opened a project at a root in the place of a session that already existed, each with its own copy of the
steps: `ProjectSession::reload_environment`, the LSP's reaction to `initialized`, and MCP's `McpState::serve`. The
copies disagreed on what survives and on whether the caller gets an `Update`:

| | environment reload | LSP open | MCP serve (other root) |
|---|---|---|---|
| Held buffers | carried | carried | not carried (intended: MCP holds none, ADR 0046 D6) |
| Verify flag | carried | reset to the default (no effect: the replaced session is detached) | reset (no effect: MCP never sets it) |
| Runtime source | the session's own | the reaction's | picked by `McpState::open` |
| The caller gets an `Update` | yes (`replaced`) | no: `Plan::apply` made `environment`, `changed` and `inputs_changed` up | yes (`replaced`) |

No live defect followed: every difference was intended or had no effect. But each fix to what an open carries (the
held buffers, the stamps before the read) had to touch the session and the LSP's copy, and the session's public
interface carried five names (`begin_open`, `OpeningProject`, `finish_holding`, `replaced`, `into_buffers`) that
existed only so surfaces could rebuild the operation.

## Decision

- **D1.** `ProjectSession::reopen(root, source, loaded) -> Update` is the one way to open a project in a session's
  place, and the one way to open in two steps. The environment loads first, `loaded` receives it before any `.spec`
  file is read, then one cold build reads every source.
- **D2.** Carried over: the held buffers (keyed again by the new environment) and whether the session verifies its
  updates. Not carried: the inputs, the stamps, the skipped-checks flag and the old runtime source (later reloads use
  the `source` given).
- **D3.** The update is the replacement: kind `Environment`, every file of the new graph rebuilt, the graph delta,
  `inputs_changed` when the inputs differ, no verification (nothing incremental ran).
- **D4.** `begin_open`, `OpeningProject`, `finish_holding` and `replaced` are private. `into_buffers` is gone.
- **D5.** `reload_environment` is a reopen at the session's own root with its own source. MCP's `serve` is a reopen
  with `McpState::source()`. The LSP's open is a reopen whose hook shows the environment to its readers. The LSP
  publishes at an open what it published before (U1), and follows after an open whatever the update says, because the
  open put the editor back on the static watchers.
- **U1. What the LSP publishes when it opens.** The files with diagnostics and those published before: the open
  records the real update but adds none of its rebuilt files to what it touches. A file never published needs no
  clearing. Recording them would send one empty `publishDiagnostics` per clean file at startup.

## Rejected

- **D1 (b)** two names (`reopen`, `reopen_showing`) for one operation. **D1 (c)** a `Reopening` guard the caller
  drives: it brings back "remember to carry X".
- **D2 (b)** a detached session carrying its surface's source. **D2 (c)** always `self.source`: a detached session's
  source loads no extension, so the LSP and MCP would lose theirs.
- **U1 (b)** recording the open like a reload: several hundred empty notifications for this repository.

## What would reopen it

A surface that must keep stamps or inputs across a reopen, or one that holds buffers it must not carry (ADR 0046's
reopen condition).
