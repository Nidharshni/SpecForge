//! The LSP's adapter over `specforge_ops::read::navigate`: it builds the
//! navigator over the session (open buffers first, then disk) and converts
//! its answers, `SourceSpan`s, to LSP locations and UTF-16 ranges. What
//! the answers are is navigation's to say (ADR 0016); `use`-path
//! navigation is the resolver's `import_target` over the compile's sources
//! (`answers::definition`).

use specforge_common::{SourceSpan, Sym, structural};
use specforge_ops::read::navigate::{Fix, FixKind, Navigator, OutlineEntry};
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
/// text) and says whether the editor still has that text. A file the
/// compile holds no text of cannot be converted: its range is `None`,
/// never byte columns passed off as UTF-16.
pub(crate) struct Compiled<'s> {
    state: &'s LspState,
    indexes: RefCell<HashMap<Sym, Option<Arc<LineIndex>>>>,
    stale: RefCell<HashMap<Sym, bool>>,
}

impl<'s> Compiled<'s> {
    pub(crate) fn new(state: &'s LspState) -> Self {
        Compiled {
            state,
            indexes: RefCell::new(HashMap::new()),
            stale: RefCell::new(HashMap::new()),
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
    pub(crate) fn navigator(&self) -> Navigator<'s, impl Fn(&str) -> Option<String> + use<'s>> {
        let state = self.state;
        Navigator::new(state.view(), move |file| {
            state.compiled_text(file).map(|text| text.to_string())
        })
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

    /// Whether the text the editor has for session file `file` (its open
    /// buffer, else the file on disk) is not the text the project was
    /// compiled from: no edit computed from the compile applies to it. True
    /// when the editor has typed since (the compile is still to come), when
    /// a closed file changed or went away on disk (the watcher's event is
    /// still to come), and when the compile holds no text of the file. Read
    /// once per file per request.
    pub(crate) fn is_stale(&self, file: &str) -> bool {
        let key = Sym::new(file);
        *self.stale.borrow_mut().entry(key).or_insert_with(|| {
            let Some(compiled) = self.state.compiled_text(file) else {
                return true;
            };
            match self.state.files().document(file) {
                Some(doc) => doc.text() != &*compiled,
                None => std::fs::read_to_string(self.state.files().file_path(file))
                    .map_or(true, |disk| disk != *compiled),
            }
        })
    }
}

/// A fix as the code action that applies it; `None` when one of its edits
/// cannot be placed (its file's text is unknown, or the open buffer is no
/// longer the text the fix was computed on): a fix applies whole or not at
/// all.
pub(crate) fn fix_to_code_action(ranges: &Compiled, fix: Fix) -> Option<CodeAction> {
    let mut changes: HashMap<Url, Vec<TextEdit>> = HashMap::new();
    for edit in &fix.edits {
        let file = edit.span.file.as_str();
        if ranges.is_stale(file) {
            return None;
        }
        changes.entry(ranges.uri(file)).or_default().push(TextEdit {
            range: ranges.range(&edit.span)?,
            new_text: edit.new_text.clone(),
        });
    }
    Some(CodeAction {
        title: fix.title,
        kind: Some(match fix.kind {
            FixKind::QuickFix => CodeActionKind::QUICKFIX,
            FixKind::Refactor => CodeActionKind::REFACTOR,
        }),
        edit: Some(WorkspaceEdit {
            changes: Some(changes),
            ..Default::default()
        }),
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
        #[allow(deprecated)]
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
        #[allow(deprecated)]
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
            #[allow(deprecated)]
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
            "the stand-in holds the compiled text"
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
            source: specforge_ops::read::navigate::FixSource::ReplaceUnresolved,
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
        assert!(fix_to_code_action(&ranges, fix(&["a.spec", "ghost.spec"])).is_none());

        // The editor typed since: the edits are positions in a text it no
        // longer has.
        state.change_document(&uri, 2, &[whole(TYPED)]);
        let ranges = Compiled::new(&state);
        assert!(fix_to_code_action(&ranges, fix(&["a.spec"])).is_none());
    }
}
