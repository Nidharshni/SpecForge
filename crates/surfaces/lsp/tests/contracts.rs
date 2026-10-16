use specforge_common::{SourceSpan, Sym};
use specforge_graph::{Graph, Node};
use specforge_ops::read::navigate::{EntityQuery, MatchScope, find_entities};
use specforge_test_macros::test as specforge_test;
use tower_lsp::lsp_types::{
    DocumentSymbolResponse, GotoDefinitionResponse, Position, PrepareRenameResponse, Url,
};

use crate::served::{buffers, uri_of_path};
use specforge_lsp::answers;

fn node(id: &str, kind: &str, title: Option<&str>) -> Node {
    Node {
        title: title.map(|t| t.to_string()),
        ..Node::new(
            id,
            kind,
            SourceSpan {
                file: Sym::new("test.spec"),
                start_line: 0,
                start_col: 0,
                end_line: 3,
                end_col: 1,
            },
        )
    }
}

fn node_at(id: &str, kind: &str, file: &str, line: usize, col: usize) -> Node {
    Node {
        title: Some(format!("{id} title")),
        ..Node::new(
            id,
            kind,
            SourceSpan {
                file: Sym::new(file),
                start_line: line,
                start_col: col,
                end_line: line + 3,
                end_col: col + id.len(),
            },
        )
    }
}

/// Every standard LSP semantic token type, in the order the server's legend
/// lists them.
pub(crate) const STANDARD_TOKEN_TYPES: [&str; 23] = [
    "namespace",
    "type",
    "class",
    "enum",
    "interface",
    "struct",
    "typeParameter",
    "parameter",
    "variable",
    "property",
    "enumMember",
    "event",
    "function",
    "method",
    "macro",
    "keyword",
    "modifier",
    "comment",
    "string",
    "number",
    "regexp",
    "operator",
    "decorator",
];

/// A temp project whose specforge.json lists `extensions`.
pub(crate) fn project_with(extensions: &[&str]) -> tempfile::TempDir {
    let dir = tempfile::TempDir::new().unwrap();
    let config = serde_json::json!({
        "name": "test-project",
        "version": "0.1.0",
        "extensions": extensions,
    });
    std::fs::write(dir.path().join("specforge.json"), config.to_string()).unwrap();
    dir
}

/// The registry build `extensions` populate.
pub(crate) fn registries_for(extensions: &[&str]) -> specforge_registry::RegistryBuild {
    let names: Vec<String> = extensions.iter().map(|s| s.to_string()).collect();
    let runtime = wasm_runtime_for(&names);
    let declarations: Vec<_> = names
        .iter()
        .map(|name| {
            let loaded = specforge_wasm::protocol::load_declaration(&runtime, name)
                .unwrap_or_else(|e| panic!("{name} does not load: {e:?}"));
            loaded.declaration
        })
        .collect();
    specforge_registry::build_registries(declarations)
}

// B:lsp_shutdown — verify contract "requires/ensures consistency for LSP shutdown"
#[specforge_test(
    behavior = "lsp_shutdown",
    verify = "LSP Shutdown: LSP shutdown holds — lsp_initialized_fired, resources_released, post_shutdown_rejected, no_disk_persistence, lsp_shutdown_complete_emitted"
)]
fn lsp_shutdown_contract() {
    // Requires: active LSP state with open documents
    // Ensures: shutdown releases state, subsequent operations rejected
    let mut state = specforge_lsp::LspState::new();
    let (a, b) = (crate::url("file:///a.spec"), crate::url("file:///b.spec"));
    state.open_document(&a, "content", 1);
    assert!(state.document(&a).is_some());

    state.shutdown();

    assert!(state.is_shutdown(), "state must be marked as shutdown");
    assert!(state.document(&a).is_none(), "documents must be released");

    // Post-shutdown operations should be rejected
    state.open_document(&b, "new content", 1);
    assert!(
        state.document(&b).is_none(),
        "must reject operations after shutdown"
    );
}

// B:document_open_close — verify contract "requires/ensures consistency for document open/close"
#[specforge_test(
    behavior = "document_open_close",
    verify = "Document Open/Close: document open/close holds — lsp_initialized_fired, document_tracked, file_changed_emitted, closed_file_published, closed_file_from_disk"
)]
#[tokio::test]
async fn document_open_close_contract() {
    // lsp_initialized_fired: the session is initialized.
    let (mut session, _) = crate::session::Session::start(None).await;
    // Only in the editor buffer: nothing on disk.
    let uri = "file:///buffer/open_close.spec";
    let text = "behavior login \"Login\" {\n  invariants [session_limit]\n}\n";

    // file_changed_emitted: didOpen compiles the buffer — the published
    // diagnostics are the buffer's dangling reference.
    session.open(uri, text).await;
    let opened = session.diagnostics(uri).await;
    assert_eq!(crate::session::codes(&opened), ["E003"], "{opened:?}");
    assert_eq!(
        opened[0]["message"],
        "unresolved reference 'session_limit' in entity 'login'"
    );

    // document_tracked: an open document is served from its buffer
    // (formatting answers only for open documents), a closed one is not.
    assert!(session.format(uri).await.is_array());
    session.close(uri).await;

    // closed_file_published: the closed file is published once, as the
    // project reports it; a file outside a project has no disk text to
    // return to, so it leaves the project and the editor's squiggles are
    // cleared (closed_file_from_disk).
    let closed = session.diagnostics(uri).await;
    assert!(closed.is_empty(), "{closed:?}");
    assert!(session.format(uri).await.is_null());
    let found = session.workspace_symbol("login").await;
    assert!(found["result"].is_null(), "{found}");
}

