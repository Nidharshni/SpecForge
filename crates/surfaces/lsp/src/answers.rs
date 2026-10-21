//! Every answer the server gives a request, decided synchronously over the
//! LSP state: the open documents, the project as last compiled (the
//! session, or a copy of it while it is out for an update), the
//! diagnostics last published and what the client declared. The backend
//! takes the state's read lock, calls one of these and returns what it
//! answers; it decides nothing (ADR 0023).

use specforge_common::Sym;
use specforge_ops::authoring::fix::{self, FixQuery, Offer};
use specforge_ops::authoring::format;
use specforge_ops::read::navigate::{EntityQuery, MatchScope, Navigator, find_entities, outline};
use tower_lsp::jsonrpc::{Error, ErrorCode};
use tower_lsp::lsp_types::{
    ClientCapabilities, CodeAction, CodeActionOrCommand, CodeActionResponse, CompletionResponse,
    DocumentSymbolResponse, FormattingOptions, GotoDefinitionResponse, Hover, HoverContents,
    Location, LocationLink, MarkupContent, MarkupKind, Position, PrepareRenameResponse, Range,
    SemanticTokens, SemanticTokensResult, SymbolInformation, TextEdit, Url, WorkspaceEdit,
};

use crate::document::{LineIndex, Target};
use crate::files::Findings;
use crate::navigation::{
    Compiled, EditorText, Unplaced, code_action_kind, fix_to_code_action,
    outline_to_document_symbols, symbol_kind_from_entity,
};
use crate::publish::diagnostic_to_lsp;
use crate::{LspState, hover};

/// What the client declared at initialize that shapes an answer or a
/// reaction.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ClientSupport {
    /// `textDocument.definition.linkSupport`: a definition is a
    /// `LocationLink` (the block, its name selected), else a `Location` at
    /// the name.
    pub definition_links: bool,
    /// `textDocument.documentSymbol.hierarchicalDocumentSymbolSupport`: the
    /// outline nests.
    pub hierarchical_symbols: bool,
    /// `textDocument.completion.completionItem.insertReplaceSupport`: a
    /// completion item's edit inserts over the word's start to the cursor
    /// and replaces the whole word, else it is a plain edit.
    pub insert_replace: bool,
    /// `workspace.semanticTokens.refreshSupport`: only then is the editor
    /// asked to refresh its semantic tokens.
    pub tokens_refresh: bool,
    /// `workspace.didChangeWatchedFiles.relativePatternSupport`: the
    /// watchers are patterns relative to each input's directory, else
    /// absolute globs.
    pub relative_patterns: bool,
    /// `textDocument.codeAction.resolveSupport` names `edit`: a stub's edits
    /// are made when the editor resolves the action, not when it lists them.
    pub code_action_resolve: bool,
}

impl ClientSupport {
    /// What `capabilities` declare of these.
    pub fn of(capabilities: &ClientCapabilities) -> ClientSupport {
        let text_document = capabilities.text_document.as_ref();
        let workspace = capabilities.workspace.as_ref();
        ClientSupport {
            definition_links: text_document
                .and_then(|t| t.definition.as_ref())
                .and_then(|d| d.link_support)
                .unwrap_or(false),
            hierarchical_symbols: text_document
                .and_then(|t| t.document_symbol.as_ref())
                .and_then(|d| d.hierarchical_document_symbol_support)
                .unwrap_or(false),
            insert_replace: text_document
                .and_then(|t| t.completion.as_ref())
                .and_then(|c| c.completion_item.as_ref())
                .and_then(|i| i.insert_replace_support)
                .unwrap_or(false),
            tokens_refresh: workspace
                .and_then(|w| w.semantic_tokens.as_ref())
                .and_then(|t| t.refresh_support)
                .unwrap_or(false),
            relative_patterns: workspace
                .and_then(|w| w.did_change_watched_files.as_ref())
                .and_then(|w| w.relative_pattern_support)
                .unwrap_or(false),
            code_action_resolve: text_document
                .and_then(|t| t.code_action.as_ref())
                .and_then(|c| c.resolve_support.as_ref())
                .is_some_and(|r| r.properties.iter().any(|p| p == "edit")),
        }
    }
}

/// A request refused because the editor's buffer is not the text the
/// project was compiled from (LSP's `ContentModified`, -32801): the answer
/// would be computed on a text the editor no longer has.
fn content_modified(file: &str) -> Error {
    Error {
        code: ErrorCode::ServerError(-32801),
        message: format!("{file} changed since the project was compiled; try again").into(),
        data: None,
    }
}

