invariant lsp_response_latency "LSP Response Latency" {
  guarantee """
    Diagnostic updates MUST appear within 100ms of the user stopping
    typing. Completion, hover, go-to-definition, and formatting responses
    MUST return within 200ms for projects under 1000 entities.
  """
  risk      medium
  verify property "LSP Response Latency guarantee holds"
}

invariant lsp_extension_reload_consistency "LSP Extension Reload Consistency" {
  guarantee """
    When extensions are added or removed while the LSP server is running,
    KindRegistry, FieldRegistry, and the semantic token legend MUST update
    atomically. No LSP request served between the start and end of the
    update MUST observe a partially-updated registry state. The LSP MUST
    NOT serve stale semantic tokens, completions, or hover information
    for entity kinds that were added or removed.
  """
  risk      medium
  verify unit "adding an extension while LSP is running updates KindRegistry atomically"
  verify unit "removing an extension while LSP is running removes kinds from KindRegistry atomically"
  verify unit "semantic token legend reflects current extensions after reload"
  verify integration "a specforge.lock change while the LSP is running reloads the environment"
  verify integration "the LSP watches every file its environment is loaded from"
}

invariant rename_atomicity "Rename Atomicity" {
  guarantee """
    A rename operation MUST update all files atomically — either all files
    are updated or none are. Partial updates MUST NOT persist.
  """
  risk      high
  verify property "Rename Atomicity guarantee holds"
}

invariant lsp_text_edit_non_overlapping "LSP TextEdit Non-Overlapping" {
  guarantee """
    TextEdit operations returned in a single LSP response MUST NOT have
    overlapping ranges. The LSP specification requires non-overlapping edits;
    overlapping edits cause undefined client behavior.
  """
  risk      high
  verify property "no LSP response contains overlapping TextEdit ranges"
  verify unit "formatting response TextEdits are sorted and non-overlapping"
}

invariant lsp_state_concurrency_safety "LSP State Concurrency Safety" {
  guarantee """
    The LSP's shared state (open documents and the compiled graph) MUST
    stay consistent under concurrent requests: readers MUST NOT block each
    other, a reader MUST never observe a half-applied update, interleaved
    reads and writes MUST NOT deadlock, and writes to different documents
    MUST NOT interfere.
  """
  risk      high
  verify unit "multiple concurrent readers complete without blocking each other"
  verify unit "concurrent readers see consistent graph and document state"
  verify unit "interleaved read and write operations do not deadlock"
  verify unit "concurrent writes to different documents do not interfere"
}

invariant lsp_utf16_positions "LSP UTF-16 Positions" {
  guarantee """
    Every position the LSP receives or returns MUST count columns in UTF-16
    code units, as the Language Server Protocol requires, so non-ASCII text
    before the cursor never shifts the word, range or edit it resolves to.
    One line index per text converts byte offsets and UTF-16 positions,
    both ways; a column past its line's end, whatever its value, is that
    end; nothing else in the LSP converts them (ADR 0023). A span of
    the graph or of a diagnostic is a position in the text the project was
    compiled from, so it converts against that text, never against the
    buffer typed since nor the disk now, and is a range only of an editor
    text (the open buffer, else the file on disk) that is that text: in a
    file typed since, changed on disk since, or of which the compile holds
    no text, its location, symbol or definition is left out and a fix or a
    rename that would need it is refused whole, never byte columns passed
    off as UTF-16. A diagnostics publish, labelled with the version of the
    text the project was compiled from, places its diagnostics in that
    text. While the document a code-action request names is not
    that text, no action is listed, whether the client resolves code
    actions or not, and none is planned.
  """
  risk      medium
  verify unit "the line index converts byte columns to UTF-16 and back on every line"
  verify unit "the word under a cursor is found by its UTF-16 column"
  verify unit "a span converts against the text the project was compiled from, and is no range of a buffer typed since"
  verify unit "a span of a file the compile holds no text of has no range, never byte columns"
  verify unit "a fix is offered whole or not at all: never over a buffer typed since, nor a file with no compiled text"
  verify unit "no code action is listed over a buffer typed since the compile, to a client that resolves them or not"
  verify unit "an outline of a buffer typed since the compile places no symbol at a line of the compiled text"
  verify unit "references from a buffer typed since leave out its lines and keep those of files that are the compiled text"
  verify unit "a definition in a buffer typed since is not placed at its compiled position; one in a file that is the compiled text is"
  verify unit "workspace symbols leave out the entities of a buffer typed since the compile"
  verify unit "a list leaves out a closed file changed on disk since the compile until its change is compiled"
  verify unit "a column past its line's end, up to the largest, resolves to the line's end"
}

invariant lsp_survives_its_faults "The LSP Survives Its Faults" {
  guarantee """
    A panic in a request or notification handler is a fault of the server,
    not the end of the editor's session: a request answers -32603
    InternalError, a notification changes nothing more, the client is told
    what failed in a window/logMessage error, and the server keeps serving.
    A recompile's panic is reported the same way (ADR 0043). No input a
    client can send ends the process: a position, a range or a buffer the
    server cannot use is clamped or refused.
  """
  risk      high
  verify unit "a panic is turned into its message"
  verify integration "a request after a handler's panic is answered and the server keeps serving"
}

invariant cursor_names_one_entity "One Entity Under the Cursor" {
  guarantee """
    Every LSP request about the entity under a cursor (hover,
    go-to-definition, references, rename) MUST resolve the same entity: the
    declaration or reference token under the cursor as navigation reads it,
    else an identifier at a reference position (an entity header's name, a
    value or list item in the entity's own body of a field not typed as
    enum, boolean, integer, string, string list or block, a use binding's
    imported name) that names an entity. A word in a string (one spanning lines included) or a comment, a kind keyword, a field name
    and a value of a non-reference field name no entity; a scheme ref ID
    (gh.issue:42) is one token. The structure around the
    cursor is read from the document's text, never from the graph, which
    lags the text while the user types (ADR 0023). While the document is not
    the text the project was compiled from (an edit waits for its compile),
    the token is never looked up by its position in the compiled text: the
    cursor names what its own word names at a reference position. Hover,
    definition and references then answer for that entity, with no
    location in a file whose editor text is not the compiled text
    (lsp_utf16_positions).
  """
  risk      medium
  verify unit "hover and go-to-definition resolve the same entity on every token of a document"
  verify unit "a word in a string or comment names no entity"
  verify unit "a scheme ref ID under the cursor names its ref"
  verify unit "a value of a field typed as no reference names no entity"
  verify unit "a cursor in a buffer typed since the compile names what its own word names, never the compiled text's token at its position"
}
