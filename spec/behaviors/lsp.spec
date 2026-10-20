// LSP behaviors — Language Server Protocol features

use "events/compilation"
use "invariants/core"
use "invariants/lsp"
use "invariants/validation"
use "invariants/wasm"
use "invariants/zero-entity-core"
use "ports/inbound"
use "ports/outbound"
use "types/core"
use "types/diagnostics"
use "types/errors"
use "types/graph"
use "types/lsp"
use "types/wasm"
use "types/zero-entity-core"

defaults behavior {
  features [live_diagnostics]
  types    [SourceSpan]
}

behavior lsp_initialize "LSP Initialize" {
  features   [lsp_lifecycle]
  invariants [zero_domain_knowledge_core, lsp_extension_reload_consistency]
  category   command
  types      [KindRegistryEntry, FieldRegistryEntry, SemanticTokenLegendEntry]
  ports      [LspProtocol]
  produces   [lsp_initialized]
  requires {
    extensions_loaded "extensions have been loaded and KindRegistry/FieldRegistry are populated"
  }
  ensures {
    capabilities_reflect_extensions "initialize response capabilities derive from loaded extension state, not hardcoded domain logic"
    semantic_legend_populated       "semantic token legend lists every standard LSP token type, so any standard type an extension declares is available"
    incremental_sync_advertised     "document sync kind is INCREMENTAL"
    lsp_initialized_emitted         "lsp_initialized event is produced on successful initialization"
  }
  contract   """
    When the LSP server receives an initialize request, it MUST respond
    with its capabilities: a static semantic token legend listing every
    standard LSP token type (it is sent before extensions load, so any
    standard type a KindRegistryEntry.semantic_token names is already in
    it), completion trigger characters, rename support, code action kinds,
    and document sync kind (INCREMENTAL). The server MUST report support for workspace symbol search
    and document symbol outline. The initialization response MUST NOT
    hardcode any domain-specific capabilities — all capabilities beyond
    structural defaults MUST derive from loaded extensions. The workspace
    (rootUri, else the first workspace folder) MUST open the project it is
    in (locate_project_root): the nearest directory at or above it holding
    specforge.json, as the CLI and MCP find it, spelled as the client
    spells the workspace; a workspace in no project opens as a loose
    root, and a workspace path that does not exist opens no project.
  """
  verify unit "initialize response includes semantic token legend"
  verify unit "initialize response advertises incremental sync"
  verify unit "initialize response includes completion trigger characters"
  verify unit "zero extensions produces structural-only capabilities"
  verify contract "LSP Initialize: LSP initialization holds — extensions_loaded, capabilities_reflect_extensions, semantic_legend_populated, incremental_sync_advertised, lsp_initialized_emitted"
  verify unit "initialize response includes server_info with name and version"
  verify unit "a workspace root inside a project opens the enclosing project"
}

behavior lsp_shutdown "LSP Shutdown" {
  features   [lsp_lifecycle]
  invariants [incremental_correctness]
  category   command
  types      [Graph]
  ports      [LspProtocol]
  produces   [lsp_shutdown_complete]
  requires {
    lsp_initialized_fired "LSP server has been initialized and is in a running state"
  }
  ensures {
    resources_released            "all held resources (graph, file watchers, Wasm engines, document buffers) are released"
    post_shutdown_rejected        "all subsequent requests except exit return InvalidRequest errors"
    no_disk_persistence           "no state is persisted to disk during shutdown"
    lsp_shutdown_complete_emitted "lsp_shutdown_complete event is produced"
  }
  contract   """
    When the LSP server receives a shutdown request, it MUST release
    all held resources: the in-memory graph, file watchers, extension
    Wasm engines, and open document buffers. After shutdown, all
    subsequent requests except exit MUST return InvalidRequest errors.
    The server MUST NOT persist any state to disk during shutdown.
  """
  verify unit "shutdown releases in-memory graph"
  verify unit "shutdown releases Wasm engines"
  verify unit "requests after shutdown return InvalidRequest"
  verify contract "LSP Shutdown: LSP shutdown holds — lsp_initialized_fired, resources_released, post_shutdown_rejected, no_disk_persistence, lsp_shutdown_complete_emitted"
}

behavior document_open_close "Document Open/Close" {
  features   [lsp_lifecycle]
  invariants [incremental_correctness, lsp_state_concurrency_safety]
  category   command
  ports      [LspProtocol]
  produces   [file_changed]
  requires {
    lsp_initialized_fired "LSP server has been initialized and is ready to receive notifications"
  }
  ensures {
    document_tracked      "open/close state of the document is correctly reflected in the open document set"
    file_changed_emitted  "file_changed event is produced on didOpen to trigger initial compilation"
    closed_file_published "the closed document's file is published as the project reports it, in place of its buffer's diagnostics"
    closed_file_from_disk "a closed project source is compiled from disk again; any other closed file leaves the project"
  }
  contract   """
    When the LSP server receives a textDocument/didOpen notification,
    it MUST register the document in its open document set and trigger
    an initial compilation for diagnostics. When the server receives a
    textDocument/didClose notification, it MUST remove the document from
    its open document set. The buffer is no longer the truth for its
    file: a project source MUST be compiled from the file on disk again
    (unsaved edits are dropped), and any other file (outside the spec root,
    excluded, inside a nested project, or any file when no project is
    open) MUST leave the project.
    While a document is open its buffer is the truth for its file,
    whatever happens to the file on disk, its deletion included (the
    session holds it, hold_editor_buffers).
    The closed document's file MUST then be published once, as the project
    reports it, in place of the buffer's diagnostics: a project source
    keeps the errors its file on disk has, and a file that left the project
    is cleared. The server MUST track which documents are open to determine
    the scope of incremental recompilation.
    A document is the file its URI names, read percent-decoded: a client
    may encode characters the server's own spelling leaves as they are
    (VS Code writes `@` as `%40` and a Windows drive's `:` as `%3A`), and
    both spellings MUST name one open document, one list of published
    diagnostics and one staleness. Every answer and publish about the file
    MUST name it by the URI the client opened it with. An environment load
    that moves the spec root keeps every open document.
  """
  verify unit "didOpen registers document and triggers compilation"
  verify unit "didClose removes document and clears diagnostics"
  verify unit "only open documents participate in incremental compilation"
  verify unit "rapid open and close cycles do not corrupt state"
  verify unit "closing a document compiles its file from disk again, dropping its unsaved edits"
  verify unit "closing a document outside a project drops its file from the project"
  verify unit "closing a project source publishes what the project reports for its file"
  verify unit "closing a saved document leaves nothing to catch up on"
  verify unit "closing a document that does not parse runs the checks its typing skipped"
  verify unit "an open document's entities stay when its file is deleted, until it is closed"
  verify unit "a document is one file whatever spelling of its URI the client sends"
  verify unit "an open document stays itself when an environment load moves its key"
  verify integration "a document in a nested project is no source of the workspace project and gets none of its diagnostics"
  verify contract "Document Open/Close: document open/close holds — lsp_initialized_fired, document_tracked, file_changed_emitted, closed_file_published, closed_file_from_disk"
}

