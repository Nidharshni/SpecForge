use crate::served::{edit_buffer, hover_text, uri_of_path};
use specforge_lsp::{ClientSupport, answers, initialize_result};
use specforge_test_macros::test as spec;
use tower_lsp::lsp_types::{
    CompletionOptions, GotoDefinitionResponse, Position, SemanticTokensServerCapabilities,
    ServerCapabilities, TextDocumentSyncCapability, TextDocumentSyncKind,
};

// -- lsp_initialize -----------------------------------------------------------

fn capabilities() -> ServerCapabilities {
    initialize_result().capabilities
}

/// The semantic token types `initialize` lists.
fn legend() -> Vec<String> {
    match capabilities().semantic_tokens_provider {
        Some(SemanticTokensServerCapabilities::SemanticTokensOptions(options)) => options
            .legend
            .token_types
            .iter()
            .map(|t| t.as_str().to_string())
            .collect(),
        other => panic!("semantic tokens are served with a legend, not {other:?}"),
    }
}

#[spec(
    behavior = "lsp_initialize",
    verify = "initialize response includes semantic token legend"
)]
fn init_includes_semantic_legend() {
    let legend = legend();
    assert_eq!(legend, specforge_lsp::TOKEN_TYPES);
    assert!(legend.contains(&"keyword".to_string()));
}

#[spec(
    behavior = "lsp_initialize",
    verify = "initialize response advertises incremental sync"
)]
fn init_advertises_incremental_sync() {
    assert_eq!(
        capabilities().text_document_sync,
        Some(TextDocumentSyncCapability::Kind(
            TextDocumentSyncKind::INCREMENTAL
        ))
    );
}

#[spec(
    behavior = "lsp_initialize",
    verify = "initialize response includes completion trigger characters"
)]
fn init_includes_completion_triggers() {
    let Some(CompletionOptions {
        trigger_characters, ..
    }) = capabilities().completion_provider
    else {
        panic!("completion is served");
    };
    assert_eq!(trigger_characters, Some(vec![" ".into(), "[".into()]));
}

#[spec(
    behavior = "lsp_initialize",
    verify = "initialize response includes server_info with name and version"
)]
fn init_includes_server_info() {
    let info = initialize_result().server_info.expect("server_info");
    assert_eq!(info.name, "specforge-lsp");
    assert_eq!(info.version.as_deref(), Some(env!("CARGO_PKG_VERSION")));
}

#[spec(
    behavior = "lsp_initialize",
    verify = "zero extensions produces structural-only capabilities"
)]
fn initialize_answers_a_static_result() {
    // The answer is one static value: nothing in it depends on a project, so a project with
    // no extension and one with extensions are offered the same (checked over the protocol
    // by `e2e_initialize_returns_all_capabilities`).
    assert_eq!(
        serde_json::to_value(initialize_result()).unwrap(),
        serde_json::json!({
            "capabilities": {
                "codeActionProvider": true,
                "completionProvider": {"triggerCharacters": [" ", "["]},
                "definitionProvider": true,
                "documentFormattingProvider": true,
                "documentRangeFormattingProvider": true,
                "documentSymbolProvider": true,
                "hoverProvider": true,
                "referencesProvider": true,
                "renameProvider": {"prepareProvider": true},
                "semanticTokensProvider": {
                    "full": true,
                    "legend": {
                        "tokenModifiers": ["declaration", "reference"],
                        "tokenTypes": specforge_lsp::TOKEN_TYPES,
                    },
                },
                "textDocumentSync": 2,
                "workspaceSymbolProvider": true,
            },
            "serverInfo": {"name": "specforge-lsp", "version": env!("CARGO_PKG_VERSION")},
        })
    );
    // No entity kind of any extension is among the legend: it is the standard LSP list.
    assert_eq!(legend(), crate::contracts::STANDARD_TOKEN_TYPES);
}

// -- lsp_shutdown -------------------------------------------------------------

#[spec(
    behavior = "lsp_shutdown",
    verify = "shutdown releases in-memory graph"
)]
fn shutdown_clears_state() {
    let mut state = specforge_lsp::LspState::new();
    state.open_document(&crate::url("file:///p/login.spec"), LOGIN, 1);
    edit_buffer(&mut state, "/p/login.spec", LOGIN);
    assert!(state.graph().node("login").is_some());
    let session = state.session().unwrap();
    assert_eq!(
        session
            .project()
            .diagnostics()
            .iter()
            .filter(|d| d.code == "E003")
            .count(),
        1,
        "the E003"
    );

    state.shutdown();

    assert_eq!(state.graph().node_count(), 0);
    assert_eq!(state.graph().edges().len(), 0);
    let session = state.session().unwrap();
    assert!(session.project().diagnostics().is_empty());
    assert!(
        state
            .document(&crate::url("file:///p/login.spec"))
            .is_none()
    );
    assert!(state.is_shutdown());
}

