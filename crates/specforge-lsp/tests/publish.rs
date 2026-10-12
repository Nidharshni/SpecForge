//! What a publish sends (`publish::Publication::of`), computed from the
//! LSP state without a client.

use specforge_lsp::LspState;
use specforge_lsp::publish::{FilePublication, Publication};
use specforge_project::{Buffer, CheckMode, ProjectSession, SourceChange};
use specforge_test_macros::test as spec;
use std::path::Path;
use std::sync::Arc;
use tower_lsp::lsp_types::{DiagnosticTag, Url};

/// A project of `files` with `extensions`, opened as the LSP opens it.
fn project(extensions: &[&str], files: &[(&str, &str)]) -> (tempfile::TempDir, LspState) {
    let dir = tempfile::TempDir::new().unwrap();
    std::fs::write(
        dir.path().join("specforge.json"),
        serde_json::json!({"name": "p", "extensions": extensions}).to_string(),
    )
    .unwrap();
    for (name, text) in files {
        std::fs::write(dir.path().join(name), text).unwrap();
    }
    let runtime = specforge_component::ComponentRuntime::with_user_cache();
    let mut state = LspState::new();
    state.set_session(ProjectSession::open_with_runtime(
        &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
        Some(Arc::new(runtime)),
    ));
    (dir, state)
}

/// The URI of `name` in `dir`.
fn uri(dir: &Path, name: &str) -> Url {
    Url::from_file_path(dir.join(name)).unwrap()
}

/// The codes a file is sent.
fn codes(file: Option<&FilePublication>) -> Vec<String> {
    file.map(|f| {
        f.diagnostics
            .iter()
            .filter_map(|d| match &d.code {
                Some(tower_lsp::lsp_types::NumberOrString::String(code)) => Some(code.clone()),
                _ => None,
            })
            .collect()
    })
    .unwrap_or_default()
}

const SOFTWARE: &[&str] = &["@specforge/software"];

/// A file the project reports nothing about.
const CLEAN: &str = "behavior beta \"B\" {\n  contract \"y\"\n  category command\n}\n";

#[spec(
    behavior = "emit_live_diagnostics",
    verify = "a diagnostic is published on the file its span names"
)]
fn each_diagnostic_goes_on_the_file_its_span_names() {
    let (dir, mut state) = project(
        SOFTWARE,
        &[
            (
                "a.spec",
                "behavior a \"A\" {\n  contract \"x\"\n  features [ghost]\n}\n",
            ),
            ("b.spec", CLEAN),
        ],
    );
    let a = uri(dir.path(), "a.spec");
    let b = uri(dir.path(), "b.spec");
    state.open_document(&b, CLEAN, 1);
    let publication = Publication::of(&state, Some(&b), &[]);
    let on_a = publication.files.get(&a).expect("a.spec is published");
    let unresolved = on_a
        .diagnostics
        .iter()
        .find(|d| d.message.contains("ghost"))
        .unwrap_or_else(|| panic!("{:?}", on_a.diagnostics));
    assert_eq!(
        (
            unresolved.range.start.line,
            unresolved.range.start.character
        ),
        (2, 12),
        "the reference's token"
    );
    assert_eq!(on_a.placed.len(), on_a.diagnostics.len());
    // The edited document is a target even with nothing on it.
    assert!(codes(publication.files.get(&b)).is_empty());
    assert!(publication.files.contains_key(&b));
}

/// Two behaviors that depend on each other (W061, a spanless cycle).
const CYCLE: &str = "behavior alpha \"A\" {\n  depends_on [beta]\n}\nbehavior beta \"B\" {\n  depends_on [alpha]\n}\n";

#[spec(
    behavior = "emit_live_diagnostics",
    verify = "a spanless diagnostic about entities is published at the first one's name"
)]
fn a_spanless_cycle_goes_on_the_first_entitys_name() {
    let (dir, mut state) = project(
        SOFTWARE,
        &[
            ("cycle.spec", CYCLE),
            ("other.spec", "behavior gamma \"G\" {\n}\n"),
        ],
    );
    let cycle = uri(dir.path(), "cycle.spec");
    let other = uri(dir.path(), "other.spec");
    state.open_document(&other, "behavior gamma \"G\" {\n}\n", 1);
    let publication = Publication::of(&state, Some(&other), &[]);
    let w061 = publication.files[&cycle]
        .diagnostics
        .iter()
        .find(|d| d.code == Some(tower_lsp::lsp_types::NumberOrString::String("W061".into())))
        .expect("W061 on the cycle's file");
    let at = |r: &tower_lsp::lsp_types::Range| (r.start.line, r.start.character, r.end.character);
    assert_eq!(at(&w061.range), (0, 9, 14), "alpha's name");
    let related = w061.related_information.as_ref().expect("the others");
    assert_eq!(related.len(), 1);
    assert_eq!(related[0].location.uri, cycle);
    assert_eq!(at(&related[0].location.range), (3, 9, 13), "beta's name");
    // What code actions read back carries the placed span.
    let placed = publication.files[&cycle]
        .placed
        .iter()
        .find(|d| d.code == "W061")
        .unwrap();
    assert!(placed.span.is_some());
}