/// Formatting a document with a parse error publishes the formatter's
/// W142 (the error region is kept verbatim) without erasing what compiling
/// the document reported: the E001 and the dangling reference stay.
#[specforge_test(
    behavior = "lsp_format_document",
    verify = "formatting keeps the document's compile diagnostics published"
)]
#[tokio::test]
async fn formatting_keeps_the_compile_diagnostics() {
    let (mut session, _) = crate::session::Session::start(None).await;
    let uri = "file:///buffer/format_keeps.spec";
    let text = "behavior login \"Login\" {\n  invariants [session_limit]\n}\n\n}}}\n";
    session.open(uri, text).await;
    let compiled = session.diagnostics(uri).await;
    let mut compile_codes = crate::session::codes(&compiled);
    compile_codes.sort();
    assert!(
        compile_codes.contains(&"E001"),
        "the stray braces are a parse error: {compiled:?}"
    );

    assert!(session.format(uri).await.is_array());
    let after = session.diagnostics(uri).await;
    let mut codes = crate::session::codes(&after);
    codes.sort();
    let mut expected = compile_codes.clone();
    expected.push("W142");
    expected.sort();
    assert_eq!(codes, expected, "{after:?}");
}

/// A project on disk (`specforge.json` = `{}`) with `files` written
/// (path, text), its root canonical.
fn format_project(files: &[(&str, &str)]) -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::TempDir::new().unwrap();
    let root = dir.path().canonicalize().unwrap();
    std::fs::write(root.join("specforge.json"), "{}").unwrap();
    for (path, text) in files {
        let path = root.join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
    (dir, root)
}

/// A session over `root` with `file` (relative to it) open, its compile
/// published; the file's URI.
async fn open_in(root: &std::path::Path, file: &str) -> (crate::session::Session, String) {
    use crate::session::{Session, uri_of};
    let path = root.join(file);
    let text = std::fs::read_to_string(&path).unwrap();
    let (mut session, _) = Session::start(Some(root)).await;
    let uri = uri_of(&path);
    session.open(&uri, &text).await;
    session.diagnostics(&uri).await;
    (session, uri)
}

/// `text` with LSP `edits` (UTF-16 columns) applied.
fn apply_edits(text: &str, edits: &[serde_json::Value]) -> String {
    let lines: Vec<&str> = text.split('\n').collect();
    let offset = |position: &serde_json::Value| {
        let line = position["line"].as_u64().unwrap() as usize;
        let character = position["character"].as_u64().unwrap() as usize;
        let start: usize = lines[..line].iter().map(|l| l.len() + 1).sum();
        let mut units = 0;
        let column = lines[line]
            .char_indices()
            .find(|(_, c)| {
                let here = units >= character;
                units += c.len_utf16();
                here
            })
            .map_or(lines[line].len(), |(i, _)| i);
        start + column
    };
    let mut ranges: Vec<(usize, usize, &str)> = edits
        .iter()
        .map(|e| {
            (
                offset(&e["range"]["start"]),
                offset(&e["range"]["end"]),
                e["newText"].as_str().unwrap(),
            )
        })
        .collect();
    ranges.sort_by_key(|r| std::cmp::Reverse(r.0));
    let mut out = text.to_string();
    for (start, end, new_text) in ranges {
        out.replace_range(start..end, new_text);
    }
    out
}

/// A behavior indented by 4.
const FOUR: &str = "behavior login \"Login\" {\n    contract \"The system MUST log in\"\n}\n";

#[specforge_test(
    behavior = "lsp_respect_editor_config",
    verify = "the editor formats a project file as specforge format --check expects"
)]
#[tokio::test]
async fn formatting_uses_the_project_format_config() {
    let (_dir, root) = format_project(&[
        (".specforgefmt.toml", "indent_width = 4\n"),
        ("spec/a.spec", FOUR),
    ]);
    let (mut session, uri) = open_in(&root, "spec/a.spec").await;

    // Canonical under the project's config: no edit at the editor's tabSize 2.
    let edits = session.format(&uri).await;
    assert_eq!(edits, serde_json::json!([]));
}

#[specforge_test(
    behavior = "load_format_config",
    verify = "a file is formatted with the configuration of its own project"
)]
#[tokio::test]
async fn the_cli_and_the_editor_agree_on_a_nested_project() {
    let two = "behavior login \"Login\" {\n  contract \"The system MUST log in\"\n}\n";
    let (_dir, root) = format_project(&[
        (".specforgefmt.toml", "indent_width = 4\n"),
        ("inner/specforge.json", "{}"),
        ("inner/spec/a.spec", two),
    ]);
    let (mut session, uri) = open_in(&root.join("inner"), "spec/a.spec").await;

    // The inner project has no configuration file: its defaults, not the
    // outer project's, and not the editor's tabSize 4.
    let edits = session.formatting(&uri, 4).await["result"].clone();
    assert_eq!(edits, serde_json::json!([]));
    let outcome =
        specforge_ops::authoring::format::run(&specforge_ops::authoring::format::Request {
            root: &specforge_common::ProjectRoot::of(&root).unwrap(),
            paths: &[root.join("inner")],
            mode: specforge_ops::authoring::format::Mode::Check,
        })
        .unwrap();
    assert_eq!((outcome.checked, outcome.changes.len()), (1, 0));
}

#[specforge_test(
    behavior = "lsp_format_document",
    verify = "LSP format produces same result as CLI format"
)]
#[tokio::test]
async fn lsp_format_matches_cli_format() {
    let messy = "behavior login \"Connexion é\" {\n  contract   \"The system MUST log in\"\n      types [a, b]\n}\n";
    let (_dir, root) = format_project(&[
        (".specforgefmt.toml", "indent_width = 4\n"),
        ("spec/a.spec", messy),
    ]);
    let (mut session, uri) = open_in(&root, "spec/a.spec").await;

    let edits = session.format(&uri).await;
    let edited = apply_edits(messy, edits.as_array().expect("an edit list"));

    let outcome =
        specforge_ops::authoring::format::run(&specforge_ops::authoring::format::Request {
            root: &specforge_common::ProjectRoot::of(&root).unwrap(),
            paths: &[],
            mode: specforge_ops::authoring::format::Mode::Check,
        })
        .unwrap();
    assert_eq!(edited, outcome.changes[0].after);
    assert!(edited.contains("\n    contract"), "{edited}");
}

