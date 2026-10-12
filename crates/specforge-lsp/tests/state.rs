use crate::session::{Session, codes};
use serde_json::json;
use specforge_test_macros::test as spec;

// -- document_open_close ------------------------------------------------------

#[spec(
    behavior = "document_open_close",
    verify = "didOpen registers document and triggers compilation"
)]
#[tokio::test]
async fn did_open_registers_document() {
    let (mut session, _) = Session::start(None).await;
    // Only in the editor buffer: nothing on disk.
    let uri = "file:///buffer/login.spec";
    session.open(uri, LOGIN).await;

    // Compiled: the diagnostics are the buffer's own, and its entity is
    // in the graph (the outline is read from the graph).
    let diagnostics = session.diagnostics(uri).await;
    assert_eq!(codes(&diagnostics), ["E003"], "{diagnostics:?}");
    let outline = session
        .request(
            "textDocument/documentSymbol",
            json!({"textDocument": {"uri": uri}}),
        )
        .await;
    let names: Vec<&str> = outline["result"]
        .as_array()
        .unwrap_or_else(|| panic!("no outline: {outline}"))
        .iter()
        .map(|s| s["name"].as_str().unwrap())
        .collect();
    assert_eq!(names.len(), 1, "{names:?}");
    assert!(names[0].contains("login"), "{names:?}");

    // Registered: buffer-only features answer for it.
    assert!(session.format(uri).await.is_array());
}

#[spec(
    behavior = "document_open_close",
    verify = "didClose removes document and clears diagnostics"
)]
#[tokio::test]
async fn did_close_removes_document() {
    let (mut session, _) = Session::start(None).await;
    let uri = "file:///buffer/login.spec";
    session.open(uri, LOGIN).await;
    assert!(!session.diagnostics(uri).await.is_empty());

    session.close(uri).await;

    // The editor is told to clear the document's squiggles...
    let cleared = session.diagnostics(uri).await;
    assert!(cleared.is_empty(), "{cleared:?}");
    // ...and the document is gone from the open set.
    assert!(session.format(uri).await.is_null());

    // The state forgets both.
    let mut state = specforge_lsp::LspState::new();
    state.open_document(uri, LOGIN);
    state.close_document(uri);
    assert!(!state.is_open(uri));
}

/// `login` references `session_limit`, which exists nowhere.
const LOGIN: &str = "behavior login \"Login\" {\n  invariants [session_limit]\n}\n";

#[test]
fn only_open_documents_tracked() {
    let mut state = specforge_lsp::LspState::new();
    state.open_document("file:///a.spec", "a");
    state.open_document("file:///b.spec", "b");

    assert_eq!(state.open_uris().len(), 2);

    state.close_document("file:///a.spec");
    assert_eq!(state.open_uris().len(), 1);
    assert_eq!(state.open_uris()[0], "file:///b.spec");
}

// -- handle_text_document_change ----------------------------------------------

#[spec(
    behavior = "handle_text_document_change",
    verify = "didChange applies incremental edits to buffer"
)]
fn did_change_applies_edits() {
    let mut state = specforge_lsp::LspState::new();
    state.open_document("file:///a.spec", "hello world\n");

    state.apply_change(
        "file:///a.spec",
        Some(crate::lsp_range(0, 6, 0, 11)),
        "rust",
    );
    assert_eq!(
        state.document("file:///a.spec").unwrap().text(),
        "hello rust\n"
    );
}

// -- the rule set -------------------------------------------------------

fn load_patterns_for(ext_names: &[&str]) -> specforge_registry::RegistryBuild {
    let names: Vec<String> = ext_names.iter().map(|s| s.to_string()).collect();
    let runtime = wasm_runtime_for(&names);
    let mut declarations = Vec::new();
    for name in &names {
        if let Ok(loaded) = specforge_wasm::protocol::load_declaration(&runtime, name) {
            declarations.push(loaded.declaration);
        }
    }
    specforge_registry::build_registries(declarations)
}

#[spec(
    invariant = "zero_domain_knowledge_core",
    verify = "core with zero extensions installed has zero entity kinds in KindRegistry"
)]
fn state_starts_with_empty_registries() {
    let state = specforge_lsp::LspState::new();
    assert!(state.kind_registry().is_empty());
    assert!(state.registries().rules().is_empty());
}

#[spec(
    behavior = "registry_build_rules",
    verify = "extensions produce E006 rules for required fields"
)]
fn extensions_produce_e006_rules() {
    let build = load_patterns_for(&[
        "@specforge/software",
        "@specforge/product",
        "@specforge/governance",
        "@specforge/formal",
    ]);
    let patterns = build.rules();
    assert!(
        !patterns.is_empty(),
        "extensions should produce validation patterns"
    );
    let e006_count = patterns.iter().filter(|p| p.code() == "E006").count();
    assert!(
        e006_count > 0,
        "E006 rules should be auto-generated from required fields"
    );
}

#[spec(
    behavior = "registry_build_rules",
    verify = "E006 covers all required fields from builtin extensions"
)]
fn e006_covers_all_required_fields() {
    let build = load_patterns_for(&[
        "@specforge/software",
        "@specforge/product",
        "@specforge/governance",
        "@specforge/formal",
    ]);
    let patterns = build.rules();

    let e006_targets: Vec<(String, String)> = patterns
        .iter()
        .filter(|p| p.code() == "E006")
        .filter_map(|p| {
            Some((
                p.target_kind()?.to_string(),
                p.describe()["field"].as_str()?.to_string(),
            ))
        })
        .collect();
    let has = |kind: &str, field: &str| {
        e006_targets
            .iter()
            .any(|(k, f)| k.as_str() == kind && f.as_str() == field)
    };

    // software
    assert!(has("behavior", "contract"));
    assert!(has("invariant", "guarantee"));
    assert!(has("port", "direction"));
    // product
    assert!(has("feature", "problem"));
    assert!(has("term", "definition"));
    assert!(has("release", "version"));
    assert!(has("journey", "flow"));
    assert!(has("deliverable", "artifact_type"));
    assert!(has("persona", "description"));
    assert!(has("channel", "description"));
    // governance
    assert!(has("decision", "status"));
    assert!(has("decision", "context"));
    assert!(has("decision", "decision"));
    assert!(has("constraint", "description"));
    assert!(has("failure_mode", "severity"));
    assert!(has("failure_mode", "cause"));
    assert!(has("failure_mode", "effect"));
    // formal
    assert!(has("property", "expression"));
    assert!(has("property", "property_type"));
    assert!(has("axiom", "expression"));
    assert!(has("refinement", "abstract_entity"));
    assert!(has("refinement", "concrete_entity"));
    assert!(has("protocol", "alphabet"));
    assert!(has("protocol", "initial_state"));
    assert!(has("process", "alphabet"));
    assert!(has("process", "initial_state"));
}
/// Build a Wasm runtime for a temp project listing `ext_names`, mirroring
/// how a real session loads extensions from specforge.json.
fn wasm_runtime_for(ext_names: &[String]) -> specforge_component::ComponentRuntime {
    let runtime = specforge_component::ComponentRuntime::new();
    let names: Vec<String> = ext_names.iter().map(|name| name.to_string()).collect();
    specforge_component::builtins::load_builtins_for(&runtime, &names).unwrap();
    runtime
}
