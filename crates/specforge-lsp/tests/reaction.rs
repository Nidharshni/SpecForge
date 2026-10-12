//! What the LSP's reaction tells the editor after a change (`specforge_lsp::reaction`, ADR
//! 0043): the workspace opening, what is published, followed, announced and refreshed. Each test
//! drives the real `Reaction` over a `Recorder` of what it sends: no client, no debounce, no
//! timeout.

use crate::contracts::{STANDARD_TOKEN_TYPES, project_with, registries_for};
use crate::recorder::{Sent, codes, last_codes, logs, publications, refreshed, watched};
use crate::served::Served;
use crate::session::{NAMES_GUIDE, docref_project};
use specforge_lsp::changes::Change;
use specforge_lsp::editor::WorkDone;
use specforge_lsp::watchers::default_watchers;
use specforge_lsp::{ClientSupport, answers, initialize_result};
use specforge_test_macros::test as spec;
use tower_lsp::lsp_types::{
    CompletionItemKind, CompletionResponse, Diagnostic, DiagnosticSeverity, FileChangeType,
    FileEvent, MessageType, NumberOrString, Position, SemanticTokensResult,
    SemanticTokensServerCapabilities, TextDocumentSyncCapability, TextDocumentSyncKind, Url,
};

fn names(served: &Served, query: &str) -> Vec<String> {
    answers::workspace_symbols(&served.state(), query)
        .unwrap_or_default()
        .into_iter()
        .map(|s| s.name)
        .collect()
}

fn event(served: &Served, file: &str, typ: FileChangeType) -> Change {
    Change::Watched(vec![FileEvent {
        uri: served.uri(file),
        typ,
    }])
}

/// The diagnostics of the last publication for `uri`.
fn last_diagnostics(sent: &[Sent], uri: &Url) -> Vec<Diagnostic> {
    publications(sent, uri)
        .pop()
        .unwrap_or_else(|| panic!("nothing was published for {uri}: {sent:?}"))
}

/// The globs of the last watch request.
fn last_watched(sent: &[Sent]) -> Vec<String> {
    watched(sent).pop().expect("the editor was asked to watch")
}

/// Top-level keyword completions: the kinds the loaded extensions declare.
fn top_level_keywords(served: &Served, file: &str) -> Vec<String> {
    match answers::completion(&served.state(), &served.uri(file), Position::new(0, 0)) {
        Some(CompletionResponse::Array(items)) => items
            .into_iter()
            .filter(|item| item.kind == Some(CompletionItemKind::KEYWORD))
            .map(|item| item.label)
            .collect(),
        _ => Vec::new(),
    }
}

// -- opening the workspace ----------------------------------------------------

#[test]
fn opening_a_workspace_watches_indexes_and_announces() {
    let served = Served::new(&[
        ("a.spec", "behavior alpha \"Alpha\" {}\n"),
        ("b.spec", "behavior beta \"Beta\" {}\n"),
    ])
    .open(&[]);
    let root = served.root().to_str().unwrap().to_string();

    let opening = served.opening();
    assert_eq!(opening.len(), 5, "{opening:?}");
    // The static watchers first, then the progress the open is shown in...
    assert_eq!(
        opening[0],
        Sent::Watched {
            watchers: default_watchers(),
            accepted: true
        }
    );
    assert_eq!(
        opening[1],
        Sent::Progress(WorkDone::Begin {
            title: "specforge: indexing workspace".into()
        })
    );
    // ...the watchers the project is built from, once it is open...
    let Sent::Watched { accepted: true, .. } = &opening[2] else {
        panic!("the project's watchers: {opening:?}");
    };
    let globs = last_watched(&opening[..3]);
    assert!(globs.contains(&format!("{root}/**/*.spec")), "{globs:?}");
    assert!(
        globs.contains(&format!("{root}/specforge.json")),
        "{globs:?}"
    );
    // ...and what was indexed.
    assert_eq!(
        opening[3],
        Sent::Logged(
            MessageType::INFO,
            format!("specforge-lsp: indexed 2 .spec files from {root}")
        )
    );
    assert_eq!(
        opening[4],
        Sent::Progress(WorkDone::End {
            message: Some("2 files".into())
        })
    );
}

