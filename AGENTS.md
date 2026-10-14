# AGENTS.md

## Agent skills

Matt Pocock engineering skills are installed at `~/.agents/skills/` (user scope, not in this repo):
`to-spec`, `implement-spec`, `to-tickets`, `triage`, `grill-me`, `grilling`, `grill-with-docs`,
`domain-modeling`, `wayfinder`, `tdd`, `code-review`, `improve-codebase-architecture`,
`diagnosing-bugs`, `handoff`, `pr`, `retro`, and others.

Project skills (the `.spec` DSL authoring skills `specforge-*`, the architecture and diagram
skills) live in `.claude/skills/`.

Configuration (created by `/setup-matt-pocock-skills`):

- **Issue tracker**: GitHub Issues on `leaderiop/SpecForge` via the `gh` CLI — see `docs/agents/issue-tracker.md` (includes "Wayfinding operations" for `/wayfinder`)
- **Triage labels**: default vocabulary (`needs-triage`, `needs-info`, `ready-for-agent`, `ready-for-human`, `wontfix`) — see `docs/agents/triage-labels.md`
- **Domain docs**: single-context layout (`CONTEXT.md` + `docs/adr/` at repo root, created lazily by `/domain-modeling`) — see `docs/agents/domain.md`
- **Architecture rounds**: the explore → plan → implement method and its agent briefs — see `docs/agents/architecture-rounds.md`
- **Re-vendoring builtins**: when a change touches what the extension blobs compile from — see `docs/agents/revendor.md`
- **Planning repository**: rounds, strategy, archived plans and experiments live in the private
  `leaderiop/SpecForge-planning`, checked out at `../SpecForge-planning`. This repository tracks only what ships
  and what builds it; `cargo gate`'s `root-layout` and `doc-links` checks hold that — see `CONTRIBUTING.md#where-things-are`

Run engineering skills before assuming workflow; they read this block and the files above.
