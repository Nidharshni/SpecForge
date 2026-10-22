# A spec file a request names is read into its source key once

**Status:** accepted (2026-10-21, architecture round 21, plan 04). Amended by round 22 plan 08 (2026-10-21): D8, a refusal names the file. Amended by round 23 plan 02 (2026-10-22): D5 also refuses a new file's project-root spelling. Amends [ADR 0116](0116-navigation-reads-the-text-the-project-was-compiled-from.md) (D2: outline asks the view; its containment check is gone), [ADR 0112](0112-every-project-file-an-operation-opens-goes-through-the-requests-containment.md) D4 and [ADR 0104](0104-containment-is-applied-where-the-operation-opens-the-file.md) D4 (outline builds no bound) and [ADR 0151](0151-a-spec-file-is-changed-by-a-typed-edit.md) (a create's file is read into its source key). Extends [ADR 0177](0177-a-reads-selection-is-checked-by-the-project-view.md) (the view checks what a request names: an entity, a selection, now a spec file).

## Context

Four request arguments name a spec file: a typed create's `file` (MCP `edit`, CLI `new --in`), outline's `file`, and
`suggest_fixes`'s `file_path`. Each operation compared the caller's spelling with the compile's source keys its own way
(eight commits, four modules, no owner): the create checked it lexically (`./x.spec` passed) and used the spelling as the
key, the fix query compared text with `==`, outline dropped `.` components, and MCP outline ran a containment check whose
answer it threw away.

Reproduced at `9d05665cc`: `new type account "Account" --in ./hello.spec` previewed the existing 28-line file as a new
empty one, then the write was refused as `file_changed`; two spellings of one new file in one MCP edit list panicked the
server (`is staged twice`); `suggest_fixes` answered `{"fixes":[]}` for every spelling but the exact key, `../../etc/passwd`
and `nope.spec` included, where an unknown `entity_id` is refused; MCP outline found a source by `hello.spec` and
`./hello.spec` but not by its absolute path, which the adapter's containment had accepted.

## Decision

- **D1.** The project view owns the reader: `ProjectView::source(named)` for a read (outline, the fixes) and
  `ProjectView::source_place(named)` for a typed create (`ops/src/view/source_file.rs`). Two methods, because their
  refusals differ: a read of what is no source is `file_not_found`, a create's is `not_a_source`.
- **D2.** One key per file: `specforge_project::source_key` (the ledger's and the LSP's key) of the spelling under the spec
  root, rebuilt from its components so a leading or inner `.` and a doubled separator go. An absolute path under the spec
  root is that file. A project with no spec root keys its sources by the path they were given.
- **D3.** Outside is refused before anything else, by both methods, as `not_a_source` (kind `InvalidInput`): a `..`
  anywhere (even `sub/../a.spec`), or an absolute path elsewhere.
- **D4.** A read of a file that is none of the compile's sources (missing, a directory, `specforge.json`, a non-`.spec`
  file, excluded) is `file_not_found`. A create is refused as `not_a_source` unless the key is a compiled source or one
  discovery would keep (`SourceRoot::holds`, which is now a predicate of a key: `./x.spec` is false).
- **D5.** The spec root is the only base. A project-root spelling of an existing source (`spec/hello.spec` when the spec
  root is `spec/`) is refused, naming the spec-root key (`did you mean 'hello.spec'?`), instead of silently planning
  `spec/spec/hello.spec`. *(Round 23.)* So is the project-root spelling of a *new* file whose spec-root spelling repeats
  the spec root as a directory no source is in (`spec/types/x.spec` for a new `types/x.spec`): `not_a_source`, naming the
  spec-root key. A directory that already holds a source (a deliberate `spec/spec/`) is a place.
- **D6.** `suggest_fixes` refuses a `file_path` that is no source, about `file_path` (`FixField { EntityId, FilePath }`),
  as it refuses an unknown `entity_id`. `fix::offers` keeps a plain key filter: the LSP passes its key.
- **D7.** `ProjectChange`'s `debug_assert` against staging one file twice stays an invariant: no typed edit reaches it now.
- **D8.** *(Round 22.)* A refusal of a named spec file names the file, as the request spelled it (`OpError::with_file`,
  `OpError::file`). It is set by the reader's three refusals: outside (`not_a_source`), no source for a read
  (`file_not_found`), no source for a create (`not_a_source`). MCP's `From<OpError>` fills `McpError::file` from it, as it
  fills `entity_id` from the entity. No tool re-mints a refusal the view raised: outline answers the view's refusal with
  its did-you-mean (D5). With nothing served, the no-project refusal (`target::without_project`, ADR 0025 D1) reads that
  file for its message and keeps it, as it keeps `entity_id`. An `OpError` is about one named thing, an entity or a file,
  kept behind one box so that it stays under the large-error threshold.

## Rejected

- Accepting the project-root spelling as an alias when the spec-root reading is no source (two spellings of one file by
  rule again) and refusing nothing (the silent `spec/spec/...` stays).
- Normalizing the project-root spelling of a new file (strip the repeated prefix): a second spelling of one file by rule.
- Answering `{"fixes":[]}` for a file that is no source (an agent cannot tell "no fix" from "wrong file").
- Normalizing in each adapter; accepting a `..` that stays inside the spec root.
- A new `EntityField::File` (eleven `ArgumentOf<EntityField>` impls would each grow an arm).
- A per-tool `file_not_found` re-mint (outline's until round 22: a second wording, no did-you-mean); a `file` inside
  `OpError::data` (the operation's own JSON, sent as `McpError.data`, so the file would be sent twice or picked back
  out); reading the file back out of the message (`3a997946c` removed that).

## Consequences

- Breaking: `affected_files` and the diff of `specforge.edit` and `new` name the key; `new --in spec/x.spec` for an
  existing `x.spec` is refused; `specforge.outline` accepts an absolute spelling of a source and answers `invalid_input` for
  `sub/../x.spec`; `specforge.suggest_fixes` refuses a `file_path` that is no source.
- Additive: `specforge.suggest_fixes` and `specforge.edit` refusals of a named spec file carry `file`, and the
  no-project refusal keeps it. Breaking (text): `specforge.outline`'s `file_not_found` message is the view's, with
  `data.suggestion` for a project-root spelling.
- Rust: `read::navigate::outline` returns `Result`; `authoring::fix::fixes` returns `Refusal<FixField>`; `SourceRoot::holds`
  refuses `./`.
- Left alone: `match_file` (anchors match source-code paths by directory and suffix), infer's `SourceKey` (project-root
  keys of source-code files), MCP `format`'s `paths` (filesystem targets from the project root, ADR 0102).

## What would reopen it

A project that needs a first file in a directory named like its spec root (then a flag, not a heuristic); a request argument that names a source-code path through this reader; Windows (keys use the OS separator, unverified).