#[spec(
    behavior = "lsp_initialize",
    verify = "a workspace root inside a project opens the enclosing project"
)]
fn a_workspace_root_inside_a_project_opens_the_enclosing_project() {
    let served = Served::new(&[
        ("spec/a.spec", "behavior alpha \"Alpha\" {}\n"),
        ("other/b.spec", "behavior beta \"Beta\" {}\n"),
    ])
    .open_in("spec", &[]);
    let root = served.root().to_str().unwrap().to_string();
    // The project's root, as the editor spells it, not the folder opened.
    assert!(
        logs(served.opening())
            .contains(&format!("specforge-lsp: indexed 2 .spec files from {root}")),
        "{:?}",
        served.opening()
    );
    let mut found = names(&served, "");
    found.sort();
    assert_eq!(found, ["alpha", "beta"]);
}

#[spec(
    behavior = "locate_project_root",
    verify = "a workspace root in a spec root holding specforge.spec opens the project around it"
)]
fn a_workspace_root_in_a_spec_root_holding_specforge_spec_opens_the_project_around_it() {
    let served = Served::new(&[
        ("spec/specforge.spec", "spec \"p\" {}\n"),
        ("spec/a.spec", "behavior alpha \"Alpha\" {}\n"),
    ])
    .extension("@specforge/software", |c| {
        c.kind("behavior", |_| {});
    })
    .open_in("spec", &[]);
    let root = served.root().to_str().unwrap().to_string();
    let logs = logs(served.opening());
    assert!(
        logs.contains(&format!("specforge-lsp: indexed 2 .spec files from {root}")),
        "{logs:?}"
    );
    assert!(
        logs.iter().any(|m| m.contains("loaded 1 extension(s)")),
        "{logs:?}"
    );
}

#[test]
fn a_workspace_root_in_no_project_is_opened_as_given() {
    let dir = tempfile::TempDir::new().unwrap();
    std::fs::create_dir_all(dir.path().join("loose")).unwrap();
    std::fs::write(
        dir.path().join("loose/a.spec"),
        "behavior alpha \"Alpha\" {}\n",
    )
    .unwrap();
    let served = Served::at(dir).open_in("loose", &[]);
    let loose = served.root().join("loose").to_str().unwrap().to_string();
    assert!(
        logs(served.opening()).contains(&format!(
            "specforge-lsp: indexed 1 .spec files from {loose}"
        )),
        "{:?}",
        served.opening()
    );
}

#[test]
fn a_workspace_with_no_root_is_done_at_once() {
    let served = Served::detached();
    assert_eq!(
        served.opening(),
        [
            Sent::Watched {
                watchers: default_watchers(),
                accepted: true
            },
            Sent::Progress(WorkDone::Begin {
                title: "specforge: indexing workspace".into()
            }),
            Sent::Logged(
                MessageType::INFO,
                "specforge-lsp initialized (no root_uri)".into()
            ),
            Sent::Progress(WorkDone::End { message: None }),
        ]
    );
}

// B:lsp_initialize — verify contract "requires/ensures consistency for LSP initialization"
#[spec(
    behavior = "lsp_initialize",
    verify = "LSP Initialize: LSP initialization holds — extensions_loaded, capabilities_reflect_extensions, semantic_legend_populated, incremental_sync_advertised, lsp_initialized_emitted"
)]
fn lsp_initialize_contract() {
    let extensions = ["@specforge/software", "@specforge/testing"];
    let capabilities = initialize_result().capabilities;

    // incremental_sync_advertised: TextDocumentSyncKind::INCREMENTAL.
    assert_eq!(
        capabilities.text_document_sync,
        Some(TextDocumentSyncCapability::Kind(
            TextDocumentSyncKind::INCREMENTAL
        ))
    );

    // semantic_legend_populated: every standard LSP token type, in order.
    let Some(SemanticTokensServerCapabilities::SemanticTokensOptions(options)) =
        capabilities.semantic_tokens_provider
    else {
        panic!("semantic tokens are served");
    };
    let legend: Vec<&str> = options
        .legend
        .token_types
        .iter()
        .map(|t| t.as_str())
        .collect();
    assert_eq!(legend, STANDARD_TOKEN_TYPES);

    // extensions_loaded, lsp_initialized_emitted: once the registries are populated the
    // server announces how many extensions and entity kinds it loaded.
    let build = registries_for(&extensions);
    let kinds = build.kinds();
    assert!(kinds.len() >= 5, "software alone declares five kinds");
    let mut served = Served::at(project_with(&extensions)).open(&[]);
    assert!(
        logs(served.opening()).contains(&format!(
            "specforge-lsp: loaded 2 extension(s), {} entity kind(s)",
            kinds.len()
        )),
        "no initialization announcement: {:?}",
        served.opening()
    );

    // capabilities_reflect_extensions: nothing domain-specific is hardcoded (the legend is
    // exactly the standard list), and the advertised legend carries what the loaded extension
    // declares: @specforge/software gives `port` IDs the `interface` token.
    let uri = Url::parse("file:///buffer/repo.spec").unwrap();
    served.open_document(&uri, "port repo \"Repo\" {\n}\n");
    let Some(SemanticTokensResult::Tokens(tokens)) =
        answers::semantic_tokens(&served.state(), &uri)
    else {
        panic!("tokens are served for an open document");
    };
    // The second token is `repo` at line 0, column 5.
    let repo = &tokens.data[1];
    assert_eq!(
        (repo.delta_line, repo.delta_start, repo.length),
        (0, 5, 4),
        "{:?}",
        tokens.data
    );
    assert_eq!(legend[repo.token_type as usize], "interface");
}

