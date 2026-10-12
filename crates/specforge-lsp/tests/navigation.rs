//! The LSP's own navigation: span conversion and `use`-path definitions.
//! What definitions and references are is `specforge_ops::navigate`'s
//! (tested in specforge-ops); the LSP's answers are in `e2e_support`.

use specforge_common::{SourceSpan, Sym};
use specforge_test_macros::test as spec;
use std::fs;

// -- LineIndex::range (1-based → 0-based conversion) --------------------------

#[spec(
    behavior = "go_to_definition",
    verify = "source spans convert from 1-based to 0-based for LSP"
)]
fn source_span_converts_1based_to_0based() {
    // Parser produces 1-based spans (line 3, col 1 means third line, first column)
    let span = SourceSpan {
        file: Sym::new("test.spec"),
        start_line: 3,
        start_col: 1,
        end_line: 5,
        end_col: 2,
    };
    let text = "a\nb\nline three\nd\n}}\n";
    let lsp = specforge_lsp::LineIndex::new(text).range(&span);
    // LSP protocol uses 0-based
    assert_eq!((lsp.start.line, lsp.start.character), (2, 0));
    assert_eq!((lsp.end.line, lsp.end.character), (4, 1));
}

#[spec(
    behavior = "go_to_definition",
    verify = "source spans convert from 1-based to 0-based for LSP"
)]
fn source_span_zero_saturates() {
    // Edge case: span with 0 values shouldn't underflow
    let span = SourceSpan {
        file: Sym::new("test.spec"),
        start_line: 0,
        start_col: 0,
        end_line: 0,
        end_col: 0,
    };
    let lsp = specforge_lsp::LineIndex::new("type token {}\n").range(&span);
    assert_eq!((lsp.start.line, lsp.start.character), (0, 0));
    assert_eq!((lsp.end.line, lsp.end.character), (0, 0));
}

// -- goto_import_definition ---------------------------------------------------

/// Go to the target of `use "<import>"` in `main.spec` under `spec_root`.
fn goto_import(spec_root: &std::path::Path, import: &str) -> Option<specforge_common::SourceSpan> {
    specforge_lsp::goto_import_definition(import, "main.spec", spec_root)
}

/// The resolver's cascade, not a hand-built `{root}/{import}.spec`: a
/// relative path from a nested file and a directory's `index.spec`.
#[spec(
    behavior = "goto_import_definition",
    verify = "go-to-def on use path navigates to target file"
)]
fn goto_import_resolves_relative_and_index_targets() {
    let tmp = tempfile::tempdir().unwrap();
    fs::create_dir_all(tmp.path().join("models")).unwrap();
    fs::create_dir_all(tmp.path().join("sub")).unwrap();
    fs::write(tmp.path().join("models/index.spec"), "term m \"M\" {}\n").unwrap();
    fs::write(tmp.path().join("types.spec"), "term t \"T\" {}\n").unwrap();
    let goto = |import: &str| {
        specforge_lsp::goto_import_definition(import, "sub/main.spec", tmp.path())
            .map(|s| s.file.to_string())
    };
    assert_eq!(goto("../types").as_deref(), Some("types.spec"));
    assert_eq!(goto("models").as_deref(), Some("models/index.spec"));
    assert_eq!(goto("@specforge/software"), None);
}

#[spec(
    behavior = "goto_import_definition",
    verify = "go-to-def on use path navigates to target file"
)]
fn goto_import_navigates_to_file() {
    let tmp = tempfile::tempdir().unwrap();
    let behaviors_dir = tmp.path().join("behaviors");
    fs::create_dir_all(&behaviors_dir).unwrap();
    fs::write(
        behaviors_dir.join("auth.spec"),
        "behavior auth \"Auth\" {}\n",
    )
    .unwrap();

    let result = goto_import(tmp.path(), "behaviors/auth");
    let loc = result.expect("should resolve import");
    assert_eq!(loc.file.as_str(), "behaviors/auth.spec");
    assert_eq!(loc.start_line, 0);
}

#[spec(
    behavior = "goto_import_definition",
    verify = "go-to-def on non-existent use path returns no result"
)]
fn goto_import_returns_none_for_missing() {
    let tmp = tempfile::tempdir().unwrap();
    assert!(goto_import(tmp.path(), "nonexistent/path").is_none());
    // Nor does a path that leaves the spec root.
    fs::write(tmp.path().join("inside.spec"), "term inside \"I\" {}\n").unwrap();
    let root = tmp.path().join("spec");
    fs::create_dir_all(&root).unwrap();
    assert!(goto_import(&root, "../inside").is_none());
    assert!(goto_import(&root, "a/../../inside").is_none());
}

// -- goto_import_definition (LSP dispatch integration) ------------------------