/// The round-7 probe rows (A-C1), as files of one project.
const CONVERGE: [(&str, &str); 7] = [
    (
        "spec/body_blank.spec",
        "behavior a \"A\" {\n\n  contract \"x\"\n}\n",
    ),
    (
        "spec/between_blocks.spec",
        "behavior a \"A\" {\n  contract \"x\"\n}\n\n\nbehavior b \"B\" {\n  contract \"y\"\n}\n",
    ),
    (
        "spec/leading.spec",
        "\n\n\nbehavior a \"A\" {\n  contract \"x\"\n}\n",
    ),
    (
        "spec/trailing.spec",
        "behavior a \"A\" {\n  contract \"x\"\n}\n\n\n",
    ),
    (
        "spec/no_final_newline.spec",
        "behavior a \"A\" {\n  contract \"x\"\n}",
    ),
    (
        "spec/crlf.spec",
        "behavior a \"A\" {\r\n  contract \"x\"\r\n}\r\n",
    ),
    (
        "spec/misindented.spec",
        "behavior a \"A\" {\n      contract \"x\"\n}\n",
    ),
];

#[specforge_test(
    behavior = "lsp_format_document",
    verify = "editor formatting reaches the text specforge format writes in one pass: blank lines removed, final newline added, CRLF made LF"
)]
#[specforge_test(
    invariant = "format_rule_determinism",
    verify = "same input and config produce identical output across CLI and LSP"
)]
#[tokio::test]
async fn editor_formatting_reaches_the_cli_text_in_one_pass() {
    use crate::session::{Session, uri_of};
    use specforge_ops::authoring::format::{Mode, Request, run};
    let (_dir, root) = format_project(&CONVERGE);
    let outcome = run(&Request {
        root: &specforge_common::ProjectRoot::of(&root).unwrap(),
        paths: &[],
        mode: Mode::Check,
    })
    .unwrap();
    let (mut session, _) = Session::start(Some(&root)).await;
    for (path, text) in CONVERGE {
        let file = root.join(path);
        let uri = uri_of(&file);
        session.open(&uri, text).await;
        session.diagnostics(&uri).await;
        let cli = &outcome
            .changes
            .iter()
            .find(|c| c.path == file)
            .unwrap_or_else(|| panic!("{path} would change"))
            .after;

        let edits = session.format(&uri).await;
        let edited = apply_edits(text, edits.as_array().expect("an edit list"));
        assert_eq!(&edited, cli, "{path}: {edits}");

        // One pass: the result is canonical, so the next request has nothing to do.
        session
            .did_change(&uri, 2, vec![serde_json::json!({ "text": edited })])
            .await;
        session.diagnostics(&uri).await;
        assert_eq!(session.format(&uri).await, serde_json::json!([]), "{path}");
    }
}

#[specforge_test(
    behavior = "lsp_format_range",
    verify = "range formatting matches full formatting for affected blocks"
)]
#[tokio::test]
async fn range_formatting_removes_a_blank_line_in_its_block() {
    let text =
        "behavior a \"A\" {\n  contract \"x\"\n}\n\nbehavior b \"B\" {\n\n  contract \"y\"\n}\n";
    let (_dir, root) = format_project(&[("spec/r.spec", text)]);
    let (mut session, uri) = open_in(&root, "spec/r.spec").await;

    let edits = session.range_formatting(&uri, 2, 4, 7).await["result"].clone();

    assert_eq!(
        apply_edits(text, edits.as_array().expect("an edit list")),
        "behavior a \"A\" {\n  contract \"x\"\n}\n\nbehavior b \"B\" {\n  contract \"y\"\n}\n",
        "{edits}"
    );
}

/// Two blocks, CRLF, `b` misindented.
const CRLF_TWO: &str = "behavior a \"A\" {\r\n  contract \"x\"\r\n}\r\n\r\nbehavior b \"B\" {\r\n      contract \"y\"\r\n}\r\n";
/// Two blocks, `b` misindented, no final newline.
const UNTERMINATED_TWO: &str =
    "behavior a \"A\" {\n  contract \"x\"\n}\n\nbehavior b \"B\" {\n      contract \"y\"\n}";

#[specforge_test(
    behavior = "lsp_format_range",
    verify = "range formatting changes no byte outside the blocks it touches"
)]
#[tokio::test]
async fn range_formatting_changes_nothing_outside_its_blocks() {
    let (_dir, root) = format_project(&[
        ("spec/crlf.spec", CRLF_TWO),
        ("spec/open.spec", UNTERMINATED_TWO),
    ]);
    let (mut session, _) = crate::session::Session::start(Some(&root)).await;
    for (path, text, exact) in [
        (
            "spec/crlf.spec",
            CRLF_TWO,
            "behavior a \"A\" {\r\n  contract \"x\"\r\n}\r\n\r\nbehavior b \"B\" {\n  contract \"y\"\n}\n",
        ),
        (
            "spec/open.spec",
            UNTERMINATED_TWO,
            "behavior a \"A\" {\n  contract \"x\"\n}\n\nbehavior b \"B\" {\n  contract \"y\"\n}",
        ),
    ] {
        let uri = crate::session::uri_of(&root.join(path));
        session.open(&uri, text).await;
        session.diagnostics(&uri).await;

        let edits = session.range_formatting(&uri, 2, 4, 6).await["result"].clone();
        let edited = apply_edits(text, edits.as_array().expect("an edit list"));

        // Block `a` (lines 0-3) is outside the range: byte for byte, its CRLF included.
        let block_b = text.match_indices('\n').nth(3).unwrap().0 + 1;
        assert!(edited.starts_with(&text[..block_b]), "{path}: {edited:?}");
        assert_eq!(edited, exact, "{path}");
        assert_eq!(edited.lines().nth(5), Some("  contract \"y\""), "{path}");
        // The final newline is the whole document's to add, not a range's.
        assert_eq!(edited.ends_with('\n'), text.ends_with('\n'), "{path}");
    }
}

