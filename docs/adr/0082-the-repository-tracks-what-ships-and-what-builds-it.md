# The repository tracks what ships and what builds it

**Status:** accepted (2026-10-14, structure round, plan 02)

## Context

`leaderiop/SpecForge` is public. By 2026-10-14 its top level held, beside the product, implementation plans and
PRDs from April to June, two April trackers, a plugin-runtime study with 125 persona statements, a wayfinder
scratch tracker, a 125-engineer roster, a business plan with financial projections and pricing, a pitch deck, a
spike, and a hand-regenerated DOT export of the repo's own graph. It also held three research pages the owner
meant to keep local, swept in by a merge. None of it shipped, built or tested anything. It was 428 of 2,388
tracked files, 369 of 645 Markdown files, and 255 of the 283 Markdown files a crate-path rename would rewrite.
CLI renames were editing the business plan and the pitch deck. No check judged Markdown links (17 were broken),
and nothing kept the top level from growing.

## Decision

D1. The repository tracks what ships (the crates, the builtin extensions, the integrations, the schemas,
install.sh, the README and licences), what documents it (`docs/`, including `docs/vision/`, `spec/`, `examples/`,
the entry documents) and what builds, tests or checks it (`gate/`, `xtask/`, `scripts/`, `fixtures/`,
`evaluation/`, the toolchain and agent configuration). Nothing else is tracked.

D2. Planning rounds, strategy, archived plans, studies and experiments live in `../SpecForge-planning`, a plain local folder that is not version-controlled
(amended 2026-10-15: it was first planned as the private repository `leaderiop/SpecForge-planning`; the owner
decided planning needs no version control): round folders at its top, `archive/`, `strategy/`, `labs/`,
`research/`. A round's `.planning/` folder stays gitignored here and is copied there when the round merges.

D3. `cargo gate` keeps both rules: `root-layout` compares the tracked top-level entries with the reviewed list
`gate/src/root_layout.rs` `ROOT` (each with its reason; CONTRIBUTING lists the same), and `doc-links` requires
every relative link in a tracked Markdown file to name a tracked path. Both run in the pre-commit hook.

D4. Generated output (`specforge-report.json`, `specforge-graph.dot`) and per-user settings
(`.claude/settings.local.json`) are ignored, never tracked. `docs/research/` tracks the round reports
(`architecture-review-*.html`) only.

## Rejected

- `internal/{planning,strategy,labs}` in this repository: the repository is public, so "internal" would publish
  what it names as private, and the material would keep taxing every path rename and CLI sweep.
- Deleting the material and relying on history: the business plan and the pitch deck are live documents.
- Ignoring `docs/research/` whole: it holds tracked round reports.

## Consequences

- Removing a file from `main` does not unpublish it: the public history keeps it. Purging history is a separate
  owner decision.
- Links from the archive into this repository may break. The archive is a record.
- A new top-level entry is a reviewed edit of `ROOT` and CONTRIBUTING.
