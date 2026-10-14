# Tests belong to extensions: `@specforge/testing` plus one extension per test runner

**Status:** accepted

Test concepts are spread across the core compiler today. `verify` is core grammar. The `analyze`
coverage pass, the `specforge-report.json` shape and the `collect` normalizer are hard-coded to
`verify`/`tests` field names and to the Rust integration's `target/specforge/*.json` output.
`@specforge/software` marks five kinds testable and owns W004/W009. `collect --collector` is only a
label: nothing is auto-detected, no extension is invoked, and the user must run the test runner
separately and in exactly the way SpecForge expects. That breaks the zero-domain-core rule, and it
makes "which runner, which report, which linkage" a core concern instead of an extension's.

We move all test vocabulary out of core and out of `@specforge/software`:

- **`@specforge/testing`** (runner-agnostic builtin) owns the `verify` statement field and its
  kinds, testability of kinds, W004/W009, the coverage analysis (A001/A002/A011–A014; A011 was later dropped as a duplicate of W003, and A015/A016 added) and the
  normalized test-result format. Core keeps `verify [kind] "..."` only as reserved syntax, like
  `method`: every meaning comes from the registry, and on a kind no extension made testable
  `verify` is an unrecognized field (W020). A fully generic `keyword [ident] "string"` statement was
  considered and rejected: its bare form `verify "..."` is indistinguishable from an ordinary string
  field before the registry exists, so the parser, resolver, formatter and editor tooling would all
  need registry-aware merging for no user-visible gain.
- **One extension per test runner** (`@specforge/cargo-test` and `@specforge/vitest` first;
  `@specforge/pytest`, `@specforge/jest` later) peer-depends on `@specforge/testing` and contributes
  a *collector*: how to detect the runner, the command that runs it, and a `collect__<runner>`
  export that maps the runner's native report to entity results. Several runners can be enabled in
  one project without field conflicts, because none of them adds fields of its own.

**Execution.** This supersedes "SpecForge never executes tests" (README, `docs/vision/README.md`,
`docs/vision/north-star.md`). The compiler still never executes anything, and extensions stay pure
wasm with no process, file or network access. What changes is that a runner extension may
*declare* one command in its manifest, and `specforge collect` runs it on the extension's behalf.
The user sees the exact command and consents once per project; consent is keyed by extension and
command, so a changed command re-prompts, and `--yes` covers CI. The host then hands the report
bytes to the extension's pure `collect__<runner>` export. This is the pattern `prove` already
uses to run z3. We rejected a guest-side `exec` host import: it would open a real hole in the
sandbox and need a new permission model, and nothing requires the guest to choose the command at
runtime. `collect --no-run` parses an existing report without running anything.

**Linkage.** A test states which entity (and optionally which `verify` obligation) it proves, in
the runner's own idiom: vitest test metadata, and in Rust the `#[specforge_test(behavior = "…",
verify = "…")]` attribute, which now registers the test itself instead of requiring a separate
`#[test]`. Spec files carry no test paths, so they don't rot when tests move. The spec-side
`tests [...]` field is retired: linkage is whatever the collected results prove, which also
removes the "declares verify obligations but no tests linkage" advisory that fired even after a
successful `collect`.

**Consequences.** This is a breaking change for any project that uses `verify`: it must enable
`@specforge/testing`, which `init` does whenever it enables `@specforge/software`. This repository's
own corpus and roughly 3,000 annotated tests migrate with it. Rust's stable libtest has no JSON
output, so `@specforge/cargo-test` keeps reading the per-test report that the attribute writes,
rather than nextest JUnit.

**Where the coverage rule lives** is amended by [ADR 0004, D2-f](0004-architecture-migration-decisions.md):
it is the pure crate `specforge-coverage`, owned by `@specforge/testing` and linked both by its Wasm
`coverage` pass and by host surfaces, which compute coverage status with no extension input (P2 in
`provide_mcp_coverage_tool`, `spec/behaviors/mcp-tools.spec`).
