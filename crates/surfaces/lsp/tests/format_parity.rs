//! The editor and the CLI format a project to the same text.

use specforge_test_macros::test as specforge_test;

use crate::contracts::{apply_edits, format_project, open_in};

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
            caller: specforge_ops::Caller::new(
                &specforge_ops::Containment::unbounded(),
                specforge_ops::asking::Asking::Nobody,
            ),
            runtime: None,
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
            caller: specforge_ops::Caller::new(
                &specforge_ops::Containment::unbounded(),
                specforge_ops::asking::Asking::Nobody,
            ),
            runtime: None,
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
        caller: specforge_ops::Caller::new(
            &specforge_ops::Containment::unbounded(),
            specforge_ops::asking::Asking::Nobody,
        ),
        runtime: None,
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

/// The editor served at an outer project writes a nested project's values as `specforge format` does.
#[tokio::test]
#[ignore = "flipped by T3"]
async fn the_editor_writes_a_nested_projects_values_as_specforge_format_does() {
    use specforge_ops::authoring::format::{Mode, Request, run};
    let tagged = "behavior bb \"B\" {\n  contract \"The system MUST x\"\n  category \"auth\"\n}\n";
    let (_dir, root) = format_project(&[
        ("specforge.json", r#"{"extensions": []}"#),
        (
            "inner/specforge.json",
            r#"{"extensions": ["@specforge/software"]}"#,
        ),
        ("inner/spec/a.spec", tagged),
    ]);
    let (mut session, uri) = open_in(&root, "inner/spec/a.spec").await;

    let edits = session.format(&uri).await;
    let edited = apply_edits(tagged, edits.as_array().expect("an edit list"));

    let outcome = run(&Request {
        root: &specforge_common::ProjectRoot::of(&root).unwrap(),
        paths: &[root.join("inner")],
        mode: Mode::Check,
        caller: specforge_ops::Caller::new(
            &specforge_ops::Containment::unbounded(),
            specforge_ops::asking::Asking::Nobody,
        ),
        runtime: Some(std::sync::Arc::new(
            specforge_component::ComponentRuntime::new(),
        )),
    })
    .unwrap();
    assert_eq!(edited, outcome.changes[0].after);
    assert!(edited.contains("\n  category auth\n"), "{edited}");
}