#[specforge_test(
    behavior = "lsp_format_document",
    verify = "TextEdit coordinates are 0-indexed lines and columns"
)]
#[tokio::test]
async fn formatting_edits_are_zero_based_utf16_positions() {
    // `é` is 2 bytes and 1 UTF-16 unit.
    let text = "behavior a \"Café\"   {\n  contract \"x\"\n}\n";
    let (_dir, root) = format_project(&[("spec/a.spec", text)]);
    let (mut session, uri) = open_in(&root, "spec/a.spec").await;

    // Only the two spaces formatting removes: after `"Café" ` (19 bytes, 18 UTF-16 units).
    assert_eq!(
        session.format(&uri).await,
        serde_json::json!([{
            "range": {"start": {"line": 0, "character": 18}, "end": {"line": 0, "character": 20}},
            "newText": ""
        }])
    );
}

#[specforge_test(
    behavior = "lsp_format_range",
    verify = "a region left unformatted is reported at its document lines"
)]
#[tokio::test]
async fn range_formatting_publishes_a_kept_region_at_its_line() {
    let text = "behavior a \"A\" {\n  contract \"a\"\n}\n\nbehavior b \"B\" {\n  contract \"b\"\n}\n\nbehavior c \"C\" {\n      contract \"c\"\n}\n\n}}}\n";
    let (_dir, root) = format_project(&[("spec/r.spec", text)]);
    let (mut session, uri) = open_in(&root, "spec/r.spec").await;

    session.range_formatting(&uri, 2, 8, 12).await;
    let published = session.diagnostics(&uri).await;

    let w142 = published
        .iter()
        .find(|d| d["code"] == "W142")
        .unwrap_or_else(|| panic!("{published:?}"));
    assert_eq!(w142["range"]["start"]["line"], 12, "{w142}");
    assert_eq!(w142["range"]["end"]["line"], 12, "{w142}");
    assert_eq!(w142["range"]["end"]["character"], 3, "{w142}");
    assert!(
        w142["message"]
            .as_str()
            .unwrap()
            .starts_with("Parse error at lines 13-13,"),
        "{w142}"
    );
}

#[specforge_test(
    behavior = "lsp_respect_editor_config",
    verify = "the editor is told once when the project's configuration overrides its settings"
)]
#[tokio::test]
async fn precedence_is_logged_once() {
    let two = "behavior login \"Login\" {\n  contract \"The system MUST log in\"\n}\n";
    let (_dir, root) = format_project(&[("spec/a.spec", two)]);
    let (mut session, uri) = open_in(&root, "spec/a.spec").await;
    let overridden = |p: &serde_json::Value| {
        p["message"]
            .as_str()
            .is_some_and(|m| m.starts_with("formatting with the defaults (indent 2, spaces)"))
    };

    assert_eq!(
        session.formatting(&uri, 4).await["result"],
        serde_json::json!([])
    );
    assert_eq!(
        session.formatting(&uri, 4).await["result"],
        serde_json::json!([])
    );

    let first = session
        .notification_within(
            "window/logMessage",
            std::time::Duration::from_secs(2),
            overridden,
        )
        .await;
    let message = first.expect("one log message")["message"].clone();
    assert!(message.as_str().unwrap().contains("tabSize 4"), "{message}");
    let second = session
        .notification_within(
            "window/logMessage",
            std::time::Duration::from_millis(300),
            overridden,
        )
        .await;
    assert!(second.is_none(), "logged twice: {second:?}");
}

// B:autocomplete_entity_ids — verify contract "requires/ensures consistency for entity ID autocomplete"
#[specforge_test(
    behavior = "autocomplete_entity_ids",
    verify = "Autocomplete Entity IDs: entity ID autocomplete holds — graph_available, field_registry_available, matching_ids_suggested, target_kind_filtering_applied"
)]
fn autocomplete_entity_ids_contract() {
    // Requires: graph with entities + prefix
    // Ensures: matching IDs returned with kind and title
    let mut g = Graph::new();
    g.add_node(node("user_login", "behavior", Some("User Login")));
    g.add_node(node("user_logout", "behavior", Some("User Logout")));
    g.add_node(node("auth_token", "type", Some("Auth Token")));

    // What completion asks of the shared ranking: ids and titles.
    let query = EntityQuery::new("user", MatchScope::Names);
    let items = find_entities(&g, &query);

    assert_eq!(items.len(), 2, "only matching IDs returned");
    for item in &items {
        assert!(
            item.node.id.raw.as_str().starts_with("user"),
            "each item must match prefix"
        );
        assert!(
            !item.node.kind.raw.as_str().is_empty(),
            "kind must be populated"
        );
    }
    // With the enclosing field's target kind, only that kind.
    let types = EntityQuery {
        kinds: &["type"],
        ..EntityQuery::new("", MatchScope::Names)
    };
    let ids: Vec<&str> = find_entities(&g, &types)
        .iter()
        .map(|m| m.node.id.raw.as_str())
        .collect();
    assert_eq!(ids, ["auth_token"]);
}

// B:complete_field_names — verify contract "requires/ensures consistency for field name completion"
#[specforge_test(
    behavior = "complete_field_names",
    verify = "Complete Field Names: field name completion holds — field_registry_available, cursor_inside_entity, fields_suggested, snippets_informed"
)]
fn complete_field_names_contract() {
    // Requires: entity kind name + populated FieldRegistry
    // Ensures: field names appropriate for that kind returned
    let text = "behavior login \"Login\" {\n  \n}\n\ngizmo g \"G\" {\n  \n}\n";
    let (_dir, project) = crate::completion::compiled(&[("test.spec", text)]);
    let view = specforge_ops::view::ProjectView::of(&project);
    let labels_at = |line: u32| -> Vec<String> {
        let doc = specforge_lsp::Document::new(text.into());
        let cursor = doc
            .at(tower_lsp::lsp_types::Position::new(line, 2))
            .unwrap();
        specforge_lsp::completion::items(&cursor.completion(), &cursor.word_edit(), false, &view)
            .into_iter()
            .map(|item| item.label)
            .collect()
    };

    let behavior_fields = labels_at(1);
    assert!(
        !behavior_fields.is_empty(),
        "known kind must have field suggestions"
    );
    assert!(
        behavior_fields.iter().any(|f| f == "contract"),
        "behavior must include 'contract'"
    );

    let unknown_fields = labels_at(5);
    assert!(
        unknown_fields.is_empty(),
        "unknown kind must return no fields"
    );
}