/// `login` references `session_limit`, which exists nowhere.
const LOGIN: &str = "behavior login \"Login\" {\n  invariants [session_limit]\n}\n";

#[test]
fn shutdown_sets_flag() {
    let mut state = specforge_lsp::LspState::new();
    state.shutdown();
    assert!(state.is_shutdown());
}

#[test]
fn requests_after_shutdown_rejected() {
    let mut state = specforge_lsp::LspState::new();
    state.shutdown();
    // Trying to open a document after shutdown should be ignored
    let uri = crate::url("file:///a.spec");
    state.open_document(&uri, "content", 1);
    assert!(state.document(&uri).is_none());
}

// -- shared_incremental_pipeline ----------------------------------------------

#[spec(
    behavior = "shared_incremental_pipeline",
    verify = "LSP and watch share the same graph"
)]
fn lsp_state_holds_graph() {
    let mut state = specforge_lsp::LspState::new();
    assert_eq!(state.graph().node_count(), 0);

    // The graph the LSP serves is the one owned by its project session,
    // the type `specforge watch` holds.
    let session: &specforge_project::ProjectSession = state.session().unwrap();
    assert!(std::ptr::eq(state.graph(), session.project().graph()));

    // A change driven through the session is what the LSP's features see.
    let limit = "invariant session_limit \"Limit\" {\n}\n";
    edit_buffer(&mut state, "/p/login.spec", LOGIN);
    edit_buffer(&mut state, "/p/limit.spec", limit);
    state.set_client(ClientSupport {
        definition_links: true,
        ..ClientSupport::default()
    });
    let login = uri_of_path("/p/login.spec");
    let on_limit = Position::new(1, 16);
    let Some(GotoDefinitionResponse::Link(links)) = answers::definition(&state, &login, on_limit)
    else {
        panic!("the session's entity is navigable");
    };
    assert_eq!(links[0].target_uri, uri_of_path("/p/limit.spec"));
    let refs = answers::references(&state, &login, on_limit, true).unwrap();
    let ref_files: Vec<&str> = refs.iter().map(|r| r.uri.path()).collect();
    assert_eq!(ref_files, ["/p/limit.spec", "/p/login.spec"]);

    // A session fed the same changes, as `specforge watch` feeds its own,
    // builds the same graph and reports the same diagnostics.
    let mut watch = specforge_project::ProjectSession::detached();
    for (path, text) in [("/p/login.spec", LOGIN), ("/p/limit.spec", limit)] {
        watch.update(specforge_project::SourceChange::Hold(&[
            specforge_project::Buffer::new(path, text),
        ]));
    }
    let ids = |g: &specforge_graph::Graph| {
        let mut ids: Vec<String> = g.nodes().iter().map(|n| n.id.raw.to_string()).collect();
        ids.sort();
        ids
    };
    assert_eq!(ids(state.graph()), ["login", "session_limit"]);
    assert_eq!(ids(state.graph()), ids(watch.project().graph()));
    assert_eq!(
        state.graph().edges().len(),
        watch.project().graph().edges().len()
    );
    assert_eq!(
        state.session().unwrap().project().diagnostics(),
        watch.project().diagnostics()
    );
}

#[spec(
    behavior = "shared_incremental_pipeline",
    verify = "graph update serves all LSP features"
)]
fn graph_update_serves_all_features() {
    let mut state = specforge_lsp::LspState::new();

    // Build a graph through the shared session.
    edit_buffer(
        &mut state,
        "/p/auth.spec",
        "behavior login \"User Login\" {\n  types [token]\n}\n",
    );
    edit_buffer(
        &mut state,
        "/p/types.spec",
        "type token \"Auth Token\" {\n}\n",
    );

    // The same graph serves go-to-definition
    let auth = uri_of_path("/p/auth.spec");
    let on_token = Position::new(1, 10);
    assert!(
        answers::definition(&state, &auth, on_token).is_some(),
        "go-to-definition must use shared graph"
    );

    // The same graph serves find-all-references
    let refs = answers::references(&state, &auth, on_token, false).unwrap();
    assert!(
        refs.iter().any(|r| r.uri == auth),
        "find-all-references must use shared graph: {refs:?}"
    );

    // The same graph serves hover, through the inspect read view
    let hover = hover_text(&state, &auth, Position::new(0, 10)).expect("a hover");
    assert!(
        hover.contains("`login`"),
        "hover must use shared graph: {hover}"
    );

    // The same graph serves workspace symbols and completions (one
    // ranking, over ids and titles)
    let symbols = answers::workspace_symbols(&state, "login").unwrap_or_default();
    assert!(
        symbols.iter().any(|s| s.name == "login"),
        "workspace symbols must use shared graph"
    );
    let Some(tower_lsp::lsp_types::CompletionResponse::Array(items)) =
        answers::completion(&state, &auth, Position::new(1, 9))
    else {
        panic!("completion answers a list");
    };
    assert!(
        items.iter().any(|i| i.label == "token"),
        "completions must use shared graph: {items:?}"
    );
}

