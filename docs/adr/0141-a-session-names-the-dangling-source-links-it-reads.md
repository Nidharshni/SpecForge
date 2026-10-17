# A session names the dangling source links it reads

**Status:** accepted (2026-10-17, round 14 follow-up F1). Amends [ADR 0133](0133-recorded-inputs-are-one-table.md)
(its closing sentence) and [ADR 0129](0129-a-source-read-is-a-recorded-read.md) D2 (discovery still records nothing, but
it now names one more kind of key). Keeps ADR 0073 D5 (only files are sources), ADR 0106 D2 and ADR 0129's rejected
"hops recorded at discovery".

## Context

Discovery lists a `.spec` symlink only when it resolves to a file, so a link that points nowhere is no source: it is
never read and records nothing. When its target appears, `specforge watch` and the LSP see only the target (no error)
while `specforge check` sees the link as a second source (E002). `bring_session_up_to_date` promises what a fresh
compile gives, and does not keep it.

## Decision

- **D1.** `SourceRoot::dangling` (and `dangling_at`) lists the `.spec` symlinks the walk reaches whose target does not
  exist (`NotFound`; a looping or unreadable link is neither a source nor dangling). `files()` and `files_at()` are
  unchanged, so format, migrate and inference never see one.
- **D2.** The session reads the keys of `files()` and of `dangling()`. The ordinary read finds the link gone and records
  the chain it followed (ADR 0129 D2); discovery records nothing.
- **D3.** A source read as gone is recorded as read (`Some(None)` in the stamps) and is unchanged while its path is
  missing. A discovered key that was never read is changed whether or not its path exists: a discovered key is read at
  least once.
- **D4.** `Unrecorded` (the one-shot compile) names the same keys, so both adapters take the same steps (ADR 0106 D2).

## Rejected

- Listing the link in `files()`: format would try to open it (the E025 that made the rule), and nine callers change.
- A `read_link` walk in the ledger: a second walker (ADR 0129's first rejection).
- Hops recorded at discovery: ADR 0129's third rejection; here discovery only names the key.
- Leaving it: watch keeps diverging from `check` with no error shown.

## What would reopen it

Symlinked directories as sources (ADR 0073 D5), or a link to a directory that later becomes a file.