// B:complete_keywords — verify contract "requires/ensures consistency for keyword completion"
#[specforge_test(
    behavior = "complete_keywords",
    verify = "Complete Keywords: keyword completion holds — kind_registry_available, cursor_at_top_level, keywords_delegated, structural_keywords_included"
)]
fn complete_keywords_contract() {
    // Requires: set of registered extension kinds
    // Ensures: all registered kinds + structural keywords returned, no duplicates
    let (_dir, project) = crate::completion::compiled(&[("test.spec", "\n")]);
    let view = specforge_ops::view::ProjectView::of(&project);
    let doc = specforge_lsp::Document::new("\n".into());
    let cursor = doc.at(tower_lsp::lsp_types::Position::new(0, 0)).unwrap();
    let keywords: Vec<String> =
        specforge_lsp::completion::items(&cursor.completion(), &cursor.word_edit(), false, &view)
            .into_iter()
            .map(|item| item.label)
            .collect();

    assert!(
        keywords.contains(&"behavior".to_string()),
        "registered kind must be included"
    );
    assert!(
        keywords.contains(&"type".to_string()),
        "registered kind must be included"
    );
    assert!(
        keywords.contains(&"use".to_string()),
        "structural keyword must be included"
    );
    assert!(
        !keywords.contains(&"define".to_string()),
        "define is reserved and registers nothing (ADR 0005)"
    );

    // No duplicates
    let mut sorted = keywords.clone();
    sorted.sort();
    sorted.dedup();
    assert_eq!(keywords.len(), sorted.len(), "must have no duplicates");
}

// B:hover_information — verify contract "requires/ensures consistency for hover information"
#[specforge_test(
    behavior = "hover_information",
    verify = "Hover Information: hover information holds — graph_available, kind_registry_available, hover_delegated, markdown_produced"
)]
fn hover_information_contract() {
    // Requires: entity ID exists in graph
    // Ensures: hover returns markdown with kind, id, title; None for missing
    let served =
        crate::served::Served::new(&[("main.spec", "behavior user_login \"User Login\" {}\n")])
            .open(&["main.spec"]);

    let text = served
        .hover_on("main.spec", "user_login")
        .expect("existing entity must produce hover");
    assert!(text.contains("behavior"), "hover must include kind");
    assert!(text.contains("user_login"), "hover must include id");
    assert!(text.contains("User Login"), "hover must include title");

    // Nothing is named at a position past the document's last line.
    assert!(
        served.hover("main.spec", 9, 0).is_none(),
        "no entity must return None"
    );
}

// B:goto_import_definition — verify contract "requires/ensures consistency for import go-to-definition"
#[specforge_test(
    behavior = "goto_import_definition",
    verify = "Go-to-Definition on Imports: import go-to-definition holds — imports_resolved, target_file_navigated"
)]
fn goto_import_definition_contract() {
    // Requires: use import paths + the sources the project was compiled from
    // Ensures: resolves to the source's location; None for missing
    let served = crate::served::Served::new(&[
        ("behaviors/auth.spec", "behavior auth \"Auth\" {}\n"),
        (
            "main.spec",
            "use \"behaviors/auth\"\nuse \"nonexistent/path\"\n",
        ),
    ])
    .open(&["main.spec"]);
    let at = |line: u32| {
        specforge_lsp::answers::definition(
            &served.state(),
            &served.uri("main.spec"),
            Position::new(line, 5),
        )
    };

    let Some(GotoDefinitionResponse::Scalar(location)) = at(0) else {
        panic!("valid import path must resolve");
    };
    assert_eq!(
        location.uri,
        served.uri("behaviors/auth.spec"),
        "must resolve to correct file"
    );
    assert_eq!(
        location.range,
        tower_lsp::lsp_types::Range::default(),
        "from its first line"
    );

    assert!(at(1).is_none(), "missing import must return None");
}

// B:prepare_rename — verify contract "requires/ensures consistency for prepare rename"
#[specforge_test(
    behavior = "prepare_rename",
    verify = "Prepare Rename: prepare rename holds — graph_available, token_range_returned, non_renameable_rejected"
)]
fn contract_prepare_rename() {
    // Requires: the graph, built from the files' text
    // Ensures: the token under the cursor (declaration or reference) is
    // renameable, with its range; anything else is not.
    let state = buffers(&[
        (
            "/p/types.spec",
            "\n\n\n\ntype auth_token \"auth_token\" {\n}\n",
        ),
        (
            "/p/auth.spec",
            "behavior login \"L\" {\n  types [auth_token]\n}\n",
        ),
    ]);
    let types = uri_of_path("/p/types.spec");
    let auth = uri_of_path("/p/auth.spec");
    let ask = |uri: &Url, line: u32, character: u32| {
        answers::prepare_rename(&state, uri, Position::new(line, character))
            .expect("the buffers are the compiled text")
    };
    let range = |response: Option<PrepareRenameResponse>| match response {
        Some(PrepareRenameResponse::Range(range)) => Some((
            range.start.line,
            range.start.character,
            range.end.line,
            range.end.character,
        )),
        None => None,
        other => panic!("a range, or nothing: {other:?}"),
    };

    assert_eq!(
        range(ask(&types, 4, 7)),
        Some((4, 5, 4, 15)),
        "the declaration's name is renameable"
    );
    assert_eq!(
        range(ask(&auth, 1, 11)),
        Some((1, 9, 1, 19)),
        "a reference's token is renameable"
    );

    // The title naming it, a keyword, nothing: not renameable.
    assert_eq!(range(ask(&types, 4, 19)), None);
    assert_eq!(range(ask(&types, 4, 1)), None);
    assert_eq!(range(ask(&types, 0, 0)), None);
}