/// Whether a rename may be asked from the open document `file` (ADR 0023 D7): yes while it is the
/// compiled text; while its compile is still to come, `ContentModified` (the client tries again); a
/// document the project does not build has nothing that renames, and no compile will give it any:
/// `Ok(false)`, answered as no rename.
fn renames_in(compiled: &Compiled, file: &str) -> tower_lsp::jsonrpc::Result<bool> {
    match compiled.editor_text(file) {
        EditorText::Compiled => Ok(true),
        EditorText::Changed => Err(content_modified(file)),
        EditorText::Unbuilt => Ok(false),
    }
}

/// The entity the cursor at `position` of the open document `key` names
/// ([`Compiled::target`]): what references and rename act on.
fn entity_under_cursor(compiled: &Compiled, key: &str, position: Position) -> Option<Sym> {
    match compiled.target(key, position)? {
        Target::Entity { id, .. } => Some(id),
        _ => None,
    }
}

/// The hover at `position` of the open document `uri`: the published
/// diagnostics under it while the document is the compiled text (their
/// ranges are positions in that text) explained from the catalog, then what the cursor names: an entity's
/// facts (the inspect read view reporting what was published; while the
/// project rebuilds, the last complete compile's coverage) or a field's help. Sections are
/// joined by a rule. `None` for a document that is not open or a position
/// with neither.
pub fn hover(state: &LspState, uri: &Url, position: Position) -> Option<Hover> {
    let key = state.key_of(uri);
    let files = state.files();
    let doc = files.document(&key)?;
    let compiled = Compiled::new(state);
    // The compile's diagnostics are positions in the text it read: under the cursor only while the
    // document is that text, through its own index (ADR 0023 D3).
    let mut shown = if compiled.is_stale(&key) {
        Vec::new()
    } else {
        hover::diagnostics_at(files.compile_diagnostics(&key), doc.index(), position)
    };
    // The formatter's findings are positions in the buffer it formatted, which is this one.
    shown.extend(hover::diagnostics_at(
        files.formatter_diagnostics(&key),
        doc.index(),
        position,
    ));
    let published: Vec<specforge_common::Diagnostic> = files.published().cloned().collect();
    let about = match compiled.target(&key, position) {
        Some(Target::Entity { id, .. }) => {
            let view = state.view().reporting(&published);
            specforge_ops::read::inspect::inspect(&view, id.as_str())
                .ok()
                .map(|facts| {
                    let coverage = specforge_ops::read::inspect::coverage(&view, &facts);
                    hover::entity(&facts, &coverage, &shown)
                })
        }
        Some(Target::Field { kind, field }) => {
            hover::hover_field_info(&field, &kind, state.field_registry())
        }
        Some(Target::Import { .. }) | None => None,
    };
    let value = match (hover::diagnostics(&shown), about) {
        (Some(diagnostics), Some(about)) => format!("{diagnostics}{}{about}", hover::SECTION),
        (diagnostics, about) => diagnostics.or(about)?,
    };
    Some(Hover {
        contents: HoverContents::Markup(MarkupContent {
            kind: MarkupKind::Markdown,
            value,
        }),
        range: None,
    })
}

/// What completes at the cursor (ADR 0023 D6).
pub fn completion(state: &LspState, uri: &Url, position: Position) -> Option<CompletionResponse> {
    let cursor = state.document(uri)?.at(position)?;
    let items = crate::completion::items(
        &cursor.completion(),
        &cursor.word_edit(),
        state.client().insert_replace,
        &state.view(),
    );
    Some(CompletionResponse::Array(items))
}

/// Where what the cursor names is declared: a `use` path's file (its
/// start), an entity's block with its name selected as a `LocationLink` for
/// a client declaring links, else a `Location` at its name. `None` when its
/// file has no compiled text.
pub fn definition(
    state: &LspState,
    uri: &Url,
    position: Position,
) -> Option<GotoDefinitionResponse> {
    let compiled = Compiled::new(state);
    let key = state.key_of(uri);
    match compiled.target(&key, position) {
        Some(Target::Import { path }) => {
            // Detached: the compile resolves no import (ADR 0047 D4).
            state.view().root()?;
            // The source the compile resolves the import to, from its
            // first line: no text needed.
            let sources = state.source_keys();
            let target = specforge_resolver::import_target(&sources, &key, &path)?;
            Some(GotoDefinitionResponse::Scalar(Location {
                uri: compiled.uri(target),
                range: Range::default(),
            }))
        }
        Some(Target::Entity { id, origin }) => {
            let definition = compiled.navigator().definition(id.as_str()).ok()?;
            // A definition whose file's text is unknown is not answered
            // (no range of it is honest).
            if state.client().definition_links {
                let (target_range, target_selection_range) = (
                    compiled.range(&definition.block)?,
                    compiled.range(&definition.name)?,
                );
                return Some(GotoDefinitionResponse::Link(vec![LocationLink {
                    origin_selection_range: Some(origin),
                    target_uri: compiled.uri(definition.block.file.as_str()),
                    target_range,
                    target_selection_range,
                }]));
            }
            compiled
                .location(&definition.name)
                .map(GotoDefinitionResponse::Scalar)
        }
        _ => None,
    }
}