// Event consumer chain: didChange -> file_changed -> debounce window ->
// incremental rebuild (see shared_incremental_pipeline and behaviors/incremental.spec).
behavior handle_text_document_change "Handle Text Document Change" {
  types      [SpecFile, SourceSpan, ContentChangeEvent]
  category   command
  ports      [LspProtocol]
  produces   [file_changed]
  invariants [incremental_correctness, lsp_response_latency, lsp_survives_its_faults]
  requires {
    document_open "the document has been opened via didOpen and is in the open document set"
  }
  ensures {
    buffer_updated       "in-memory document buffer reflects the incremental text edits"
    file_changed_emitted "file_changed event is produced to schedule recompile"
    event_loop_unblocked "the LSP event loop is not blocked by the handler"
  }
  contract   """
    On textDocument/didChange notification, the LSP MUST apply
    incremental text edits to the in-memory document buffer, trigger
    incremental_document_sync, and schedule a recompile via the shared
    incremental pipeline. The edits to every document that arrive within
    one debounce window MUST be applied as one update and published once,
    so an edit the editor applies to several files at once (a rename) never
    publishes the diagnostics of a half-applied edit. The handler MUST NOT
    block the LSP event loop.
  """
  verify unit "didChange applies incremental edits to buffer"
  verify unit "didChange triggers incremental recompile"
  verify unit "edits to several documents in one debounce window are one update and one publication"
  verify contract "Handle Text Document Change: text document change holds — document_open, buffer_updated, file_changed_emitted, event_loop_unblocked"
}

behavior go_to_definition "Go-to-Definition" {
  features   [go_to_definition_and_references]
  category   query
  invariants [
    reference_resolution_completeness,
    lsp_response_latency,
    zero_domain_knowledge_core,
    cursor_names_one_entity,
  ]
  types      [EntityId, SourceSpan]
  ports      [LspProtocol]
  requires {
    graph_available "in-memory graph is built and contains resolved entity declarations"
  }
  ensures {
    declaration_site_returned "the file path, line, and column of the entity declaration block header are returned"
    name_selected             "the entity's name token is the definition's selection range"
  }
  contract   """
    When a user Ctrl+clicks on an entity ID in a .spec file, the LSP
    MUST navigate to the declaration site of that entity. The declaration
    site MUST include the file path, line, and column of the entity's
    block header. On a use statement, the cursor on a binding's imported
    name that names an entity goes to that entity; anywhere else on the
    statement, to the imported file.
  """
  verify unit "go-to-def navigates to entity declaration"
  verify unit "go-to-def on non-existent ID returns no result"
  verify integration "go-to-def works across files"
  verify unit "source spans convert from 1-based to 0-based for LSP"
  verify unit "the definition's selection is the entity's name token"
  verify unit "a use binding's imported name goes to the entity it names"
  verify unit "a definition is a location link for a client that declares linkSupport, else a location at the name"
  verify unit "the entity after a body being typed is navigated to"
  verify contract "Go-to-Definition: go-to-definition holds — graph_available, declaration_site_returned"
}

behavior find_all_references "Find All References" {
  features   [go_to_definition_and_references]
  category   query
  invariants [
    reference_resolution_completeness,
    lsp_response_latency,
    zero_domain_knowledge_core,
    cursor_names_one_entity,
  ]
  types      [EntityId, SourceSpan]
  ports      [LspProtocol]
  requires {
    graph_available "in-memory graph is built and contains resolved entity references"
  }
  ensures {
    all_references_returned "every location across all .spec files where the entity is referenced is returned"
    declaration_included    "the entity's own declaration site is included when the request asks for it"
  }
  contract   """
    When a user triggers find-references on an entity ID, the LSP MUST
    return every location across all .spec files where that entity's ID is
    written as a reference: another entity's field, or a file's defaults
    block whether or not a block takes it; the identifier token as written, one
    location per occurrence. What the entity itself references is not a reference
    to it. The entity's own declaration (its name token) MUST be included
    when the request asks for it (includeDeclaration) and MUST NOT be
    otherwise. The LSP and MCP specforge.find_references answer from the
    same navigation (specforge_ops::read::navigate).
  """
  verify unit "find-refs returns all reference sites"
  verify unit "find-refs includes the declaration site"
  verify unit "find-refs across multiple files"
  verify unit "find-refs excludes what the entity itself references"
  verify unit "find-refs omits the declaration when the request excludes it"
  verify unit "each reference is the identifier token as written"
  verify unit "a parameter's name is not a reference to the type it spells"
  verify unit "a name written in a field and in a method signature is one location each"
  verify unit "reference ranges are UTF-16 after multi-byte text"
  verify contract "Find All References: find all references holds — graph_available, all_references_returned, declaration_included"
}