// -- publishing ---------------------------------------------------------------

/// `text` opened as `file`, in a server with no project; what was published for it.
fn opened_alone(file: &str, text: &str) -> Vec<Diagnostic> {
    let mut served = Served::detached();
    let uri = served.uri(file);
    served.open_document(&uri, text);
    let sent = served.sent();
    assert_eq!(publications(&sent, &uri).len(), 1, "{sent:?}");
    last_diagnostics(&sent, &uri)
}

#[test]
fn a_clean_document_publishes_no_diagnostics() {
    let diagnostics = opened_alone(
        "clean.spec",
        "behavior foo \"Foo\" {\n  contract \"Does something\"\n  category \"core\"\n  features [some_feature]\n}\nfeature some_feature \"SF\" {\n  problem \"Needs solving\"\n}\n",
    );
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
}

#[test]
fn a_parse_error_is_published_as_e001() {
    let diagnostics = opened_alone("broken.spec", "behavior {");
    let first = diagnostics.first().expect("at least one diagnostic");
    assert_eq!(first.severity, Some(DiagnosticSeverity::ERROR));
    assert_eq!(first.code, Some(NumberOrString::String("E001".into())));
}

#[test]
fn an_unresolved_reference_is_published_with_its_data() {
    let diagnostics = opened_alone(
        "resolve.spec",
        "behavior foo \"Foo\" {\n  types [nonexistent]\n}\n",
    );
    let e003 = diagnostics
        .iter()
        .find(|d| {
            d.code == Some(NumberOrString::String("E003".into()))
                && d.message.contains("unresolved")
        })
        .unwrap_or_else(|| panic!("an unresolved reference: {diagnostics:?}"));
    // Its typed payload rides in the LSP diagnostic's `data`, which a client echoes back with a
    // code-action request.
    assert_eq!(
        e003.data,
        Some(serde_json::json!({
            "kind": "unresolved_reference",
            "target": "nonexistent",
            "entity": "foo",
            "field": "types",
        })),
        "{e003:?}"
    );
}

#[test]
fn validator_warnings_are_published() {
    // A `ref` with no incoming refs draws a validator warning.
    let diagnostics = opened_alone("validate.spec", "ref gh.issue:42 \"Fix bug\"\n");
    assert!(
        diagnostics
            .iter()
            .any(|d| d.severity == Some(DiagnosticSeverity::WARNING)),
        "a validator warning: {diagnostics:?}"
    );
}

/// A reference cycle has no one place: the LSP publishes it at the first entity its data names,
/// on that entity's name, pointing at the others as related information (ADR 0016, D8), not on
/// line 1 of the document last edited.
#[spec(
    behavior = "emit_live_diagnostics",
    verify = "a spanless diagnostic about entities is published at the first one's name"
)]
fn a_spanless_diagnostic_about_entities_is_published_at_its_name() {
    let dir = tempfile::TempDir::new().unwrap();
    std::fs::write(
        dir.path().join("specforge.json"),
        r#"{"name":"c","extensions":["@specforge/software"]}"#,
    )
    .unwrap();
    let cycle_text = "behavior alpha \"A\" {\n  depends_on [beta]\n}\nbehavior beta \"B\" {\n  depends_on [alpha]\n}\n";
    std::fs::write(dir.path().join("cycle.spec"), cycle_text).unwrap();
    std::fs::write(dir.path().join("other.spec"), "behavior gamma \"G\" {\n}\n").unwrap();
    let mut served = Served::at(dir).open(&[]);
    // Edit the other document: the cycle is still published on its own.
    let other = served.uri("other.spec");
    served.open_document(&other, "behavior gamma \"G\" {\n}\n");

    let cycle = served.uri("cycle.spec");
    let sent = served.sent();
    let diagnostics = last_diagnostics(&sent, &cycle);
    let w061 = diagnostics
        .iter()
        .find(|d| d.code == Some(NumberOrString::String("W061".into())))
        .unwrap_or_else(|| panic!("W061 is published on the cycle's file: {diagnostics:?}"));
    let range = |r: &tower_lsp::lsp_types::Range| {
        (r.start.line, r.start.character, r.end.line, r.end.character)
    };
    assert_eq!(range(&w061.range), (0, 9, 0, 14), "alpha's name: {w061:?}");
    let related = w061.related_information.as_ref().expect("the others");
    assert_eq!(related.len(), 1, "{w061:?}");
    assert_eq!(related[0].location.uri, cycle);
    assert_eq!(
        range(&related[0].location.range),
        (3, 9, 3, 13),
        "beta's name"
    );
}