/// The references to the entity the cursor names: incoming, its
/// declaration when asked (ADR 0016); an occurrence whose file has no
/// compiled text is left out; each written token once (an inherited
/// reference is one location, ADR 0149 D11), a token in a defaults block
/// no block takes included (ADR 0163); `None` when none.
pub fn references(
    state: &LspState,
    uri: &Url,
    position: Position,
    include_declaration: bool,
) -> Option<Vec<Location>> {
    let compiled = Compiled::new(state);
    let id = entity_under_cursor(&compiled, &state.key_of(uri), position)?;
    let refs = compiled
        .navigator()
        .tokens(id.as_str(), include_declaration)
        .unwrap_or_default();
    // An occurrence whose file's text is unknown is left out.
    let locations: Vec<Location> = refs
        .iter()
        .filter_map(|o| compiled.location(&o.span))
        .collect();
    (!locations.is_empty()).then_some(locations)
}

/// The range of the declaration or reference token under the cursor.
/// Refused as `ContentModified` (-32801) while the document is typed since the
/// compile; `None` for a document the project does not build (ADR 0023 D7).
pub fn prepare_rename(
    state: &LspState,
    uri: &Url,
    position: Position,
) -> tower_lsp::jsonrpc::Result<Option<PrepareRenameResponse>> {
    let file = state.key_of(uri);
    let Some(cursor) = state
        .files()
        .document(&file)
        .and_then(|doc| doc.at(position))
    else {
        return Ok(None);
    };
    let compiled = Compiled::new(state);
    // Any range it gives is a position in a text the editor no longer has
    // while the document is typed in (ADR 0023 D7).
    if !renames_in(&compiled, &file)? {
        return Ok(None);
    }
    // The token as written under the cursor, declaration or reference;
    // nothing else renames.
    let occurrence = cursor.occurrence(&compiled.navigator(), &file);
    Ok(occurrence
        .and_then(|o| compiled.range(&o.span))
        .map(PrepareRenameResponse::Range))
}

/// The shared rename plan (`specforge_ops::authoring::rename::plan`) as one workspace
/// edit, all or nothing: `None` when the cursor names no entity;
/// `invalid_params` with the plan's reason when it is refused (a file without compiled text
/// among them); `ContentModified` when the document or a
/// file it edits is not the compiled text.
pub fn rename(
    state: &LspState,
    uri: &Url,
    position: Position,
    new_name: &str,
) -> tower_lsp::jsonrpc::Result<Option<WorkspaceEdit>> {
    let compiled = Compiled::new(state);
    let key = state.key_of(uri);
    // The edits come from the compile: asked from a document typed in since,
    // the rename waits for it, whatever the cursor names (ADR 0023 D7).
    if state.files().document(&key).is_none() || !renames_in(&compiled, &key)? {
        return Ok(None);
    }
    let Some(id) = entity_under_cursor(&compiled, &key, position) else {
        return Ok(None);
    };

    // The declaration's name and every reference's token, read from the
    // compiled text, planned by the shared rename (the MCP tool's rules). A
    // rename is all or nothing: one that cannot be done whole is refused
    // with why.
    let edits = match specforge_ops::authoring::rename::plan(
        &compiled.navigator(),
        id.as_str(),
        new_name,
    ) {
        Ok(plan) => plan.edits().to_vec(),
        Err(e) if e.error.kind == specforge_ops::OpErrorKind::EntityNotFound => return Ok(None),
        Err(e) => return Err(Error::invalid_params(e.error.message)),
    };

    // The edits are positions in the compiled text and apply to the editor's buffer: a file
    // typed in since is not that text, so the rename waits for the compile (ContentModified).
    compiled
        .workspace_edit(edits)
        .map(Some)
        .map_err(|Unplaced(file)| content_modified(file.as_str()))
}