behavior hover_information "Hover Information" {
  features   [hover_and_autocomplete]
  category   query
  invariants [
    zero_domain_knowledge_core,
    reference_resolution_completeness,
    lsp_response_latency,
    cursor_names_one_entity,
  ]
  types      [EntityId, Node, KindRegistryEntry, FieldRegistryEntry, HoverContent]
  ports      [LspProtocol]
  requires {
    graph_available         "in-memory graph is built and entity nodes are queryable"
    kind_registry_available "KindRegistry is populated with extension-defined entity metadata"
  }
  ensures {
    hover_delegated   "hover content generation is delegated to provide_extension_entity_hover"
    markdown_produced "the returned hover content is formatted as markdown"
  }
  // Delegation: hover_information delegates ALL entity metadata, reference
  // counts, and field summaries to provide_extension_entity_hover
  // (behaviors/zero-entity-lsp.spec). This behavior is the LSP entry point;
  // provide_extension_entity_hover is the authoritative owner of hover content.
  contract   """
    When a user hovers over an entity ID, the LSP MUST delegate to
    provide_extension_entity_hover (behaviors/zero-entity-lsp.spec) for
    all extension-aware hover content: entity kind, title, source extension,
    testability, headline summary, coverage, reference counts, field values
    and the diagnostics about the entity.
    This behavior is responsible only for dispatching the hover request
    and returning the formatted result. The hover content MUST be
    formatted as markdown. Field help (the field's declared type, named
    as E061 names it — an enum field with its declared values — and its
    description) answers when the cursor is on a field's name in an
    entity's own body, nowhere else.
  """
  verify unit "hover delegates to provide_extension_entity_hover"
  verify unit "hover returns markdown-formatted content"
  verify unit "hover lists a type member named title"
  verify unit "field help answers only on a field's name"
  verify unit "field help names a field's type as E061 does, an enum's declared values included"
  verify unit "field help says what an absent value counts as"
  verify contract "Hover Information: hover information holds — graph_available, kind_registry_available, hover_delegated, markdown_produced"
}

behavior hover_diagnostic "Hover a Diagnostic" {
  category   query
  invariants [lsp_response_latency]
  types      [HoverContent]
  ports      [LspProtocol]
  requires {
    diagnostics_published "the file's diagnostics are published"
  }
  ensures {
    diagnostic_explained "hovering inside a published diagnostic shows its code, the catalogue's title and explanation, and the docs link"
  }
  contract   """
    When a user hovers inside the range of a published diagnostic, the LSP
    MUST show, as markdown and before any entity hover, the diagnostic's
    code with the catalogue's title, the catalogue's explanation and the
    link to the code's section of docs/diagnostics.md. A code the catalogue
    does not have shows its code and message only. The entity hover that
    follows does not list that diagnostic again. A region the formatter
    left unformatted (W142) is such a diagnostic while the document is the
    text formatting reported it on. A diagnostic of the compile is such a
    diagnostic while the document is the text the project was compiled from:
    over a buffer typed since, its range is a position in another text, and
    the hover shows none of the compile's diagnostics.
  """
  verify unit "hovering a diagnostic shows its catalogued title and explanation"
  verify unit "an uncatalogued diagnostic's hover shows its code and message only"
  verify unit "the diagnostic under the cursor comes before the entity's hover, which does not list it again"
  verify unit "hovering a region the formatter left unformatted shows its W142"
  verify unit "hovering a buffer typed since the compile shows none of the compile's diagnostics"
}

// Completion behaviors (autocomplete_entity_ids, complete_field_names, complete_keywords)
// also cover verify declaration editing — verify kind names are suggested via the
// same completion pipeline.
behavior autocomplete_entity_ids "Autocomplete Entity IDs" {
  features   [hover_and_autocomplete]
  category   query
  invariants [zero_domain_knowledge_core, reference_resolution_completeness, lsp_response_latency]
  types      [EntityId, CompletionItem]
  ports      [LspProtocol]
  requires {
    graph_available          "in-memory graph is built and entity IDs are queryable"
    field_registry_available "FieldRegistry is populated with extension-declared field metadata"
  }
  ensures {
    matching_ids_suggested        "matching entity IDs from the current scope are returned as completions"
    target_kind_filtering_applied "suggestions are filtered by target_kind constraint when present in FieldRegistry"
  }
  contract   """
    When a user types inside a reference list (e.g., deps [...]),
    the LSP MUST suggest matching entity IDs from the current scope.
    Suggestions MUST include the entity title and kind. When the
    enclosing field has a target_kind constraint in the FieldRegistry
    (e.g., a "deps" field with a target_kind constraint filters to
    entities of that kind), suggestions MUST be filtered to entities of
    that kind. When no target_kind constraint exists, all entity IDs
    MUST be suggested. Entity IDs are globally unique regardless of
    kind — the filtering is a UX optimization based on
    extension-declared field metadata, not a compiler requirement.
    Suggestions are ranked by the shared ranking over IDs and titles
    (exact, prefix, substring, then within the fuzzy threshold), as
    workspace symbols and MCP specforge.search rank. The value of a
    single-reference field is completed the same way, filtered by the
    field's target_kind. Entity IDs are suggested only in a reference
    list, never in a string list, whose items are strings: there only the
    scheme ref IDs of refs are suggested, which the core links to their
    ref from any list. Each suggestion MUST carry an edit over the word
    under the cursor (a scheme ref ID whole), an insert-and-replace edit
    when the client supports one, so accepting it replaces exactly that
    word.
  """
  verify unit "autocomplete suggests matching IDs"
  verify unit "suggestions include entity titles and kinds"
  verify unit "suggestions filtered by target_kind when FieldRegistry has constraint"
  verify unit "all IDs suggested when no target_kind constraint exists"
  verify unit "a single-reference field's value suggests the IDs of its target kind"
  verify unit "a string list's items suggest no entity IDs but scheme ref IDs"
  verify integration "accepting an ID replaces the word under the cursor, a scheme ref ID whole"
  verify contract "Autocomplete Entity IDs: entity ID autocomplete holds — graph_available, field_registry_available, matching_ids_suggested, target_kind_filtering_applied"
}

