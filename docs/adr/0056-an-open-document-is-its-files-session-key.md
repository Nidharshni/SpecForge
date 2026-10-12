# An open document is its file's session key; its published diagnostics have one writer

**Status:** accepted (2026-10-12). Amends [ADR 0023](0023-the-lsp-reads-a-document-through-one-module.md) D2 and
[ADR 0043](0043-the-lsp-reaction-talks-to-the-editor-through-a-port.md) D6; refines
[ADR 0046](0046-the-project-session-holds-the-editor-buffers.md) D4.

The LSP kept its open documents, the diagnostics it last published and the anchor of diagnostics about no entity
under the URI string the client sent, while navigation and publication rebuilt URIs from session keys with
`Url::from_file_path`. VS Code (vscode-uri) percent-encodes `@`, `+`, `=` and a Windows drive's `:`; `url` does not.
So a file in `@acme/` had two identities. The editor was sent an empty list under one spelling and the real list
under the other. Hover and code actions found no diagnostics. Rename over a buffer typed since the compile was
answered from the compiled text instead of refused. A closed document was re-published under the server's spelling.
Formatting replaced the compile list with `[W142]` in an editor that, rightly, treats both spellings as one
resource. Fifteen URI-to-path conversions in six files used three spellings. Separately, formatting published its own
merged list, labelled with the buffer's version and never recorded: the next publish of the file erased W142, hover
never showed it, and a format right after typing sent the compile's old positions under the new version.

## Decisions

- **D1. A document's identity is its file's session key**: `source_key` of the path its URI names, percent-decoded.
  The open document, the file's published diagnostics, the anchor and staleness are all found by the key. Spans,
  navigation and the session's held buffers already speak it, and `source_key`'s canonical comparison makes a
  symlinked directory and a lowercase drive letter one key, where a decoded path or a normalized URI would not.
- **D2. The client's URI only names the file back.** Every publish, location, link and workspace edit about a file
  the client opened uses the URI it opened it with; other files get `Url::from_file_path`. A file is named by one
  URI at a time: a reopening under another spelling sends nothing to the old one.
- **D3. One module, `specforge_lsp::files::Files`**, holds per key the URI, the path, the open document and the
  published layers, and is rekeyed when an environment load moves the spec root. A change the client reports
  carries its URI until the plan reads it, so nothing queued holds a stale key.
- **D4. One conversion.** `specforge_lsp::uri` is the only reader of a URI into a path and the only writer of a
  path into a URI.
- **D5. Published diagnostics are layers with versions.** The compile layer is what the project reports, labelled
  with the version of the buffer the project was compiled from (ADR 0046 D4). The formatter layer is what formatting
  the open document reported, labelled with the version it formatted. A publish sends the compile layer plus the
  formatter layer of the same version; a compile of another version drops the formatter layer. Formatting
  publishes at once only when the compile layer is at its version, else the compile of that version carries it.
  Hover shows both layers under the cursor; code actions and the entity facts read the compile layer.
- **D6. The store is the one writer.** The reaction and the formatting request each record a layer and send what
  the store answers. Formatting still sends on the request path (ADR 0043 D6), through the store.
- **D7. A closed file sent an empty list is forgotten**, its URI with it; a formatting run that finds nothing
  clears the formatter layer.

## Consequences

- With a percent-encoded URI, hover, code actions, rename's refusal over a stale buffer and closing behave as with
  the plain one (user-visible in VS Code for paths with `@`, `+`, `=`, and on Windows).
- Answers name an opened file by the client's spelling (user-visible to clients that compare URIs as strings).
- W142 stays published until the document changes and hover explains it; a format right after typing shows W142
  with the next compile instead of with the previous one's positions (user-visible).
- Publishes go out in key order, not URI order. The reaction and the formatting request send from two threads
  after recording; if they interleave the editor can briefly show the list recorded first. The store is right
  either way and the next publish of the file carries both layers (holding the state lock across a client send was
  rejected by ADR 0043 D1).
- `LspState` loses seventeen methods (three had no caller); `Document` no longer holds a URI.

## Rejected

- **The decoded path as the identity**: misses a symlinked spelling and a drive letter's case, which `source_key`
  already handles, and every span lookup would join and compare paths.
- **Normalizing URI strings** (re-encoding as vscode-uri does): a second encoding rule to keep in step with every
  client's.
- **Publishing the formatter's findings alone, or with the buffer's version**: a publish replaces the list, so the
  compile list vanishes, or carries positions from another text.
- **Queueing formatting's publish behind the reaction**: ADR 0043 D6 stands; queueing would stall a format behind an
  update.
- **Holding the state lock while sending** to order the two writers' sends: ADR 0043 D1.
- **Recording the compiled version per file** (ADR 0046's rejection) is not reopened: the compile layer keeps the
  version it was sent with, the label of what the editor shows, which is what the formatter layer must match.

## What would reopen it

A second document-producing request with its own findings (a third layer, same rule); a client that negotiates
something other than `file:` URIs for project files (the conversion module grows a scheme); several workspace roots
with one session each (one `Files` per root).