/// The fixes offered for the published diagnostics of `file` overlapping `range` (the editor's own
/// positions), read by `navigator` over the compile. Every fix edits `file` (`fix::offers`), so over
/// a text the editor no longer has none is chosen and none is planned (ADR 0023 D3):
/// `ContentModified`. What both code-action requests ask (ADR 0161 D5: a resolve chooses again what
/// the list chose).
fn offered(
    compiled: &Compiled,
    navigator: &Navigator,
    file: &str,
    range: Range,
) -> Result<Vec<Offer>, Error> {
    if compiled.is_stale(file) {
        return Err(content_modified(file));
    }
    let files = compiled.state().files();
    let within = files
        .document(file)
        .map(|doc| doc.index().span(Sym::new(file), range));
    let query = FixQuery {
        file: Some(file),
        within: within.as_ref(),
        ..FixQuery::default()
    };
    Ok(fix::offers(
        navigator,
        files.compile_diagnostics(file),
        &query,
    ))
}

/// The fixes for the published diagnostics of `uri` overlapping `range`
/// (the editor's own positions), each offered whole or not at all (ADR
/// 0016: the fixes MCP suggest_fixes returns for the same diagnostics).
pub fn code_actions(state: &LspState, uri: &Url, range: Range) -> Option<CodeActionResponse> {
    let compiled = Compiled::new(state);
    let file = state.key_of(uri);
    let navigator = compiled.navigator();
    let offers = offered(&compiled, &navigator, &file, range).ok()?;
    // A client that resolves `edit` is shown a stub without its edits: planning one costs a
    // compile of the project, made when the editor resolves the action (ADR 0161). A fix that
    // cannot be placed whole (`Compiled::workspace_edit`) is not offered.
    let lazily = state.client().code_action_resolve;
    let actions: Vec<CodeActionOrCommand> = offers
        .into_iter()
        .filter_map(|offer| {
            if lazily && offer.is_typed() {
                return Some(CodeAction {
                    title: offer.title().to_string(),
                    kind: Some(code_action_kind(offer.kind())),
                    data: Some(
                        serde_json::json!({"uri": uri, "range": range, "title": offer.title()}),
                    ),
                    ..Default::default()
                });
            }
            fix_to_code_action(&compiled, offer.fix(&navigator).ok()?).ok()
        })
        .map(CodeActionOrCommand::CodeAction)
        .collect();
    (!actions.is_empty()).then_some(actions)
}

/// A code action listed without its edits, with them: the offers for the file and range it
/// names are chosen again over the current compile, the one titled so is planned, and a
/// refused plan answers why (E080, ...).
pub fn resolve_code_action(state: &LspState, action: CodeAction) -> Result<CodeAction, Error> {
    let data = action.data.clone().unwrap_or_default();
    let (Some(uri), Some(range)) = (
        data.get("uri")
            .and_then(|u| u.as_str())
            .and_then(|u| Url::parse(u).ok()),
        data.get("range")
            .and_then(|r| serde_json::from_value::<Range>(r.clone()).ok()),
    ) else {
        return Err(Error::invalid_params(
            "the code action carries no uri and range to resolve",
        ));
    };
    let compiled = Compiled::new(state);
    let file = state.key_of(&uri);
    let navigator = compiled.navigator();
    let offer = offered(&compiled, &navigator, &file, range)?
        .into_iter()
        .find(|offer| offer.title() == action.title)
        .ok_or_else(|| {
            Error::invalid_params(format!("the fix '{}' is no longer offered", action.title))
        })?;
    let fix = offer
        .fix(&navigator)
        .map_err(|refusal| Error::invalid_params(refusal.error.message))?;
    fix_to_code_action(&compiled, fix).map_err(|Unplaced(file)| content_modified(file.as_str()))
}

/// The document's outline (`specforge_ops::read::navigate::outline`, what MCP
/// outline returns): nested symbols, methods as children, each selecting
/// its name, for a client that declared hierarchicalDocumentSymbolSupport;
/// flat otherwise.
pub fn document_symbols(state: &LspState, uri: &Url) -> Option<DocumentSymbolResponse> {
    let compiled = Compiled::new(state);
    let entries = outline(&compiled.navigator(), &state.key_of(uri)).filter(|e| !e.is_empty())?;
    Some(outline_to_document_symbols(
        &compiled,
        entries,
        state.client().hierarchical_symbols,
    ))
}

