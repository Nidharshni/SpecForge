# A project's sources end at a nested project

**Status:** accepted (2026-10-13, architecture round 9, plan 11). Amends ADR 0021 D3; completes ADR 0053 D1
and ADR 0055 D1.

A directory below a project's spec root that held its own `specforge.json` was two projects' at once. The outer
project's compile read its files as outer sources, under the outer configuration's extensions and with keys
relative to the outer spec root. `ProjectRoot::of`, `check --path`, MCP's `path` and format's walk of a
directory named the inner project. The outer `check` reported E024 or E002 on files whose own `check` was clean,
and E025 on each of their imports. MCP `rename` rewrote them. `migrate` selected them. `format` with no paths
formatted a file the inner project excluded. An editor opened on the outer project showed the outer
diagnostics on them. Discovery walked with no stop, format's directory walk stopped at projects, and the
single-path answer (`is_discovered`) was lexical and could not know.

## Decisions

- **D1. The walk ends at a nested project.** A directory strictly below the spec root that holds
  `specforge.json` (`ProjectRoot::is_root`) is a nested project. No file inside it is a source of the outer
  project, on every surface: compile, session, watch, the LSP, MCP, format, migrate, rename. Its own project
  reads it (`check --path <dir>`).
- **D2. The spec root is entered whatever it holds.** `spec_root` states where the sources are and may lie
  outside the project root. A spec root that is itself another project's root is read as this project's
  sources. That is the configuration's explicit choice.
- **D3. One value answers it.** `specforge_common::SourceRoot` (the spec root and `exclude`, built by
  `ProjectConfig::sources`):
  - `files()` lists the sources, and `files_at(path)` those at or under one path;
  - `holds(key)` answers for one path, reading whether each directory on the way holds `specforge.json`.

  `discover_spec_files`, `discover_spec_files_at`, `is_discovered` and
  `ProjectConfig::{spec_files, excludes, spec_root_in}` are deleted.
- **D4. Nothing reports a nested project.** Like a skipped directory or an `exclude` entry, it is silent: a
  project that keeps fixture projects under its spec root would otherwise carry one finding per fixture.
- **D5. A session follows a nested project's `specforge.json`.**
  - Its creation or deletion is a change to every source under its directory.
  - A source that leaves leaves the graph and the freshness record. One that comes back is read.
  - The LSP asks its client to report `specforge.json` under the spec root. Watch's roots already cover it. MCP
    compares the whole record on each call.
- **D6. Format.**
  - A run with no paths, or naming a project, formats that project's sources and no nested project's files.
  - A walk of a directory that is no project takes the sources of each project it reaches, and not those of
    projects nested in them (reaching a project is naming it).

## Consequences

- User-visible: in a project whose spec root contains a nested project, `check`, `stats`, `export`, `watch`, the
  LSP and MCP no longer show that project's files, and `format`, `migrate` and `rename` no longer write them. A
  document of a nested project opened in the outer project's editor workspace gets no diagnostics, as any file
  outside the sources. Open the nested project's directory to edit it with diagnostics.
- This repository's projects, examples and fixtures nest nothing under a spec root: their output is unchanged.

## What would reopen it

A workspace concept (one configuration over several projects), or an LSP that serves several projects at once
(a nested project's document would then be served by its own session).
