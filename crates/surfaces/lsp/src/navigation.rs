//! The LSP's adapter over `specforge_ops::read::navigate` (and `authoring::fix` for code actions): it builds the
//! navigator over the session (the text the project was compiled from) and converts
//! its answers, `SourceSpan`s, to LSP locations and UTF-16 ranges. What
//! the answers are is navigation's to say (ADR 0016); `use`-path
//! navigation is the resolver's `import_target` over the compile's sources
//! (`answers::definition`).

use specforge_common::{SourceSpan, Sym, structural};
use specforge_ops::authoring::fix::{Fix, FixKind};
use specforge_ops::read::navigate::{Navigator, OutlineEntry};
use specforge_registry::KindRegistry;
use std::collections::HashMap;
use tower_lsp::lsp_types::{
    CodeAction, CodeActionKind, DocumentSymbol, DocumentSymbolResponse, Location, Position, Range,
    SymbolInformation, SymbolKind, TextEdit, Url, WorkspaceEdit,
};

use crate::LspState;
use crate::document::{LineIndex, Target};
use std::cell::RefCell;
use std::sync::Arc;

/// The project as its last compile saw it, read by one request. A span of
/// the graph or of a diagnostic is a position in the text the compile read
/// (ADR 0023 D3): `Compiled` converts it against that text (byte columns
/// to UTF-16, through that text's [`LineIndex`], built once per file per
/// request, or the open document's own index when the document is that
/// text) and says what the editor's text is to it ([`EditorText`]). A file the
/// compile holds no text of cannot be converted: its range is `None`,
/// never byte columns passed off as UTF-16.
pub(crate) struct Compiled<'s> {
    state: &'s LspState,
    indexes: RefCell<HashMap<Sym, Option<Arc<LineIndex>>>>,
    texts: RefCell<HashMap<Sym, EditorText>>,
}

impl<'s> Compiled<'s> {
    pub(crate) fn new(state: &'s LspState) -> Self {
        Compiled {
            state,
            indexes: RefCell::new(HashMap::new()),
            texts: RefCell::new(HashMap::new()),
        }
    }

    /// The state the spans are read against.
    pub(crate) fn state(&self) -> &'s LspState {
        self.state
    }

    /// Navigation over the session, reading the text of each file the
    /// project was compiled from, not the buffer now (an edit not yet
    /// compiled would move the spans) nor the disk. A file the compile
    /// does not hold has no text.
    pub(crate) fn navigator(&self) -> Navigator<'s> {
        Navigator::new(self.state.view())
    }

    /// The URI of a session file key: the one the client names it by.
    pub(crate) fn uri(&self, key: &str) -> Url {
        self.state.files().uri(key)
    }

    /// The line index of the text session file `file` was compiled from
    /// (once per request); `None` when the compile holds no text of it.
    pub(crate) fn index_of(&self, file: &str) -> Option<Arc<LineIndex>> {
        let key = Sym::new(file);
        if let Some(index) = self.indexes.borrow().get(&key) {
            return index.clone();
        }
        let index =
            self.state
                .compiled_text(file)
                .map(|text| match self.state.files().document(file) {
                    Some(doc) if doc.text() == &*text => Arc::clone(doc.index()),
                    _ => Arc::new(LineIndex::new(&text)),
                });
        self.indexes.borrow_mut().insert(key, index.clone());
        index
    }

    /// The LSP range of a span of the compile, against the text it is a
    /// position in; `None` when that text is unknown.
    pub(crate) fn range(&self, span: &SourceSpan) -> Option<Range> {
        Some(self.index_of(span.file.as_str())?.range(span))
    }

    /// The location of a span of a session file; `None` when its text is
    /// unknown.
    pub(crate) fn location(&self, span: &SourceSpan) -> Option<Location> {
        Some(Location {
            uri: self.uri(span.file.as_str()),
            range: self.range(span)?,
        })
    }

    /// What the cursor at `position` of the open document `key` names (ADR
    /// 0023 D5). Navigation is asked about the position only while the
    /// document is the compiled text; while it is stale, the cursor names
    /// what its own word names ([`crate::Cursor::named`], D7).
    pub(crate) fn target(&self, key: &str, position: Position) -> Option<Target> {
        let cursor = self.state.files().document(key)?.at(position)?;
        if self.is_stale(key) {
            cursor.named(&self.state.view())
        } else {
            cursor.target(&self.navigator(), key)
        }
    }

    /// What the editor's text of session file `file` (its open buffer, else the file on disk) is to
    /// the compile ([`EditorText`]). Read once per file per request.
    pub(crate) fn editor_text(&self, file: &str) -> EditorText {
        let key = Sym::new(file);
        *self.texts.borrow_mut().entry(key).or_insert_with(|| {
            let document = self.state.files().document(file);
            let Some(compiled) = self.state.compiled_text(file) else {
                // The session holds this very buffer and built nothing from it: no compile is coming.
                return match document {
                    Some(doc)
                        if doc.version().is_some()
                            && self.state.compiled_version(file) == doc.version() =>
                    {
                        EditorText::Unbuilt
                    }
                    _ => EditorText::Changed,
                };
            };
            let same = match document {
                Some(doc) => doc.text() == &*compiled,
                None => std::fs::read_to_string(self.state.files().file_path(file))
                    .is_ok_and(|disk| disk == *compiled),
            };
            if same {
                EditorText::Compiled
            } else {
                EditorText::Changed
            }
        })
    }

    /// Whether the editor's text of `file` is not the compiled text ([`Self::editor_text`]): no
    /// position of the compile is a position in it.
    pub(crate) fn is_stale(&self, file: &str) -> bool {
        self.editor_text(file) != EditorText::Compiled
    }
}