// B:rename_entity_id — verify contract "requires/ensures consistency for entity rename"
#[specforge_test(
    behavior = "rename_entity_id",
    verify = "Rename Entity ID: entity rename holds — graph_available, prepare_rename_ready, all_references_updated, rename_atomic, entity_renamed_emitted"
)]
fn rename_entity_id_contract() {
    // Requires: entity in graph with references from other entities + new name
    // Ensures: edits for the declaration and every reference, nothing
    // else; a taken name, or a file the rename cannot read, is refused.
    let state = buffers(&[
        ("/p/types.spec", "type auth_token \"auth_token\" {\n}\n"),
        (
            "/p/auth.spec",
            "behavior user_login \"L\" {\n  types [auth_token]\n  // auth_token\n}\n",
        ),
    ]);
    let types = uri_of_path("/p/types.spec");
    let rename = |new_name: &str| answers::rename(&state, &types, Position::new(0, 6), new_name);
    let edit = rename("session_token")
        .expect("valid rename must produce edits")
        .expect("an edit");
    let mut edits: Vec<(String, u32, u32)> = edit
        .changes
        .expect("changes")
        .into_iter()
        .flat_map(|(uri, edits)| {
            let file = uri.path().to_string();
            edits
                .into_iter()
                .map(move |e| (file.clone(), e.range.start.line, e.range.start.character))
        })
        .collect();
    edits.sort();
    assert_eq!(
        edits,
        [
            ("/p/auth.spec".to_string(), 1, 9),
            ("/p/types.spec".to_string(), 0, 5)
        ]
    );

    // Reject rename to existing ID
    let dup = rename("user_login").expect_err("a taken name is refused");
    assert_eq!(dup.code, tower_lsp::jsonrpc::ErrorCode::InvalidParams);
    assert!(
        dup.message.contains("'user_login' exists"),
        "the plan's reason: {dup:?}"
    );

    // All or nothing: a file the rename cannot read refuses the whole.
    let blind = specforge_ops::read::navigate::Navigator::new(state.view(), |file: &str| {
        (file != "/p/auth.spec").then(|| "type auth_token \"auth_token\" {\n}\n".to_string())
    });
    let refused = specforge_ops::authoring::rename::plan(&blind, "auth_token", "session_token");
    assert_eq!(
        refused.unwrap_err().error.code,
        specforge_ops::authoring::rename::UNREADABLE
    );
}

// B:outline_view — verify contract "requires/ensures consistency for outline view"
#[specforge_test(
    behavior = "outline_view",
    verify = "Outline View: outline view holds — graph_available, kind_registry_available, all_entities_listed, symbol_kind_delegated"
)]
fn outline_view_contract() {
    // Requires: graph with entities across files
    // Ensures: the outline of a file is its entities, in line order, each
    // with its kind, id and title, selecting its name
    let mut state = buffers(&[
        (
            "/p/test.spec",
            "type b \"B\" {\n}\n\nbehavior a \"A\" {\n}\n",
        ),
        ("/p/other.spec", "event c \"C\" {\n}\n"),
    ]);
    state.set_client(specforge_lsp::ClientSupport {
        hierarchical_symbols: true,
        ..Default::default()
    });
    let Some(DocumentSymbolResponse::Nested(symbols)) =
        answers::document_symbols(&state, &uri_of_path("/p/test.spec"))
    else {
        panic!("a hierarchical client gets nested symbols");
    };

    let ids: Vec<&str> = symbols.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(
        ids,
        ["b", "a"],
        "only entities from target file, in line order"
    );
    for symbol in &symbols {
        assert!(
            symbol.detail.as_deref().is_some_and(|d| !d.is_empty()),
            "each symbol must have kind"
        );
        assert!(
            symbol.detail.as_deref().is_some_and(|d| d.contains(" — ")),
            "each symbol must have title"
        );
        assert_eq!(
            symbol.selection_range.start.line, symbol.range.start.line,
            "the name is selected"
        );
    }
}

// B:workspace_symbol_search — verify contract "requires/ensures consistency for workspace symbol search"
#[specforge_test(
    behavior = "workspace_symbol_search",
    verify = "Workspace Symbol Search: workspace symbol search holds — graph_available, kind_registry_available, matching_entities_returned, symbol_kind_delegated"
)]
fn workspace_symbol_search_contract() {
    // Requires: graph with entities + search query
    // Ensures: results match by ID prefix or title fragment with kind
    let mut g = Graph::new();
    g.add_node(node_at("user_login", "behavior", "a.spec", 0, 0));
    g.add_node(node_at("user_logout", "behavior", "a.spec", 5, 0));
    g.add_node(node_at("auth_token", "type", "b.spec", 0, 0));

    // What workspace symbols ask of the shared ranking: ids and titles.
    let by_prefix = find_entities(&g, &EntityQuery::new("user", MatchScope::Names));
    assert_eq!(by_prefix.len(), 2, "ID prefix search must match");

    let by_title = find_entities(&g, &EntityQuery::new("Auth", MatchScope::Names));
    assert_eq!(by_title.len(), 1, "title fragment search must match");
    assert_eq!(
        by_title[0].node.kind.raw, "type",
        "result must include kind"
    );
}