/// The entities whose names match `query`, by navigation's one ranking:
/// what MCP search and completion rank alike.
pub fn workspace_symbols(state: &LspState, query: &str) -> Option<Vec<SymbolInformation>> {
    let query = EntityQuery::new(query, MatchScope::Names);
    let found = find_entities(state.graph(), &query);
    if found.is_empty() {
        return None;
    }

    let kind_reg = state.kind_registry();
    let compiled = Compiled::new(state);
    #[expect(
        deprecated,
        reason = "lsp-types marks SymbolInformation::deprecated deprecated; a struct literal must still set it"
    )]
    let symbols: Vec<SymbolInformation> = found
        .into_iter()
        .filter_map(|m| {
            Some(SymbolInformation {
                // Graph byte columns convert to UTF-16 against the text
                // the graph was compiled from; an entity whose file's
                // text is unknown is left out.
                location: compiled.location(&m.node.source_span)?,
                name: m.node.id.raw.to_string(),
                kind: symbol_kind_from_entity(m.node.kind.raw.as_str(), kind_reg),
                tags: None,
                deprecated: None,
                container_name: Some(m.node.kind.raw.to_string()),
            })
        })
        .collect();
    (!symbols.is_empty()).then_some(symbols)
}

/// The document's semantic tokens.
pub fn semantic_tokens(state: &LspState, uri: &Url) -> Option<SemanticTokensResult> {
    let doc = state.document(uri)?;
    Some(SemanticTokensResult::Tokens(SemanticTokens {
        result_id: None,
        data: doc.semantic_tokens(&state.view()),
    }))
}

/// What formatting the open document `uri` answers: its edits, what formatting
/// reported (for the store to publish beside the compile's), and the notice
/// that the project's configuration overrode the editor's settings.
pub struct Formatted {
    pub edits: Vec<TextEdit>,
    /// What formatting reported, positions in the document at its version
    /// (`LspState::publish_formatting`).
    pub findings: Findings,
    /// The configuration key and the message, sent once per configuration
    /// (`LspState::first_format_notice`).
    pub notice: Option<(String, String)>,
}

/// Format the open document `uri` as `specforge format` formats its file
/// (ADR 0021). Inside a project the project's configuration wins over
/// `options`. `None` for a document that is not open.
pub fn formatting(
    state: &LspState,
    uri: &Url,
    options: &FormattingOptions,
    lines: Option<format::Lines>,
) -> Option<Formatted> {
    let editor = format::EditorOptions {
        tab_size: options.tab_size as usize,
        insert_spaces: options.insert_spaces,
    };
    let key = state.key_of(uri);
    let doc = state.files().document(&key)?;
    let file = crate::uri::file_path(uri);
    let place = file
        .as_deref()
        .map_or(format::Place::Detached, format::Place::File);
    // The served project's field forms; a document outside any file has none (ADR 0147).
    let none = specforge_graph::FieldForms::default();
    let forms = match place {
        format::Place::Detached => &none,
        _ => state.registries().field_forms(),
    };
    let formatted = format::document(place, doc.text(), lines, Some(editor), forms);
    // The formatter's diagnostics are positions in the document it formatted;
    // the store publishes them beside the compile's, never in their place.
    let findings = Findings {
        version: doc.version(),
        sent: formatted
            .diagnostics
            .iter()
            .map(|d| diagnostic_to_lsp(d, |span| Some(doc.index().range(span))))
            .collect(),
        placed: formatted.diagnostics.clone(),
        key,
    };
    Some(Formatted {
        edits: lsp_edits(formatted.edits(), doc.index()),
        findings,
        notice: overridden_editor_options(&formatted, editor),
    })
}

/// A formatted document's edits (byte ranges of the open document's text,
/// which `index` indexes) as LSP edits: its positions, UTF-16 columns
/// (ADR 0023 D3).
fn lsp_edits(edits: Vec<format::Edit>, index: &LineIndex) -> Vec<TextEdit> {
    edits
        .into_iter()
        .map(|e| TextEdit {
            range: Range {
                start: index.position(e.range.start),
                end: index.position(e.range.end),
            },
            new_text: e.new_text,
        })
        .collect()
}

/// When a project's configuration formatted `formatted` and the editor's
/// `editor` settings differ from it: the configuration (its key for the
/// once-per-session notice) and the message telling the editor so.
fn overridden_editor_options(
    formatted: &format::FormattedDocument,
    editor: format::EditorOptions,
) -> Option<(String, String)> {
    let configuration = match &formatted.config_source {
        format::ConfigSource::File(path) => path.display().to_string(),
        format::ConfigSource::Defaults => "the defaults".to_string(),
        format::ConfigSource::Editor => return None,
    };
    let config = &formatted.config;
    let same = editor.insert_spaces != config.use_tabs
        && (config.use_tabs || editor.tab_size == config.indent_width);
    if same {
        return None;
    }
    let indent = if config.use_tabs { "tabs" } else { "spaces" };
    let message = format!(
        "formatting with {configuration} (indent {}, {indent}); the editor's tabSize {} / insertSpaces {} apply only outside a project",
        config.indent_width, editor.tab_size, editor.insert_spaces
    );
    Some((configuration, message))
}