/// `types/`, `invariants/`, `features/` and the behavior `behaviors/error-reporting.spec`
/// referencing entities of the others, as the repository's own specs do.
fn cross_file_tree(uses: &[&str]) -> Vec<(&'static str, String)> {
    let mut behavior: String = uses.iter().map(|u| format!("use \"{u}\"\n")).collect();
    if !uses.is_empty() {
        behavior.push('\n');
    }
    behavior.push_str(concat!(
        "behavior format_diagnostics \"Format Diagnostics\" {\n",
        "  invariants [multi_error_collection, diagnostic_determinism, zero_domain_knowledge_core]\n",
        "  types      [Diagnostic, SourceSpan, CodePrefix]\n",
        "  features   [diagnostic_reporting]\n",
        "}\n",
    ));
    vec![
        (
            "types/diagnostics.spec",
            "type Diagnostic \"Diagnostic\" {}\ntype CodePrefix = E | W | I\n".to_string(),
        ),
        (
            "types/core.spec",
            "type SourceSpan \"Source Span\" {}\n".to_string(),
        ),
        (
            "invariants/core.spec",
            "invariant multi_error_collection \"Multi-Error Collection\" {}\n".to_string(),
        ),
        (
            "invariants/validation.spec",
            "invariant diagnostic_determinism \"Diagnostic Determinism\" {}\n".to_string(),
        ),
        (
            "invariants/zero-entity-core.spec",
            "invariant zero_domain_knowledge_core \"Zero Domain Knowledge Core\" {}\n".to_string(),
        ),
        (
            "features/validation.spec",
            "feature diagnostic_reporting \"Diagnostic Reporting\" {}\n".to_string(),
        ),
        ("behaviors/error-reporting.spec", behavior),
    ]
}

const BEHAVIOR: &str = "behaviors/error-reporting.spec";

fn served_tree(uses: &[&str]) -> Served {
    let files = cross_file_tree(uses);
    let files: Vec<(&str, &str)> = files.iter().map(|(n, t)| (*n, t.as_str())).collect();
    Served::new(&files)
}

#[test]
fn a_cross_file_diagnostic_is_published_on_its_own_file() {
    let mut served = Served::new(&[
        ("b.spec", "type shared_token \"Token\" {}\n"),
        (
            "a.spec",
            "behavior consumer \"Consumer\" {\n  types [shared_token]\n}\n",
        ),
    ])
    .open(&["b.spec", "a.spec"]);
    let (a, b) = (served.uri("a.spec"), served.uri("b.spec"));

    // Delete the type from b.spec: the consumer's reference is broken.
    served.edit("b.spec", "// empty\n");
    let sent = served.sent();

    // The E003 for the unresolved 'shared_token' is published under a.spec, where the broken
    // reference lives, not under b.spec, which was edited.
    assert!(
        last_codes(&sent, &a).is_some_and(|codes| codes.contains(&"E003".to_string())),
        "{sent:?}"
    );
    let b_unresolved = publications(&sent, &b).iter().any(|diagnostics| {
        diagnostics.iter().any(|d| {
            d.code == Some(NumberOrString::String("E003".into()))
                && d.message.contains("unresolved")
        })
    });
    assert!(!b_unresolved, "the broken reference is not in b.spec");
}

#[test]
fn cross_file_references_resolve_once_indexed() {
    let served = served_tree(&[
        "types/diagnostics",
        "types/core",
        "invariants/core",
        "invariants/validation",
        "invariants/zero-entity-core",
        "features/validation",
    ])
    .open(&[BEHAVIOR]);
    let diagnostics = last_diagnostics(served.opening(), &served.uri(BEHAVIOR));
    assert!(
        !codes(&diagnostics).contains(&"E003".to_string()),
        "every reference resolves: {diagnostics:?}"
    );
}