behavior prepare_rename "Prepare Rename" {
  features   [rename_refactoring]
  category   query
  invariants [
    entity_id_uniqueness,
    lsp_response_latency,
    zero_domain_knowledge_core,
    cursor_names_one_entity,
  ]
  types      [EntityId, SourceSpan]
  ports      [LspProtocol]
  requires {
    graph_available "in-memory graph is built and entity declarations are locatable"
  }
  ensures {
    token_range_returned    "the range of the renameable token is returned when cursor is on an entity ID"
    non_renameable_rejected "rename is reported as not available when cursor is not on a renameable token"
  }
  contract   """
    When a user initiates a rename, the LSP MUST first respond to
    textDocument/prepareRename to validate that the cursor is on a
    renameable token (entity ID in a declaration or reference). The
    response MUST include the range of the token to be renamed. If
    the cursor is not on a renameable token, the response MUST indicate
    that rename is not available at that position. While the document is
    not the text the project was compiled from, prepareRename MUST be
    refused as ContentModified (-32801): a range in the compiled text is not
    a range in the buffer. A document the project does not build (outside the
    spec root, or excluded) has nothing to rename: prepareRename answers that
    rename is not available there, never ContentModified, which no compile
    would end. A document opened since the compile waits for it.
  """
  verify unit "prepare rename on entity ID returns token range"
  verify unit "prepare rename on non-renameable token returns not available"
  verify unit "prepare rename answers on a default no block takes"
  verify unit "prepare rename answers on a reference in a duplicate declaration"
  verify unit "prepare rename over a buffer typed since the compile is refused as content modified"
  verify unit "prepare rename from a document the project does not build answers that rename is not available"
  verify unit "prepare rename from a document opened since the compile waits for it"
  verify contract "Prepare Rename: prepare rename holds — graph_available, token_range_returned, non_renameable_rejected"
}

behavior rename_entity_id "Rename Entity ID" {
  features   [rename_refactoring]
  invariants [
    entity_id_uniqueness,
    lsp_response_latency,
    rename_atomicity,
    zero_domain_knowledge_core,
    cursor_names_one_entity,
  ]
  category   mutation
  types      [EntityId, TextEdit, WorkspaceEditResult]
  ports      [LspProtocol]
  produces   [entity_renamed]
  requires {
    graph_available      "in-memory graph is built and all references are resolved"
    prepare_rename_ready "prepareRename has validated the cursor is on a renameable token"
  }
  ensures {
    all_references_updated "entity declaration and every reference across all .spec files are updated"
    rename_atomic          "all files are updated or none are (atomic operation)"
    entity_renamed_emitted "entity_renamed event is produced on successful rename"
  }
  contract   """
    When a user renames an entity ID via the LSP, the system MUST
    update the entity declaration and every reference to it across
    all .spec files. The rename MUST be atomic — all files are updated
    or none are. The new ID follows the same rule as the MCP rename tool's:
    a name that is not a legal entity ID, that is a reserved word (a structural
    keyword or a declared kind's keyword, E013), or that is taken, MUST be refused
    with an error saying why. The edits are exactly every token that names the
    entity: its declaration's name and each reference, a reference written in
    a defaults block that no block takes included, as the editor's
    find-references returns them;
    text in strings, comments and verify statements that mentions the ID
    is not a reference and is not edited. The edits are positions in the
    text the project was compiled from: a rename over a file whose text (an
    open buffer, else the file on disk) is no longer that text MUST be
    refused as ContentModified (-32801), never applied from stale positions;
    so is a rename asked from a document typed since the compile. A rename
    asked from a document the project does not build answers no edit.
    A rename over a project whose specforge.json does not say which files
    are its sources MUST be refused with config_invalid's reason, as the
    MCP rename tool refuses it.
  """
  verify unit "rename updates declaration and all references"
  verify unit "rename leaves strings, comments and verify texts alone"
  verify unit "rename edits a type named in a method signature and no parameter name"
  verify unit "rename is atomic — all or nothing"
  verify unit "rename is refused as content modified when a file it edits changed since the compile"
  verify unit "rename from a buffer typed since the compile is refused as content modified"
  verify unit "rename from a document the project does not build answers no edit"
  verify unit "rename across multiple files"
  verify unit "rename rejects new name that duplicates existing entity ID"
  verify unit "rename to an illegal entity ID is refused with why"
  verify unit "rename to a reserved word is refused with why"
  verify unit "rename refuses a reserved word as the new ID before anything is planned"
  verify unit "a rename is refused when specforge.json does not say which files are the project's sources"
  verify contract "Rename Entity ID: entity rename holds — graph_available, prepare_rename_ready, all_references_updated, rename_atomic, entity_renamed_emitted"
}

behavior emit_live_diagnostics "Live Diagnostics" {
  invariants [
    multi_error_collection,
    incremental_correctness,
    diagnostic_determinism,
    lsp_response_latency,
    zero_domain_knowledge_core,
  ]
  category   command
  types      [DiagnosticBag]
  ports      [LspProtocol, Editor]
  consumes   [incremental_rebuild_complete] // delegates to the shared incremental pipeline's rebuild event
  requires {
    lsp_initialized_fired "LSP server has been initialized and the incremental pipeline is ready"
    graph_available       "in-memory graph is built and can be incrementally updated"
  }
  ensures {
    diagnostics_pushed "updated diagnostics are pushed to the editor after each file change"
    latency_enforced   "error squiggles appear within 100ms of the user stopping typing"
  }
  contract   """
    The LSP MUST provide real-time diagnostics as the user types.
    After each file change, the LSP MUST incrementally recompile and
    push updated diagnostics to the editor. Error squiggles MUST appear
    within 100ms of the user stopping typing. A diagnostic without a span
    that is about entities (its data names them, as a reference cycle's
    does) MUST be published at the first one's name, with related
    information at each other's. Each publish sends every file that has
    diagnostics, and an empty list to each file that had some and has none
    now. Each file's list carries the version of the buffer the project was
    compiled from, none for a file compiled from disk, so a client drops a
    list a newer edit superseded. A file's list is published under the one
    URI the client names it by (the one it opened the document with, else
    the file's own file: URI), never under two spellings of one file. The
    list is the project's diagnostics for the file and, while the open
    document is the text formatting reported on, the formatter's
    (lsp_format_document).
    A diagnostic without a span about no
    entity is published on the
    document being edited, else on the last one such a diagnostic went on
    while it is open, else on the first open document. W143 (a define
    block, which registers nothing) MUST be published with the Unnecessary
    tag, so editors fade the block.
  """
  verify unit "diagnostics update after file change"
  verify unit "code actions act on the diagnostics last published for the document"
  verify unit "a publish clears the files whose diagnostics are gone"
  verify unit "a publish is labelled with the version of the buffer the project was compiled from"
  verify unit "each file's diagnostics are published under the URI the client named it by"
  verify integration "diagnostics appear within 100ms"
  verify unit "a spanless diagnostic about entities is published at the first one's name"
  verify unit "a spanless diagnostic is published at its entity's new name when only the entity moved"
  verify unit "a diagnostic is published on the file its span names"
  verify unit "a diagnostic about no entity is published on the edited document"
  verify unit "a define block's W143 is published as unnecessary code"
  verify contract "Live Diagnostics: live diagnostics holds — lsp_initialized_fired, graph_available, diagnostics_pushed, latency_enforced"
}

