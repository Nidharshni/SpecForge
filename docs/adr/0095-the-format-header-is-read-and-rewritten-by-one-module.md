# The format header is read and rewritten by one module

**Status:** accepted (2026-10-15, round 10, plan 03). D3 restored by [ADR 0202](0202-a-blocks-doc-comment-is-where-the-parse-read-it.md) after ADR 0148 widened it: `header_row` is private.

The parser read the `// specforge-format: MAJOR.MINOR` header from the file's first non-blank line, but
`specforge migrate` rewrote it only when it was on line 1 and otherwise put a new header above the file. A file
that began with a blank line then held two headers after `migrate`, `1.0` on top and the stale `0.9` below, and the
formatter kept both (the run was idempotent, so the `migration_idempotency` invariant never broke; a stale
comment stayed for good).

Separately, `MIN_SUPPORTED_VERSION` was documented as the minimum for migration but was only the I007 threshold.
Every E019 hint read "Use a format version between 1.0 and 1.0." although files at 0.1 and 0.5 were read, and
`migrate --target-version 0.5` was accepted. I007 fired below MIN while its message said "older than current".

## Decisions

- **D1.** `crates/kernel/parser/src/format_version.rs` owns the placement rule (`header_line`: the first non-blank
  line, when it starts with the prefix), the reader (`detect_format_version`) and the writer
  (`with_format_version`). The reader has to live in the parser, and a kernel crate cannot depend on
  `pipeline/migrate` (ADR 0084).
- **D2.** The writer replaces the header the reader finds, in place, keeping its line ending and any blank lines
  above it. A file with none gets the header as its first line, ended like the file's first line (CRLF stays CRLF).
  A file whose first non-blank line is another comment has no header by the reader's rule, so it gets one.
- **D3.** The prefix is private; the module is private. The interface is `detect_format_version`,
  `with_format_version` and the three constants. `migrate`'s `transform_content` calls `with_format_version`.
- **D4.** `MIN_SUPPORTED_VERSION` is the oldest version a file may declare: the previous major's first (0.0 today,
  following `migration.spec`: "the current and previous major format versions"). I007 compares with
  `CURRENT_FORMAT_VERSION`; a garbled header reads as `CURRENT_FORMAT_VERSION`.
- **D5.** No W160 for a header-like comment that is not the header (pre-release, a harmless comment). The header rule
  skips one leading byte-order mark, so a BOM file's header is read and rewritten like any other.

## Consequences

A v2 transform composes after `with_format_version` in `transform_content`. Files the old writer already damaged
stay as they are. Removed: `specforge_parser::FORMAT_HEADER_PREFIX` and the `specforge_parser::format_version` path.
