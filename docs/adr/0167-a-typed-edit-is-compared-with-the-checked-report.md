# A typed edit is compared with the checked report

**Status:** accepted (2026-10-19, round 17, plan 03). Amends [ADR 0151](0151-a-spec-file-is-changed-by-a-typed-edit.md) D3 (what the edited project is compared with) and [ADR 0136](0136-every-session-update-goes-through-one-bracket.md) D4 (who may read the skipped-checks state). Keeps [ADR 0032](0032-one-graph-build.md)'s cold build for the edited compile and [ADR 0158](0158-an-edited-compile-and-a-migration-read-no-source-past-the-bound.md) D1.

## Context

E080 refuses a typed edit when the edited project reports an error the project does not. The edited compile
(`CompiledProject::edited`) always runs every check. The project's own report does not, after the editor's typing fast
path (`CheckMode::SyntaxOnlyIfParseErrors`) skipped them while a buffer did not parse. So the two sides were measured
differently: with one open buffer that did not parse, every check error the project already had looked new, and a
stub fix over a file the edit does not touch was withheld, or refused on resolve (reproduced with the real
`specforge-lsp`: a "Create feature stub" listed with all buffers parsing, nothing while another buffer did not). The
stub fixes are typed edits since ADR 0161, which is when the skipped state first met E080.

The review also proposed making the edited compile an incremental apply on a copy of the project, because it rebuilds
cold. Measured on this repository's spec (187 files, debug build): the edited compile is 1,030 ms; parsing is 21% of
it, the graph build 17%, the checks 61%. A copy plus one incremental apply is 987 ms (-4%), and 1,878 ms (+82%) with
the verification a session's project carries in every debug build, because `GraphBuild::apply` declares every file and
links the whole graph again, then computes a delta over every edge.

## Decision

- **D1.** A typed edit is refused with E080 when the edited project reports an error the project's **checked report**
  does not. `CompiledProject::checked_diagnostics` is that report: `diagnostics()`, or, after an update that skipped
  the checks, that report with every check run over a copy. The project keeps skipping them; no caller learns the flag.
- **D2.** The edited compile stays ADR 0032's cold build from what the project holds (ADR 0158 D1). It is not an
  incremental apply on a copy.

## Rejected

- The edited compile as a copy plus `apply` (D2's numbers; it also needs a verification flag switched off).
- The edited compile mirroring the skip: weaker validation while typing, a stub that leaves a check error is written.
- A public `checks_skipped`, with the caller re-running the checks (against ADR 0136 D4's placement).
- Offering no typed stub while the checks are skipped: hides fixes exactly when the user types.
- Merging `Environment::holds_source` and `SessionInputs::excludes`: both delegate to `SourceRoot::holds`, and they
  differ on purpose for a detached session.

## Consequences

CLI and MCP never skip the checks, so nothing changes for them. In the LSP a stub is offered, and resolves, while
another open buffer does not parse. Each plan made in the skipped state runs the checks twice (the base and the edited
compile); the base is not memoized, because the state lasts only until the next update that parses.

## What would reopen it

A release preview measured above 100 ms on a real project; `GraphBuild::apply` becoming proportional to the changed
files (then the incremental edited compile gains what the review hoped); a second surface that skips the checks.