/// What the editor's text of a session file (its open buffer, else the file on disk) is to the
/// compile (ADR 0023 D3, D7).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EditorText {
    /// The text the project was compiled from: a position of the compile is a position in it.
    Compiled,
    /// Not that text, and a compile that reads it is still to come: a buffer typed since, a closed
    /// file changed or gone on disk, a document opened since that the session does not hold yet.
    Changed,
    /// A text the project does not build, and no compile is coming: the session holds this very
    /// buffer and compiled none of it (outside the spec root, excluded, not `.spec`).
    Unbuilt,
}

/// The LSP kind of a fix's kind.
pub(crate) fn code_action_kind(kind: FixKind) -> CodeActionKind {
    match kind {
        FixKind::QuickFix => CodeActionKind::QUICKFIX,
        FixKind::Refactor => CodeActionKind::REFACTOR,
    }
}

/// A file an edit of the compile cannot be placed in: the editor's text of it (its open buffer,
/// else the disk) is not the text the project was compiled from, or the compile holds no text of
/// it ([`Compiled::is_stale`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Unplaced(pub(crate) Sym);

impl Compiled<'_> {
    /// `edits`, positions in the compiled text (ADR 0164), as one workspace edit: every edit
    /// placed, in the order given within its file, or none: the first edit whose file is
    /// [`Self::is_stale`] answers its file.
    pub(crate) fn workspace_edit(
        &self,
        edits: impl IntoIterator<Item = specforge_ops::read::navigate::TextEdit>,
    ) -> Result<WorkspaceEdit, Unplaced> {
        let mut changes: HashMap<Url, Vec<TextEdit>> = HashMap::new();
        for edit in edits {
            let file = edit.span.file;
            // `is_stale` holds whenever the compile has no text of `file`, so `range` is then `Some`.
            let range = (!self.is_stale(file.as_str()))
                .then(|| self.range(&edit.span))
                .flatten()
                .ok_or(Unplaced(file))?;
            changes
                .entry(self.uri(file.as_str()))
                .or_default()
                .push(TextEdit {
                    range,
                    new_text: edit.new_text,
                });
        }
        Ok(WorkspaceEdit {
            changes: Some(changes),
            ..Default::default()
        })
    }
}

/// A fix as the code action that applies it, whole ([`Compiled::workspace_edit`]).
pub(crate) fn fix_to_code_action(ranges: &Compiled, fix: Fix) -> Result<CodeAction, Unplaced> {
    Ok(CodeAction {
        title: fix.title,
        kind: Some(code_action_kind(fix.kind)),
        edit: Some(ranges.workspace_edit(fix.edits)?),
        ..Default::default()
    })
}

