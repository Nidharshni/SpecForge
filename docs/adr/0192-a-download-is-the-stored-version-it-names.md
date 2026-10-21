# A download is the stored version it names

**Status:** accepted (2026-10-21, architecture round 21, plan 01). Restores [ADR 0185](0185-a-scope-is-owned-by-an-account.md) D3 (a yanked version is taken down) for the binary.

## Context

The registry's download route parsed the package name but joined the raw version segment into a file path, read the
binary before asking whether the version existed, and checked the recorded digest only when a row was found. A version
segment that was not a version reached the disk, no row meant no digest check, and a yanked version's binary was still
served although its metadata was not found. An upload past the body limit was answered as unsigned or badly signed.

## Decision

- **D1** A download names a stored version: the version segment is parsed as a version, else not found; it never
  reaches a file path unparsed.
- **D2** The row comes first: no row, or a yanked row, is not found and no binary is read. A served binary is always
  held to the row's digest (integrity violation otherwise).
- **D3** A multipart field that cannot be read is an error of the request: past the body limit it is 413, any other
  read failure is a bad request. It is no longer taken for a missing field.

## Rejected

- Sanitising the segment (`..`, separators): a parsed version has no such characters; refusing is simpler than cleaning.
- Serving a yanked version digest-checked: it defeats the takedown.

## Consequences

`GET …/{version}/download` answers 404 for a yanked or unpublished version and for a segment that is no version. A
server restart is not needed; no stored data changes.
