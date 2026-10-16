# Every reader of the versioned schema reports its notices

**Status:** accepted (2026-10-16, architecture round 11, plan 08). Amends ADR 0015 D10, ADR 0067 D5 and ADR 0103 D4;
reaffirms ADR 0015 D2.

Versioning the schema against `.specforge/schema-cache.json` reports I016 (a record that cannot be read: the version
restarts at 1.0.0) and W053 (one per breaking change). Only `export_recorded` handed these notices to a surface. Eight
production callers read `view.versioned_schema()`, and one read its diagnostics.

Reproduced at `63e4d7d9` over a corrupt record: `specforge schema`, `schema --publish`, `trace`, `model` and `query`
answered 1.0.0 and said nothing on stderr; every MCP tool and resource that names the version (`specforge.schema`,
`export`, `render`, `query`, `trace`, `model`, `specforge://schema`, the graph, context and brief views) carried no
`_meta.diagnostics`. Only `specforge export` printed I016, and then it recorded over the file. Over a record that
differed from the schema, every reader answered the right bumped version and omitted the W053 lines that say what broke.
The I016 suggestion also said "This export records a new cache", which is false for any reader but export.

The review card asked to hold the versioned schema once per compile. Measured in a debug build with 22 kinds, 210
fields and 56 edge types (record 79 501 bytes): versioning costs about 1 ms per MCP call with 5 kinds and about 4 ms
with 22 (`specforge.trace` 4.1 ms, a `specforge://schema` read 6.1 ms, with the schema serialized), about 4 ms of a 290 ms
one-shot command (1.4%). The `debug_assert` in `SchemaCache::version` clones and sorts both schemas in debug builds
only, so release is cheaper.

## Decision

- **D1. `VersionedSchema::notices` is the one source.** It was `diagnostics()`, with one caller. Every read outcome that
  names the version carries its result: `Exported` (the read-only `export`), `RecordedExport`, `QueryOutcome`,
  `SchemaOutcome`, `PublishedSchema` (what `json_schema` returns), `TraceOutcome` and `ModelOutcome`. A refusal carries
  none, except that `export_recorded` reports them whether the export succeeds or fails. No reader writes the record;
  only `export_recorded` does, and the next read then reports nothing. A view without a root has no record and no notice.
- **D2. Surfaces.** The CLI prints the notices on stderr before the document, as it printed `model` and `query`'s I020.
  MCP puts them in `_meta.diagnostics`, on tool results (`Answer::with_diagnostics`) and, new here, on resource reads
  (`ResourceText::with_notices`, written by `resource_envelope`, the shape a tool result's has). The LSP reads no
  versioned schema, so it has nothing to report. Notices never change an exit code or `isError`.
- **D3. Both codes, every reader.** One rule: I016 and W053 on each reader that names the version, until the next
  `specforge export` records. A stale local record shows the notices, which is the point; `.specforge/` is git-ignored,
  so CI never has a record.
- **D4. The I016 wording is reader-neutral.** "The next `specforge export` records a new cache over it; until then every
  reader answers 1.0.0, and agents that read earlier exports must not take the version as continuing theirs." The
  message is unchanged. The catalog text says the same.
- **D5. The schema is not memoized.** ADR 0015 D2 stands, now measured (see above). A memo keyed on the record's content
  still has to read the record to learn its content (as `RecordedCoverage::memo` reads the report), so it would save only
  the generation, the parse and the diff.
- **D6. Extension commands and prompts report nothing.** `command::run` hands the versioned graph to the extension, and
  its output is the extension's (`CommandOutput`, a protocol type: changing it is a re-vendor). The trace prompt's
  payload has no `_meta` (ADR 0015 P4).
- **D7. `CacheRecord` (check) and `CacheWrite` (export) stay two types.** They record different files, have different
  triggers (`--cache` is opt-in; export always records) and different stakes (a check that cannot write the cache it was
  asked for failed its request; an export whose document is already on stdout warns). Merging them couples two
  operations for no caller.
- **D8. The notices come first.** The schema's notices go before the request's I020 in `QueryOutcome` and
  `ModelOutcome`, and the CLI prints them before the document.

## Rejected

- The memo in `specforge-project` (moving `schema_version` down beside `coverage.rs`, a new project to emitter edge, a
  Mutex memo on `CompiledProject`): one more dependency edge, a module move and a fourth reason for `CompiledProject` to
  change, to save about 4 ms.
- Memoizing `generate_schema` in `Environment`: the part keyed on the registry build alone, but it still needs the
  emitter edge, and the rest of the call stays.
- W053 for export only: an agent could not learn what broke without exporting, which rewrites the record.
- Silent resources, pointing agents to `specforge.schema`: `specforge://schema` itself would stay silent.

## What would reopen it

Versioning showing up in a profile (a registry an order of magnitude larger, or a surface that versions per keystroke,
such as an LSP hover that names the version), or a reader that must see the record a compile saw rather than the one on
disk now.