/// Opening a file BEFORE the workspace is indexed must not leave stale E003 diagnostics: once
/// indexed, the file is published again with every cross-file reference resolved (the race of
/// the Neovim bug).
#[test]
fn a_document_opened_before_indexing_is_published_again_after_it() {
    let served = served_tree(&[]);
    // The handler recorded the buffer at once; its reaction queues behind the open.
    served.hold(BEHAVIOR);
    let served = served.open(&[]);
    let diagnostics = last_diagnostics(served.opening(), &served.uri(BEHAVIOR));
    assert!(
        !codes(&diagnostics).contains(&"E003".to_string()),
        "the open applied the buffer against the indexed workspace: {diagnostics:?}"
    );
}

// C4-07 acceptance: a syntax-broken edit publishes only the E001 layer, and the validator,
// registry and Wasm-rule passes stay off until the file parses again.
#[test]
fn a_broken_edit_publishes_only_syntax() {
    // Parses cleanly but references an undeclared entity: full passes would add W022
    // (mistyped reference) on top of the pipeline's diagnostics.
    let mut served = Served::detached();
    let uri = served.uri("test.spec");
    served.open_document(&uri, "behavior login \"Login\" {\n  types [widget]\n}\n");
    served.sent();

    served.type_text(
        "test.spec",
        "behavior login \"Login\" {\n  types [widget!! ]\n",
    );
    served.apply(Change::Edited(vec![uri.clone()]));
    let broken = last_codes(&served.sent(), &uri).expect("broken-state diagnostics published");
    assert!(
        broken.contains(&"E001".to_string()),
        "a broken file reports E001: {broken:?}"
    );

    // Fix the syntax: full passes resume (the reference is still unresolved, so E003 comes
    // back, which proves the graph-level pipeline resumed after the fast path).
    served.edit(
        "test.spec",
        "behavior login \"Login\" {\n  types [widget]\n}\n",
    );
    let fixed = last_codes(&served.sent(), &uri).expect("fixed-state diagnostics published");
    assert!(
        !fixed.contains(&"E001".to_string()),
        "a fixed file reports no parse error: {fixed:?}"
    );
    assert!(fixed.contains(&"E003".to_string()), "{fixed:?}");
}

// -- changes on disk ----------------------------------------------------------

#[test]
fn a_changed_file_is_compiled_again() {
    let mut served = Served::new(&[("a.spec", "behavior alpha \"Alpha\" {}\n")]).open(&[]);
    assert_eq!(names(&served, "alpha"), ["alpha"]);

    served.write(
        "a.spec",
        "behavior alpha \"Alpha\" {}\nbehavior beta \"Beta\" {}\n",
    );
    served.apply(event(&served, "a.spec", FileChangeType::CHANGED));
    assert_eq!(names(&served, "beta"), ["beta"]);
}

#[test]
fn a_created_file_joins_the_project() {
    let mut served = Served::new(&[("a.spec", "behavior alpha \"Alpha\" {}\n")]).open(&[]);
    served.write("b.spec", "behavior gamma \"Gamma\" {}\n");
    served.apply(event(&served, "b.spec", FileChangeType::CREATED));
    assert_eq!(names(&served, "gamma"), ["gamma"]);
}

#[test]
fn a_deleted_file_leaves_the_project() {
    let mut served = Served::new(&[
        ("a.spec", "behavior alpha \"Alpha\" {}\n"),
        ("b.spec", "behavior beta \"Beta\" {}\n"),
    ])
    .open(&[]);
    assert_eq!(names(&served, "beta"), ["beta"]);

    std::fs::remove_file(served.root().join("b.spec")).unwrap();
    served.apply(event(&served, "b.spec", FileChangeType::DELETED));
    assert!(names(&served, "beta").is_empty());
}

#[test]
fn deleting_a_file_publishes_the_references_it_broke() {
    let mut served = Served::new(&[
        ("b.spec", "type shared_token \"Token\" {}\n"),
        (
            "a.spec",
            "behavior consumer \"Consumer\" {\n  types [shared_token]\n}\n",
        ),
    ])
    .open(&["a.spec"]);
    let a = served.uri("a.spec");

    std::fs::remove_file(served.root().join("b.spec")).unwrap();
    served.apply(event(&served, "b.spec", FileChangeType::DELETED));
    let codes = last_codes(&served.sent(), &a).expect("a.spec is published again");
    assert!(codes.contains(&"E003".to_string()), "{codes:?}");
}

// -- closing ------------------------------------------------------------------

#[test]
fn a_closed_document_is_not_answered_for() {
    let mut served = Served::detached();
    let uri = served.uri("test.spec");
    served.open_document(&uri, "behavior foo \"Foo\" {}\n");
    assert!(answers::hover(&served.state(), &uri, Position::new(0, 10)).is_some());

    served.close_uri(&uri);
    assert!(answers::hover(&served.state(), &uri, Position::new(0, 10)).is_none());
}