// B:provide_semantic_tokens — verify contract "requires/ensures consistency for semantic tokens"
#[specforge_test(
    behavior = "provide_semantic_tokens",
    verify = "Provide Semantic Tokens: semantic tokens holds — graph_available, kind_registry_available, tokens_classified, structural_keywords_enforced, extension_delegation_applied"
)]
fn provide_semantic_tokens_contract() {
    // Requires: source text + registered kinds
    // Ensures: tokens classified with correct types (keyword, property, string for triple-quoted)
    let source = "behavior foo \"Foo\" {\n  contract \"\"\"\n    hello\n  \"\"\"\n}\n";
    let registries = verifiable(&["behavior"], &[]);
    let graph = Graph::new();
    let project = specforge_project::testing::Assembled::over(registries).graph(graph);
    let view = specforge_ops::view::ProjectView::of(project.detached());
    let tokens = specforge_lsp::Document::new(source.into()).tokens(&view);

    assert!(!tokens.is_empty(), "must produce tokens");
    assert!(
        tokens
            .iter()
            .any(|t| t.text == "behavior" && t.token_type == "type"),
        "entity keyword must be classified as type"
    );
    assert!(
        tokens
            .iter()
            .any(|t| t.text == "contract" && t.token_type == "property"),
        "field names must be classified as property"
    );
    assert!(
        tokens.iter().any(|t| t.token_type == "string"),
        "triple-quoted strings must be classified as string"
    );
}

// B:code_action_create_entity_stub — verify contract "requires/ensures consistency for create entity stub"
#[specforge_test(
    behavior = "code_action_create_entity_stub",
    verify = "Code Action: Create Entity Stub: create entity stub holds — graph_available, field_registry_available, stub_created, kind_inferred, no_code_generated"
)]
fn code_action_create_entity_stub_contract() {
    // Requires: an E003 for an id no entity has + the enclosing field's
    // target kind in the FieldRegistry
    // Ensures: a refactoring that appends a bare stub of that kind to the
    // current file; none without a target kind
    let text = "behavior login \"L\" {\n  invariants [missing_inv]\n}\n";
    let state = buffers(&[("/p/auth.spec", text)]);
    let diagnostics = state.session().unwrap().project().diagnostics();
    let fixes_with = |target_kind: Option<&str>| {
        let project = specforge_project::testing::Assembled::over(invariants_field(target_kind))
            .graph(state.graph().clone());
        let view = specforge_ops::view::ProjectView::of(project.detached());
        let nav =
            specforge_ops::read::navigate::Navigator::new(view, |_: &str| Some(text.to_string()));
        nav.fixes(
            &diagnostics,
            &specforge_ops::read::navigate::FixQuery::default(),
        )
    };

    let fixes = fixes_with(Some("invariant"));
    let stub = fixes
        .iter()
        .find(|f| f.kind == specforge_ops::read::navigate::FixKind::Refactor)
        .expect("entity stub must be created with target_kind");
    assert_eq!(stub.title, "Create invariant stub for missing_inv");
    let edit = &stub.edits[0];
    assert!(
        edit.new_text.contains("invariant missing_inv"),
        "stub must use correct kind and ID"
    );
    assert_eq!(
        edit.span.file, "/p/auth.spec",
        "stub must target current file"
    );
    assert!(!edit.new_text.contains("fn "), "no application code");

    assert!(
        fixes_with(None)
            .iter()
            .all(|f| f.kind != specforge_ops::read::navigate::FixKind::Refactor),
        "no stub without target_kind"
    );
}

/// `behavior.invariants` as a reference list targeting `target_kind`.
fn invariants_field(target_kind: Option<&str>) -> specforge_registry::RegistryBuild {
    crate::registries::registries("@test/ext", |c| {
        c.kind("behavior", |k| {
            k.field("invariants", |f| {
                f.field_type(specforge_extension_sdk::prelude::FieldType::ReferenceList);
                if let Some(target_kind) = target_kind {
                    f.target_kind(target_kind);
                }
            });
        });
        if let Some(target_kind) = target_kind {
            c.kind(target_kind, |k| {
                k.keyword(target_kind);
            });
        }
    })
}

// B:code_actions_for_missing_verify — verify contract "requires/ensures consistency for missing verify code actions"
/// A registry where `kinds` accept verify statements of `verify_kinds`.
fn verifiable(kinds: &[&str], verify_kinds: &[&str]) -> specforge_registry::RegistryBuild {
    crate::registries::registries("@test/ext", |c| {
        for kind in kinds {
            c.kind(kind, |k| {
                k.testable(true)
                    .supports_verify(true)
                    .verify_kinds(verify_kinds);
            });
        }
    })
}

#[specforge_test(
    behavior = "code_actions_for_missing_verify",
    verify = "Code Actions for Missing Verify: missing verify code actions holds — kind_registry_available, graph_available, quickfix_offered, verify_stubs_produced, no_code_generated"
)]
fn code_actions_for_missing_verify_contract() {
    // Requires: testable entity without verify statements
    // Ensures: quickfix code action with verify stub targeting the .spec file
    let text = "\n\n\n\nbehavior my_behavior \"B\" {\n  contract \"c\"\n}\n";
    let state = buffers(&[("/p/a.spec", text)]);
    let project = specforge_project::testing::Assembled::over(verifiable(&["behavior"], &[]))
        .graph(state.graph().clone());
    let view = specforge_ops::view::ProjectView::of(project.detached());
    let nav = specforge_ops::read::navigate::Navigator::new(view, |_: &str| Some(text.to_string()));
    let fixes = nav.fixes(&[], &specforge_ops::read::navigate::FixQuery::default());

    assert!(
        !fixes.is_empty(),
        "untested testable entity must produce code action"
    );
    assert_eq!(fixes[0].subject, Some("my_behavior".into()));
    assert_eq!(
        fixes[0].kind,
        specforge_ops::read::navigate::FixKind::QuickFix,
        "must be quickfix action"
    );
    let edit = &fixes[0].edits[0];
    assert!(
        edit.new_text.contains("verify unit"),
        "stub must include verify statement"
    );
    assert!(
        edit.span.file.as_str().ends_with(".spec"),
        "edit must target .spec file"
    );
    assert_eq!(edit.span.start_line, 7, "before the block's closing brace");
}