/// An outline as the LSP's document symbols: nested (methods as children,
/// each selecting its name) when `hierarchical`, else flat (a method's
/// container is its entity). An entry or method whose file's text is
/// unknown is left out (no range of it is honest).
pub(crate) fn outline_to_document_symbols(
    ranges: &Compiled,
    entries: Vec<OutlineEntry>,
    hierarchical: bool,
) -> DocumentSymbolResponse {
    let kinds = ranges.state().kind_registry();
    if hierarchical {
        #[expect(
            deprecated,
            reason = "lsp-types marks DocumentSymbol::deprecated deprecated; a struct literal must still set it"
        )]
        let symbols = entries
            .into_iter()
            .filter_map(|entry| {
                let kind = entry.kind.as_str();
                let children: Vec<DocumentSymbol> = entry
                    .children
                    .iter()
                    .filter_map(|method| {
                        Some(DocumentSymbol {
                            name: method.name.clone(),
                            detail: Some(format!("method {}", method.signature)),
                            kind: SymbolKind::METHOD,
                            tags: None,
                            deprecated: None,
                            range: ranges.range(&method.block)?,
                            selection_range: ranges.range(&method.name_span)?,
                            children: None,
                        })
                    })
                    .collect();
                Some(DocumentSymbol {
                    name: entry.id.to_string(),
                    detail: Some(match &entry.title {
                        Some(title) => format!("{kind} — {title}"),
                        None => kind.to_string(),
                    }),
                    kind: symbol_kind_from_entity(kind, kinds),
                    tags: None,
                    deprecated: None,
                    range: ranges.range(&entry.block)?,
                    selection_range: ranges.range(&entry.name)?,
                    children: (!children.is_empty()).then_some(children),
                })
            })
            .collect();
        return DocumentSymbolResponse::Nested(symbols);
    }
    let mut symbols = Vec::new();
    for entry in entries {
        let Some(location) = ranges.location(&entry.block) else {
            continue;
        };
        #[expect(
            deprecated,
            reason = "lsp-types marks SymbolInformation::deprecated deprecated; a struct literal must still set it"
        )]
        symbols.push(SymbolInformation {
            location,
            name: entry.id.to_string(),
            kind: symbol_kind_from_entity(entry.kind.as_str(), kinds),
            tags: None,
            deprecated: None,
            container_name: Some(entry.kind.to_string()),
        });
        for method in &entry.children {
            let Some(location) = ranges.location(&method.block) else {
                continue;
            };
            #[expect(
                deprecated,
                reason = "lsp-types marks SymbolInformation::deprecated deprecated; a struct literal must still set it"
            )]
            symbols.push(SymbolInformation {
                location,
                name: method.name.clone(),
                kind: SymbolKind::METHOD,
                tags: None,
                deprecated: None,
                container_name: Some(entry.id.to_string()),
            });
        }
    }
    DocumentSymbolResponse::Flat(symbols)
}

/// The symbol kind of an entity kind: its extension-declared LSP icon
/// (`spec` is a namespace).
pub(crate) fn symbol_kind_from_entity(kind: &str, kind_registry: &KindRegistry) -> SymbolKind {
    if kind == structural::SPEC {
        return SymbolKind::NAMESPACE;
    }
    if let Some(entry) = kind_registry.get(kind)
        && let Some(ref icon) = entry.declared.lsp_icon
    {
        return lsp_icon_to_symbol_kind(icon);
    }
    SymbolKind::VARIABLE
}