// -- reloads ------------------------------------------------------------------

#[spec(
    invariant = "lsp_extension_reload_consistency",
    verify = "removing an extension while LSP is running removes kinds from KindRegistry atomically"
)]
fn removing_every_extension_clears_the_kinds() {
    let dir = project_with(&["@specforge/software"]);
    std::fs::write(
        dir.path().join("main.spec"),
        "behavior login \"Login\" {\n  contract \"logs in\"\n}\n",
    )
    .unwrap();
    let mut served = Served::at(dir).open(&["main.spec"]);
    let before = top_level_keywords(&served, "main.spec");
    assert!(before.iter().any(|k| k == "behavior"), "{before:?}");

    // Every extension is removed from specforge.json.
    served.write(
        "specforge.json",
        r#"{"name": "test", "version": "0.1.0", "extensions": []}"#,
    );
    served.apply(event(&served, "specforge.json", FileChangeType::CHANGED));
    assert!(
        logs(&served.sent())
            .iter()
            .any(|m| m.contains("reloaded 0 extension(s)")),
        "the reload is announced"
    );

    let after = top_level_keywords(&served, "main.spec");
    assert!(
        !after.iter().any(|k| k == "behavior"),
        "software's kinds outlive its removal: {after:?}"
    );
}

#[spec(
    invariant = "lsp_extension_reload_consistency",
    verify = "a specforge.lock change while the LSP is running reloads the environment"
)]
fn a_lock_change_reloads_the_environment() {
    let dir = project_with(&["@specforge/software", "@acme/missing"]);
    std::fs::write(
        dir.path().join("main.spec"),
        "behavior alpha \"Alpha\" {}\n",
    )
    .unwrap();
    let mut served = Served::at(dir).open(&[]);

    // The lock now names the extension: the environment reads the lock, so it must load again.
    served.write(
        "specforge.lock",
        &serde_json::json!({
            "lockfile_version": 1,
            "entries": [{"name": "@acme/missing", "version": "1.0.0", "source": "local:missing.wasm", "wasm_hash": "sha256:00"}]
        })
        .to_string(),
    );
    served.apply(event(&served, "specforge.lock", FileChangeType::CREATED));
    let logs = logs(&served.sent());
    assert!(
        logs.iter()
            .any(|m| m.contains("extension environment changed")
                && m.contains("reloaded 1 extension(s)")),
        "a specforge.lock change must reload the environment: {logs:?}"
    );
}

// -- watchers -----------------------------------------------------------------

#[spec(
    invariant = "lsp_extension_reload_consistency",
    verify = "the LSP watches every file its environment is loaded from"
)]
fn the_watchers_cover_the_environment_inputs() {
    let dir = tempfile::TempDir::new().unwrap();
    let config = serde_json::json!({
        "name": "test",
        "version": "0.1.0",
        "spec_root": "spec",
        "extensions": ["@specforge/software", "@acme/local=ext/local.wasm", "@acme/installed"],
    });
    std::fs::write(dir.path().join("specforge.json"), config.to_string()).unwrap();
    std::fs::create_dir_all(dir.path().join("spec")).unwrap();
    std::fs::write(dir.path().join("spec/main.spec"), "").unwrap();
    let root = dir.path().to_str().unwrap().to_string();
    let served = Served::at(dir).open(&[]);

    // The static watchers first, then, once the project is open, the ones it is built from.
    let all = watched(served.opening());
    assert_eq!(all.len(), 2, "{all:?}");
    let globs = last_watched(served.opening());
    for expected in [
        format!("{root}/spec/**/*.spec"),
        format!("{root}/specforge.json"),
        format!("{root}/specforge.lock"),
        format!("{root}/ext/local.wasm"),
        format!("{root}/.specforge/extensions/@acme/installed/extension.wasm"),
    ] {
        assert!(globs.contains(&expected), "{expected} not in {globs:?}");
    }
    // A .wasm no extension loads is not watched.
    assert!(!globs.iter().any(|g| g == "**/*.wasm"), "{globs:?}");
}

/// A docref project (an extension whose `docs` field names files) whose `spec/a.spec` is
/// `spec`, opened with `files` as documents.
fn docref(spec: &str, files: &[&str]) -> Served {
    Served::at(docref_project(spec)).open(files)
}

const PLAIN: &str = "gadget gadget_one \"G\" {\n}\n";