behavior code_actions_for_missing_verify "Code Actions for Missing Verify" {
  invariants [
    zero_domain_knowledge_core,
    testable_entity_classification,
    lsp_response_latency,
    lsp_text_edit_non_overlapping,
  ]
  category   validation
  types      [KindRegistryEntry, CodeAction, CodeActionKind]
  ports      [LspProtocol]
  features   [extension_driven_code_actions, code_actions]
  requires {
    kind_registry_available "KindRegistry is populated with testable flags and allowed_verify_kinds"
    graph_available         "in-memory graph is built and entity verify declarations are queryable"
  }
  ensures {
    quickfix_offered      "code action with CodeActionKind::QuickFix is offered on untested testable entities"
    verify_stubs_produced "generated stubs use allowed_verify_kinds from KindRegistry, not hardcoded kinds"
    no_code_generated     "no test source files or application code are generated"
  }
  contract   """
    The LSP SHOULD offer code actions on entities that declare no verify
    statements and either owe obligations (a no_verify_statements rule
    applies to their kind) or are of a testable kind; an entity a union
    body or an exempting flag exempts is offered none, since a stub there
    is not an obligation it owes (a union has no block to hold one). A
    stub fixes the diagnostic of the rule that reports its entity, and
    none when no rule reports it. The code actions offered for a request are those
    whose diagnostic, or whose entity, overlaps the requested range. The code action MUST add verify stub declarations
    to the entity block in the .spec file, using verify kinds from the
    entity kind's allowed_verify_kinds in the KindRegistry (not hardcoded
    kinds). If no allowed_verify_kinds are specified, the stub MUST use
    the first verify kind from the extension's verify_kinds list. The
    generated stub MUST use the format
    verify <kind> "<entity_id> — TODO" where <kind> is the first entry
    from the entity kind's allowed_verify_kinds. The code action MUST use
    CodeActionKind::QuickFix. The LSP MUST NOT generate test source files
    or application code — SpecForge provides context, agents produce
    output. The set of testable kinds comes from extension manifests, not
    hardcoded logic. The stub is the typed add_verify edit (ADR 0151, ADR
    0161) planned over the text the project was compiled from: laid out
    with the file's format configuration, and not offered when its plan is
    refused. A client that resolves code actions receives a stub's edits
    when it resolves it.
  """
  verify unit "code action offered on untested testable entity"
  verify unit "generated verify stubs added to entity block in .spec file"
  verify unit "verify stub uses allowed_verify_kinds from KindRegistry"
  verify unit "stub format is verify <kind> entity_id TODO"
  verify unit "code action kind is QuickFix"
  verify unit "no test source files or application code generated"
  verify unit "code actions are those whose diagnostic or entity overlaps the requested range"
  verify unit "no verify stub is offered for an entity a union body or an exempting flag exempts"
  verify unit "a verify stub fixes the diagnostic that reports its entity, or none when nothing reports it"
  verify unit "the verify stub is laid out with the file's format configuration"
  verify unit "a client that resolves code actions gets a stub's edits on resolve"
  verify contract "Code Actions for Missing Verify: missing verify code actions holds — kind_registry_available, graph_available, quickfix_offered, verify_stubs_produced, no_code_generated"
}

behavior outline_view "Outline View" {
  features   [outline_and_symbol_search]
  category   query
  invariants [zero_domain_knowledge_core, reference_resolution_completeness, lsp_response_latency]
  types      [Node, EntityId, KindRegistryEntry, DocumentSymbolEntry, SymbolKind]
  ports      [LspProtocol]
  requires {
    graph_available         "in-memory graph is built and entities in the current file are queryable"
    kind_registry_available "KindRegistry is populated with lsp_icon metadata for SymbolKind mapping"
  }
  ensures {
    all_entities_listed   "all entities in the current file are represented in the outline tree"
    symbol_kind_delegated "SymbolKind for each entry is determined by provide_extension_defined_lsp_icons, not hardcoded"
  }
  contract   """
    The LSP MUST provide an outline view showing all entities in the
    current file as a tree. Each entry MUST show the entity kind,
    ID, and title. The SymbolKind for each entry MUST be determined
    by delegating to provide_extension_defined_lsp_icons which reads
    the lsp_icon field from the KindRegistry — the outline MUST NOT
    hardcode any SymbolKind mappings for specific entity types. Test
    coverage indicators SHOULD be shown when coverage data is available.
    The tree is the one specforge.outline returns: entities in line order,
    each entity's method members as its children, each entry selecting its
    name.
  """
  verify unit "outline lists all entities in file"
  verify unit "outline shows entity kind, ID, and title"
  verify integration "the outline nests an entity's methods as the MCP outline does"
  verify unit "outline uses extension-defined SymbolKind from KindRegistry lsp_icon"
  verify contract "Outline View: outline view holds — graph_available, kind_registry_available, all_entities_listed, symbol_kind_delegated"
}

behavior workspace_symbol_search "Workspace Symbol Search" {
  features   [outline_and_symbol_search]
  category   query
  invariants [zero_domain_knowledge_core, reference_resolution_completeness, lsp_response_latency]
  types      [EntityId, SourceSpan, KindRegistryEntry, WorkspaceSymbolEntry]
  ports      [LspProtocol]
  requires {
    graph_available         "in-memory graph is built and all workspace entities are indexed"
    kind_registry_available "KindRegistry is populated for SymbolKind resolution"
  }
  ensures {
    matching_entities_returned "matching entities across all .spec files are returned for the query"
    symbol_kind_delegated      "SymbolKind for each result is determined by provide_extension_defined_lsp_icons"
  }
  contract   """
    The LSP MUST support workspace symbol search. Typing an entity ID or
    title fragment, exactly or within the fuzzy threshold, MUST return
    matching entities across all .spec files in the workspace, ranked by
    the shared ranking (specforge_ops::read::navigate, the one MCP
    specforge.search and completion use). The SymbolKind for each result
    MUST be determined by provide_extension_defined_lsp_icons.
  """
  verify unit "search by ID prefix returns matches"
  verify unit "search by title fragment returns matches"
  verify unit "a misspelled query within the fuzzy threshold finds the entity"
  verify unit "search results use extension-defined SymbolKind"
  verify contract "Workspace Symbol Search: workspace symbol search holds — graph_available, kind_registry_available, matching_entities_returned, symbol_kind_delegated"
}

