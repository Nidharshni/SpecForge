# Imports name the compile's sources; `exclude` entries are patterns

**Status:** accepted (2026-10-12). Amends [ADR 0006](0006-one-incremental-rebuild.md) (the sentence
"ignored, as a cold compile ignores it"), [ADR 0032](0032-one-graph-build.md) ("The resolver returns what is
read") and [ADR 0004](0004-architecture-migration-decisions.md) D1-b; completes
[ADR 0030](0030-a-sessions-inputs-are-one-value.md) D1.

The resolver decided what a `use` path names by asking the disk: `resolve_imports(spec_root, files, exists)`,
with `exists` always `Path::is_file`, and go-to-definition's `resolve_import` calling `is_file` itself. The
disk is neither a source of the compile nor an input of the session, so what an import named depended on
files the compile never read. An excluded `drafts/x.spec` satisfied `use "drafts/x"` while none of its
entities was in the graph, and so did a file under `build/`. `use "Types"` resolved on macOS and failed on
Linux. `use "api.v2"` looked for `api.spec`. A selective re-export from any of them was a W027 naming a file
the compile never read. In the other direction, an editor buffer not yet saved was in the graph, yet an
import of it was E025, "did you mean" itself. Watch, the LSP and MCP never see a change to a file that is no
source, so after one they kept reporting what a cold compile no longer did, until an unrelated file changed.
The session carried a special case ("a disk change always applies") so that its tests, which name excluded
paths directly, matched a cold compile that read the disk.

`exclude` entries were substrings of a source's path, which the spec promised and the JSON schema guarded
only in editors: `"drafts/**"` and every glob excluded nothing, `"gen"` excluded `general.spec`, and
`"/drafts"` excluded `sub/drafts/` but not `drafts/`.

## Decisions

- **D1. An import names a source.** A compile's sources are the `.spec` files it holds, by key (the path
  relative to the spec root that `specforge check` names a file by): those in its graph build (discovered
  files and the editor buffers the session holds) and those that could not be read.
  `specforge_resolver::resolve_imports(parsed, unreadable)` and `import_target(sources, importing, path)`
  resolve against those keys and read, list and stat nothing. `exists`, `resolve_import` and the spec-root
  parameter are gone. `SourceCache::import_diagnostics` is the one caller for a compile and every update,
  and `CompiledProject::source_keys` is what the LSP resolves a `use` against
  (`LspState::source_keys`, the stand-in's while an update runs).
- **D2. An unreadable source is a source.** An import of it reports nothing beyond its own E025, and a
  re-export from it no W027.
- **D3. Exact spelling.** Keys compare exactly on every platform. `.spec` is appended to the path's last
  name (`"api.v2"` names `api.v2.spec`); a path ending in `.spec` names itself; then `<path>/index.spec`.
- **D4. E025 says where it looked.** "import target not found among the project's sources: X", with
  `did you mean` when a close match exists, else the rule as the suggestion. It never says why a file on
  disk is no source: that would make the disk an input again.
- **D5. The session's inputs are complete.** Nothing a compile reports depends on a file it does not hold,
  so an update that reads nothing changes nothing, whatever kind of change asked for it. The session's
  special case is gone. ADR 0006's "a changed file discovery would not find is ignored, as a cold compile
  ignores it" now holds.
- **D6. Go-to-definition on `use`** answers from the sources the project was last compiled from, open
  buffers included. It never reads the disk.
- **D7. `exclude` entries are gitignore-style patterns** over a source's path relative to the spec root,
  written with `/`: `*` and `?` within one name, `**` across directories, `[...]` a set. An entry with no
  `/` except a trailing one matches a name at any depth. An entry with a `/` at its start or in its middle is
  anchored at the spec root. A trailing `/` matches directories only. A file is left out when an entry
  matches its path or one of its directories. `specforge_common::Exclude` parses the entries once per config
  read. `glob 0.3` matches them (already a workspace dependency, so no builtin is re-vendored).
- **D8. An entry that can match no source is E069** (empty, `!`-negated, a `.` or `..` name, or not a
  glob), and the item is ignored, as a non-string item already is. The JSON schema states the same rule.

## Rejected

- Keeping `exists` as a closure over the keys: a seam whose only honest adapter is a set lookup, and the
  door the disk came in by.
- A `Sources` trait for the source cache and the LSP stand-in: two adapters that are both a set of strings.
- Reading the disk to say "X.spec exists but is excluded" in E025: a file the compile does not hold would
  change what it reports again.
- Substrings with glob characters rejected (E069): keeps the substring misfires.
- Globs anchored at the spec root only: `"*.draft.spec"` would match only at the root, a trap for anyone
  who has written a `.gitignore`.
- `globset`: a new workspace dependency (every builtin re-vendored) for brace alternation that two
  entries express.
- Negation (`!`): it needs ordered evaluation, and nothing asks for it. A `!` entry is E069 rather than
  silently doing the opposite.

## Consequences

- User-visible:
  - an import of an excluded file, of one under a skipped directory, or of a name that differs only in
    case is E025 on every platform;
  - E025's message and suggestion change;
  - a re-export from an unreadable or excluded file no longer reports W027;
  - an editor's unsaved file can be imported;
  - `exclude` entries are patterns: `"drafts/**"` and `"**/*.draft.spec"` work, a name fragment no longer
    matches inside a longer name, `"/drafts"` means the root's `drafts/`, and an unusable entry is E069.
- A detached project still resolves no import (ADR 0047 D4): its keys are absolute paths.
- The resolver's tests need no temporary directory.

**What would reopen it.** A `paths` key in `specforge.json` (ADR 0032's trigger): aliases would be
configuration `resolve_imports` reads, still over keys. A request for negated `exclude` entries.