fn lsp_icon_to_symbol_kind(icon: &str) -> SymbolKind {
    match icon {
        "Method" => SymbolKind::METHOD,
        "Struct" => SymbolKind::STRUCT,
        "Class" => SymbolKind::CLASS,
        "Module" => SymbolKind::MODULE,
        "Constant" => SymbolKind::CONSTANT,
        "Event" => SymbolKind::EVENT,
        "Interface" => SymbolKind::INTERFACE,
        "Property" => SymbolKind::PROPERTY,
        "Variable" => SymbolKind::VARIABLE,
        "Text" => SymbolKind::STRING,
        "Package" => SymbolKind::PACKAGE,
        "Folder" => SymbolKind::NAMESPACE,
        _ => SymbolKind::VARIABLE,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use specforge_project::ProjectSession;
    use specforge_test_macros::test as spec;
    use tower_lsp::lsp_types::TextDocumentContentChangeEvent;

    /// The editor replaces the whole text with `text`.
    fn whole(text: &str) -> TextDocumentContentChangeEvent {
        TextDocumentContentChangeEvent {
            range: None,
            range_length: None,
            text: text.to_string(),
        }
    }

    /// A title with non-ASCII letters: `{` is at byte column 25 and UTF-16
    /// column 21 of the first line.
    const COMPILED: &str = "behavior alpha \"Ééé\" {\n}\n";
    /// The same file as the editor has it after the title was edited: not
    /// yet compiled.
    const TYPED: &str = "behavior alpha \"E\" {\n}\n";

    fn brace() -> SourceSpan {
        SourceSpan {
            file: Sym::new("a.spec"),
            start_line: 1,
            start_col: 25,
            end_line: 1,
            end_col: 26,
        }
    }

    /// A state serving the project at `dir` (one file, `a.spec`, holding
    /// `COMPILED`), and the URI of that file.
    fn serving(dir: &tempfile::TempDir) -> (LspState, Url) {
        std::fs::write(
            dir.path().join("specforge.json"),
            r#"{"name":"t","version":"0.1.0","extensions":[]}"#,
        )
        .unwrap();
        std::fs::write(dir.path().join("a.spec"), COMPILED).unwrap();
        let mut state = LspState::new();
        state.set_session(ProjectSession::open(
            &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
        ));
        let uri = Compiled::new(&state).uri("a.spec");
        (state, uri)
    }

    #[spec(
        invariant = "lsp_utf16_positions",
        verify = "a span converts against the text the project was compiled from, not the buffer typed since"
    )]
    fn a_span_converts_against_the_compiled_text_not_the_stale_buffer() {
        let dir = tempfile::tempdir().unwrap();
        let (mut state, uri) = serving(&dir);

        // The buffer is the compiled text: the span lands on its UTF-16
        // column, through the document's own index.
        state.open_document(&uri, COMPILED, 1);
        let ranges = Compiled::new(&state);
        assert!(!ranges.is_stale("a.spec"));
        let range = ranges.range(&brace()).expect("the compile holds a.spec");
        assert_eq!((range.start.line, range.start.character), (0, 21));
        assert_eq!((range.end.line, range.end.character), (0, 22));

        // The editor typed since, and the compile has not run: the span is
        // still a position in the compiled text, so it keeps its column
        // (against the buffer, byte column 25 is past the line's end).
        state.change_document(&uri, 2, &[whole(TYPED)]);
        let ranges = Compiled::new(&state);
        assert!(ranges.is_stale("a.spec"));
        assert_eq!(ranges.range(&brace()), Some(range));
        assert_eq!(ranges.location(&brace()).map(|l| l.uri), Some(uri.clone()));
        // What navigation reads is that text too (`Compiled::navigator`).
        assert_eq!(state.compiled_text("a.spec").as_deref(), Some(COMPILED));
    }

    #[spec(
        invariant = "lsp_utf16_positions",
        verify = "a span of a file the compile holds no text of has no range, never byte columns"
    )]
    fn a_span_of_a_file_without_compiled_text_has_no_range() {
        let dir = tempfile::tempdir().unwrap();
        let (mut state, _) = serving(&dir);

        // On disk and readable, but not part of the compile; and a name
        // the project has never held.
        std::fs::write(dir.path().join("ghost.spec"), COMPILED).unwrap();
        for file in ["ghost.spec", "nope.spec"] {
            let span = SourceSpan {
                file: Sym::new(file),
                ..brace()
            };
            let ranges = Compiled::new(&state);
            assert_eq!(ranges.range(&span), None, "{file}");
            assert_eq!(ranges.location(&span), None, "{file}");
            assert!(ranges.index_of(file).is_none(), "{file}");
        }

        // A file that cannot be read from disk now still has the text it
        // was compiled from: the disk is not asked.
        std::fs::remove_file(dir.path().join("a.spec")).unwrap();
        let ranges = Compiled::new(&state);
        let range = ranges.range(&brace()).expect("compiled text is kept");
        assert_eq!(range.start.character, 21);

        // While the session is out for an update, readers keep its texts.
        let session = state.take_session().expect("held");
        assert_eq!(
            state.compiled_text("a.spec").as_deref(),
            Some(COMPILED),
            "the copy holds the compiled text"
        );
        state.set_session(session);
    }

    #[spec(
        invariant = "lsp_utf16_positions",
        verify = "a fix is offered whole or not at all: never over a buffer typed since, nor a file with no compiled text"
    )]
    fn a_fix_is_offered_whole_or_not_at_all() {
        let dir = tempfile::tempdir().unwrap();
        let (mut state, uri) = serving(&dir);
        let fix = |files: &[&str]| Fix {
            title: "fix".into(),
            kind: FixKind::QuickFix,
            source: specforge_ops::authoring::fix::FixSource::ReplaceUnresolved,
            diagnostic_code: None,
            subject: None,
            edits: files
                .iter()
                .map(|file| specforge_ops::read::navigate::TextEdit {
                    span: SourceSpan {
                        file: Sym::new(file),
                        ..brace()
                    },
                    new_text: "x".into(),
                })
                .collect(),
            anchor: None,
        };

        state.open_document(&uri, COMPILED, 1);
        let ranges = Compiled::new(&state);
        let action = fix_to_code_action(&ranges, fix(&["a.spec"])).expect("placeable");
        let changes = action.edit.unwrap().changes.unwrap();
        assert_eq!(changes[&uri][0].range.start.character, 21);
        // One edit with no text refuses the whole fix.
        assert_eq!(
            fix_to_code_action(&ranges, fix(&["a.spec", "ghost.spec"])).err(),
            Some(Unplaced(Sym::new("ghost.spec")))
        );

        // The editor typed since: the edits are positions in a text it no
        // longer has.
        state.change_document(&uri, 2, &[whole(TYPED)]);
        let ranges = Compiled::new(&state);
        assert_eq!(
            fix_to_code_action(&ranges, fix(&["a.spec"])).err(),
            Some(Unplaced(Sym::new("a.spec")))
        );
    }
}