// Delegates to behaviors/incremental.spec pipeline: watch_file_system_for_changes ->
// debounce_file_changes -> invalidate_changed_files -> rebuild_affected_subgraph ->
// emit_incremental_diagnostics
behavior shared_incremental_pipeline "Shared Incremental Pipeline" {
  features   [incremental_compilation, live_diagnostics]
  invariants [
    incremental_correctness,
    zero_domain_knowledge_core,
    watch_mode_response_latency,
    lsp_extension_reload_consistency,
    lsp_survives_its_faults,
  ]
  category   command
  types      [Graph]
  consumes   [incremental_rebuild_complete]
  // Delegates to emit_incremental_diagnostics which produces incremental_diagnostics_complete.
  // This behavior does not independently trigger the event.
  ports      [LspProtocol]
  requires {
    incremental_rebuild_complete_fired "incremental_rebuild_complete event has fired, confirming the rebuild pipeline produced updated results"
  }
  ensures {
    shared_graph_updated     "the in-memory graph shared between diagnostics, navigation, and completion is updated"
    diagnostics_pushed       "incremental diagnostics are pushed to the client after pipeline completes"
    pipeline_parity_enforced "the parse-validate-emit pipeline is identical between LSP and CLI watch mode"
  }
  contract   """
    The LSP server MUST share the same incremental compilation pipeline
    as specforge watch. File change MUST trigger an identical
    parse-validate-emit pipeline in both watch mode and LSP, using the
    same debounce window and validator dispatch order. The in-memory
    graph MUST be shared between diagnostics, navigation, and completion
    features. There MUST NOT be separate compilation passes for each
    LSP feature.
  """
  verify unit "LSP and watch share the same graph"
  verify integration "graph update serves all LSP features"
  verify integration "the LSP publishes the diagnostics specforge check reports"
  verify unit "a reload applies every open buffer again, in one update"
  verify unit "opening a project with open documents runs the checks once"
  verify property "CLI and LSP share identical debounce window"
  verify property "CLI and LSP share identical validator dispatch order"
  verify contract "Shared Incremental Pipeline: shared incremental pipeline holds — incremental_rebuild_complete_fired, shared_graph_updated, diagnostics_pushed, pipeline_parity_enforced"
}

behavior provide_semantic_tokens "Provide Semantic Tokens" {
  features   [semantic_tokens]
  category   query
  invariants [zero_domain_knowledge_core, lsp_response_latency]
  types      [SourceSpan, KindRegistryEntry, SemanticTokenLegendEntry, SemanticToken]
  ports      [LspProtocol]
  requires {
    graph_available         "in-memory graph is built and entity kinds are resolvable"
    kind_registry_available "KindRegistry is populated with semantic_token classification metadata"
  }
  ensures {
    tokens_classified            "all tokens in .spec files are classified according to their semantic role"
    structural_keywords_enforced "use, define, and verify are always classified as keyword regardless of extensions"
    extension_delegation_applied "entity ID declaration classification follows provide_extension_entity_semantic_tokens"
  }
  contract   """
    The LSP MUST provide semantic tokens for .spec files. The legend is
    sent at initialize, before extensions load, so it MUST be static: the
    full list of standard LSP semantic token types. Entity kind keywords
    MUST be classified as "type". An entity ID at its declaration site
    MUST be classified with its kind's semantic_token from the
    KindRegistry when the kind declares one that the legend contains, and
    as "function" otherwise; it MUST carry the declaration modifier.
    provide_extension_entity_semantic_tokens (behaviors/zero-entity-lsp.spec)
    states this rule from the extension's side. Structural keywords (use,
    define, verify) MUST always be classified as "keyword". Triple-quoted
    strings MUST be classified as strings. Enhanced fields from entity
    enhancements MUST be classified as property. For entity kinds with
    registered grammar contributions, semantic token classification MAY
    delegate to the extension grammar for finer-grained highlighting
    within entity bodies. Semantic token updates MUST use the shared
    incremental pipeline. The LSP does not subscribe to watch-mode graph
    deltas: it recompiles on its own document changes, and after a
    recompile whose graph differs from the previous one in anything that
    affects tokens (an entity ID, kind or title, the KindRegistry's
    semantic_token classification, or a field's declared type) it MUST send
    workspace/semanticTokens/refresh so the client re-requests tokens. It
    MUST send the refresh only to a client that declared
    workspace.semanticTokens.refreshSupport at initialize, and MUST NOT
    send it after a recompile that changed nothing token-relevant.
    Classification reads the document's lexemes and their block structure:
    strings and comments hold no other token, a comment after code on its
    line is a comment, the fields after a nested block are classified like
    the ones before it, and a define block's name is not a declaration
    (define blocks register nothing, ADR 0005). A line that starts a
    top-level form of the grammar (lex_spec_text) closes every body, block
    and list left open before it, so a union or an inline ref after a body
    being typed is classified as its own form; a triple-quoted string left
    unclosed ends before such a line, where the parser resumes, so the
    blocks after it are classified as blocks, not as the string. A reference is classified
    as the token type of the kind of the entity it names (an unresolved one
    as variable), with the reference modifier; an enum field's value is an
    enumMember and a boolean field's value a keyword.
  """
  verify unit "entity ID declaration uses its kind's semantic_token from the KindRegistry"
  verify unit "a titled union is a form and its variants are values"
  verify unit "structural keywords are classified as keyword"
  verify unit "a string list's bare item is no reference"
  verify unit "triple-quoted strings are classified as strings"
  verify unit "a string spanning lines is one string, holding no other token"
  verify unit "entity ID declaration without a declared semantic_token is 'function'"
  verify unit "entity ID declaration whose semantic_token is not in the legend is 'function'"
  verify unit "semantic token legend lists every standard LSP token type"
  verify unit "enhanced fields are classified as property"
  verify unit "reference list items classified as 'variable' with reference modifier"
  verify unit "verify kind classified as enumMember"
  verify unit "comments classified as comment"
  verify unit "number values classified as number"
  verify unit "entity title strings classified as string"
  verify unit "use path classified as string"
  verify contract "Provide Semantic Tokens: semantic tokens holds — graph_available, kind_registry_available, tokens_classified, structural_keywords_enforced, extension_delegation_applied"
  verify unit "entity ID declarations carry the declaration modifier"
  verify unit "entity keywords classified as 'type'"
  verify integration "a recompile that changes the graph asks the client to refresh semantic tokens"
  verify integration "a recompile that changes nothing token-relevant sends no semantic token refresh"
  verify integration "no semantic token refresh is sent to a client without refreshSupport"
  verify unit "every field of an entity body is classified, after a nested block too"
  verify unit "a comment after code on its line is classified as comment"
  verify unit "a define block's name is not a declaration"
  verify unit "classification agrees with the grammar on every spec file of the repository"
  verify unit "a reference is classified as the kind of the entity it names"
  verify unit "an enum field's value is an enumMember and a boolean field's value a keyword"
  verify unit "a union and an inline ref after an unclosed body are classified as top-level forms"
  verify unit "the forms after an unclosed triple-quoted string are classified as the parser reads them"
  verify unit "the document resumes at every top-level form of the repository's spec after an unclosed body"
}