// B:go_to_definition — verify contract "requires/ensures consistency for go-to-definition"
#[specforge_test(
    behavior = "go_to_definition",
    verify = "Go-to-Definition: go-to-definition holds — graph_available, declaration_site_returned"
)]
fn go_to_definition_contract() {
    // Requires: graph with resolved entity declarations
    // Ensures: the declaration site (file, line, column of the block
    // header) returned for an existing entity, its name selected; none for
    // a missing one.
    let mut state = buffers(&[
        ("/p/types.spec", "\n\ntype   auth_token \"Token\" {\n}\n"),
        (
            "/p/auth.spec",
            "behavior login \"L\" {\n  types [auth_token, nonexistent]\n}\n",
        ),
    ]);
    state.set_client(specforge_lsp::ClientSupport {
        definition_links: true,
        ..Default::default()
    });
    let auth = uri_of_path("/p/auth.spec");

    let Some(GotoDefinitionResponse::Link(links)) =
        answers::definition(&state, &auth, Position::new(1, 12))
    else {
        panic!("existing entity must return declaration site");
    };
    assert_eq!(
        links[0].target_uri,
        uri_of_path("/p/types.spec"),
        "must return correct file"
    );
    let block = links[0].target_range.start;
    assert_eq!((block.line, block.character), (2, 0), "the block header");
    let name = links[0].target_selection_range;
    assert_eq!(
        (name.start.line, name.start.character, name.end.character),
        (2, 7, 17),
        "the name is selected"
    );

    assert!(
        answers::definition(&state, &auth, Position::new(1, 24)).is_none(),
        "missing entity must return nothing"
    );
}

// B:incremental_document_sync — verify contract "requires/ensures consistency for incremental document sync"
#[specforge_test(
    behavior = "incremental_document_sync",
    verify = "Incremental Document Sync: incremental document sync holds — lsp_initialized_fired, document_open, buffer_consistent, partial_update_applied"
)]
fn incremental_document_sync_contract() {
    // Requires: LSP initialized with INCREMENTAL sync, document open
    // Ensures: buffer consistent after partial update; only changed range applied
    let mut buf =
        specforge_lsp::Document::new("behavior foo \"Foo\" {\n  contract \"old\"\n}\n".into());

    // Apply partial change: only replace "old" with "new"
    buf.apply_change(Some(crate::lsp_range(1, 12, 1, 15)), "new");
    assert_eq!(
        buf.text(),
        "behavior foo \"Foo\" {\n  contract \"new\"\n}\n",
        "buffer must reflect incremental change"
    );

    // Apply another partial change at a different location
    buf.apply_change(Some(crate::lsp_range(0, 9, 0, 12)), "bar");
    assert_eq!(
        buf.text(),
        "behavior bar \"Foo\" {\n  contract \"new\"\n}\n",
        "buffer must reflect second incremental change"
    );
}

// B:live_diagnostics — verify contract "requires/ensures consistency for live diagnostics"
#[specforge_test(
    behavior = "emit_live_diagnostics",
    verify = "Live Diagnostics: live diagnostics holds — lsp_initialized_fired, graph_available, diagnostics_pushed, latency_enforced"
)]
#[tokio::test]
async fn live_diagnostics_contract() {
    // lsp_initialized_fired, graph_available: an initialized session with
    // a document compiled into the graph, cleanly.
    let (mut session, _) = crate::session::Session::start(None).await;
    let uri = "file:///buffer/live.spec";
    let text = "behavior login \"Login\" {\n  invariants [session_limit]\n}\n\n\
                invariant session_limit \"Limit\" {\n}\n";
    session.open(uri, text).await;
    let clean = session.diagnostics(uri).await;
    assert!(clean.is_empty(), "{clean:?}");

    // diagnostics_pushed: an edit renaming the invariant (line 4, columns
    // 10..23) leaves the reference dangling; the recompiled diagnostics
    // are pushed without being asked for.
    let edit = |version: i32, new_id: &str, old_len: u32| {
        serde_json::json!({
            "textDocument": {"uri": uri, "version": version},
            "contentChanges": [{
                "range": {
                    "start": {"line": 4, "character": 10},
                    "end": {"line": 4, "character": 10 + old_len},
                },
                "text": new_id,
            }],
        })
    };
    let typed = std::time::Instant::now();
    session
        .notify("textDocument/didChange", edit(2, "quota", 13))
        .await;
    let broken = session.diagnostics(uri).await;
    // latency_enforced: squiggles within 100ms of the last keystroke.
    let latency = typed.elapsed();
    assert_eq!(crate::session::codes(&broken), ["E003"], "{broken:?}");
    assert_eq!(
        broken[0]["message"],
        "unresolved reference 'session_limit' in entity 'login'"
    );
    assert!(latency.as_millis() <= 100, "diagnostics took {latency:?}");

    // Every change is recompiled: restoring the ID clears them again.
    session
        .notify("textDocument/didChange", edit(3, "session_limit", 5))
        .await;
    let fixed = session.diagnostics(uri).await;
    assert!(fixed.is_empty(), "{fixed:?}");
}

#[test]
fn shared_incremental_pipeline_contract() {
    // Requires: incremental_rebuild_complete event has fired
    // Ensures: shared graph updated, diagnostics pushed, pipeline parity enforced
    // Open a doc and build the graph through the session
    let state = buffers(&[("/a.spec", "behavior a \"A\" {}\n")]);

    // Graph is shared: navigation works on the same graph instance
    let def = answers::definition(&state, &uri_of_path("/a.spec"), Position::new(0, 10));
    assert!(def.is_some(), "shared graph must serve navigation");
}

/// Build a Wasm runtime for a temp project listing `ext_names`, mirroring
/// how a real session loads extensions from specforge.json.
fn wasm_runtime_for(ext_names: &[String]) -> specforge_component::ComponentRuntime {
    let runtime = specforge_component::ComponentRuntime::new();
    let names: Vec<String> = ext_names.iter().map(|name| name.to_string()).collect();
    specforge_component::builtins::load_builtins_for(&runtime, &names).unwrap();
    runtime
}
