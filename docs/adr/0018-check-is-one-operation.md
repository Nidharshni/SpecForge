# Check is one operation

**Status:** accepted (2026-10-05). Amended by ADR 0132: `check` returns `Result<CheckOutcome, OpError>`; `parse_severity` and `parse_lint_profiles` return `Refusal<CheckField>`.

What `specforge check` reports was decided in two places. The CLI decided it in
`cli/check.rs::run_in`, MCP in `tools/validate.rs::call`. Each built a `DiagnosticPolicy`, applied it
and then did its own extra step: the CLI computed an exit code and recorded the build cache
(`specforge_project::record_build_cache`), MCP filtered by severity with a hand-written `match`. The
shared part, the policy, was extracted earlier (P8, `a67eaac4`); the orchestration around it drifted
four times:

- MCP accepted any `severity_filter`, including `"Error"` (how the payload itself spells severities)
  and `"errors"`, and returned every diagnostic for them.
- Both surfaces accepted any lint profile. The CLI broke `exit_code_reflects_diagnostic_severity`
  ("a value outside a flag's allowed set MUST be rejected … with exit code 2"), and the documented
  `--lint=pedantic` did nothing: infos are always reported.
- MCP's answer had no verdict. Under `severity_filter: "info"` an agent saw two infos and
  `isError: false` for a project with two errors, where the CLI exits 1.
- "Only a clean check records the build cache, judged after strict" was tested only through the
  binary and only for errors; nothing tested that no other surface writes the cache.

Watch and the LSP apply no policy: they publish the compile's diagnostics, which the parity harness
holds equal to `check` with default options. So the operation has two callers; its case is locality,
not reuse.

## The operation

`specforge_ops::check::check(&ProjectView, reported, &CheckOptions) -> Result<CheckOutcome,
CheckError>`. `reported` is what the compile of the view reported (the surface's call target decides
which compile counts, ADR 0014). The operation applies the policy (lint profiles, then strict), takes
the verdict over everything reported (`CheckOutcome::ok`: no error), counts severities (`Counts`,
also used by `stats` and `watch`), says which diagnostics the severity filter shows (`shown()`), and,
when asked, records the build cache and returns what became of it as data (`CacheRecord`).
`parse_severity` and `parse_lint_profiles` are the one place argument names are read. The surfaces
keep presentation: the CLI maps `ok` and `CacheRecord::WriteFailed` to its exit code and words the
cache note; MCP maps the outcome to the diagnostics array and a `_meta` verdict. ADR 0004 D1-c still
holds: two presenters, one orchestration.

## Decisions

- **D1. The operation owns the cache write; MCP validate never records the build cache.** The rule
  (the check passed under the policy, whatever the filter shows) and the write sit together and are
  tested in-process. MCP passes `record_cache: false` and takes no cache argument: the build cache is
  opt-in and the CLI's (`write_build_cache.opt_in`), the tool is read-only, and a validate in an
  agent loop must not move CI's committed baseline. The parity harness records it as the
  `check_cache` Files divergence. `specforge_project::record_build_cache` is deleted.
- **D2. One severity filter, on both surfaces.** `CheckOptions.severity`; the CLI gains
  `--severity <error|warning|info>`, MCP keeps `severity_filter`. Names match ignoring ASCII case,
  so `"Error"` works. Anything else is refused: by clap with exit 2 on the CLI, as `invalid_input`
  naming `severity_filter` (with a did-you-mean suggestion) on MCP. The filter never changes the
  verdict, the exit code or the cache decision; the CLI's human summary still counts everything and
  adds `(showing <severity> only)`.
- **D3. Lint profiles are a closed set; `pedantic` is kept as a named no-op.**
  `specforge_project::LintProfile { Inferred, Pedantic }`. Rejecting `pedantic` would break commands
  documented in six places; implementing it as "show infos" would hide infos by default on every
  surface. It is accepted as the explicit name of the default, and the CLI prints one note saying
  info diagnostics are always reported. Any other name is refused (CLI exit 2, MCP `invalid_input`
  naming `lint`). The docs no longer promise pedantic semantics; the VS Code extension's unused
  `specforge.lint.profile` setting, which promised the same, is removed.
- **D4. MCP validate keeps the bare array and `isError: false`, and adds the verdict as
  `_meta["specforge/check"] = {ok, errors, warnings, infos, shown}`.** `isError: true` on errors is
  ruled out by ADR 0004 D4-a, and an `{ok, diagnostics}` payload would break the one diagnostics
  shape every reader takes. `_meta` is MCP's channel for result metadata; the `specforge/` prefix
  follows its key grammar and avoids the reserved `mcp` and `modelcontextprotocol` prefixes, with no
  domain name. `ToolOutcome::with_meta` is the only envelope change; other tools' results are
  unchanged. The parity harness records "finding errors is a successful call, the CLI exits 1" as the
  `check_failing` Outcome divergence.
- **D5. Rendering quotes what was compiled.** The resolver keeps the text it parsed
  (`ResolvedFile::source`, `ResolvedProject::source_texts`), and `cli/check.rs::build_source_map`,
  which read every file a second time (also under `analyze --json`, and racing edits), is deleted.

## Consequences

- `specforge check --lint nonsense` exits 2 before compiling; `--lint pedantic` prints a note.
- `specforge check --severity <s>` is new; MCP validate calls with a capitalised severity now
  filter, and calls with a misspelt severity or an unknown lint profile fail instead of silently
  returning everything.
- Every validate result carries `_meta["specforge/check"]`.
- Watch and the LSP still report a compile's diagnostics without a policy; giving them `--strict`
  or lint profiles is a product decision, not part of this one.