#[spec(
    behavior = "classify_project_changes",
    verify = "the LSP's watchers follow an edit that names a new file the checks read"
)]
fn the_watchers_follow_an_edit_that_names_a_file() {
    let mut served = docref(PLAIN, &["spec/a.spec"]);
    let root = served.root().to_str().unwrap().to_string();
    let guide = format!("{root}/docs/guide.md");
    let a = served.uri("spec/a.spec");
    assert!(
        !last_watched(served.opening()).contains(&guide),
        "{:?}",
        served.opening()
    );

    // The edit names a file the checks read: the watchers are asked for again, and now cover it.
    served.edit("spec/a.spec", NAMES_GUIDE);
    let sent = served.sent();
    assert!(last_watched(&sent).contains(&guide), "{sent:?}");

    // The file appears; the editor reports it, and E016 goes.
    served.write("docs/guide.md", "# guide\n");
    served.apply(event(&served, "docs/guide.md", FileChangeType::CREATED));
    let codes = last_codes(&served.sent(), &a).expect("a.spec is published again");
    assert!(!codes.contains(&"E016".to_string()), "{codes:?}");
}

#[spec(
    behavior = "classify_project_changes",
    verify = "the LSP watches a missing referenced file and its directory, spelled under the project root"
)]
fn a_referenced_file_is_watched_under_its_root() {
    let served = docref(NAMES_GUIDE, &[]);
    let root = served.root().to_str().unwrap();
    let globs = last_watched(served.opening());
    assert!(
        globs.contains(&format!("{root}/docs/guide.md")),
        "{globs:?}"
    );
    assert!(!globs.iter().any(|g| g.contains("/../")), "{globs:?}");
}

#[spec(
    behavior = "classify_project_changes",
    verify = "the LSP watches a missing referenced file and its directory, spelled under the project root"
)]
fn a_missing_files_directory_is_watched() {
    let served = docref(NAMES_GUIDE, &[]);
    let root = served.root().to_str().unwrap();
    let globs = last_watched(served.opening());
    assert!(globs.contains(&format!("{root}/docs/*")), "{globs:?}");
}

/// An edit that names a file moves the editor's watchers, and the session catches up on what
/// changed on disk while they moved (ADR 0035). The file is written while the watchers move,
/// before the editor answers, so no event can report it: only the catch-up can.
#[spec(
    behavior = "bring_session_up_to_date",
    verify = "after the LSP's watchers move, the session catches up on what changed while they did"
)]
fn an_edit_naming_a_file_registers_it_and_catches_up() {
    let mut served = docref(PLAIN, &["spec/a.spec"]);
    let guide = served.root().join("docs/guide.md");
    let glob = guide.to_str().unwrap().to_string();
    let a = served.uri("spec/a.spec");
    served
        .editor()
        .on_watch(move || std::fs::write(&guide, "# guide\n").unwrap());

    served.edit("spec/a.spec", NAMES_GUIDE);
    let sent = served.sent();

    // The watchers moved to cover the guide before the last publication of a.spec, and that
    // publication lacks E016. No `Watched` change was applied: only the catch-up can clear it.
    let moved = sent
        .iter()
        .position(|s| {
            matches!(s, Sent::Watched { .. }) && watched(std::slice::from_ref(s))[0].contains(&glob)
        })
        .unwrap_or_else(|| panic!("the watchers did not follow the edit: {sent:?}"));
    let last_published = sent
        .iter()
        .rposition(|s| matches!(s, Sent::Published { uri, .. } if *uri == a))
        .unwrap();
    assert!(moved < last_published, "{sent:?}");
    let codes = last_codes(&sent, &a).unwrap();
    assert!(!codes.contains(&"E016".to_string()), "{codes:?}");
}

#[spec(
    behavior = "classify_project_changes",
    verify = "the LSP's watchers follow an edit that names a new file the checks read"
)]
fn a_disk_change_naming_a_file_registers_it() {
    let mut served = docref(PLAIN, &[]);
    let root = served.root().to_str().unwrap().to_string();

    served.write("spec/a.spec", NAMES_GUIDE);
    served.apply(event(&served, "spec/a.spec", FileChangeType::CHANGED));
    let globs = last_watched(&served.sent());
    assert!(
        globs.contains(&format!("{root}/docs/guide.md")),
        "{globs:?}"
    );
}