#[spec(
    behavior = "emit_live_diagnostics",
    verify = "a diagnostic about no entity is published on the edited document"
)]
fn a_diagnostic_about_nothing_goes_on_the_edited_document_else_the_anchor_else_the_first_open() {
    // No extension configured: I002, about no entity and no file.
    let (dir, mut state) = project(&[], &[("a.spec", "\n"), ("b.spec", "\n")]);
    let a = uri(dir.path(), "a.spec");
    let b = uri(dir.path(), "b.spec");
    state.open_document(&a, "\n", 1);
    state.open_document(&b, "\n", 1);
    let about_nothing = |publication: &Publication, file: &Url| {
        codes(publication.files.get(file)).contains(&"I002".to_string())
    };

    // On the edited document.
    let publication = Publication::of(&state, Some(&b), &[]);
    assert!(about_nothing(&publication, &b), "{:?}", publication.files);
    assert!(!about_nothing(&publication, &a));
    assert_eq!(publication.anchor.as_ref(), Some(&b));
    state.record(&publication);

    // With no edit: on the anchor while it is open.
    let publication = Publication::of(&state, None, &[]);
    assert!(about_nothing(&publication, &b));
    state.record(&publication);

    // The anchor closed: on the first open document.
    state.close_document(&b);
    let publication = Publication::of(&state, None, &[]);
    assert!(about_nothing(&publication, &a), "{:?}", publication.files);
}

#[spec(
    behavior = "emit_live_diagnostics",
    verify = "a publish clears the files whose diagnostics are gone"
)]
fn a_publish_clears_files_whose_diagnostics_are_gone() {
    let broken =
        "behavior alpha \"A\" {\n  contract \"x\"\n  category command\n  features [ghost]\n}\n";
    let (dir, mut state) = project(SOFTWARE, &[("a.spec", broken)]);
    let a = uri(dir.path(), "a.spec");
    let publication = Publication::of(&state, None, &[]);
    assert!(!codes(publication.files.get(&a)).is_empty());
    state.record(&publication);
    assert!(!state.diagnostics(a.as_str()).is_empty());

    // The reference is fixed: the file is sent an empty list, once.
    let fixed = "behavior alpha \"A\" {\n  contract \"x\"\n  category command\n}\n";
    state.session_mut().unwrap().update_with(
        SourceChange::Hold(&[Buffer::new(dir.path().join("a.spec"), fixed)]),
        CheckMode::Full,
    );
    let publication = Publication::of(&state, None, &[]);
    let file = publication.files.get(&a).expect("a.spec is cleared");
    assert!(file.diagnostics.is_empty(), "{:?}", file.diagnostics);
    state.record(&publication);
    assert!(state.diagnostics(a.as_str()).is_empty());
    let publication = Publication::of(&state, None, &[]);
    assert!(
        !publication.files.contains_key(&a),
        "a cleared file is not sent again"
    );
}

#[spec(
    behavior = "emit_live_diagnostics",
    verify = "a define block's W143 is published as unnecessary code"
)]
fn a_define_block_is_published_as_unnecessary() {
    let (dir, state) = project(
        SOFTWARE,
        &[(
            "a.spec",
            "define widget {\n  x \"y\"\n}\n\nbehavior b \"B\" {\n  features [ghost]\n}\n",
        )],
    );
    let a = uri(dir.path(), "a.spec");
    let publication = Publication::of(&state, None, &[]);
    let file = &publication.files[&a];
    let w143 = file
        .diagnostics
        .iter()
        .find(|d| d.code == Some(tower_lsp::lsp_types::NumberOrString::String("W143".into())))
        .unwrap_or_else(|| panic!("{:?}", file.diagnostics));
    assert_eq!(w143.tags, Some(vec![DiagnosticTag::UNNECESSARY]));
    assert!(
        file.diagnostics
            .iter()
            .filter(|d| d.code != w143.code)
            .all(|d| d.tags.is_none()),
        "{:?}",
        file.diagnostics
    );
}