#[spec(
    behavior = "goto_import_definition",
    verify = "go-to-def on use path navigates to target file"
)]
fn goto_definition_dispatches_to_import_on_use_line() {
    // A document with a use statement: the cursor on it names its path, and
    // the definition is the imported file, from its first line.
    let served = crate::served::Served::new(&[
        ("behaviors/auth.spec", "behavior auth \"Auth\" {}\n"),
        (
            "main.spec",
            "use \"behaviors/auth\"\n\nbehavior login \"Login\" {}\n",
        ),
    ])
    .open(&["main.spec"]);
    let definition = specforge_lsp::answers::definition(
        &served.state(),
        &served.uri("main.spec"),
        tower_lsp::lsp_types::Position::new(0, 1),
    );
    let Some(tower_lsp::lsp_types::GotoDefinitionResponse::Scalar(location)) = definition else {
        panic!("should resolve import from use line: {definition:?}");
    };
    assert_eq!(location.uri, served.uri("behaviors/auth.spec"));
    assert_eq!(location.range, tower_lsp::lsp_types::Range::default());
}

// -- go-to-definition on a `use` names a project source (04-T0) -------------------

fn definition_of_use_at_line_zero(
    state: &specforge_lsp::LspState,
    uri: &tower_lsp::lsp_types::Url,
) -> Option<tower_lsp::lsp_types::GotoDefinitionResponse> {
    specforge_lsp::answers::definition(state, uri, tower_lsp::lsp_types::Position::new(0, 5))
}

#[spec(
    behavior = "goto_import_definition",
    verify = "go-to-def on a use path names only a project source, an open unsaved document included"
)]
#[ignore = "fixed by T1"]
fn go_to_definition_on_a_use_names_an_open_unsaved_document() {
    let mut served = crate::served::Served::new(&[("a.spec", "use \"b\"\n\nterm a \"A\" {\n}\n")])
        .open(&["a.spec"]);
    served.sent();
    let b = served.uri("b.spec");
    served.open_document(&b, "term b \"B\" {\n}\n");

    let a = served.uri("a.spec");
    let definition = definition_of_use_at_line_zero(&served.state(), &a);
    assert_eq!(
        definition,
        Some(tower_lsp::lsp_types::GotoDefinitionResponse::Scalar(
            tower_lsp::lsp_types::Location {
                uri: b,
                range: tower_lsp::lsp_types::Range::default(),
            }
        ))
    );
    let last = served
        .sent()
        .into_iter()
        .rev()
        .find_map(|sent| match sent {
            crate::recorder::Sent::Published {
                uri, diagnostics, ..
            } if uri == a => Some(diagnostics),
            _ => None,
        })
        .expect("a.spec is published again");
    assert!(
        !last.iter().any(|d| matches!(
            &d.code,
            Some(tower_lsp::lsp_types::NumberOrString::String(code)) if code == "E025"
        )),
        "{last:?}"
    );
}

#[spec(
    behavior = "goto_import_definition",
    verify = "go-to-def on a use path names only a project source, an open unsaved document included"
)]
#[ignore = "fixed by T1"]
fn go_to_definition_on_a_use_of_an_excluded_file_names_nothing() {
    let dir = tempfile::tempdir().unwrap();
    for (path, text) in [
        (
            "specforge.json",
            r#"{"name":"l","version":"0.1.0","extensions":[],"exclude":["drafts/"]}"#,
        ),
        ("drafts/x.spec", "term x \"X\" {\n}\n"),
        ("main.spec", "use \"drafts/x\"\n\nterm m \"M\" {\n}\n"),
    ] {
        let path = dir.path().join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }
    let served = crate::served::Served::at(dir).open(&["main.spec"]);

    let main = served.uri("main.spec");
    assert_eq!(definition_of_use_at_line_zero(&served.state(), &main), None);
}

#[spec(
    behavior = "goto_import_definition",
    verify = "go-to-def on use path navigates to target file"
)]
fn go_to_definition_on_a_use_answers_while_the_session_is_out() {
    let mut served = crate::served::Served::new(&[
        ("types.spec", "term t \"T\" {\n}\n"),
        ("main.spec", "use \"types\"\n\nterm m \"M\" {\n}\n"),
    ])
    .open(&["main.spec"]);
    let types = served.uri("types.spec");
    let main = served.uri("main.spec");

    let definition = served.rebuilding(|state| definition_of_use_at_line_zero(state, &main));

    assert_eq!(
        definition,
        Some(tower_lsp::lsp_types::GotoDefinitionResponse::Scalar(
            tower_lsp::lsp_types::Location {
                uri: types,
                range: tower_lsp::lsp_types::Range::default(),
            }
        ))
    );
}
