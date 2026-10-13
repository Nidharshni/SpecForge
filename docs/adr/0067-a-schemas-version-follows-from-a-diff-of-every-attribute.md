# A schema's version follows from a diff of every attribute

**Status:** accepted (2026-10-13, architecture round 8, plan 05). Amends ADR 0003 ("Built instead of trimmed"),
ADR 0007 (where things live) and ADR 0015 D10.

The Graph Protocol schema's version was the recorded version bumped by `diff_schemas`, which compared 4 of a
field's 9 attributes, 2 of a kind's 5, an edge type's label only and none of the extensions. A field that became
optional, an edge type whose ends changed, an extension upgrade: no bump, no W053, and `graph-protocol-v1.0.0.json`
named two documents with different content hashes. `--schema-version` relabelled the current document with an older
version, an unreadable schema cache restarted the version at 1.0.0 without a word, and a refused version was E027,
the catalog's unsatisfiable peer. Versioning was spread over the emitter, two ops modules, the project view and
migrate, whose own comparison of two compiles of one environment never found anything.

## Decisions

- **D1. One module versions the schema**: `specforge_ops::schema_version`. `SchemaCache::version(generated)` gives a
  `VersionedSchema` (schema, version, changes, diagnostics, `admits`, content hash); `SchemaCache::record` is the one
  write, by `specforge export` only (ADR 0015 D10). The emitter keeps the schema types, `generate_schema`,
  `content_hash` and publication.
- **D2. Every attribute is compared and classified.** The diff destructures each `Schema*` value whole, so a new
  attribute does not compile until it is classified. The rule: within a major version a reader of an earlier version
  reads every later document. A published set (kinds, edge types, a kind's fields, an enum's values, an edge type's
  ends) grows in a minor version and shrinks in a major one; a field's type, required flag, edge type and target
  kind change in a major one; a field added as required is major; descriptions, defaults, colours, the testable
  flag, source extensions, extension versions and load order are patch. The table, one row per attribute:

  | Subject | Change | Bump |
  |---|---|---|
  | extension | added, removed, version, load order | patch |
  | kind | added | minor |
  | kind | removed | major |
  | kind | `source_extension`, `testable`, `dot_color` | patch |
  | field | added optional | minor |
  | field | added required, removed | major |
  | field | `field_type`, `required`, `edge`, `target_kind` | major |
  | field | `enum_values` gained / lost / reordered | minor / major / patch |
  | field | `description`, `default_value`, `source_extension` | patch |
  | edge type | added | minor |
  | edge type | removed | major |
  | edge type | `source_kinds` / `target_kinds` gained / lost / reordered | minor / major / patch |
  | edge type | `source_extension` | patch |

  An absent set is the empty set. When a field's type changes its enum values are not compared as well.
- **D3. One version names one document.** Against one record, two schemas with the same version have the same
  content hash; the record keeps the hash a scoped export's `schema_ref` names. A debug assertion holds that the
  diff finds a change exactly when the two schemas differ.
- **D4. A requested version is admitted, never applied.** `--schema-version` / `schema_version` is admitted when a
  reader of it reads the current schema (same major, not newer) and refused as `incompatible_schema_version`
  (kind `Conflict`) otherwise, for every format; one that does not parse is `invalid_schema_version`
  (`InvalidInput`). The export always carries the current schema at its own version.
- **D5. I016 is an unreadable cache**, the one case where the version restarts; the "prior exports in an output
  directory" case and fallback are gone (exports go to stdout).
- **D6. A migration compares no schema.** The schema is the registry build's (ADR 0060 D5), the build is the
  environment's, and a migration changes only sources, its hooks running with no file system (ADR 0037). Migrate's
  `check_schema_compatibility` and `schema_warnings` are gone; W053 is `specforge export`'s.
- **D7. `specforge export` writes `.specforge/schema-cache.json` into the project it exports** (ADR 0015 D10),
  after a successful export of any format. A test that runs `export` on a committed project works on a temporary
  copy.

Rejected: classifying any change of an edge type's ends as major (loading an extension that writes an existing
undeclared edge type would be a major bump while the kinds it adds are minor); keeping every recorded version's
document so a request can be served the old one (the nodes are the current graph's); comparing the migrated
schema against the cache (it would attribute to the migration what the extension set changed).

## Consequences

- More exports move the version: extension upgrades bump the patch, edge ends and required flags the major.
- `specforge migrate --format json` and `specforge.migrate` lose `schema_warnings`.
- `--schema-version 1.0.0` on a 1.3.0 project exports 1.3.0; `--format brief --schema-version 9.0.0` fails.
- `docs/diagnostics.md`: I016 is "Schema cache unreadable"; W053 names the versions it moves between.
- The MCP error for a refused version carries the kind's code (`conflict`) and the message; an operation's own
  code is the CLI's `error[incompatible_schema_version]`.

## What would reopen it

- A consumer that must receive an older document (then records keep every version's schema and the export
  projects the graph onto it).
- A migration that rewrites `specforge.json` or the installed extensions.
