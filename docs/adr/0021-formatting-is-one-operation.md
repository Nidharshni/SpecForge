# Formatting a document is one operation

**Status:** accepted (2026-10-06)

Three surfaces formatted `.spec` text and each decided for itself what configuration applied and
which files were the project's:

- The LSP called `format_document(text, None, None, editor)`. With no file and no root its
  config-file branch never ran, so the editor ignored `.specforgefmt.toml` and reindented files that
  `specforge format --check` called clean.
- `specforge format` and MCP `specforge.format` loaded one configuration per run, from the
  directory the run started in, not from each file's. A nested `.specforgefmt.toml` applied or not
  depending on the working directory.
- Format (and migrate) searched `<root>/spec` if it existed, else the root — not the configured
  `spec_root` — and ignored `exclude`.
- MCP built its answer from part of the run's outcome and reported `all_clean: true` for files it
  could not read or only partly parse.
- The tests that should have caught this called a private function production never reached,
  compared the CLI with itself, and proved formatting rules through eight functions nothing called.

## The operation

`specforge_ops::format` formats one document or every source of a project:

- `document(place, text, lines, editor) -> FormattedDocument` formats one document (whole, or the
  blocks `lines` touch) with the configuration `specforge format` uses for its file, and reports
  the edits, the configuration used and the diagnostics (W141, W142 spanned at the document's lines).
- `run(request) -> Outcome` formats the project's sources (or the named paths) through the same
  implementation, one configuration per configuration file, and reports changes, typed failures
  (read or write) and one diagnostics list.

The surfaces keep presentation: the CLI prints and picks the exit code, MCP builds its payload, the
LSP converts byte columns to UTF-16 and publishes diagnostics. The formatter crate is the text
engine (source and `FormatConfig` in, text and diagnostics out, plus reading a config file); project
rules live in ops and `specforge-common`.

## Decisions

- **D1. Inside a project, the project's configuration decides; editor settings only outside one.**
  `.specforgefmt.toml` nearest the document, else the defaults — never the editor's tab size, so a
  file the editor formats passes `format --check`. An unsaved buffer or a file with no
  `specforge.json` above it uses the editor's settings. (Changes `lsp_respect_editor_config`: a
  project without a config file formats with the defaults, not the editor's tab size.)
- **D2. Configuration is per file.** Discovery starts at each file's directory and stops at the
  file's own project root (its nearest `specforge.json`), in every surface and whichever project the
  run started in; a run reads each configuration file once and reports its W141 once.
- **D3. Format and migrate rewrite the project sources.** `ProjectConfig::spec_files`: under
  `spec_root` (the root when unset), without what `exclude` leaves out — the files a compile reads.
  A file named explicitly is formatted all the same. A directory named explicitly that holds
  `specforge.json` is that project's sources; a walk of any other named directory takes a nested
  project's sources where it reaches one, so naming a project never reaches its fixtures.
  `specforge_formatter::discover` is deleted.
  *(Round 6 plan 12: a named directory is walked by `specforge_common::walk_spec_files`, the walk discovery
  uses, stopping at nested project roots (`is_project_root`). Target selection stays in
  `specforge_ops::format`, its only caller: no paths, a named file, a named project, any other directory.
  Migrate takes no paths. A file several arguments reach is one target, shown as first spelled.)*
- **D4. Clean means canonical; an unreadable file is a failure.** `Outcome::clean()` holds when every
  target was read, nothing would change and no region was left unformatted (W142). The CLI exits 1,
  in every mode (write, `--check`, `--diff`, `--stdin`), when a file could not be read or written or
  has a region left unformatted, and under `--check` also when a file would change; MCP fails the
  call for read failures as for write failures and sets `all_clean` from `clean()`. *(ADR 0029 D3:
  the rule is `format::Outcome::ok()`; MCP returns it as `ok` beside `all_clean`; `Mode::Preview` is
  a `--diff` that does not fail.)*
- **D5. One diagnostics list.** The outcome has one `diagnostics` list (W141, W142 with file and
  lines) so no adapter can drop part of it. A W142 spans the whole region it kept, in document
  lines, also for a range.
- **D6. The formatting rules are the engine's.** The line-rule functions the source-order emitter
  replaced are deleted; the rules' obligations are proven through `format_source`. String literals are
  kept byte-for-byte, and the spec says so instead of promising to re-indent them.
- **D7. No diagnostic for an unreadable file.** It is a typed `Failure` of the run, not a fact about
  spec content; W149 was reserved for it and is released. The `Failure` carries the kind of the OS
  error (`OpErrorKind::of_io`, ADR 0024 D7), not its text alone: MCP answers a run whose files all
  failed alike with that kind (`permission_denied` for a locked file, `file_not_found` for a missing
  one), else `internal_error`, and lists each file's own code in `data.failures`; the CLI prints each
  failure as every operation's failure (`error[file_unreadable]: failed to read …`,
  `error[file_unwritable]: …`).

## Consequences

- An editor no longer reformats a project file to its own tab size; projects that relied on that add
  a `.specforgefmt.toml`. The LSP logs once when it overrides differing editor settings, and the
  VS Code extension defaults `.spec` files to the formatter's indentation.
- `specforge format` from the project root honours nested `.specforgefmt.toml` files.
- In a project with `spec_root` set, format and migrate touch the project's sources and nothing
  else; in one without, they cover every source under the root (as `check` does), not only `spec/`.
- `specforge format` fails (exit 1) on an unreadable or unwritable file or a region left
  unformatted, in every mode; `--check` also fails on a file that would change. MCP
  `specforge.format` reports all of them.
- Naming a project directory (`specforge format --check .`, or a nested project's directory) formats
  that project's sources, never its fixtures or test data.
- The LSP publishes W142 at the region it kept, also for a range. A range that starts or ends on the
  blank lines between blocks leaves them as they are.
- The LSP's byte-to-UTF-16 conversion of format edits is the LSP document module's (ADR 0023,
  `LineIndex`).
