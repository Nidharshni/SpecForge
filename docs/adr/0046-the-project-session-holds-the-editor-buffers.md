# The project session holds the editor buffers

**Status:** accepted (2026-10-08). Amends [ADR 0023](0023-the-lsp-reads-a-document-through-one-module.md) D9
and [ADR 0035](0035-one-reaction-to-a-session-update.md) D2. Amended by [ADR 0109](0109-reopening-a-project-in-place-is-one-session-operation.md) (D3: the open in a session's place is `ProjectSession::reopen`; `finish_holding` is private).

A project session accepted editor buffers (`SourceChange::Buffer`, `Buffers`) and forgot them: it did not know
which of its sources an editor held. So the LSP's change plan wrote "an open buffer is the truth for its file"
five times: the open and every environment reload applied every open buffer again after building from disk, a
watched change and the catch-up filtered open documents out except their deletion, and a close read the disk
itself and compared it with the compiled text. Each of those cost something. With one buffer open, the open and
every reload ran the full checks twice (1.6 s each in a debug build of this repository); a `didOpen` of an
unchanged file ran them once for nothing; a close read the disk outside the session's one read (ADR 0032) and
stamped nothing, so a saved-then-closed buffer stayed stale and the next catch-up ran the checks again; closing
a saved buffer that did not parse left the checks skipped; deleting the file of an open document removed its
entities while the editor still showed them, until the next keystroke. And since nothing recorded which version
of a buffer was compiled, every publication labelled its diagnostics with the buffer's version at publish time:
a list computed while an edit waited in the debounce went out with the newer version and the older positions.

## Decisions

- **D1. The session holds the buffers.** `SourceChange::Hold(&[Buffer])` gives it buffers (path, text, the
  editor's version); it keys each as it keys every source, again after each environment load. While it holds a
  buffer, the buffer is the truth for its file whatever happens on disk, its deletion included (the LSP
  specification's "the server must not try to read the document's content using the document's Uri"):
  `stale()`, `changes(paths)` and `apply` leave every held file out. A buffer of a file that is not a project
  source is held and builds nothing; a reload that brings it into the project builds it from its text.
  *(ADR 0100: `SourceChange::Hold` is `ProjectSession::hold`/`hold_with`.)*
- **D2. Releasing a buffer is the one read.** `ProjectSession::release(paths)` stamps a project source and
  reads it through the session's one read (ADR 0032); any other file leaves the project (with no project, the
  buffer was its only text). A release that changes nothing returns `None` and runs no check, unless the typing
  fast path had skipped them: then it runs them.
- **D3. One cold build, one run of the checks.** `OpeningProject::finish_holding(buffers)` reads each held
  buffer in place of its file; the LSP opens its project holding the buffers of the session it replaces, and
  `reload_environment` keeps the session's own. The compiled-project core stays buffer-agnostic: the session
  layer passes the held texts to its cold build. A buffer whose text is the text its file was built from
  changes nothing, and a hold or release that changes no file runs no check. A disk update always applies: what
  an import names is read from disk.
- **D4. The version travels with the text.** The session keeps each held buffer's version; the LSP labels a
  file's diagnostics with the version of the buffer the project was compiled from (none for a file compiled from
  disk), so a client that checks versions drops a list a newer edit superseded. (ADR 0056: the formatter's
  findings carry the version they were computed on and are published only beside a compile list of that
  version.)
- **D5. The LSP translates protocol events.** `specforge_lsp::changes::Plan`: `Open` is a root, `Edited` is a
  hold, `Closed` is a release, `Watched` is `ProjectSession::changes`, `CatchUp` is `ProjectSession::stale`.
- **D6. Watch and MCP hold no buffer** and see the session as before; no surface reads another's buffers
  (MCP serves projects from disk, ADR 0025).
- **D7. `CheckMode::SyntaxOnlyIfParseErrors`** skips the checks when any file the update changes does not parse;
  the keys it took were always the update's own.

This amends ADR 0023 D9 ("A closed project source is read from disk again; any other closed file leaves the
project. A reload applies every open buffer once."): the session releases a closed buffer, and a reload keeps
the held buffers in its cold build instead of applying them after it. It amends ADR 0035 D2 ("the LSP leaves
out an open document whose file exists, because its buffer is the truth"): the session leaves out every held
file, whether or not it exists, and the LSP's catch-up is `stale()` as it is. ADR 0023's rejection of a read
snapshot published by the session stands: the LSP's stand-in is unchanged.

## Consequences

- Opening a project with open documents and reloading the environment run the checks once; a `didOpen` of an
  unchanged file publishes without running them (user-visible: milliseconds, not seconds).
- Deleting the file of an open document keeps its entities until the document is closed (user-visible).
- A published list carries the version it was computed from (user-visible to clients that check versions).
- A saved-then-closed buffer leaves nothing stale; closing a saved buffer that does not parse runs the checks
  `specforge check` runs.
- `SourceChange::Buffer`/`Buffers` and `CheckMode::SyntaxOnlyIfParseErrorsIn` are gone; the LSP reads no
  source file itself (`Compiled::is_stale`'s comparison of a closed file with the compiled text stays: a
  reader's question, asked also while the session is out for an update).

## Rejected

- **The rule in an LSP overlay over the session**: `stale`, the release and the cold build need the session's
  stamps, read and build; the rule would live in two crates.
- **Building from disk and applying the held buffers before the checks** (round 4 plan 10 D8's
  `reload_environment_with(buffers)`): every held file read and parsed twice.
- **Keeping a deleted file's deletion as the exception**: the editor still shows the text and can save it;
  an editor that closes the tab sends `didClose`.
- **The LSP recording the compiled version per file**: a second record of what the session holds.
- **Skipping unchanged disk updates too**: watch reports a touched file as rebuilt, and an import's target is
  read from disk; not this decision's subject.

## What would reopen it

A surface that holds buffers across processes (an MCP tool reading the editor's unsaved text), or a client that
sends `didChange` for files it has not opened (then a hold would need a release without a close).