behavior complete_field_names "Complete Field Names" {
  features   [hover_and_autocomplete, extension_driven_lsp]
  category   query
  invariants [zero_domain_knowledge_core, lsp_response_latency]
  types      [EntityId, FieldRegistryEntry, CompletionItem]
  ports      [LspProtocol]
  requires {
    field_registry_available "FieldRegistry is populated with field definitions for all registered entity kinds"
    cursor_inside_entity     "the cursor is positioned inside an entity block body"
  }
  ensures {
    fields_suggested  "valid field names for the current entity kind are returned as completions"
    snippets_informed "completion snippets reflect field types from the FieldRegistry"
  }
  contract   """
    When a user types inside an entity block body, the LSP MUST query
    the FieldRegistry for valid field names for the current entity kind
    and suggest them as completions. Suggestions MUST be filtered to
    fields registered for the entity kind by its extension manifest.
    Field types from the registry MUST inform the completion snippet
    (e.g., reference fields offer bracket-list scaffolding). The cursor is
    in an entity body when the document's lexemes put it there: brackets,
    braces and quotes inside strings and comments do not count. Inside a
    string, a comment or a nested block nothing is suggested. A field's
    value is completed from its declared type: an enum field's declared
    values, true and false for a boolean field; a string, integer or
    string-list field's value completes nothing.
  """
  verify unit "field name completion uses FieldRegistry for entity kind"
  verify unit "suggestions are filtered by entity kind"
  verify unit "no field name suggestions outside entity blocks"
  verify unit "a bracket inside a string opens no reference list"
  verify unit "nothing is suggested inside a string, a comment or a nested block"
  verify unit "an enum field's value suggests its declared values"
  verify unit "a builtin vocabulary's field completes its values"
  verify unit "a boolean field's value suggests true and false"
  verify contract "Complete Field Names: field name completion holds — field_registry_available, cursor_inside_entity, fields_suggested, snippets_informed"
}

behavior complete_keywords "Complete Keywords" {
  features   [hover_and_autocomplete]
  category   query
  invariants [zero_domain_knowledge_core]
  types      [EntityId, KindRegistryEntry, CompletionItem]
  ports      [LspProtocol]
  requires {
    kind_registry_available "KindRegistry is populated with extension-defined entity kinds"
    cursor_at_top_level     "the cursor is positioned at the top level of a .spec file, outside any entity block"
  }
  ensures {
    keywords_delegated           "keyword completion delegates to complete_extension_defined_keywords for extension-aware results"
    structural_keywords_included "use is always included in suggestions regardless of extensions; define never is"
  }
  contract   """
    When a user types at the top level of a .spec file (outside any entity
    block), the LSP MUST delegate to complete_extension_defined_keywords
    (behaviors/zero-entity-lsp.spec) for extension-aware keyword completions.
    A line that starts a top-level form of the grammar (lex_spec_text) ends
    any entity block left unclosed before it, so the keywords are suggested
    after such a form.
    The structural keyword use MUST always be included in addition to
    extension-defined keywords; define MUST NOT be suggested: it is a
    reserved word whose blocks register nothing (W143, ADR 0005). Each
    suggestion SHOULD include a snippet template for block scaffolding
    based on the kind's field definitions from the FieldRegistry. The
    detail string MUST show the source extension name for each keyword.
    Each kind's item MUST document the kind with its description and its
    inference guide (compute_inference_guide), as markdown; use has no
    documentation.
    After verify in an entity's body, the verify kinds the entity's kind
    allows (its allowed_verify_kinds) MUST be suggested, and nothing when
    the kind takes no verify statements. The registered kinds come from the
    environment (CONTEXT: Environment), which is loaded before any .spec
    file is read, so keyword completion MUST name them as soon as the
    environment is loaded, while the workspace is still being indexed.
  """
  verify unit "keyword completion includes all registered kinds"
  verify unit "a kind keyword's completion documents the kind with its description and inference guide"
  verify unit "keyword completion answers with the registered kinds as soon as the environment is loaded, before indexing ends"
  verify unit "use is always suggested and define never is"
  verify unit "verify suggests the kinds the entity's kind allows"
  verify unit "keywords are suggested after a top-level form that follows an unclosed body"
  verify unit "no keyword suggestions inside entity blocks"
  verify unit "snippet templates based on kind field definitions"
  verify contract "Complete Keywords: keyword completion holds — kind_registry_available, cursor_at_top_level, keywords_delegated, structural_keywords_included"
}