/// The catch-up after the watchers move reads what changed on disk, but an open document's
/// buffer is the truth for its file: its file, rewritten meanwhile, does not replace it.
#[spec(
    behavior = "bring_session_up_to_date",
    verify = "the LSP's catch-up keeps an open buffer"
)]
fn a_catch_up_keeps_an_open_buffer() {
    let mut served = docref(PLAIN, &["spec/a.spec"]);
    let root = served.root().to_path_buf();
    let a = served.uri("spec/a.spec");
    // While the watchers move: the guide appears and the file of the open document is rewritten
    // with something else.
    served.editor().on_watch(move || {
        std::fs::write(root.join("docs/guide.md"), "# guide\n").unwrap();
        std::fs::write(root.join("spec/a.spec"), "gadget gadget_zero \"Z\" {\n}\n").unwrap();
    });

    // The buffer names the guide and declares an entity only it holds.
    let buffer = format!("{NAMES_GUIDE}gadget gadget_two \"T\" {{\n}}\n");
    served.edit("spec/a.spec", &buffer);
    let codes = last_codes(&served.sent(), &a).unwrap();
    assert!(!codes.contains(&"E016".to_string()), "{codes:?}");

    // The buffer is still what is compiled: its second entity answers.
    let hover = served.hover("spec/a.spec", 3, 10).unwrap_or_default();
    assert!(hover.contains("gadget_two"), "{hover}");
}

#[test]
fn an_editor_that_refuses_the_projects_watchers_keeps_the_static_ones() {
    let served = Served::new(&[("a.spec", "behavior alpha \"Alpha\" {}\n")]);
    served
        .editor()
        .refuse_when(|watchers| watchers != default_watchers().as_slice());
    let served = served.open(&[]);

    let opening = served.opening();
    let refused = opening
        .iter()
        .position(|s| {
            matches!(
                s,
                Sent::Watched {
                    accepted: false,
                    ..
                }
            )
        })
        .unwrap_or_else(|| panic!("the project's watchers are refused: {opening:?}"));
    assert_eq!(
        opening[refused + 1],
        Sent::Watched {
            watchers: default_watchers(),
            accepted: true
        },
        "the static watchers follow at once"
    );
    assert!(
        matches!(opening.last(), Some(Sent::Progress(WorkDone::End { .. }))),
        "the open still ends: {opening:?}"
    );
}

// -- semantic tokens ----------------------------------------------------------

const LOGIN: &str = "behavior login \"Login\" {\n  contract \"x\"\n}\n";

/// A server with no project whose client declared `refresh`, `refresh.spec` open and compiled.
fn refreshing(refresh: bool) -> Served {
    let mut served = Served::detached().client(ClientSupport {
        tokens_refresh: refresh,
        ..Default::default()
    });
    let uri = served.uri("refresh.spec");
    served.open_document(&uri, LOGIN);
    served
}

#[spec(
    behavior = "provide_semantic_tokens",
    verify = "a recompile that changes the graph asks the client to refresh semantic tokens"
)]
fn graph_changing_recompile_requests_token_refresh() {
    let mut served = refreshing(true);
    // Opening compiled `login` into an empty graph: that is a change too.
    assert!(
        refreshed(&served.sent()),
        "the first compile of an entity must ask for a refresh"
    );

    // An edit adds an entity: the recompiled graph differs.
    let grown = format!("{LOGIN}\ninvariant quota \"Quota\" {{\n}}\n");
    served.edit("refresh.spec", &grown);
    assert!(
        refreshed(&served.sent()),
        "adding an entity must ask the client to refresh semantic tokens"
    );
}

#[spec(
    behavior = "provide_semantic_tokens",
    verify = "a recompile that changes nothing token-relevant sends no semantic token refresh"
)]
fn whitespace_only_recompile_sends_no_token_refresh() {
    let mut served = refreshing(true);
    served.sent();

    // Same entities, kinds and titles; only the layout moves.
    let spaced = format!("\n\n{}", LOGIN.replace("  contract", "      contract"));
    served.edit("refresh.spec", &spaced);
    assert!(
        !refreshed(&served.sent()),
        "a whitespace-only edit must not refresh"
    );

    // A retitle does change what is highlighted: it refreshes.
    served.edit("refresh.spec", &spaced.replace("\"Login\"", "\"Sign in\""));
    assert!(
        refreshed(&served.sent()),
        "a changed title must ask for a refresh"
    );
}

#[spec(
    behavior = "provide_semantic_tokens",
    verify = "no semantic token refresh is sent to a client without refreshSupport"
)]
fn client_without_refresh_support_never_gets_token_refresh() {
    let mut served = refreshing(false);
    let grown = format!("{LOGIN}\ninvariant quota \"Quota\" {{\n}}\n");
    served.edit("refresh.spec", &grown);
    assert!(
        !refreshed(&served.sent()),
        "a client that did not declare refreshSupport must never be asked"
    );
}