#[spec(behavior = "lsp_shutdown", verify = "shutdown releases Wasm engines")]
fn shutdown_frees_the_wasm_runtime() {
    let project = tempfile::TempDir::new().unwrap();
    std::fs::write(
        project.path().join("specforge.json"),
        r#"{"name":"t","version":"0.1.0","extensions":[]}"#,
    )
    .unwrap();
    let runtime: specforge_project::SharedRuntime =
        std::sync::Arc::new(specforge_component::ComponentRuntime::new());
    let engine = std::sync::Arc::downgrade(&runtime);
    let mut state = specforge_lsp::LspState::new();
    state.set_session(specforge_project::ProjectSession::open_with_runtime(
        &specforge_common::ProjectRoot::of(project.path()).unwrap(),
        Some(runtime),
    ));
    assert!(state.session().unwrap().runtime().is_some());

    state.shutdown();

    assert!(state.session().unwrap().runtime().is_none());
    assert!(
        engine.upgrade().is_none(),
        "the engine is freed, not just forgotten"
    );
}

// While the session is out for an update, readers see the stand-in of its
// last complete graph: its entity snapshot is that graph's own, and a later
// stand-in never reads the memo of an earlier one.
#[spec(
    behavior = "snapshot_entities_once",
    verify = "a session's snapshot follows every update"
)]
fn a_stand_in_reads_the_snapshot_of_its_own_graph() {
    let project = tempfile::TempDir::new().unwrap();
    std::fs::write(
        project.path().join("specforge.json"),
        r#"{"name":"t","version":"0.1.0","extensions":[]}"#,
    )
    .unwrap();
    std::fs::write(project.path().join("a.spec"), "behavior a \"A\" {\n}\n").unwrap();
    let mut state = specforge_lsp::LspState::new();
    state.set_session(specforge_project::ProjectSession::open_with_runtime(
        &specforge_common::ProjectRoot::of(project.path()).unwrap(),
        None,
    ));

    let first_snapshot =
        std::sync::Arc::clone(state.session().unwrap().project().recorded().entities());
    let session = state.take_session().expect("the session is held");
    let first = state.view().entities().kind_of("a").map(str::to_string);
    assert_eq!(first.as_deref(), Some("behavior"));
    assert!(state.view().entities().kind_of("b").is_none());
    assert!(std::ptr::eq(state.view().entities(), &*first_snapshot));
    state.set_session(session);

    // The graph changes while the session is held; the next stand-in is of
    // the new graph, with a snapshot of its own.
    std::fs::write(project.path().join("b.spec"), "behavior b \"B\" {\n}\n").unwrap();
    let mut session = state.take_session().expect("the session is held");
    session.update(specforge_project::SourceChange::Disk(&[
        "b.spec".to_string()
    ]));
    state.set_session(session);
    let session = state.take_session().expect("the session is held");
    let view = state.view();
    assert_eq!(view.entities().kind_of("b"), Some("behavior"));
    assert!(!std::ptr::eq(view.entities(), &*first_snapshot));
    // It is the snapshot the session holds for that graph.
    assert!(std::ptr::eq(view.entities(), session.project().entities()));
}