behavior goto_import_definition "Go-to-Definition on Imports" {
  features   [go_to_definition_and_references]
  category   query
  invariants [reference_resolution_completeness, lsp_response_latency, zero_domain_knowledge_core]
  ports      [LspProtocol]
  requires {
    imports_resolved "use import paths are resolved against the sources the project was last compiled from"
  }
  ensures {
    target_file_navigated "navigation jumps to the first line of the resolved target .spec file"
  }
  contract   """
    When a user Ctrl+clicks on a `use` import path (e.g., `use behaviors/core`),
    the LSP MUST navigate to the source it names, resolved as the
    compile resolves imports (resolve_use_imports: relative, bare,
    index.spec, never above the spec root) against the sources the
    project was last compiled from, open documents included, saved or
    not. A path that names no source (a missing file, an excluded one)
    has no definition, whatever is on disk. The definition site MUST be
    the first line of the resolved file.
  """
  verify unit "go-to-def on use path navigates to target file"
  verify unit "go-to-def on non-existent use path returns no result"
  verify contract "Go-to-Definition on Imports: import go-to-definition holds — imports_resolved, target_file_navigated"
  verify unit "go-to-def on a use path names only a project source, an open unsaved document included"
}

behavior code_action_replace_unresolved "Code Action: Replace an Unresolved Reference" {
  category   mutation
  invariants [zero_domain_knowledge_core, lsp_response_latency, lsp_text_edit_non_overlapping]
  types      [CodeAction, Diagnostic, TextEdit]
  ports      [LspProtocol]
  features   [extension_driven_code_actions, code_actions]
  requires {
    did_you_mean_known "the diagnostic's data names a close match"
  }
  ensures {
    token_replaced "the unresolved token, and only it, is replaced by the close match"
  }
  contract   """
    For an unresolved reference (E003) or import (E025) whose data names a
    close match (did_you_mean), the LSP and MCP specforge.suggest_fixes MUST
    offer one quick fix that replaces the unresolved token with the match.
    Target, path and match MUST be read from the diagnostic's data, never
    its message or suggestion text.
  """
  verify unit "an unresolved reference with a close match is replaced at its token"
  verify unit "an unresolved import with a close match is replaced inside its quotes"
  verify unit "the replacement is read from the diagnostic's data, whatever its message says"
}

behavior code_action_create_entity_stub "Code Action: Create Entity Stub" {
  category   mutation
  invariants [zero_domain_knowledge_core, lsp_response_latency, lsp_text_edit_non_overlapping]
  types      [EntityId, KindRegistryEntry, FieldRegistryEntry, CodeAction, Diagnostic]
  ports      [LspProtocol]
  features   [extension_driven_code_actions, code_actions]
  requires {
    graph_available          "in-memory graph is built and E003 diagnostics are available"
    field_registry_available "FieldRegistry is populated with target_kind constraints for kind inference"
  }
  ensures {
    stub_created      "a block of the target kind is created by the typed create edit in the current file: after its last block of that kind, else at its end, laid out with the file's format configuration"
    kind_inferred     "entity kind is inferred from the enclosing field's target_kind constraint in the FieldRegistry"
    no_code_generated "no application code, test files, or implementation scaffolding are generated"
  }
  contract   """
    When an E003 diagnostic (unresolved reference) exists for an entity ID
    that does not exist in any file, the LSP SHOULD offer a code action
    to create a stub entity definition. The entity kind for the stub MUST
    be inferred from the enclosing field's target_kind constraint in the
    FieldRegistry — this is extension-driven metadata, not hardcoded logic.
    When no target_kind constraint exists on the enclosing field, the code
    action MUST NOT be offered (the kind cannot be inferred without domain
    knowledge). The unresolved target, the entity that names it and the
    field it is named in MUST be read from the diagnostic's data
    (DiagnosticData), never parsed from its message, which is
    presentation. The stub MUST be placed in the current file, after the
    file's last block of its kind, else at its end. The stub is the typed
    create edit (ADR 0151): it is offered only when its plan succeeds, so
    an ID the create refuses, or a stub after which the project would
    report a new error (E080), offers none. An error the project already
    reports, whether or not the checks ran since the last edit, never
    withholds a stub. Each field the kind requires is
    written TODO. A client that resolves code actions receives a stub's
    edits when it resolves it. The code action MUST use
    CodeActionKind::Refactor.
    The generated stub MUST contain only the structural entity block
    (keyword, ID, placeholder fields). It MUST NOT generate application
    code, test files, or implementation scaffolding. SpecForge provides
    structural context; agents produce implementation.
  """
  verify unit "code action offered on E003 for non-existent entity"
  verify unit "the stub is read from the diagnostic's data, whatever its message says"
  verify unit "stub uses correct entity kind from FieldRegistry target_kind"
  verify unit "no code action when enclosing field has no target_kind"
  verify unit "stub goes after the last block of its kind in the current file"
  verify unit "code action kind is Refactor"
  verify unit "generated stub contains no application code or test files"
  verify unit "an offered stub adds no error to the project"
  verify unit "stub is laid out with the file's format configuration"
  verify unit "stub writes TODO in each required field"
  verify unit "a stub is offered while another open buffer does not parse"
  verify contract "Code Action: Create Entity Stub: create entity stub holds — graph_available, field_registry_available, stub_created, kind_inferred, no_code_generated"
}

behavior incremental_document_sync "Incremental Document Sync" {
  invariants [incremental_correctness, lsp_response_latency, lsp_utf16_positions]
  category   command
  ports      [LspProtocol]
  requires {
    lsp_initialized_fired "LSP server has been initialized with INCREMENTAL sync kind advertised"
    document_open         "the document is registered in the open document set"
  }
  ensures {
    buffer_consistent      "the in-memory source buffer is identical to the full content after applying changes"
    partial_update_applied "only the changed range is applied, not the entire file content"
  }
  contract   """
    The LSP MUST support incremental text document synchronization
    (TextDocumentSyncKind::INCREMENTAL). On each change event, the LSP
    MUST apply only the changed range to its in-memory source buffer
    rather than replacing the entire file content. The resulting source
    MUST be identical to the full content at all times.
  """
  verify unit "incremental change applies correctly to source buffer"
  verify unit "multiple incremental changes produce correct source"
  verify unit "an edit at any column past its line's end applies at the line's end"
  verify integration "incremental sync reduces transfer size vs full sync"
  verify contract "Incremental Document Sync: incremental document sync holds — lsp_initialized_fired, document_open, buffer_consistent, partial_update_applied"
}

// -- Extension Grammar Highlighting -------------------------------------------