/// The watchers derive from the session: relative to each input's
/// directory for a client with relative pattern support, absolute globs
/// otherwise, and the static set with no project.
#[test]
fn file_watchers_follow_what_the_session_is_built_from() {
    use tower_lsp::lsp_types::{GlobPattern, OneOf};

    let dir = tempfile::TempDir::new().unwrap();
    std::fs::write(
        dir.path().join("specforge.json"),
        r#"{"name":"w","version":"0.1.0","extensions":["@acme/local=ext/local.wasm"]}"#,
    )
    .unwrap();
    let session = specforge_project::ProjectSession::open_with_runtime(
        &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
        None,
    );
    let root = dir.path().to_string_lossy().into_owned();

    let absolute: Vec<String> = specforge_lsp::watchers::file_watchers(session.inputs(), false)
        .into_iter()
        .map(|w| match w.glob_pattern {
            GlobPattern::String(glob) => glob,
            other => panic!("expected an absolute glob, got {other:?}"),
        })
        .collect();
    assert_eq!(
        absolute,
        vec![
            format!("{root}/**/*.spec"),
            format!("{root}/specforge.json"),
            format!("{root}/specforge.lock"),
            format!("{root}/ext/local.wasm"),
        ]
    );

    let relative: Vec<(std::path::PathBuf, String)> =
        specforge_lsp::watchers::file_watchers(session.inputs(), true)
            .into_iter()
            .map(|w| match w.glob_pattern {
                GlobPattern::Relative(pattern) => {
                    let OneOf::Right(base) = pattern.base_uri else {
                        panic!("a base URI")
                    };
                    (base.to_file_path().unwrap(), pattern.pattern)
                }
                other => panic!("expected a relative pattern, got {other:?}"),
            })
            .collect();
    assert!(
        relative.contains(&(dir.path().join("ext"), "local.wasm".to_string())),
        "{relative:?}"
    );
    assert!(
        relative.contains(&(dir.path().to_path_buf(), "**/*.spec".to_string())),
        "{relative:?}"
    );

    let detached = specforge_project::ProjectSession::detached();
    assert_eq!(
        specforge_lsp::watchers::file_watchers(detached.inputs(), true),
        specforge_lsp::watchers::default_watchers()
    );
}

/// Whether `glob` (`**` any depth, `*` one path segment, else literal)
/// matches `path`.
fn glob_matches(glob: &str, path: &str) -> bool {
    fn go(glob: &[&str], path: &[&str]) -> bool {
        match glob.split_first() {
            None => path.is_empty(),
            Some((&"**", rest)) => (0..=path.len()).any(|skipped| go(rest, &path[skipped..])),
            Some((segment, rest)) => path.split_first().is_some_and(|(first, tail)| {
                (*segment == "*" || segment == first) && go(rest, tail)
            }),
        }
    }
    go(
        &glob.split('/').collect::<Vec<_>>(),
        &path.split('/').collect::<Vec<_>>(),
    )
}

/// Every input the session classifies, a file created beside a missing
/// referenced file included, is matched by a watcher glob, spelled under
/// the project root (or canonical outside it).
#[spec(
    behavior = "classify_project_changes",
    verify = "the LSP's watchers cover every input the session classifies"
)]
fn the_watchers_cover_what_the_session_classifies() {
    use specforge_project::ProjectSession;
    use tower_lsp::lsp_types::GlobPattern;

    let outside = tempfile::TempDir::new().unwrap();
    let far = tempfile::TempDir::new().unwrap();
    let module_dir = std::fs::canonicalize(far.path()).unwrap().join("mods");
    std::fs::create_dir_all(&module_dir).unwrap();
    let module = module_dir.join("ext.wasm");
    // From `spec/`, two levels up is the parent of the root: the sibling
    // temp directory `outside`.
    let outside_name = outside.path().file_name().unwrap().to_string_lossy();
    let reference = format!("../../{outside_name}/guide.md");
    let dir = crate::session::docref_project(&format!(
        "gadget gadget_one \"G\" {{\n  docs [\"{reference}\", \"missing/sub.md\"]\n}}\n"
    ));
    let root = dir.path();
    std::fs::write(
        root.join("specforge.json"),
        serde_json::json!({
            "name": "p",
            "version": "0.1.0",
            "spec_root": "spec",
            "extensions": [
                "@specforge/software",
                "@sdk/docref=ext/docref.wasm",
                format!("@acme/far={}", module.display()),
            ],
        })
        .to_string(),
    )
    .unwrap();
    let session = ProjectSession::open(&specforge_common::ProjectRoot::of(root).unwrap());

    let globs: Vec<String> = specforge_lsp::watchers::file_watchers(session.inputs(), false)
        .into_iter()
        .map(|w| match w.glob_pattern {
            GlobPattern::String(glob) => glob,
            other => panic!("expected an absolute glob, got {other:?}"),
        })
        .collect();
    let spec_root = root.join("spec");
    let canonical_outside = std::fs::canonicalize(outside.path()).unwrap();
    // The inputs as the watchers spell them: under the root as opened,
    // canonical outside it.
    let spelled = [
        (root.join("specforge.json"), root.join("specforge.json")),
        (root.join("specforge.lock"), root.join("specforge.lock")),
        (root.join("ext/docref.wasm"), root.join("ext/docref.wasm")),
        (module.clone(), module),
        (
            spec_root.join(&reference),
            canonical_outside.join("guide.md"),
        ),
        (
            spec_root.join("missing/sub.md"),
            spec_root.join("missing/sub.md"),
        ),
        // A file created beside a missing referenced file changes E016's
        // suggestion.
        (
            spec_root.join("missing/x.md"),
            spec_root.join("missing/x.md"),
        ),
    ];
    for (input, watched) in &spelled {
        std::fs::create_dir_all(input.parent().unwrap()).unwrap();
        std::fs::write(input, "new content").unwrap();
        assert!(
            !session.changes([input.as_path()]).is_empty(),
            "{}",
            input.display()
        );
        let watched = watched.display().to_string();
        assert!(
            globs.iter().any(|glob| glob_matches(glob, &watched)),
            "{watched} is matched by none of {globs:?}"
        );
    }
    assert!(!globs.iter().any(|g| g.contains("/../")), "{globs:?}");
}

// -- validate_delta_correctness -----------------------------------------------

/// The session the LSP holds verifies each rebuild against a cold build in a
/// debug build, with no flag set (ADR 0032).
#[cfg(debug_assertions)]
#[spec(
    behavior = "validate_delta_correctness",
    verify = "the LSP and MCP check each rebuild in a debug build"
)]
fn a_debug_build_of_the_lsp_verifies_each_rebuild() {
    let mut state = specforge_lsp::LspState::new();
    let limit = "invariant session_limit \"Limit\" {\n}\n";
    for (path, text) in [
        ("/p/login.spec", LOGIN),
        ("/p/limit.spec", limit),
        ("/p/limit.spec", "invariant session_cap \"Cap\" {\n}\n"),
    ] {
        let update = state.session_mut().expect("no update is running").update(
            specforge_project::SourceChange::Hold(&[specforge_project::Buffer::new(path, text)]),
        );
        assert_eq!(update.verification, Some(Ok(())), "after {path}");
    }
}

/// A symlinked source's target lies outside the project: a client that is
/// asked to report only the root never sees it change.
#[cfg(unix)]
#[spec(
    behavior = "classify_project_changes",
    verify = "a session's watch roots and the LSP's watchers cover every symlinked source's target"
)]
fn the_watchers_cover_a_symlinked_sources_target() {
    use tower_lsp::lsp_types::GlobPattern;

    let outside = tempfile::TempDir::new().unwrap();
    let outside_dir = std::fs::canonicalize(outside.path()).unwrap();
    std::fs::write(outside_dir.join("real.spec"), "").unwrap();
    let dir = tempfile::TempDir::new().unwrap();
    std::fs::write(
        dir.path().join("specforge.json"),
        r#"{"name":"w","version":"0.1.0"}"#,
    )
    .unwrap();
    std::os::unix::fs::symlink(outside_dir.join("real.spec"), dir.path().join("link.spec"))
        .unwrap();
    let session = specforge_project::ProjectSession::open_with_runtime(
        &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
        None,
    );

    let target = outside_dir.join("real.spec").display().to_string();
    let globs: Vec<String> = specforge_lsp::watchers::file_watchers(session.inputs(), false)
        .into_iter()
        .map(|w| match w.glob_pattern {
            GlobPattern::String(glob) => glob,
            other => panic!("expected an absolute glob, got {other:?}"),
        })
        .collect();
    assert!(
        globs.iter().any(|glob| glob_matches(glob, &target)),
        "{target} is matched by none of {globs:?}"
    );
}

/// A folder moved or deleted sends no event for a `**/*.spec` glob: the
/// client is asked for every entry created or deleted under the root.
#[spec(
    behavior = "classify_project_changes",
    verify = "the LSP asks its client to report every directory created or deleted under the project root"
)]
#[ignore = "fixed by T5"]
fn the_client_reports_directories_under_the_root() {
    use tower_lsp::lsp_types::{GlobPattern, WatchKind};

    let dir = tempfile::TempDir::new().unwrap();
    std::fs::write(
        dir.path().join("specforge.json"),
        r#"{"name":"w","version":"0.1.0"}"#,
    )
    .unwrap();
    let session = specforge_project::ProjectSession::open_with_runtime(
        &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
        None,
    );
    let root = dir.path().to_string_lossy().into_owned();

    let watchers = specforge_lsp::watchers::file_watchers(session.inputs(), false);
    assert!(
        watchers.iter().any(|w| {
            matches!(&w.glob_pattern, GlobPattern::String(glob) if *glob == format!("{root}/**/*"))
                && w.kind == Some(WatchKind::Create | WatchKind::Delete)
        }),
        "{watchers:?}"
    );
}
