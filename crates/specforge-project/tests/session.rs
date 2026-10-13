use std::fs;
use std::path::Path;
use std::sync::Arc;

use specforge_common::Diagnostic;
use specforge_graph::Graph;
use specforge_project::{
    Buffer, CheckMode, CompiledProject, ProjectSession, SourceChange, UpdateKind,
};
use specforge_test::prelude::*;
use tempfile::TempDir;

const CONFIG: &str =
    r#"{"name":"s","version":"0.1.0","extensions":["@specforge/software","@specforge/testing"]}"#;

fn project(config: &str, files: &[(&str, &str)]) -> TempDir {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("specforge.json"), config).unwrap();
    for (path, text) in files {
        write(dir.path(), path, text);
    }
    dir
}

fn write(root: &Path, path: &str, text: &str) {
    let path = root.join(path);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

/// What the session reports with a span in `path`.
fn in_file(diagnostics: &[Diagnostic], path: &str) -> Vec<Diagnostic> {
    diagnostics
        .iter()
        .filter(|d| d.span.as_ref().is_some_and(|s| s.file == path))
        .cloned()
        .collect()
}

/// Every node (id, kind, file, title, fields) and edge of a graph.
fn graph_contents(graph: &Graph) -> (Vec<String>, Vec<String>) {
    let mut nodes: Vec<String> = graph
        .nodes()
        .iter()
        .map(|n| {
            format!(
                "{} {} {} {:?} {}",
                n.id.raw,
                n.kind.raw,
                n.source_span.file,
                n.title,
                serde_json::to_string(&specforge_graph::field_form::write_fields(&n.fields))
                    .unwrap()
            )
        })
        .collect();
    nodes.sort();
    let mut edges: Vec<String> = graph
        .edges()
        .iter()
        .map(|e| format!("{} -{}-> {}", e.source, e.label, e.target))
        .collect();
    edges.sort();
    (nodes, edges)
}

/// The session agrees with a fresh compile of what is on disk now.
fn assert_matches_a_fresh_compile(session: &ProjectSession, root: &Path) {
    let runtime = Arc::new(specforge_component::ComponentRuntime::with_user_cache());
    let fresh = CompiledProject::compile(
        &specforge_common::ProjectRoot::of(root).unwrap(),
        Some(runtime.clone()),
    );
    assert_eq!(
        graph_contents(session.project().graph()),
        graph_contents(fresh.graph())
    );
    assert_eq!(
        session.project().diagnostics(),
        fresh.diagnostics(),
        "a session reports a fresh compile's diagnostics, in its order"
    );
}

fn w113(diagnostics: &[Diagnostic]) -> Vec<String> {
    diagnostics
        .iter()
        .filter(|d| d.code == "W113")
        .map(|d| d.message.clone())
        .collect()
}

fn changed(paths: &[&str]) -> Vec<String> {
    paths.iter().map(|p| p.to_string()).collect()
}

/// Edits that add and remove imports, a duplicate ID that moves between
/// files, a missing import and a deleted file: after each, the session's
/// graph and diagnostics are those of a fresh compile.
#[specforge_test(
    invariant = "incremental_correctness",
    verify = "incremental recompilation produces the same graph as a full rebuild"
)]
fn every_update_leaves_what_a_fresh_compile_builds() {
    let dir = project(
        CONFIG,
        &[
            (
                "a.spec",
                "behavior alpha \"A\" {\n  category command\n  contract \"The system MUST a\"\n  verify unit \"a\"\n}\n",
            ),
            (
                "b.spec",
                "behavior beta \"B\" {\n  category command\n  contract \"The system MUST b\"\n  verify unit \"b\"\n}\n",
            ),
        ],
    );
    let root = dir.path();
    let mut session = ProjectSession::open(&specforge_common::ProjectRoot::of(root).unwrap());
    session.set_verify_incremental(true);
    assert_matches_a_fresh_compile(&session, root);

    let steps: &[(&str, Option<&str>)] = &[
        // An import that does not resolve (E025).
        (
            "a.spec",
            Some(
                "use \"missing\"\n\nbehavior alpha \"A\" {\n  category command\n  contract \"The system MUST a\"\n  verify unit \"a\"\n}\n",
            ),
        ),
        // A cycle a -> b -> a (W113) and a duplicate of alpha in b.spec.
        (
            "b.spec",
            Some(
                "use \"a\"\n\nbehavior alpha \"Dup\" {\n  category command\n  contract \"The system MUST dup\"\n}\nbehavior beta \"B\" {\n  category command\n  contract \"The system MUST b\"\n  verify unit \"b\"\n}\n",
            ),
        ),
        (
            "a.spec",
            Some(
                "use \"b\"\n\nbehavior alpha \"A\" {\n  category command\n  verify unit \"a\"\n}\n",
            ),
        ),
        // The first declaration goes away: b.spec's alpha takes over.
        ("a.spec", None),
        (
            "c.spec",
            Some(
                "behavior gamma \"C\" {\n  category command\n  contract \"The system MUST c\"\n}\n",
            ),
        ),
    ];
    for (path, text) in steps {
        match text {
            Some(text) => write(root, path, text),
            None => fs::remove_file(root.join(path)).unwrap(),
        }
        let update = session.update(SourceChange::Disk(&changed(&[path])));
        assert_eq!(update.verification, Some(Ok(())), "after {path}");
        assert_eq!(update.diagnostics[..], session.project().diagnostics()[..]);
        assert_matches_a_fresh_compile(&session, root);
    }
}

/// The format version header is a compile diagnostic like any other: an
/// edit that adds, changes or removes it changes what the session reports,
/// the same as a fresh compile does.
#[specforge_test(
    behavior = "detect_format_version_mismatch",
    verify = "older format version detected and reported as I007"
)]
fn an_edited_format_version_header_is_reported_as_a_fresh_compile_reports_it() {
    const BODY: &str = "behavior alpha \"A\" {\n  contract \"The system MUST a\"\n}\n";
    let dir = project(CONFIG, &[("a.spec", BODY)]);
    let root = dir.path();
    let mut session = ProjectSession::open(&specforge_common::ProjectRoot::of(root).unwrap());
    session.set_verify_incremental(true);
    let codes = |session: &ProjectSession| -> Vec<String> {
        session
            .project()
            .diagnostics()
            .into_iter()
            .map(|d| d.code)
            .filter(|code| code == "I007" || code == "E019")
            .collect()
    };
    assert!(codes(&session).is_empty());

    for (header, expected) in [
        ("// specforge-format: 0.9\n", vec!["I007"]),
        ("// specforge-format: 9.0\n", vec!["E019"]),
        ("// specforge-format: 1.0\n", vec![]),
        ("// specforge-format: 0.1\n", vec!["I007"]),
        ("", vec![]),
    ] {
        write(root, "a.spec", &format!("{header}{BODY}"));
        let update = session.update(SourceChange::Disk(&changed(&["a.spec"])));
        assert_eq!(update.verification, Some(Ok(())), "after {header:?}");
        assert_eq!(codes(&session), expected, "after {header:?}");
        assert_matches_a_fresh_compile(&session, root);
    }
}

/// Define blocks are reported, not registered: adding one, renaming it
/// and removing it leave what a fresh compile reports.
#[specforge_test(
    behavior = "report_define_blocks",
    verify = "an incremental rebuild reports a define block as a fresh compile does"
)]
fn an_incremental_rebuild_reports_define_blocks_as_a_fresh_compile() {
    let dir = project(
        CONFIG,
        &[(
            "a.spec",
            "behavior alpha \"A\" {\n  category command\n  contract \"The system MUST a\"\n}\n",
        )],
    );
    let root = dir.path();
    let mut session = ProjectSession::open(&specforge_common::ProjectRoot::of(root).unwrap());
    session.set_verify_incremental(true);

    for text in [
        Some("define user_story {\n  refs [alpha]\n}\n"),
        Some("define behavior {\n}\n"),
        None,
    ] {
        match text {
            Some(text) => write(root, "b.spec", text),
            None => fs::remove_file(root.join("b.spec")).unwrap(),
        }
        let update = session.update(SourceChange::Disk(&changed(&["b.spec"])));
        assert_eq!(update.verification, Some(Ok(())));
        assert_matches_a_fresh_compile(&session, root);
        let w143 = session
            .project()
            .diagnostics()
            .iter()
            .filter(|d| d.code == "W143")
            .count();
        assert_eq!(w143, usize::from(text.is_some()), "{text:?}");
    }
}

#[specforge_test(
    behavior = "resolve_imports_on_update",
    verify = "cycle detection re-runs after an update"
)]
fn an_edit_that_closes_an_import_cycle_reports_it() {
    let dir = project(
        CONFIG,
        &[
            ("a.spec", "behavior foo \"Foo\" { contract \"x\" }\n"),
            (
                "b.spec",
                "use \"a\"\nbehavior bar \"Bar\" { contract \"y\" }\n",
            ),
        ],
    );
    let root = dir.path();
    let mut session = ProjectSession::open(&specforge_common::ProjectRoot::of(root).unwrap());
    assert!(w113(&session.project().diagnostics()).is_empty());

    write(
        root,
        "a.spec",
        "use \"b\"\nbehavior foo \"Foo\" { contract \"x\" }\n",
    );
    let update = session.update(SourceChange::Disk(&changed(&["a.spec"])));

    assert_eq!(
        w113(&update.diagnostics),
        ["circular import detected: a.spec -> b.spec"]
    );
}

#[specforge_test(
    behavior = "resolve_imports_on_update",
    verify = "cycle detection re-runs after an update"
)]
fn an_edit_that_breaks_an_import_cycle_clears_it() {
    let dir = project(
        CONFIG,
        &[
            (
                "a.spec",
                "use \"b\"\nbehavior foo \"Foo\" { contract \"x\" }\n",
            ),
            (
                "b.spec",
                "use \"a\"\nbehavior bar \"Bar\" { contract \"y\" }\n",
            ),
        ],
    );
    let root = dir.path();
    let mut session = ProjectSession::open(&specforge_common::ProjectRoot::of(root).unwrap());
    assert_eq!(
        w113(&session.project().diagnostics()),
        ["circular import detected: a.spec -> b.spec"]
    );

    write(root, "a.spec", "behavior foo \"Foo\" { contract \"x\" }\n");
    let update = session.update(SourceChange::Disk(&changed(&["a.spec"])));

    assert!(
        w113(&update.diagnostics).is_empty(),
        "{:?}",
        update.diagnostics
    );
}

fn e025(diagnostics: &[Diagnostic]) -> Vec<String> {
    diagnostics
        .iter()
        .filter(|d| d.code == "E025")
        .map(|d| d.message.clone())
        .collect()
}

/// Imports of every kind the resolver knows: bare, relative from a nested
/// file, a directory's `index.spec`, an extension (I004). Each update
/// resolves them all again, so the import diagnostics are a fresh
/// compile's, and an importer is never re-parsed for its target's sake.
#[specforge_test(
    behavior = "resolve_imports_on_update",
    verify = "import diagnostics after an update match a full rebuild"
)]
fn imports_of_every_kind_stay_resolved_across_updates() {
    let dir = project(
        CONFIG,
        &[
            ("types.spec", "type Shared {\n  id string\n}\n"),
            (
                "sub/main.spec",
                "use \"../types\"\nuse \"models\"\nuse \"@acme/ext\"\n\nbehavior main \"Main\" {\n  category command\n  contract \"The system MUST m\"\n}\n",
            ),
        ],
    );
    let root = dir.path();
    let mut session = ProjectSession::open(&specforge_common::ProjectRoot::of(root).unwrap());
    session.set_verify_incremental(true);
    assert_eq!(
        e025(&session.project().diagnostics()),
        ["import target not found among the project's sources: models"]
    );
    assert!(
        session
            .project()
            .diagnostics()
            .iter()
            .any(|d| d.code == "I004")
    );
    assert_matches_a_fresh_compile(&session, root);

    // The directory appears: `models` now names models/index.spec, though
    // sub/main.spec (its importer) is not re-parsed.
    write(root, "models/index.spec", "type Model {\n  id string\n}\n");
    let update = session.update(SourceChange::Disk(&changed(&["models/index.spec"])));
    assert_eq!(update.rebuilt_files, ["models/index.spec"]);
    assert_eq!(update.verification, Some(Ok(())));
    assert!(
        e025(&update.diagnostics).is_empty(),
        "{:?}",
        update.diagnostics
    );
    assert_matches_a_fresh_compile(&session, root);

    // The relative target goes away: E025 on the importer, again without
    // re-parsing it.
    fs::remove_file(root.join("types.spec")).unwrap();
    let update = session.update(SourceChange::Disk(&changed(&["types.spec"])));
    assert_eq!(update.rebuilt_files, ["types.spec"]);
    assert_eq!(
        e025(&update.diagnostics),
        ["import target not found among the project's sources: ../types"]
    );
    assert_matches_a_fresh_compile(&session, root);
}

/// `@alias/...` imports resolve through the resolver's path aliases, which
/// `specforge.json` does not configure: in a project an `@` import names an
/// extension (I004), and the session reports it as a fresh compile does.
#[specforge_test(
    behavior = "resolve_imports_on_update",
    verify = "an added use import is resolved on the next update"
)]
fn an_added_import_is_resolved_on_the_next_update() {
    let dir = project(
        CONFIG,
        &[
            ("lib/shared.spec", "type Shared {\n  id string\n}\n"),
            ("main.spec", "type Main {\n  id string\n}\n"),
        ],
    );
    let root = dir.path();
    let mut session = ProjectSession::open(&specforge_common::ProjectRoot::of(root).unwrap());

    for (text, e025s, i004s) in [
        ("use \"lib/shared\"\n", 0, 0),
        ("use \"@shared/thing\"\n", 0, 1),
        ("use \"lib/missing\"\n", 1, 0),
    ] {
        write(
            root,
            "main.spec",
            &format!("{text}type Main {{\n  id string\n}}\n"),
        );
        let update = session.update(SourceChange::Disk(&changed(&["main.spec"])));
        assert_eq!(e025(&update.diagnostics).len(), e025s, "{text}");
        let i004 = update
            .diagnostics
            .iter()
            .filter(|d| d.code == "I004")
            .count();
        assert_eq!(i004, i004s, "{text}");
        assert_matches_a_fresh_compile(&session, root);
    }
}

#[specforge_test(
    behavior = "resolve_imports_on_update",
    verify = "a removed use import no longer reports"
)]
fn a_removed_import_no_longer_reports() {
    let dir = project(
        CONFIG,
        &[(
            "main.spec",
            "use \"missing\"\ntype Main {\n  id string\n}\n",
        )],
    );
    let root = dir.path();
    let mut session = ProjectSession::open(&specforge_common::ProjectRoot::of(root).unwrap());
    assert_eq!(e025(&session.project().diagnostics()).len(), 1);

    write(root, "main.spec", "type Main {\n  id string\n}\n");
    let update = session.update(SourceChange::Disk(&changed(&["main.spec"])));
    assert!(e025(&update.diagnostics).is_empty());
}

#[specforge_test(
    behavior = "resolve_imports_on_update",
    verify = "Resolve Imports on Every Update: import resolution after each update holds — subgraph_invalidated_fired, import_dag_updated_emitted, cycle_detection_rerun"
)]
fn resolve_imports_on_update_contract() {
    let dir = project(
        CONFIG,
        &[
            ("a.spec", "behavior foo \"Foo\" { contract \"x\" }\n"),
            ("b.spec", "behavior bar \"Bar\" { contract \"y\" }\n"),
            ("c.spec", "behavior qux \"Qux\" { contract \"z\" }\n"),
        ],
    );
    let root = dir.path();
    let mut session = ProjectSession::open(&specforge_common::ProjectRoot::of(root).unwrap());

    // b imports a: no cycle yet.
    write(
        root,
        "b.spec",
        "use \"a\"\nbehavior bar \"Bar\" { contract \"y\" }\n",
    );
    let update = session.update(SourceChange::Disk(&changed(&["b.spec"])));
    assert!(w113(&update.diagnostics).is_empty(), "no cycle yet");

    // subgraph_invalidated_fired: editing a.spec re-parses a.spec alone,
    // not its importer b.spec nor the unrelated c.spec.
    // import_dag_updated_emitted, cycle_detection_rerun: the edit closes
    // a -> b -> a, and W113 appears on this update.
    write(
        root,
        "a.spec",
        "use \"b\"\nbehavior foo \"Foo\" { contract \"x\" }\n",
    );
    let update = session.update(SourceChange::Disk(&changed(&["a.spec"])));
    assert_eq!(update.rebuilt_files, ["a.spec"]);
    assert_eq!(
        w113(&update.diagnostics),
        ["circular import detected: a.spec -> b.spec"]
    );

    // Removing the import clears the cycle on the next update.
    write(root, "b.spec", "behavior bar \"Bar\" { contract \"y\" }\n");
    let update = session.update(SourceChange::Disk(&changed(&["b.spec"])));
    assert!(w113(&update.diagnostics).is_empty(), "cycle must be gone");
    assert_matches_a_fresh_compile(&session, root);
}

/// `exclude` is relative to the spec root, and a change to an excluded
/// file leaves the session as it was.
#[specforge_test(
    behavior = "resolve_use_imports",
    verify = "files matching an exclude entry are not compiled"
)]
fn excluded_files_stay_out_of_the_compile_and_the_session() {
    let dir = project(
        r#"{"name":"s","version":"0.1.0","extensions":["@specforge/software"],"spec_root":"spec","exclude":["drafts/"]}"#,
        &[
            ("spec/main.spec", "term alpha \"Alpha\" {\n}\n"),
            (
                "spec/drafts/draft.spec",
                "term alpha \"Alpha again\" {\n}\n",
            ),
        ],
    );
    let root = dir.path();
    let runtime = Arc::new(specforge_component::ComponentRuntime::with_user_cache());
    let compiled = CompiledProject::compile(
        &specforge_common::ProjectRoot::of(root).unwrap(),
        Some(runtime.clone()),
    );
    let mut files: Vec<String> = compiled.source_texts().into_keys().collect();
    files.sort();
    assert_eq!(files, ["main.spec"]);
    assert!(
        !compiled.diagnostics().iter().any(|d| d.code == "E002"),
        "{:?}",
        compiled.diagnostics()
    );

    let mut session = ProjectSession::open(&specforge_common::ProjectRoot::of(root).unwrap());
    write(root, "spec/drafts/draft.spec", "term beta \"Beta\" {\n}\n");
    let update = session.update(SourceChange::Disk(&changed(&["drafts/draft.spec"])));
    assert!(
        update.rebuilt_files.is_empty(),
        "{:?}",
        update.rebuilt_files
    );
    assert!(session.project().graph().node("beta").is_none());
    assert_eq!(session.project().file_count(), 1);
}

/// A project whose spec root holds a nested project (`sub`, with its own
/// `specforge.json`) that declares what the outer project does too.
#[specforge_test(
    behavior = "resolve_use_imports",
    verify = "a nested project's files are not compiled, an editor buffer of one included"
)]
#[ignore = "fixed by T2"]
fn a_nested_projects_files_are_not_compiled() {
    let dir = project(
        CONFIG,
        &[
            ("a.spec", "term login_flow \"Login\" {\n}\n"),
            ("sub/specforge.json", "{}"),
            ("sub/b.spec", "term login_flow \"Login (sub)\" {\n}\n"),
            ("sub/c.spec", "term sub_c \"C\" {\n}\n"),
            ("sub/d.spec", "use \"c\"\n\nterm sub_d \"D\" {\n}\n"),
        ],
    );
    let root = dir.path();
    let project_root = specforge_common::ProjectRoot::of(root).unwrap();

    let compiled = CompiledProject::compile(&project_root, None);
    let mut keys: Vec<String> = compiled.source_texts().into_keys().collect();
    keys.sort();
    assert_eq!(keys, ["a.spec"]);
    let diagnostics = compiled.diagnostics();
    let codes: Vec<&str> = diagnostics.iter().map(|d| d.code.as_str()).collect();
    for code in ["E002", "E025", "E024"] {
        assert!(!codes.contains(&code), "{code} in {codes:?}");
    }

    let mut session = ProjectSession::open(&project_root);
    let held = root.join("sub/b.spec");
    let update = session.update(SourceChange::Hold(&[Buffer::new(
        held.to_str().unwrap(),
        "term login_flow \"Held\" {\n}\n",
    )]));
    assert!(
        update.rebuilt_files.is_empty(),
        "{:?}",
        update.rebuilt_files
    );
    assert_eq!(session.project().file_count(), 1);
    assert!(session.project().graph().node("sub_c").is_none());
}

/// A nested project appearing below the spec root takes its files out of
/// the session, and its removal puts them back.
#[specforge_test(
    behavior = "bring_session_up_to_date",
    verify = "a nested project appearing below the spec root takes its files out of the session, and its removal puts them back"
)]
#[ignore = "fixed by T3"]
fn a_nested_project_appearing_takes_its_files_out_and_its_removal_puts_them_back() {
    let bare = r#"{"name":"s","version":"0.1.0","extensions":["@specforge/software"]}"#;
    let dir = project(
        bare,
        &[
            ("a.spec", "term outer_one \"O\" {\n}\n"),
            ("sub/c.spec", "term sub_only \"S\" {\n}\n"),
        ],
    );
    let root = dir.path();
    let mut session = ProjectSession::open(&specforge_common::ProjectRoot::of(root).unwrap());
    assert!(session.project().graph().node("sub_only").is_some());

    write(root, "sub/specforge.json", "{}");
    assert_eq!(session.stale().sources, ["sub/c.spec"]);
    assert!(session.ensure_fresh().is_some());
    assert!(session.project().graph().node("sub_only").is_none());
    assert_matches_a_fresh_compile(&session, root);

    fs::remove_file(root.join("sub/specforge.json")).unwrap();
    assert_eq!(session.stale().sources, ["sub/c.spec"]);
    assert!(session.ensure_fresh().is_some());
    assert!(session.project().graph().node("sub_only").is_some());
    assert_matches_a_fresh_compile(&session, root);

    // A buffer released while the marker exists leaves the graph.
    let path = root.join("sub/c.spec");
    session.update(SourceChange::Hold(&[Buffer::new(
        path.to_str().unwrap(),
        "term sub_only \"S\" {\n}\n",
    )]));
    write(root, "sub/specforge.json", "{}");
    session.release(std::slice::from_ref(&path));
    assert!(session.project().graph().node("sub_only").is_none());
}

/// A reload picks up a changed `specforge.json`: here an extension that
/// can't load (E028), which a fresh compile reports too.
#[test]
fn a_reload_reads_the_environment_again() {
    let dir = project(CONFIG, &[("a.spec", "term alpha \"Alpha\" {\n}\n")]);
    let root = dir.path();
    let mut session = ProjectSession::open(&specforge_common::ProjectRoot::of(root).unwrap());
    assert!(
        !session
            .project()
            .diagnostics()
            .iter()
            .any(|d| d.code == "E028")
    );

    fs::write(
        root.join("specforge.json"),
        r#"{"name":"s","version":"0.1.0","extensions":["@specforge/software","@specforge/no-such-extension"]}"#,
    )
    .unwrap();
    let update = session.reload_environment();

    assert!(update.diagnostics.iter().any(|d| d.code == "E028"));
    assert_matches_a_fresh_compile(&session, root);
}

/// A `.wasm` file entry loads in a session (what the LSP and MCP hold)
/// under the name its component declares, its file is an environment
/// input, and a reload after it stops loading reports E028 naming it.
#[test]
fn a_wasm_file_entry_loads_in_a_session_and_reloads_with_its_file() {
    let greet = fs::read(specforge_component::committed::GREET.path()).unwrap();
    let dir = project(
        r#"{"name":"s","version":"0.1.0","extensions":["@specforge/software","ext/greet.wasm"]}"#,
        &[("a.spec", "greeting hello \"Hello\" {\n  style warm\n}\n")],
    );
    let root = dir.path();
    fs::create_dir_all(root.join("ext")).unwrap();
    fs::write(root.join("ext/greet.wasm"), &greet).unwrap();
    let mut session = ProjectSession::open(&specforge_common::ProjectRoot::of(root).unwrap());

    let enabled = session.project().environment().enabled();
    assert_eq!(enabled[1].name, "@sdk/greet");
    assert_eq!(enabled[1].file.as_deref(), Some("ext/greet.wasm"));
    let codes: Vec<String> = session
        .project()
        .diagnostics()
        .into_iter()
        .map(|d| d.code)
        .collect();
    for code in ["E024", "E028", "W019", "W112"] {
        assert!(!codes.iter().any(|c| c == code), "{code}: {codes:?}");
    }

    fs::write(root.join("ext/greet.wasm"), b"\0asm not a component").unwrap();
    assert!(
        !session
            .changes([root.join("ext/greet.wasm").as_path()])
            .environment
            .is_empty()
    );
    let update = session.reload_environment();
    let e028: Vec<&Diagnostic> = update
        .diagnostics
        .iter()
        .filter(|d| d.code == "E028")
        .collect();
    assert_eq!(e028.len(), 1, "{:?}", update.diagnostics);
    assert!(
        e028[0].message.contains("'ext/greet.wasm'"),
        "{}",
        e028[0].message
    );
    assert_matches_a_fresh_compile(&session, root);

    fs::write(root.join("ext/greet.wasm"), &greet).unwrap();
    let update = session.reload_environment();
    assert!(
        !update
            .diagnostics
            .iter()
            .any(|d| d.code == "E028" || d.code == "E024")
    );
}

fn behavior(id: &str, extra: &str) -> String {
    format!(
        "behavior {id} \"{id}\" {{\n  category command\n  contract \"The system MUST {id}\"\n{extra}}}\n"
    )
}

fn ids(nodes: &[specforge_project::NodeChange]) -> Vec<&str> {
    nodes.iter().map(|n| n.id.as_str()).collect()
}

/// a.spec imports types.spec, main.spec imports a.spec, c.spec stands
/// alone.
fn three_files() -> TempDir {
    project(
        CONFIG,
        &[
            ("types.spec", &behavior("alpha", "")),
            (
                "a.spec",
                &format!(
                    "use \"types\"\n\n{}",
                    behavior("beta", "  invariants [alpha]\n")
                ),
            ),
            (
                "main.spec",
                &format!("use \"a\"\n\n{}", behavior("gamma", "")),
            ),
            ("c.spec", &behavior("delta", "")),
        ],
    )
}

#[specforge_test(
    behavior = "invalidate_changed_files",
    verify = "only the changed files are re-parsed"
)]
fn only_the_changed_files_are_re_parsed() {
    let dir = three_files();
    let root = dir.path();
    let mut session = ProjectSession::open(&specforge_common::ProjectRoot::of(root).unwrap());
    write(root, "types.spec", &behavior("alpha2", ""));
    write(root, "c.spec", &behavior("delta2", ""));

    let update = session.update(SourceChange::Disk(&changed(&["types.spec", "c.spec"])));

    assert_eq!(update.rebuilt_files, ["c.spec", "types.spec"]);
    assert_matches_a_fresh_compile(&session, root);
}

/// References resolve across the project without `use`: an importer parses
/// the same whatever its import's target says, so it is not re-parsed, and
/// its references still follow the target's entities.
#[specforge_test(
    behavior = "invalidate_changed_files",
    verify = "an importer of a changed file is not re-parsed"
)]
fn an_importer_of_a_changed_file_is_not_re_parsed() {
    let dir = three_files();
    let root = dir.path();
    let mut session = ProjectSession::open(&specforge_common::ProjectRoot::of(root).unwrap());
    session.set_verify_incremental(true);
    assert!(
        session
            .project()
            .graph()
            .edges_to("alpha")
            .iter()
            .any(|e| e.source == "beta")
    );

    // alpha goes away: beta (a.spec) now has an unresolved reference, and
    // neither a.spec nor main.spec (which imports it in turn) is re-parsed.
    write(root, "types.spec", &behavior("other", ""));
    let update = session.update(SourceChange::Disk(&changed(&["types.spec"])));
    assert_eq!(update.rebuilt_files, ["types.spec"]);
    assert_eq!(update.verification, Some(Ok(())));
    assert!(
        update
            .diagnostics
            .iter()
            .any(|d| d.code == "E003" && d.span.as_ref().is_some_and(|s| s.file == "a.spec"))
    );
    assert_matches_a_fresh_compile(&session, root);
}

#[specforge_test(
    behavior = "invalidate_changed_files",
    verify = "unrelated files are not re-parsed"
)]
fn unrelated_files_are_not_re_parsed() {
    let dir = three_files();
    let root = dir.path();
    let mut session = ProjectSession::open(&specforge_common::ProjectRoot::of(root).unwrap());
    write(root, "c.spec", &behavior("delta", "  invariants [alpha]\n"));

    let update = session.update(SourceChange::Disk(&changed(&["c.spec"])));

    assert_eq!(update.rebuilt_files, ["c.spec"]);
    assert_matches_a_fresh_compile(&session, root);
}

#[specforge_test(
    behavior = "invalidate_changed_files",
    verify = "deleted file entities removed from graph"
)]
fn deleted_file_entities_are_removed_from_the_graph() {
    let dir = three_files();
    let root = dir.path();
    let mut session = ProjectSession::open(&specforge_common::ProjectRoot::of(root).unwrap());
    fs::remove_file(root.join("types.spec")).unwrap();

    let update = session.update(SourceChange::Disk(&changed(&["types.spec"])));

    assert!(session.project().graph().node("alpha").is_none());
    assert_eq!(ids(&update.delta.removed_nodes), ["alpha"]);
    assert_eq!(update.delta.removed_edges.len(), 1, "beta -> alpha");
    assert_eq!(session.project().file_count(), 3);
    assert_matches_a_fresh_compile(&session, root);
}

#[specforge_test(
    behavior = "invalidate_changed_files",
    verify = "new file entities added to graph"
)]
fn new_file_entities_are_added_to_the_graph() {
    let dir = three_files();
    let root = dir.path();
    let mut session = ProjectSession::open(&specforge_common::ProjectRoot::of(root).unwrap());
    write(
        root,
        "nested/new.spec",
        &behavior("epsilon", "  invariants [alpha]\n"),
    );

    let update = session.update(SourceChange::Disk(&changed(&["nested/new.spec"])));

    assert!(session.project().graph().node("epsilon").is_some());
    assert_eq!(ids(&update.delta.added_nodes), ["epsilon"]);
    assert_eq!(update.delta.affected_files, ["nested/new.spec"]);
    assert_eq!(session.project().file_count(), 5);
    assert_matches_a_fresh_compile(&session, root);
}

#[specforge_test(
    behavior = "invalidate_changed_files",
    verify = "Invalidate Changed Files: file invalidation holds — file_changes_coalesced_fired, invalidation_set_computed, subgraph_invalidated_emitted, unrelated_files_untouched"
)]
fn invalidate_changed_files_contract() {
    let dir = three_files();
    let root = dir.path();
    let mut session = ProjectSession::open(&specforge_common::ProjectRoot::of(root).unwrap());
    session.set_verify_incremental(true);

    // file_changes_coalesced_fired: one batch with an edit, a deletion, a
    // creation, and a path that never existed.
    write(root, "a.spec", &behavior("beta", ""));
    fs::remove_file(root.join("c.spec")).unwrap();
    write(root, "d.spec", &behavior("zeta", ""));
    let update = session.update(SourceChange::Disk(&changed(&[
        "a.spec",
        "c.spec",
        "d.spec",
        "never.spec",
    ])));

    // invalidation_set_computed, subgraph_invalidated_emitted: exactly the
    // changed files; unrelated_files_untouched: types.spec and main.spec
    // are not among them.
    assert_eq!(update.rebuilt_files, ["a.spec", "c.spec", "d.spec"]);
    assert_eq!(update.verification, Some(Ok(())));
    assert_eq!(ids(&update.delta.added_nodes), ["zeta"]);
    assert_eq!(ids(&update.delta.removed_nodes), ["delta"]);
    assert_matches_a_fresh_compile(&session, root);
}

#[specforge_test(
    behavior = "rebuild_affected_subgraph",
    verify = "stale nodes are removed"
)]
fn stale_nodes_are_removed() {
    let dir = three_files();
    let root = dir.path();
    let mut session = ProjectSession::open(&specforge_common::ProjectRoot::of(root).unwrap());
    write(root, "c.spec", &behavior("renamed", ""));

    let update = session.update(SourceChange::Disk(&changed(&["c.spec"])));

    assert!(session.project().graph().node("delta").is_none());
    assert_eq!(ids(&update.delta.removed_nodes), ["delta"]);
    assert_eq!(ids(&update.delta.added_nodes), ["renamed"]);
}

#[specforge_test(behavior = "rebuild_affected_subgraph", verify = "new nodes are added")]
fn new_nodes_are_added() {
    let dir = three_files();
    let root = dir.path();
    let mut session = ProjectSession::open(&specforge_common::ProjectRoot::of(root).unwrap());
    write(
        root,
        "c.spec",
        &format!(
            "{}{}",
            behavior("delta", ""),
            behavior("extra", "  invariants [delta]\n")
        ),
    );

    let update = session.update(SourceChange::Disk(&changed(&["c.spec"])));

    assert!(session.project().graph().node("extra").is_some());
    assert_eq!(ids(&update.delta.added_nodes), ["extra"]);
    assert_eq!(update.delta.added_edges.len(), 1);
    assert!(update.delta.modified_nodes.is_empty(), "{:?}", update.delta);
}

/// The editor's path: each keystroke is a buffer, never read from disk.
/// Growing, shrinking and breaking the text, the session stays what a
/// fresh compile of the same texts builds, and every delta is the full
/// comparison's.
#[specforge_test(
    behavior = "rebuild_affected_subgraph",
    verify = "incremental rebuild equals cold rebuild"
)]
fn buffer_edits_leave_what_a_fresh_compile_builds() {
    let dir = three_files();
    let root = dir.path();
    let mut session = ProjectSession::open(&specforge_common::ProjectRoot::of(root).unwrap());
    session.set_verify_incremental(true);

    let texts = [
        behavior("beta", "  invariants [alpha, gamma]\n"),
        behavior("beta", ""),
        "behavior beta \"beta\" {\n  category comm".to_string(),
        format!("{}{}", behavior("beta", ""), behavior("alpha", "")),
        String::new(),
        behavior("beta", "  invariants [alpha]\n"),
    ];
    for text in &texts {
        let update = session.update(SourceChange::Hold(&[Buffer::new(
            root.join("a.spec"),
            text,
        )]));
        assert_eq!(update.rebuilt_files, ["a.spec"]);
        assert_eq!(update.verification, Some(Ok(())), "{text}");
        write(root, "a.spec", text);
        assert_matches_a_fresh_compile(&session, root);
    }

    // A buffer that is gone takes its file's entities with it. (Whether an
    // import's target exists is read from disk, as `check` reads it.)
    fs::remove_file(root.join("a.spec")).unwrap();
    let update = session
        .release(&[root.join("a.spec")])
        .expect("the file is gone on disk: the parse leaves");
    assert_eq!(ids(&update.delta.removed_nodes), ["beta"]);
    assert_matches_a_fresh_compile(&session, root);
}

/// A span of the graph is a position in the text the build parsed, which
/// the session keeps: the file as read by the cold build, the buffer as
/// given by an update (whatever the disk says now), gone with the file.
#[test]
fn the_session_keeps_the_text_each_file_was_built_from() {
    let dir = three_files();
    let root = dir.path();
    let mut session = ProjectSession::open(&specforge_common::ProjectRoot::of(root).unwrap());
    let on_disk = fs::read_to_string(root.join("a.spec")).unwrap();
    assert_eq!(
        session.project().source_text("a.spec").as_deref(),
        Some(&*on_disk)
    );
    assert_eq!(session.project().source_text("nope.spec"), None);
    assert_eq!(
        session.project().source_texts().len(),
        session.project().file_count()
    );

    // A buffer the disk does not hold yet: the build, and so the text, is
    // the buffer's.
    let buffer = behavior("beta", "  invariants [alpha]\n");
    session.update(SourceChange::Hold(&[Buffer::new(
        root.join("a.spec"),
        buffer.clone(),
    )]));
    assert_eq!(
        session.project().source_text("a.spec").as_deref(),
        Some(&*buffer)
    );
    assert_eq!(
        fs::read_to_string(root.join("a.spec")).unwrap(),
        on_disk,
        "the disk is untouched"
    );
    // The copy a reader keeps while the session is out for an update.
    let kept = session.project().source_texts();
    assert_eq!(kept.get("a.spec").map(|t| &**t), Some(&*buffer));

    fs::remove_file(root.join("a.spec")).unwrap();
    session.release(&[root.join("a.spec")]);
    assert_eq!(session.project().source_text("a.spec"), None);
    assert!(kept.contains_key("a.spec"), "a kept copy does not change");
}

/// The editor applied an edit to two files at once (a rename): one update,
/// one run of the checks, and never the diagnostics of a half-applied edit.
#[specforge_test(
    behavior = "invalidate_changed_files",
    verify = "several editor buffers changed at once are one update"
)]
fn several_buffers_are_one_update() {
    let dir = project(
        CONFIG,
        &[
            ("a.spec", &behavior("alpha", "")),
            ("b.spec", &behavior("beta", "  invariants [alpha]\n")),
        ],
    );
    let root = dir.path();
    let mut session = ProjectSession::open(&specforge_common::ProjectRoot::of(root).unwrap());
    session.set_verify_incremental(true);

    let buffers = [
        Buffer::new(root.join("a.spec"), behavior("omega", "")),
        Buffer::new(
            root.join("b.spec"),
            behavior("beta", "  invariants [omega]\n"),
        ),
    ];
    let update = session.update_with(SourceChange::Hold(&buffers), CheckMode::Full);
    assert_eq!(update.rebuilt_files, ["a.spec", "b.spec"]);
    assert_eq!(update.verification, Some(Ok(())));
    assert!(
        !update.diagnostics.iter().any(|d| d.code == "E003"),
        "no state in which beta names a missing alpha: {:?}",
        update.diagnostics
    );
    // The disk does not hold the buffers; once it does, a fresh compile of
    // it is what the session reports (codes and spans).
    for buffer in &buffers {
        fs::write(&buffer.path, &buffer.text).unwrap();
    }
    assert_matches_a_fresh_compile(&session, root);

    // Files discovery would not find are held and build nothing.
    let ignored = [Buffer::new(root.join("notes.txt"), "x")];
    let update = session.update(SourceChange::Hold(&ignored));
    assert!(update.rebuilt_files.is_empty());
    assert_matches_a_fresh_compile(&session, root);
}

#[specforge_test(
    behavior = "invalidate_changed_files",
    verify = "the typing fast path skips the checks while any edited buffer does not parse"
)]
fn a_parse_error_in_any_edited_buffer_skips_the_checks() {
    let dir = project(
        CONFIG,
        &[
            ("a.spec", &behavior("alpha", "")),
            ("b.spec", &behavior("beta", "")),
        ],
    );
    let root = dir.path();
    let mut session = ProjectSession::open(&specforge_common::ProjectRoot::of(root).unwrap());
    let buffers = [
        Buffer::new(root.join("a.spec"), behavior("alpha", "")),
        Buffer::new(
            root.join("b.spec"),
            "behavior beta \"B\" {\n  contract \"\n",
        ),
    ];
    let full = session.update_with(SourceChange::Hold(&buffers), CheckMode::Full);

    // The same buffers again change nothing: b.spec gets one more line, still
    // unclosed.
    let more = [
        buffers[0].clone(),
        Buffer::new(
            root.join("b.spec"),
            "behavior beta \"B\" {\n  contract \"\n\n",
        ),
    ];
    let fast = session.update_with(
        SourceChange::Hold(&more),
        CheckMode::SyntaxOnlyIfParseErrors,
    );
    let codes: Vec<&str> = fast.diagnostics.iter().map(|d| d.code.as_str()).collect();
    assert!(
        codes.contains(&"E001"),
        "the parse error is reported: {codes:?}"
    );
    assert!(
        fast.diagnostics.len() < full.diagnostics.len(),
        "the checks are skipped: {} against {}",
        fast.diagnostics.len(),
        full.diagnostics.len()
    );

    // Every edited buffer parses: the checks run.
    let parses = [
        Buffer::new(root.join("a.spec"), behavior("alpha", "")),
        Buffer::new(root.join("b.spec"), behavior("beta", "")),
    ];
    let update = session.update_with(
        SourceChange::Hold(&parses),
        CheckMode::SyntaxOnlyIfParseErrors,
    );
    assert!(!update.diagnostics.iter().any(|d| d.code == "E001"));
}

#[specforge_test(
    behavior = "rebuild_affected_subgraph",
    verify = "debug --verify-incremental performs cold rebuild comparison"
)]
fn verify_incremental_compares_each_update_with_a_cold_rebuild() {
    let dir = three_files();
    let root = dir.path();
    let mut session = ProjectSession::open(&specforge_common::ProjectRoot::of(root).unwrap());
    session.set_verify_incremental(false);
    write(root, "c.spec", &behavior("renamed", ""));
    let update = session.update(SourceChange::Disk(&changed(&["c.spec"])));
    assert_eq!(update.verification, None, "off when turned off");

    session.set_verify_incremental(true);
    write(root, "c.spec", &behavior("delta", ""));
    let update = session.update(SourceChange::Disk(&changed(&["c.spec"])));
    assert_eq!(update.verification, Some(Ok(())));
}

#[specforge_test(
    behavior = "rebuild_affected_subgraph",
    verify = "Rebuild Affected Subgraph: affected subgraph rebuild holds — subgraph_invalidated, import_dag_updated, graph_reflects_reparse, stale_removed, new_added, rebuild_event_fired, unaffected_subgraph_intact"
)]
fn rebuild_affected_subgraph_contract() {
    let dir = three_files();
    let root = dir.path();
    let mut session = ProjectSession::open(&specforge_common::ProjectRoot::of(root).unwrap());
    session.set_verify_incremental(true);
    let untouched = |session: &ProjectSession| {
        let gamma = session.project().graph().node("gamma").unwrap();
        (gamma.title.clone(), gamma.source_span.clone())
    };
    let before = untouched(&session);

    write(root, "types.spec", &behavior("omega", ""));
    let update = session.update(SourceChange::Disk(&changed(&["types.spec"])));

    // stale_removed, new_added, graph_reflects_reparse.
    assert!(session.project().graph().node("alpha").is_none());
    assert!(session.project().graph().node("omega").is_some());
    // rebuild_event_fired: the update says what it rebuilt and changed.
    assert_eq!(update.rebuilt_files, ["types.spec"]);
    assert_eq!(ids(&update.delta.added_nodes), ["omega"]);
    assert_eq!(ids(&update.delta.removed_nodes), ["alpha"]);
    // unaffected_subgraph_intact.
    assert_eq!(untouched(&session), before);
    assert_eq!(update.verification, Some(Ok(())));
    assert_matches_a_fresh_compile(&session, root);
}

#[specforge_test(
    behavior = "emit_incremental_diagnostics",
    verify = "diagnostics from changed files are refreshed"
)]
fn diagnostics_from_changed_files_are_refreshed() {
    let dir = three_files();
    let root = dir.path();
    let mut session = ProjectSession::open(&specforge_common::ProjectRoot::of(root).unwrap());
    write(
        root,
        "c.spec",
        &behavior("delta", "  invariants [nowhere]\n"),
    );

    let update = session.update(SourceChange::Disk(&changed(&["c.spec"])));

    assert!(
        update
            .diagnostics_delta()
            .added
            .0
            .iter()
            .any(|d| d.code == "E003" && d.span.as_ref().is_some_and(|s| s.file == "c.spec"))
    );
    assert!(
        in_file(&session.project().diagnostics(), "c.spec")
            .iter()
            .any(|d| d.code == "E003")
    );
}

#[specforge_test(
    behavior = "emit_incremental_diagnostics",
    verify = "diagnostics from unchanged files are preserved"
)]
fn diagnostics_from_unchanged_files_are_preserved() {
    let dir = three_files();
    let root = dir.path();
    write(
        root,
        "main.spec",
        &behavior("gamma", "  invariants [nowhere]\n"),
    );
    let mut session = ProjectSession::open(&specforge_common::ProjectRoot::of(root).unwrap());
    let before = in_file(&session.project().diagnostics(), "main.spec");
    assert!(before.iter().any(|d| d.code == "E003"));

    write(
        root,
        "c.spec",
        &behavior("delta", "  invariants [also_nowhere]\n"),
    );
    let update = session.update(SourceChange::Disk(&changed(&["c.spec"])));

    assert_eq!(
        in_file(&session.project().diagnostics(), "main.spec"),
        before
    );
    let delta = update.diagnostics_delta();
    assert!(
        delta
            .added
            .0
            .iter()
            .chain(&delta.removed.0)
            .all(|d| d.span.as_ref().is_none_or(|s| s.file != "main.spec")),
        "{delta:?}"
    );
}

/// No extension runs here: the time is the rebuild's and the import
/// resolution's, not an extension's checks.
#[specforge_test(
    behavior = "emit_incremental_diagnostics",
    verify = "file change to diagnostics emitted within 100ms"
)]
fn file_change_to_diagnostics_within_100ms() {
    let dir = three_files();
    let root = dir.path();
    let mut session =
        ProjectSession::open_with_runtime(&specforge_common::ProjectRoot::of(root).unwrap(), None);
    write(
        root,
        "c.spec",
        &behavior("delta", "  invariants [nowhere]\n"),
    );

    let start = std::time::Instant::now();
    let update = session.update(SourceChange::Disk(&changed(&["c.spec"])));
    let elapsed = start.elapsed();

    assert!(elapsed.as_millis() < 100, "took {}ms", elapsed.as_millis());
    assert!(update.diagnostics.iter().any(|d| d.code == "E003"));
}

#[specforge_test(
    behavior = "emit_incremental_diagnostics",
    verify = "Emit Incremental Diagnostics: incremental diagnostics holds for the declared obligations"
)]
fn emit_incremental_diagnostics_contract() {
    let dir = three_files();
    let root = dir.path();
    write(
        root,
        "main.spec",
        &behavior("gamma", "  invariants [nowhere]\n"),
    );
    let mut session = ProjectSession::open(&specforge_common::ProjectRoot::of(root).unwrap());
    let main_before = in_file(&session.project().diagnostics(), "main.spec");

    write(
        root,
        "c.spec",
        &behavior("delta", "  invariants [also_nowhere]\n"),
    );
    let update = session.update(SourceChange::Disk(&changed(&["c.spec"])));

    // Refreshed for the changed file, preserved for the others, and the
    // whole set is a fresh compile's.
    let delta = update.diagnostics_delta();
    assert!(!delta.added.0.is_empty(), "{delta:?}");
    assert!(
        delta
            .added
            .0
            .iter()
            .chain(&delta.removed.0)
            .all(|d| d.span.as_ref().is_some_and(|s| s.file == "c.spec")),
        "{delta:?}"
    );
    assert_eq!(
        in_file(&session.project().diagnostics(), "main.spec"),
        main_before
    );
    assert_eq!(update.diagnostics[..], session.project().diagnostics()[..]);
    assert_matches_a_fresh_compile(&session, root);
}

#[specforge_test(
    behavior = "validate_delta_correctness",
    verify = "check disabled in release builds"
)]
fn the_delta_check_runs_only_when_asked_for() {
    let dir = three_files();
    let root = dir.path();
    let mut session = ProjectSession::open(&specforge_common::ProjectRoot::of(root).unwrap());
    // A release build verifies only when asked (`--verify-incremental`).
    session.set_verify_incremental(false);
    write(root, "c.spec", &behavior("renamed", ""));
    let update = session.update(SourceChange::Disk(&changed(&["c.spec"])));
    assert_eq!(update.rebuilt_files, ["c.spec"], "the rebuild itself ran");
    assert_eq!(update.verification, None);
}

#[specforge_test(
    behavior = "validate_delta_correctness",
    verify = "a rebuild that passes the check is reported as passed"
)]
fn a_rebuild_that_passes_the_check_is_reported_as_passed() {
    let dir = three_files();
    let root = dir.path();
    let mut session = ProjectSession::open(&specforge_common::ProjectRoot::of(root).unwrap());
    session.set_verify_incremental(true);
    // Edges change outside the edited file: beta's reference to alpha
    // breaks, so beta is modified through its edges.
    write(root, "types.spec", &behavior("omega", ""));
    let update = session.update(SourceChange::Disk(&changed(&["types.spec"])));
    assert_eq!(update.verification, Some(Ok(())));
    let beta = update
        .delta
        .modified_nodes
        .iter()
        .find(|n| n.id == "beta")
        .expect("beta lost its edge");
    assert!(beta.changed_fields.is_empty());
    assert_eq!(beta.changed_aspects, [specforge_project::NodeAspect::Edges]);
}

#[specforge_test(
    behavior = "notify_delta_subscribers",
    verify = "an update reports its delta and the files it affects"
)]
fn an_update_reports_its_delta_and_the_files_it_affects() {
    let dir = three_files();
    let root = dir.path();
    let mut session = ProjectSession::open(&specforge_common::ProjectRoot::of(root).unwrap());
    write(root, "types.spec", &behavior("omega", ""));

    let update = session.update(SourceChange::Disk(&changed(&["types.spec"])));

    assert_eq!(ids(&update.delta.added_nodes), ["omega"]);
    assert_eq!(ids(&update.delta.removed_nodes), ["alpha"]);
    // beta (a.spec) lost its edge to alpha.
    assert_eq!(update.delta.affected_files, ["a.spec", "types.spec"]);
}

#[specforge_test(
    behavior = "notify_delta_subscribers",
    verify = "Notify Delta Subscribers: delta reporting holds — graph_delta_computed_fired, affected_files_delivered, delta_subscribers_notified_emitted"
)]
fn notify_delta_subscribers_contract() {
    let dir = three_files();
    let root = dir.path();
    let mut session = ProjectSession::open(&specforge_common::ProjectRoot::of(root).unwrap());

    // An update that changes nothing reports an empty delta.
    let update = session.update(SourceChange::Disk(&changed(&["c.spec"])));
    assert!(update.delta.is_empty(), "{:?}", update.delta);

    // A reload reports the delta between the two projects it replaces.
    write(root, "c.spec", &behavior("renamed", ""));
    let update = session.reload_environment();
    assert_eq!(ids(&update.delta.added_nodes), ["renamed"]);
    assert_eq!(ids(&update.delta.removed_nodes), ["delta"]);
    assert_eq!(update.delta.affected_files, ["c.spec"]);
    assert_eq!(update.rebuilt_files.len(), 4);
}

/// A small deterministic generator (xorshift), so a failing sequence
/// replays from its seed.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }

    fn pick<'a>(&mut self, items: &[&'a str]) -> &'a str {
        items[self.below(items.len())]
    }
}

/// A random `.spec` text: imports of every kind (relative, bare, index,
/// missing, above the spec root) and entities whose IDs collide across
/// files and kinds and whose references may dangle.
fn random_spec(rng: &mut Rng) -> String {
    const IMPORTS: &[&str] = &[
        "a",
        "b.spec",
        "sub",
        "sub/c",
        "./c",
        "../a",
        "missing",
        "../../outside",
        "drafts/d",
    ];
    const BEHAVIORS: &[&str] = &["b0", "b1", "b2", "b3", "i1"];
    const INVARIANTS: &[&str] = &["i0", "i1", "i2", "b1"];
    let mut text = String::new();
    for _ in 0..rng.below(3) {
        text.push_str(&format!("use \"{}\"\n", rng.pick(IMPORTS)));
    }
    for n in 0..1 + rng.below(3) {
        if rng.below(3) == 0 {
            let id = rng.pick(INVARIANTS);
            text.push_str(&format!(
                "\ninvariant {id} \"I{n}\" {{\n  guarantee \"The system MUST {id}\"\n  risk low\n}}\n"
            ));
        } else {
            let id = rng.pick(BEHAVIORS);
            let refs = [rng.pick(INVARIANTS), rng.pick(&["i0", "i2", "gone"])].join(", ");
            text.push_str(&format!(
                "\nbehavior {id} \"B{n}\" {{\n  category command\n  invariants [{refs}]\n  contract \"The system MUST {id} {n}\"\n}}\n"
            ));
        }
    }
    if rng.below(8) == 0 {
        text.push_str("\nbehavior broken \"Broken\" {\n");
    }
    text
}

/// Random sequences of edits, creations, deletions, renames (a delete and
/// an add in one batch) and editor buffers, over nested, excluded and
/// never-discovered (`build/`) paths: after each update the session holds
/// what a fresh compile of the disk builds, and its delta is the full one.
#[specforge_test(
    invariant = "incremental_correctness",
    verify = "incremental recompilation produces the same graph as a full rebuild"
)]
fn random_updates_leave_what_a_fresh_compile_builds() {
    const PATHS: &[&str] = &[
        "a.spec",
        "b.spec",
        "sub/c.spec",
        "sub/index.spec",
        "sub/deep/e.spec",
        "drafts/d.spec",
        "build/f.spec",
    ];
    let config = r#"{"name":"s","version":"0.1.0","extensions":["@specforge/software","@specforge/testing"],"spec_root":"spec","exclude":["drafts/"]}"#;
    for seed in [
        0x9E37_79B9_7F4A_7C15_u64,
        0xD1B5_4A32_D192_ED03,
        0x2545_F491_4F6C_DD1D,
    ] {
        let mut rng = Rng(seed);
        let dir = project(config, &[]);
        let root = dir.path();
        let spec = root.join("spec");
        fs::write(root.join("outside.spec"), "behavior outside \"O\" {\n}\n").unwrap();
        for path in &PATHS[..3] {
            write(&spec, path, &random_spec(&mut rng));
        }
        let mut session = ProjectSession::open(&specforge_common::ProjectRoot::of(root).unwrap());
        session.set_verify_incremental(true);
        assert_matches_a_fresh_compile(&session, root);

        for step in 0..30 {
            let mut touched: Vec<String> = Vec::new();
            let mut buffer: Option<(String, String)> = None;
            match rng.below(5) {
                // Rename: one file moves to another path in the same batch.
                0 => {
                    let (from, to) = (rng.pick(PATHS), rng.pick(PATHS));
                    if from != to && spec.join(from).is_file() {
                        let text = fs::read_to_string(spec.join(from)).unwrap();
                        fs::remove_file(spec.join(from)).unwrap();
                        write(&spec, to, &text);
                        touched.extend([from.to_string(), to.to_string()]);
                    }
                }
                1 => {
                    let path = rng.pick(PATHS);
                    if spec.join(path).is_file() {
                        fs::remove_file(spec.join(path)).unwrap();
                        touched.push(path.to_string());
                    }
                }
                // An editor buffer, saved so the fresh compile sees it.
                2 => {
                    let path = rng.pick(PATHS);
                    let text = random_spec(&mut rng);
                    write(&spec, path, &text);
                    buffer = Some((path.to_string(), text));
                }
                _ => {
                    for _ in 0..1 + rng.below(3) {
                        let path = rng.pick(PATHS);
                        write(&spec, path, &random_spec(&mut rng));
                        touched.push(path.to_string());
                    }
                }
            }
            let previous = session.project().graph().clone();
            let update = match &buffer {
                Some((path, text)) => {
                    let update = session.update(SourceChange::Hold(&[Buffer::new(
                        spec.join(path),
                        text.clone(),
                    )]));
                    // Saved, so released to nothing: the next disk steps apply.
                    assert!(session.release(&[spec.join(path)]).is_none());
                    update
                }
                None => session.update(SourceChange::Disk(&touched)),
            };
            let context = format!("seed {seed:#x} step {step}");
            assert!(
                matches!(update.verification, None | Some(Ok(()))),
                "{context}: {:?}",
                update.verification
            );
            assert_eq!(
                update.delta,
                specforge_project::compute_graph_delta(&previous, session.project().graph()),
                "{context}"
            );
            let runtime = Arc::new(specforge_component::ComponentRuntime::with_user_cache());
            let fresh = CompiledProject::compile(
                &specforge_common::ProjectRoot::of(root).unwrap(),
                Some(runtime.clone()),
            );
            assert_eq!(
                graph_contents(session.project().graph()),
                graph_contents(fresh.graph()),
                "{context}"
            );
            assert_eq!(
                session.project().diagnostics(),
                fresh.diagnostics(),
                "{context}"
            );
        }
    }
}

#[specforge_test(
    behavior = "emit_incremental_diagnostics",
    verify = "an update reports the diagnostics it added and removed"
)]
fn an_update_reports_the_diagnostics_it_added_and_removed() {
    let dir = three_files();
    let root = dir.path();
    let mut session = ProjectSession::open(&specforge_common::ProjectRoot::of(root).unwrap());
    let broken = behavior("delta", "  invariants [nowhere]\n");
    let e003 = |delta: &[Diagnostic]| -> Vec<(String, usize)> {
        delta
            .iter()
            .filter(|d| d.code == "E003")
            .map(|d| {
                let span = d.span.as_ref().unwrap();
                (span.file.as_str().to_string(), span.start_line)
            })
            .collect()
    };

    write(root, "c.spec", &broken);
    let update = session.update(SourceChange::Disk(&changed(&["c.spec"])));
    let delta = update.diagnostics_delta();
    let added = e003(&delta.added.0);
    assert_eq!(added.len(), 1, "{:?}", delta.added);
    assert_eq!(added[0].0, "c.spec");
    assert!(e003(&delta.removed.0).is_empty(), "{:?}", delta.removed);

    // The same error two lines lower: removed where it was, added where it is.
    write(root, "c.spec", &format!("\n\n{broken}"));
    let update = session.update(SourceChange::Disk(&changed(&["c.spec"])));
    let delta = update.diagnostics_delta();
    let (removed, added_now) = (e003(&delta.removed.0), e003(&delta.added.0));
    assert_eq!(removed.len(), 1, "{delta:?}");
    assert_eq!(added_now.len(), 1, "{delta:?}");
    assert_eq!(added_now[0].1, removed[0].1 + 2);

    // Written again unchanged: nothing is added or removed.
    write(root, "c.spec", &format!("\n\n{broken}"));
    let update = session.update(SourceChange::Disk(&changed(&["c.spec"])));
    assert!(update.diagnostics_delta().is_empty());
}

#[test]
fn a_reload_and_a_recheck_report_their_delta() {
    let dir = three_files();
    let root = dir.path();
    let mut session = ProjectSession::open(&specforge_common::ProjectRoot::of(root).unwrap());
    let update = session.reload_environment();
    assert_eq!(update.kind, UpdateKind::Environment);
    assert!(update.diagnostics_delta().is_empty());
    assert_eq!(update.previous_diagnostics[..], update.diagnostics[..]);

    // A session with no project reloads nothing: one list, held twice.
    let mut detached = ProjectSession::detached();
    let update = detached.reload_environment();
    assert!(update.diagnostics_delta().is_empty());
    assert!(Arc::ptr_eq(
        &update.previous_diagnostics,
        &update.diagnostics
    ));
}

const CYCLE: &str = "behavior alpha \"A\" {\n  depends_on [beta]\n}\nbehavior beta \"B\" {\n  depends_on [alpha]\n}\n";

#[specforge_test(
    behavior = "emit_incremental_diagnostics",
    verify = "a diagnostic without a span is unchanged when only its entities move"
)]
fn a_diagnostic_without_a_span_is_unchanged_when_only_its_entities_move() {
    let dir = project(
        r#"{"name":"c","extensions":["@specforge/software"]}"#,
        &[("cycle.spec", CYCLE)],
    );
    let root = dir.path();
    let mut session = ProjectSession::open(&specforge_common::ProjectRoot::of(root).unwrap());
    let w061 = |diagnostics: &[Diagnostic]| diagnostics.iter().filter(|d| d.code == "W061").count();
    assert_eq!(w061(&session.project().diagnostics()), 1);

    write(root, "cycle.spec", &format!("\n\n{CYCLE}"));
    let update = session.update(SourceChange::Disk(&changed(&["cycle.spec"])));
    assert_eq!(w061(&update.diagnostics), 1, "still reported");
    let delta = update.diagnostics_delta();
    assert_eq!(w061(&delta.added.0), 0, "{delta:?}");
    assert_eq!(w061(&delta.removed.0), 0, "{delta:?}");
}

/// `update`'s delta takes what the session reported before it (`previous`)
/// to what it reports now: counting `previous` by diagnostic, taking away
/// each removed and adding each added gives the count of the current report.
fn assert_delta_takes(previous: &[Diagnostic], update: &specforge_project::Update, context: &str) {
    let mut counts: std::collections::HashMap<&Diagnostic, i64> = std::collections::HashMap::new();
    for d in previous {
        *counts.entry(d).or_default() += 1;
    }
    let delta = update.diagnostics_delta();
    for d in &delta.removed.0 {
        *counts.entry(d).or_default() -= 1;
    }
    for d in &delta.added.0 {
        *counts.entry(d).or_default() += 1;
    }
    let mut now: std::collections::HashMap<&Diagnostic, i64> = std::collections::HashMap::new();
    for d in update.diagnostics.iter() {
        *now.entry(d).or_default() += 1;
    }
    counts.retain(|_, n| *n != 0);
    assert_eq!(
        counts, now,
        "{context}: the delta takes the report to the next"
    );
}

/// Random sequences of every kind of update a surface applies: disk edits,
/// creations, deletions and renames named by path; one editor buffer;
/// several buffers as one update; disk writes caught up by `ensure_fresh`;
/// a `specforge.json` change caught up the same way (an environment
/// reload); an explicit reload. After each update that runs the checks,
/// the session reports exactly what a fresh compile of the disk reports,
/// in the same order, and its graph's nodes come in the same order.
#[specforge_test(
    invariant = "incremental_correctness",
    verify = "a session reports what a fresh compile of the same sources reports, in the same order, after every update that runs the checks"
)]
fn a_session_reports_what_a_fresh_compile_reports_in_order() {
    const PATHS: &[&str] = &[
        "a.spec",
        "b.spec",
        "m.spec",
        "z.spec",
        "sub/c.spec",
        "sub/index.spec",
        "sub/deep/e.spec",
        "drafts/d.spec",
        "build/f.spec",
    ];
    let config = |excluding: bool| {
        let exclude: Vec<&str> = if excluding {
            vec!["drafts/"]
        } else {
            Vec::new()
        };
        serde_json::json!({
            "name": "s", "version": "0.1.0",
            "extensions": ["@specforge/software", "@specforge/testing"],
            "spec_root": "spec", "exclude": exclude,
        })
        .to_string()
    };
    let node_ids = |graph: &Graph| -> Vec<String> {
        graph.nodes().iter().map(|n| n.id.raw.to_string()).collect()
    };
    for seed in [
        0x9E37_79B9_7F4A_7C15_u64,
        0xD1B5_4A32_D192_ED03,
        0x2545_F491_4F6C_DD1D,
        0x0F0F_F0F0_1357_9BDF,
    ] {
        let mut rng = Rng(seed);
        let mut excluding = true;
        let dir = project(&config(excluding), &[]);
        let root = dir.path();
        let spec = root.join("spec");
        fs::write(root.join("outside.spec"), "behavior outside \"O\" {\n}\n").unwrap();
        for path in &PATHS[..3] {
            write(&spec, path, &random_spec(&mut rng));
        }
        let runtime = Arc::new(specforge_component::ComponentRuntime::with_user_cache());
        let mut session = ProjectSession::open(&specforge_common::ProjectRoot::of(root).unwrap());
        session.set_verify_incremental(true);

        for step in 0..30 {
            let previous = session.project().diagnostics();
            let update = match rng.below(7) {
                // A rename: one file moves to another path in one batch.
                0 => {
                    let (from, to) = (rng.pick(PATHS), rng.pick(PATHS));
                    if from == to || !spec.join(from).is_file() {
                        continue;
                    }
                    let text = fs::read_to_string(spec.join(from)).unwrap();
                    fs::remove_file(spec.join(from)).unwrap();
                    write(&spec, to, &text);
                    Some(session.update(SourceChange::Disk(&changed(&[from, to]))))
                }
                1 => {
                    let path = rng.pick(PATHS);
                    if !spec.join(path).is_file() {
                        continue;
                    }
                    fs::remove_file(spec.join(path)).unwrap();
                    Some(session.update(SourceChange::Disk(&changed(&[path]))))
                }
                // One editor buffer, saved so the fresh compile sees it.
                2 => {
                    let (path, text) = (rng.pick(PATHS), random_spec(&mut rng));
                    write(&spec, path, &text);
                    let update = session.update(SourceChange::Hold(&[Buffer::new(
                        spec.join(path),
                        text.clone(),
                    )]));
                    assert!(session.release(&[spec.join(path)]).is_none());
                    Some(update)
                }
                // Two buffers as one update, both saved.
                3 => {
                    let (first, second) = (rng.pick(PATHS), rng.pick(PATHS));
                    let mut buffers = vec![(first.to_string(), random_spec(&mut rng))];
                    if second != first {
                        buffers.push((second.to_string(), random_spec(&mut rng)));
                    }
                    for (path, text) in &buffers {
                        write(&spec, path, text);
                    }
                    let held: Vec<Buffer> = buffers
                        .iter()
                        .map(|(path, text)| Buffer::new(spec.join(path), text.clone()))
                        .collect();
                    let update = session.update(SourceChange::Hold(&held));
                    let paths: Vec<_> = held.iter().map(|b| b.path.clone()).collect();
                    assert!(session.release(&paths).is_none());
                    Some(update)
                }
                // Disk writes no path names: caught up by `ensure_fresh`.
                4 => {
                    for _ in 0..1 + rng.below(3) {
                        write(&spec, rng.pick(PATHS), &random_spec(&mut rng));
                    }
                    session.ensure_fresh()
                }
                // `exclude` toggled: an environment reload, caught up.
                5 => {
                    excluding = !excluding;
                    fs::write(root.join("specforge.json"), config(excluding)).unwrap();
                    session.ensure_fresh()
                }
                _ => Some(session.reload_environment()),
            };
            let Some(update) = update else { continue };
            let context = format!("seed {seed:#x} step {step}");
            assert!(
                matches!(update.verification, None | Some(Ok(()))),
                "{context}: {:?}",
                update.verification
            );
            assert_eq!(
                update.diagnostics[..],
                session.project().diagnostics()[..],
                "{context}"
            );
            assert_eq!(update.previous_diagnostics[..], previous[..], "{context}");
            assert_delta_takes(&previous, &update, &context);
            let fresh = CompiledProject::compile(
                &specforge_common::ProjectRoot::of(root).unwrap(),
                Some(runtime.clone()),
            );
            assert_eq!(
                session.project().diagnostics(),
                fresh.diagnostics(),
                "{context}"
            );
            assert_eq!(
                node_ids(session.project().graph()),
                node_ids(fresh.graph()),
                "{context}"
            );
            assert_eq!(
                graph_contents(session.project().graph()),
                graph_contents(fresh.graph()),
                "{context}"
            );
        }
    }
}

/// Bring `session` up to date with what is on disk, as MCP does before
/// every request: without a watcher, applying exactly what changed.
fn bring_up_to_date(session: &mut ProjectSession) {
    session.ensure_fresh().expect("something changed on disk");
}

/// Sources edited, created and deleted, the config rewritten and the lock
/// written: after each, a session brought up to date with disk is what a
/// fresh compile of the files on disk builds.
#[specforge_test(
    behavior = "bring_session_up_to_date",
    verify = "after bringing itself up to date a session matches a fresh compile"
)]
fn every_update_kind_leaves_what_a_fresh_compile_builds() {
    let dir = project(
        CONFIG,
        &[
            (
                "a.spec",
                "behavior alpha \"A\" {\n  category command\n  contract \"The system MUST a\"\n  verify unit \"a\"\n}\n",
            ),
            (
                "b.spec",
                "behavior beta \"B\" {\n  category command\n  contract \"The system MUST b\"\n}\n",
            ),
        ],
    );
    let root = dir.path();
    let mut session = ProjectSession::open(&specforge_common::ProjectRoot::of(root).unwrap());
    session.set_verify_incremental(true);
    assert_matches_a_fresh_compile(&session, root);

    // An edit that leaves a reference dangling.
    write(
        root,
        "a.spec",
        "behavior alpha \"A\" {\n  category command\n  contract \"The system MUST a\"\n  invariants [missing]\n}\n",
    );
    bring_up_to_date(&mut session);
    assert_matches_a_fresh_compile(&session, root);

    // A file created, another deleted, in one go.
    write(
        root,
        "nested/c.spec",
        "behavior gamma \"C\" {\n  category command\n  contract \"The system MUST c\"\n}\n",
    );
    fs::remove_file(root.join("b.spec")).unwrap();
    bring_up_to_date(&mut session);
    assert_matches_a_fresh_compile(&session, root);
    assert!(session.project().graph().node("gamma").is_some());
    assert!(session.project().graph().node("beta").is_none());

    // The config enables an extension that is not installed: E028, no
    // lock entry.
    let config = r#"{"name":"s","version":"0.1.0","extensions":["@specforge/software","@specforge/testing","@acme/missing"]}"#;
    fs::write(root.join("specforge.json"), config).unwrap();
    bring_up_to_date(&mut session);
    assert_matches_a_fresh_compile(&session, root);
    let e028 = |session: &ProjectSession| -> Vec<String> {
        session
            .project()
            .diagnostics()
            .iter()
            .filter(|d| d.code == "E028")
            .map(|d| d.message.clone())
            .collect()
    };
    let unlocked = e028(&session);
    assert_eq!(unlocked.len(), 1, "{unlocked:?}");

    // The lock now names it: the environment reads the lock, so the E028
    // becomes "binary not found".
    let lock = serde_json::json!({
        "lockfile_version": 1,
        "entries": [{"name": "@acme/missing", "version": "1.0.0", "source": "local:missing.wasm", "wasm_hash": "sha256:00"}]
    });
    fs::write(root.join("specforge.lock"), lock.to_string()).unwrap();
    bring_up_to_date(&mut session);
    assert_matches_a_fresh_compile(&session, root);
    let locked = e028(&session);
    assert_eq!(locked.len(), 1, "{locked:?}");
    assert_ne!(
        locked, unlocked,
        "the lock changed what the environment loads"
    );
}

/// Set the modification time of every file under `root` `age` in the
/// past, so none is racy when a session stamps it.
fn age_files(root: &Path, age: std::time::Duration) {
    let then = std::time::SystemTime::now() - age;
    for entry in walk(root) {
        fs::File::options()
            .write(true)
            .open(&entry)
            .unwrap()
            .set_modified(then)
            .unwrap();
    }
}

fn walk(dir: &Path) -> Vec<std::path::PathBuf> {
    let mut files = Vec::new();
    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            files.extend(walk(&path));
        } else {
            files.push(path);
        }
    }
    files
}

#[specforge_test(
    behavior = "bring_session_up_to_date",
    verify = "an up-to-date session reports no change and re-parses nothing"
)]
fn an_up_to_date_session_changes_nothing() {
    let dir = project(
        CONFIG,
        &[
            ("a.spec", "behavior alpha \"A\" {\n  category command\n}\n"),
            (
                "nested/b.spec",
                "behavior beta \"B\" {\n  category command\n}\n",
            ),
        ],
    );
    age_files(dir.path(), std::time::Duration::from_secs(10));
    let mut session = ProjectSession::open(&specforge_common::ProjectRoot::of(dir.path()).unwrap());
    let before = graph_contents(session.project().graph());

    assert!(session.stale().is_empty(), "{:?}", session.stale());
    assert!(session.ensure_fresh().is_none());
    assert_eq!(graph_contents(session.project().graph()), before);
}

/// Files written just now are racy (their stamp alone cannot be trusted):
/// unchanged, they are still not reported.
#[test]
fn an_unchanged_racy_file_is_not_reported() {
    let dir = project(CONFIG, &[("a.spec", "behavior alpha \"A\" {\n}\n")]);
    let mut session = ProjectSession::open(&specforge_common::ProjectRoot::of(dir.path()).unwrap());
    assert!(session.stale().is_empty(), "{:?}", session.stale());
    assert!(session.ensure_fresh().is_none());
}

#[specforge_test(
    behavior = "bring_session_up_to_date",
    verify = "edits, creations and deletions since the last build are applied as one update"
)]
fn edits_creations_and_deletions_apply_as_one_update() {
    let dir = project(
        CONFIG,
        &[
            ("a.spec", "behavior alpha \"A\" {\n  category command\n}\n"),
            ("b.spec", "behavior beta \"B\" {\n  category command\n}\n"),
        ],
    );
    let root = dir.path();
    let mut session = ProjectSession::open(&specforge_common::ProjectRoot::of(root).unwrap());
    session.set_verify_incremental(true);

    write(
        root,
        "a.spec",
        "behavior alpha \"A, edited\" {\n  category command\n}\n",
    );
    write(
        root,
        "c.spec",
        "behavior gamma \"C\" {\n  category command\n}\n",
    );
    fs::remove_file(root.join("b.spec")).unwrap();

    let stale = session.stale();
    assert_eq!(stale.sources, vec!["a.spec", "b.spec", "c.spec"]);
    assert!(stale.environment.is_empty() && stale.check_inputs.is_empty());
    let update = session.ensure_fresh().expect("three files changed");
    assert_eq!(update.kind, specforge_project::UpdateKind::Sources);
    assert_eq!(update.rebuilt_files, vec!["a.spec", "b.spec", "c.spec"]);
    assert_eq!(update.verification, Some(Ok(())));
    assert_matches_a_fresh_compile(&session, root);

    // Applied: nothing is left to apply.
    assert!(session.stale().is_empty(), "{:?}", session.stale());
    assert!(session.ensure_fresh().is_none());
}

#[specforge_test(
    behavior = "bring_session_up_to_date",
    verify = "a file rewritten within the timestamp granularity of the last build is still seen"
)]
fn a_racy_rewrite_is_still_seen() {
    let dir = project(CONFIG, &[("a.spec", "behavior alpha \"A\" {\n}\n")]);
    let root = dir.path();
    let path = root.join("a.spec");
    let stamp = fs::metadata(&path).unwrap().modified().unwrap();
    let mut session = ProjectSession::open(&specforge_common::ProjectRoot::of(root).unwrap());

    // Same length, same modification time: only the content tells.
    fs::write(&path, "behavior omega \"A\" {\n}\n").unwrap();
    fs::File::options()
        .write(true)
        .open(&path)
        .unwrap()
        .set_modified(stamp)
        .unwrap();
    assert_eq!(fs::metadata(&path).unwrap().modified().unwrap(), stamp);

    assert_eq!(session.stale().sources, vec!["a.spec"]);
    session.ensure_fresh().expect("the rewrite is seen");
    assert!(session.project().graph().node("omega").is_some());
    assert!(session.project().graph().node("alpha").is_none());
    assert_matches_a_fresh_compile(&session, root);
}

#[specforge_test(
    behavior = "bring_session_up_to_date",
    verify = "a specforge.lock change reloads the environment"
)]
fn a_lock_change_reloads_the_environment() {
    let config =
        r#"{"name":"s","version":"0.1.0","extensions":["@specforge/software","@acme/missing"]}"#;
    let dir = project(config, &[("a.spec", "behavior alpha \"A\" {\n}\n")]);
    let root = dir.path();
    let mut session = ProjectSession::open(&specforge_common::ProjectRoot::of(root).unwrap());
    let e028 = |session: &ProjectSession| -> Vec<String> {
        session
            .project()
            .diagnostics()
            .iter()
            .filter(|d| d.code == "E028")
            .map(|d| d.message.clone())
            .collect()
    };
    let unlocked = e028(&session);
    assert!(
        unlocked[0].contains("no specforge.lock entry"),
        "{unlocked:?}"
    );

    let lock = serde_json::json!({
        "lockfile_version": 1,
        "entries": [{"name": "@acme/missing", "version": "1.0.0", "source": "local:missing.wasm", "wasm_hash": "sha256:00"}]
    });
    fs::write(root.join("specforge.lock"), lock.to_string()).unwrap();

    let stale = session.stale();
    assert!(
        !stale.environment.is_empty() && stale.sources.is_empty(),
        "{stale:?}"
    );
    let update = session.ensure_fresh().expect("the lock changed");
    assert_eq!(update.kind, specforge_project::UpdateKind::Environment);
    let locked = e028(&session);
    assert_eq!(locked.len(), 1);
    assert_ne!(locked, unlocked);
    assert_matches_a_fresh_compile(&session, root);
}

/// How long bringing an unchanged project of 1 000 files up to date takes
/// (plan 01 T2 records it; run with `--ignored --nocapture`).
#[test]
#[ignore = "a measurement, not a check"]
fn measure_ensure_fresh_on_a_thousand_files() {
    let files: Vec<(String, String)> = (0..1000)
        .map(|i| {
            (
                format!("dir{}/f{i}.spec", i % 20),
                format!("behavior b{i} \"B{i}\" {{\n  category command\n}}\n"),
            )
        })
        .collect();
    let refs: Vec<(&str, &str)> = files
        .iter()
        .map(|(p, t)| (p.as_str(), t.as_str()))
        .collect();
    let dir = project(CONFIG, &refs);
    age_files(dir.path(), std::time::Duration::from_secs(10));
    let mut session = ProjectSession::open(&specforge_common::ProjectRoot::of(dir.path()).unwrap());
    let started = std::time::Instant::now();
    let rounds = 20;
    for _ in 0..rounds {
        assert!(session.ensure_fresh().is_none());
    }
    eprintln!(
        "ensure_fresh, 1000 unchanged files: {:?} per call",
        started.elapsed() / rounds
    );
}

#[specforge_test(
    behavior = "read_views_over_the_project_view",
    verify = "coverage is computed once per compile and report content, and again after the report changes"
)]
fn an_update_starts_a_fresh_coverage_memo() {
    let dir = project(
        CONFIG,
        &[(
            "a.spec",
            "behavior a \"A\" {\n  contract \"The system MUST a\"\n  verify unit \"a works\"\n}\n",
        )],
    );
    let root = dir.path();
    let mut session = ProjectSession::open(&specforge_common::ProjectRoot::of(root).unwrap());
    let coverage = |session: &ProjectSession| {
        session
            .project()
            .recorded()
            .at(Some(root))
            .unwrap()
            .coverage
    };
    let first = coverage(&session);
    assert!(std::sync::Arc::ptr_eq(&first, &coverage(&session)));
    assert!(first.standing("a").unwrap().counts());

    // A source update: the memo scores the new graph.
    write(
        root,
        "b.spec",
        "behavior b \"B\" {\n  contract \"The system MUST b\"\n}\n",
    );
    session.update(SourceChange::Disk(&changed(&["b.spec"])));
    let updated = coverage(&session);
    assert!(!std::sync::Arc::ptr_eq(&first, &updated));
    assert!(updated.is_unverified("b"));

    // Brought up to date with disk, without a watcher, likewise.
    write(
        root,
        "c.spec",
        "behavior c \"C\" {\n  contract \"The system MUST c\"\n}\n",
    );
    assert!(session.ensure_fresh().is_some());
    assert!(coverage(&session).standing("c").is_some());
}

/// `item` (testable, accepts verify), `P300` obliging it, and a check pass
/// `echo` that reports nothing, so its inputs are read from the runtime.
fn obliging_items() -> specforge_extension_sdk::prelude::ContributionsBuilder {
    use specforge_extension_sdk::prelude::*;
    let mut c = ContributionsBuilder::new(ExtensionMeta::new("@pin/items", "0.1.0"));
    c.kind("item", |k| {
        k.testable(true).supports_verify(true).open_fields(true);
    });
    c.rule("P300", |r| {
        r.check(CheckKind::NoVerifyStatements)
            .target_kind("item")
            .field("verify")
            .message_template("{kind} '{id}' declares no verify obligations");
    });
    c.pass("echo", |p| {
        p.phase("check")
            .run(|_: &PassInput| Vec::<PassDiagnostic>::new());
    });
    c
}

#[specforge_test(
    behavior = "snapshot_entities_once",
    verify = "a session's snapshot follows every update"
)]
fn a_sessions_snapshot_follows_every_update() {
    use specforge_wasm::testing::InProcessRuntime;
    use std::sync::Arc;

    let dir = project(
        r#"{"name":"s","version":"0.1.0","extensions":["@pin/items"]}"#,
        &[("a.spec", "item gizmo \"Gizmo\" {\n}\n")],
    );
    let root = dir.path();
    specforge_installed::testing::install_configured(root, &specforge_project::builtins());
    let runtime = Arc::new(InProcessRuntime::new().with(obliging_items));
    let mut session = ProjectSession::open_with_runtime(
        &specforge_common::ProjectRoot::of(root).unwrap(),
        Some(runtime.clone()),
    );
    let last_pass_input = || {
        runtime
            .calls()
            .into_iter()
            .rev()
            .find(|c| c.export == "__pass_echo")
            .expect("the echo pass ran")
            .input
    };
    let standing = session
        .project()
        .entities()
        .standing("gizmo")
        .unwrap()
        .clone();
    assert_eq!(standing.declared, 0);
    assert_eq!(standing.reported_by(), Some("P300"));
    assert_eq!(
        last_pass_input()["entities"][0]["verify_texts"],
        serde_json::json!([])
    );

    // The update adds an obligation: the snapshot, the coverage and the
    // check pass all see it.
    write(
        root,
        "a.spec",
        "item gizmo \"Gizmo\" {\n  verify unit \"x\"\n}\n",
    );
    session.update(SourceChange::Disk(&changed(&["a.spec"])));
    let standing = session.project().entities().standing("gizmo").unwrap();
    assert_eq!(standing.declared, 1);
    assert!(standing.counts() && standing.reported_by().is_none());
    let coverage = session
        .project()
        .recorded()
        .at(Some(root))
        .unwrap()
        .coverage;
    assert!(std::ptr::eq(
        session.project().entities(),
        coverage.entities()
    ));
    assert_eq!(coverage.verdict("gizmo").unwrap().obligations, 1);
    assert_eq!(coverage.summary.testable_total, 1);
    assert_eq!(
        last_pass_input()["entities"][0]["verify_texts"],
        serde_json::json!(["x"])
    );
    assert!(
        !session
            .project()
            .diagnostics()
            .iter()
            .any(|d| d.code == "P300"),
        "the rule read the same snapshot"
    );
}

#[specforge_test(
    behavior = "snapshot_entities_once",
    verify = "a session's snapshot follows every update"
)]
fn an_update_that_skips_the_checks_still_scores_its_own_graph() {
    let dir = project(
        CONFIG,
        &[(
            "a.spec",
            "behavior a \"A\" {\n  contract \"The system MUST a\"\n}\n",
        )],
    );
    let root = dir.path();
    let mut session = ProjectSession::open(&specforge_common::ProjectRoot::of(root).unwrap());
    let before = std::sync::Arc::clone(session.project().recorded().entities());
    assert!(session.project().entities().kind_of("a").is_some());

    // The file now has a parse error: the checks are skipped, and nothing
    // seeded the memo. What the session scores is still its own graph's
    // snapshot, taken once, in its environment (the spec root included).
    write(root, "b.spec", "behavior b \"B\" {\n  contract \"\n");
    session.update_with(
        SourceChange::Disk(&changed(&["b.spec"])),
        CheckMode::SyntaxOnlyIfParseErrors,
    );
    let entities = session.project().entities();
    assert!(!std::ptr::eq(entities, &*before), "a fresh memo per update");
    assert_eq!(
        entities.spec_root(),
        session.project().environment().spec_root()
    );
    assert!(
        std::ptr::eq(entities, session.project().entities()),
        "taken once"
    );
    for node in session.project().graph().nodes() {
        assert!(
            entities.standing(node.id.raw.as_str()).is_some(),
            "{} is scored",
            node.id.raw
        );
    }
    let coverage = session
        .project()
        .recorded()
        .at(Some(root))
        .unwrap()
        .coverage;
    assert!(std::ptr::eq(entities, coverage.entities()));
}

/// An in-process extension `name` declaring one kind, `kind`.
fn kind_extension(
    name: &'static str,
    kind: &'static str,
) -> specforge_extension_sdk::prelude::ContributionsBuilder {
    use specforge_extension_sdk::prelude::*;
    let mut c = ContributionsBuilder::new(ExtensionMeta::new(name, "0.1.0"));
    c.kind(kind, |k| {
        k.description("a kind");
    });
    c
}

/// The runtime of the extensions `config` names (`@test/a`, `@test/b`).
fn runtime_of(config: &specforge_common::ProjectConfig) -> specforge_project::SharedRuntime {
    use specforge_wasm::testing::InProcessRuntime;
    let mut runtime = InProcessRuntime::new();
    for entry in &config.extensions {
        match entry.as_str() {
            "@test/a" => runtime = runtime.with(|| kind_extension("@test/a", "alpha")),
            "@test/b" => runtime = runtime.with(|| kind_extension("@test/b", "beta")),
            _ => {}
        }
    }
    std::sync::Arc::new(runtime)
}

const V1: &str = r#"{"name":"s","version":"0.1.0","extensions":["@test/a"]}"#;
const V2: &str = r#"{"name":"s","version":"0.1.0","extensions":["@test/a","@test/b"]}"#;

type SeenConfigs = std::sync::Arc<std::sync::Mutex<Vec<Vec<String>>>>;

/// A source whose first build runs `during` (a write while the extension
/// runtime loads), and which records the extensions of every config it
/// was called with.
fn source_writing(
    during: impl Fn(&Path) + Send + Sync + 'static,
) -> (specforge_project::RuntimeSource, SeenConfigs) {
    let seen: SeenConfigs = Default::default();
    let recorded = std::sync::Arc::clone(&seen);
    let source = specforge_project::RuntimeSource::Build(std::sync::Arc::new(
        move |root: &Path, config: &specforge_common::ProjectConfig| {
            let first = {
                let mut seen = recorded.lock().unwrap();
                seen.push(config.extensions.clone());
                seen.len() == 1
            };
            if first {
                during(root);
            }
            runtime_of(config)
        },
    ));
    (source, seen)
}

#[specforge_test(
    behavior = "bring_session_up_to_date",
    verify = "a specforge.json or module written while the extension runtime loads is seen next time"
)]
fn a_config_written_while_the_runtime_loads_is_seen_next_time() {
    let dir = project(V1, &[("a.spec", "")]);
    let root = dir.path();
    specforge_installed::testing::install(root, &["@test/a", "@test/b"]);
    let (source, seen) = source_writing(|root| fs::write(root.join("specforge.json"), V2).unwrap());

    let mut session =
        ProjectSession::open_from(&specforge_common::ProjectRoot::of(root).unwrap(), source);

    // The runtime and the environment were built from the one read (v1),
    // so the environment asks the runtime for nothing it did not load.
    assert_eq!(seen.lock().unwrap().clone(), [["@test/a"]]);
    let e028 = |session: &ProjectSession| {
        session
            .project()
            .diagnostics()
            .iter()
            .any(|d| d.code == "E028")
    };
    assert!(!e028(&session), "{:?}", session.project().diagnostics());
    // The write was after the config's stamp: the session is stale.
    assert!(!session.stale().environment.is_empty());

    let update = session.ensure_fresh().expect("the config changed");
    assert_eq!(update.kind, UpdateKind::Environment);
    assert_eq!(seen.lock().unwrap()[1], ["@test/a", "@test/b"]);
    assert!(!e028(&session), "{:?}", session.project().diagnostics());
    let kinds = session.project().environment().registries().kinds();
    assert!(kinds.contains("alpha") && kinds.contains("beta"));
    assert!(session.stale().environment.is_empty());
}

#[specforge_test(
    behavior = "bring_session_up_to_date",
    verify = "a specforge.json or module written while the extension runtime loads is seen next time"
)]
fn a_module_rewritten_while_the_runtime_loads_is_seen_next_time() {
    let dir = project(
        r#"{"name":"s","version":"0.1.0","extensions":["@acme/local=ext/local.wasm"]}"#,
        &[("a.spec", ""), ("ext/local.wasm", "version one")],
    );
    let root = dir.path();
    let (source, _) =
        source_writing(|root| fs::write(root.join("ext/local.wasm"), "version two!").unwrap());

    let mut session =
        ProjectSession::open_from(&specforge_common::ProjectRoot::of(root).unwrap(), source);

    assert!(!session.stale().environment.is_empty());
    let update = session.ensure_fresh().expect("the module changed");
    assert_eq!(update.kind, UpdateKind::Environment);
    assert!(session.stale().environment.is_empty());
}

#[test]
fn a_reload_builds_its_runtime_from_the_config_it_read() {
    let dir = project(V1, &[("a.spec", "")]);
    let root = dir.path();
    specforge_installed::testing::install(root, &["@test/a", "@test/b"]);
    let (source, seen) = source_writing(|_| {});
    let mut session =
        ProjectSession::open_from(&specforge_common::ProjectRoot::of(root).unwrap(), source);

    fs::write(root.join("specforge.json"), V2).unwrap();
    session.reload_environment();
    fs::write(root.join("specforge.json"), V1).unwrap();
    session.reload_environment();
    assert_eq!(
        seen.lock().unwrap().clone(),
        [vec!["@test/a"], vec!["@test/a", "@test/b"], vec!["@test/a"]]
    );

    // A fixed runtime is the same one after a reload.
    let fixed = runtime_of(&specforge_common::ProjectConfig::default());
    let mut session = ProjectSession::open_with_runtime(
        &specforge_common::ProjectRoot::of(root).unwrap(),
        Some(fixed.clone()),
    );
    session.reload_environment();
    assert!(std::sync::Arc::ptr_eq(session.runtime().unwrap(), &fixed));
}

// --- 07-T0: pins for the graph build and the unreadable source ---

/// `a.spec` and a `bad.spec` that is not UTF-8.
fn project_with_an_unreadable_source() -> TempDir {
    let dir = project(CONFIG, &[("a.spec", &behavior("alpha", ""))]);
    fs::write(
        dir.path().join("bad.spec"),
        b"term beta \"B\xff\xfe\" {\n}\n",
    )
    .unwrap();
    dir
}

fn e025_messages(diagnostics: &[Diagnostic]) -> Vec<String> {
    diagnostics
        .iter()
        .filter(|d| d.code == "E025")
        .map(|d| d.message.clone())
        .collect()
}

#[specforge_test(
    behavior = "resolve_imports_on_update",
    verify = "an unreadable source stays E025 after an update of another file"
)]
fn an_unreadable_source_is_reported_after_an_update_of_another_file() {
    let dir = project_with_an_unreadable_source();
    let root = dir.path();
    let runtime = Arc::new(specforge_component::ComponentRuntime::with_user_cache());
    let message = "cannot read bad.spec: stream did not contain valid UTF-8";
    let mut session = ProjectSession::open(&specforge_common::ProjectRoot::of(root).unwrap());
    assert_eq!(e025_messages(&session.project().diagnostics()), [message]);

    write(root, "a.spec", &behavior("alpha", "  invariants []\n"));
    session.update(SourceChange::Disk(&changed(&["a.spec"])));

    assert_eq!(e025_messages(&session.project().diagnostics()), [message]);
    let fresh = CompiledProject::compile(
        &specforge_common::ProjectRoot::of(root).unwrap(),
        Some(runtime.clone()),
    );
    assert_eq!(e025_messages(&fresh.diagnostics()), [message]);
    assert_matches_a_fresh_compile(&session, root);
}

#[test]
fn a_source_that_becomes_readable_joins_the_build() {
    let dir = project_with_an_unreadable_source();
    let root = dir.path();
    let mut session = ProjectSession::open(&specforge_common::ProjectRoot::of(root).unwrap());
    assert!(session.project().graph().node("beta").is_none());

    write(root, "bad.spec", "term beta \"B\" {\n}\n");
    let update = session.update(SourceChange::Disk(&changed(&["bad.spec"])));

    assert_eq!(update.rebuilt_files, ["bad.spec"]);
    assert!(session.project().graph().node("beta").is_some());
    assert!(e025_messages(&session.project().diagnostics()).is_empty());
    assert_matches_a_fresh_compile(&session, root);

    // And unreadable again: out of the graph, reported once more.
    fs::write(root.join("bad.spec"), b"term beta \"B\xff\" {\n}\n").unwrap();
    session.update(SourceChange::Disk(&changed(&["bad.spec"])));
    assert!(session.project().graph().node("beta").is_none());
    assert_eq!(e025_messages(&session.project().diagnostics()).len(), 1);
    assert_matches_a_fresh_compile(&session, root);
}

/// `b.spec`: `behavior dup`; `c.spec`: `invariant dup`; `d.spec`:
/// `invariant dup` and a define block.
#[specforge_test(
    behavior = "rebuild_affected_subgraph",
    verify = "incremental rebuild equals cold rebuild"
)]
fn duplicates_across_files_and_kinds_stay_what_a_fresh_compile_reports() {
    let invariant = "invariant dup \"Dup\" {\n  contract \"The system MUST dup\"\n}\n";
    let dir = project(
        CONFIG,
        &[
            ("b.spec", &behavior("dup", "")),
            ("c.spec", invariant),
            ("d.spec", &format!("{invariant}define thing {{\n}}\n")),
        ],
    );
    let root = dir.path();
    let mut session = ProjectSession::open(&specforge_common::ProjectRoot::of(root).unwrap());
    session.set_verify_incremental(true);
    assert_matches_a_fresh_compile(&session, root);

    // The pinned messages: E002 names the first declaration of the same
    // kind (c.spec), not the retained node (b.spec).
    let diagnostics = session.project().diagnostics();
    let on = |code: &str, file: &str| {
        diagnostics
            .iter()
            .filter(|d| d.code == code && d.span.as_ref().is_some_and(|s| s.file == file))
            .map(|d| d.message.clone())
            .collect::<Vec<_>>()
    };
    assert_eq!(
        on("E002", "d.spec"),
        ["duplicate entity ID 'dup' (first declared at c.spec:1:1)"]
    );
    assert_eq!(
        on("W060", "c.spec"),
        [
            "entity ID 'dup' is used by kind 'behavior' and kind 'invariant'; first declaration (kind 'behavior') is retained"
        ]
    );

    let steps: [(&str, Option<String>); 5] = [
        ("a.spec", Some(behavior("dup", ""))),
        ("a.spec", None),
        ("c.spec", Some(behavior("dup", ""))),
        ("b.spec", None),
        ("d.spec", Some(invariant.to_string())),
    ];
    for (path, text) in steps {
        match &text {
            Some(text) => write(root, path, text),
            None => fs::remove_file(root.join(path)).unwrap(),
        }
        let update = session.update(SourceChange::Disk(&changed(&[path])));
        assert_eq!(update.verification, Some(Ok(())), "after {path}");
        assert_matches_a_fresh_compile(&session, root);
    }
}

#[test]
fn the_session_reports_graph_diagnostics_in_build_order() {
    let dir = project(
        CONFIG,
        &[
            ("a.spec", &behavior("alpha", "  invariants [ghost]\n")),
            (
                "b.spec",
                &format!("{}{}", behavior("beta", ""), behavior("beta", "")),
            ),
        ],
    );
    let root = dir.path();
    let runtime = Arc::new(specforge_component::ComponentRuntime::with_user_cache());
    let graph_codes = |diagnostics: &[Diagnostic]| -> Vec<String> {
        diagnostics
            .iter()
            .filter(|d| d.code == "E002" || d.code == "E003")
            .map(|d| d.code.to_string())
            .collect()
    };
    let compiled = CompiledProject::compile(
        &specforge_common::ProjectRoot::of(root).unwrap(),
        Some(runtime.clone()),
    );
    let session = ProjectSession::open(&specforge_common::ProjectRoot::of(root).unwrap());

    assert_eq!(graph_codes(&compiled.diagnostics()), ["E002", "E003"]);
    // Build order, the order `specforge check` lists them in (ADR 0032).
    assert_eq!(
        session.project().diagnostics(),
        compiled.diagnostics(),
        "the same sequence"
    );
}

/// Every session verifies its updates in a debug build, with no flag; a
/// release build only when asked.
#[specforge_test(
    behavior = "validate_delta_correctness",
    verify = "a debug build checks each rebuild without the flag"
)]
fn a_session_verifies_its_updates_in_a_debug_build() {
    let dir = three_files();
    let root = dir.path();
    let mut session = ProjectSession::open(&specforge_common::ProjectRoot::of(root).unwrap());
    for name in ["renamed", "again"] {
        write(root, "c.spec", &behavior(name, ""));
        let update = session.update(SourceChange::Disk(&changed(&["c.spec"])));
        assert_eq!(update.verification.is_some(), cfg!(debug_assertions));
        if let Some(verification) = update.verification {
            assert_eq!(verification, Ok(()));
        }
    }
    // A reloaded session verifies too.
    session.reload_environment();
    write(root, "c.spec", &behavior("reloaded", ""));
    let update = session.update(SourceChange::Disk(&changed(&["c.spec"])));
    assert_eq!(update.verification.is_some(), cfg!(debug_assertions));
}

/// What every surface reports of a verified update: how its graph differs
/// from a cold rebuild, only when it does.
#[specforge_test(
    behavior = "validate_delta_correctness",
    verify = "a discrepancy is reported with a message naming what differs"
)]
fn a_divergence_is_the_failed_verification() {
    let dir = three_files();
    let mut session = ProjectSession::open(&specforge_common::ProjectRoot::of(dir.path()).unwrap());
    let mut update = session.update(SourceChange::Disk(&changed(&["c.spec"])));
    for (verification, divergence) in [
        (Some(Err("nodes differ".to_string())), Some("nodes differ")),
        (Some(Ok(())), None),
        (None, None),
    ] {
        update.verification = verification;
        assert_eq!(update.divergence(), divergence);
    }
}

#[specforge_test(
    invariant = "extensions_run_in_their_loading_runtime",
    verify = "an environment holds the runtime it loaded its extensions in, and its session and compiled project read it there"
)]
fn an_environment_holds_the_runtime_it_loaded_its_extensions_in() {
    use specforge_project::{Environment, SharedRuntime};
    use specforge_wasm::testing::InProcessRuntime;

    let dir = project(
        r#"{"name":"s","version":"0.1.0","extensions":["@pin/items"]}"#,
        &[("a.spec", "item gizmo \"Gizmo\" {\n}\n")],
    );
    let root = dir.path();
    specforge_installed::testing::install_configured(root, &specforge_project::builtins());
    let rt: SharedRuntime = Arc::new(InProcessRuntime::new().with(obliging_items));

    let loaded = Environment::load(
        &specforge_common::ProjectRoot::of(root).unwrap(),
        Some(rt.clone()),
    );
    assert!(Arc::ptr_eq(loaded.runtime().unwrap(), &rt));

    let compiled = CompiledProject::compile(
        &specforge_common::ProjectRoot::of(root).unwrap(),
        Some(rt.clone()),
    );
    assert!(Arc::ptr_eq(compiled.environment().runtime().unwrap(), &rt));

    let mut session = ProjectSession::open_with_runtime(
        &specforge_common::ProjectRoot::of(root).unwrap(),
        Some(rt.clone()),
    );
    assert!(Arc::ptr_eq(session.runtime().unwrap(), &rt));
    session.reload_environment();
    assert!(Arc::ptr_eq(session.runtime().unwrap(), &rt));
    assert!(Arc::ptr_eq(
        session.project().environment().runtime().unwrap(),
        &rt
    ));

    let bare = CompiledProject::compile(&specforge_common::ProjectRoot::of(root).unwrap(), None);
    assert!(bare.environment().runtime().is_none());
    assert!(bare.environment().registries().declarations().is_empty());
}

#[specforge_test(
    behavior = "resolve_use_imports",
    verify = "a spec root named like a skipped directory is compiled"
)]
fn a_spec_root_named_like_a_skipped_directory_is_compiled() {
    let dir = project(
        r#"{"name":"s","version":"0.1.0","extensions":[],"spec_root":"build"}"#,
        &[("build/main.spec", "behavior a \"A\" {\n}\n")],
    );
    let compiled = CompiledProject::compile(
        &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
        None,
    );
    let mut files: Vec<String> = compiled.source_texts().into_keys().collect();
    files.sort();
    assert_eq!(files, ["main.spec"]);
}

/// A session keeps the root it was opened at: a reload does not locate the
/// project again, even when a nearer `specforge.json` appeared since.
#[test]
fn a_reload_keeps_the_root_it_was_opened_at() {
    let dir = project(r#"{"name": "p"}"#, &[("sub/a.spec", "term a \"A\" {\n}\n")]);
    let sub = dir.path().join("sub");
    let opened = specforge_common::ProjectRoot::of(&sub).unwrap();
    assert_eq!(opened.dir(), dir.path());
    let mut session = ProjectSession::open(&opened);

    // A project of its own appears under the opened one.
    write(&sub, "specforge.json", r#"{"name": "inner"}"#);
    session.reload_environment();

    let kept = session.project().project_root().expect("on disk");
    assert_eq!(kept.dir(), dir.path());
    assert_eq!(session.project().root(), Some(dir.path()));
}

// --- 04-T0: imports name the compile's sources ---

const BARE: &str = r#"{"name":"s","version":"0.1.0","extensions":[]}"#;
const EXCLUDING_DRAFTS: &str =
    r#"{"name":"s","version":"0.1.0","extensions":[],"exclude":["drafts/"]}"#;

#[specforge_test(
    behavior = "resolve_use_imports",
    verify = "an import of a file that is no project source produces E025"
)]
fn an_import_of_a_file_that_is_no_source_is_e025() {
    let dir = project(
        EXCLUDING_DRAFTS,
        &[
            ("drafts/x.spec", "term x \"X\" {\n}\n"),
            ("build/f.spec", "term f \"F\" {\n}\n"),
            (
                "a.spec",
                "use \"drafts/x\"\nuse \"build/f\"\nuse \"escape/secret\"\n\nterm a \"A\" {\n}\n",
            ),
        ],
    );
    let root = dir.path();
    #[cfg(unix)]
    {
        let outside = TempDir::new().unwrap();
        write(outside.path(), "secret.spec", "term secret \"S\" {\n}\n");
        std::os::unix::fs::symlink(outside.path(), root.join("escape")).unwrap();
        let compiled =
            CompiledProject::compile(&specforge_common::ProjectRoot::of(root).unwrap(), None);
        let found: Vec<(usize, String)> = compiled
            .diagnostics()
            .iter()
            .filter(|d| d.code == "E025")
            .map(|d| (d.span.as_ref().unwrap().start_line, d.message.clone()))
            .collect();
        assert_eq!(
            found,
            [
                (
                    1,
                    "import target not found among the project's sources: drafts/x".to_string()
                ),
                (
                    2,
                    "import target not found among the project's sources: build/f".to_string()
                ),
                (
                    3,
                    "import target not found among the project's sources: escape/secret"
                        .to_string()
                ),
            ]
        );
        for id in ["x", "f", "secret"] {
            assert!(compiled.graph().node(id).is_none(), "{id}");
        }
    }
}

#[specforge_test(
    behavior = "resolve_use_imports",
    verify = "an import of an unreadable source reports only that source's E025"
)]
fn an_import_of_an_unreadable_source_reports_only_its_e025() {
    let dir = project(
        BARE,
        &[(
            "c.spec",
            "use \"bad\"\npub use { q } from \"bad\"\nterm c \"C\" {\n}\n",
        )],
    );
    fs::write(dir.path().join("bad.spec"), b"term bad \"B\xff\" {\n}\n").unwrap();
    let compiled = CompiledProject::compile(
        &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
        None,
    );
    let diagnostics = compiled.diagnostics();
    assert_eq!(
        e025(&diagnostics),
        ["cannot read bad.spec: stream did not contain valid UTF-8"]
    );
    assert!(
        !diagnostics.iter().any(|d| d.code == "W027"),
        "{diagnostics:?}"
    );
}

#[specforge_test(
    behavior = "resolve_use_imports",
    verify = "an import names an editor buffer's source, saved or not"
)]
fn an_import_names_an_unsaved_buffer() {
    let dir = project(BARE, &[("a.spec", "use \"b\"\n\nterm a \"A\" {\n}\n")]);
    let root = dir.path();
    let mut session = ProjectSession::open(&specforge_common::ProjectRoot::of(root).unwrap());
    assert_eq!(e025(&session.project().diagnostics()).len(), 1);

    let buffers = [Buffer::new(root.join("b.spec"), "term b \"B\" {\n}\n")];
    let update = session.update(SourceChange::Hold(&buffers));

    assert!(
        e025(&update.diagnostics).is_empty(),
        "{:?}",
        update.diagnostics
    );
    assert!(session.project().graph().node("b").is_some());
}

#[specforge_test(
    invariant = "incremental_correctness",
    verify = "a file discovery does not find changes nothing a compile or a session reports"
)]
fn a_file_discovery_does_not_find_changes_nothing_a_session_reports() {
    let dir = project(
        EXCLUDING_DRAFTS,
        &[("a.spec", "use \"drafts/x\"\n\nterm a \"A\" {\n}\n")],
    );
    let root = dir.path();
    let mut session = ProjectSession::open(&specforge_common::ProjectRoot::of(root).unwrap());
    assert_eq!(e025(&session.project().diagnostics()).len(), 1);

    write(root, "drafts/x.spec", "term x \"X\" {\n}\n");

    assert!(session.stale().is_empty());
    assert!(session.ensure_fresh().is_none());
    assert_matches_a_fresh_compile(&session, root);
    assert_eq!(e025(&session.project().diagnostics()).len(), 1);
}

#[specforge_test(
    behavior = "classify_project_changes",
    verify = "an excluded or undiscovered .spec file changes nothing"
)]
fn an_update_naming_only_files_discovery_does_not_find_changes_nothing() {
    let dir = project(EXCLUDING_DRAFTS, &[("a.spec", "term a \"A\" {\n}\n")]);
    let root = dir.path();
    let mut session = ProjectSession::open(&specforge_common::ProjectRoot::of(root).unwrap());
    let before = session.project().diagnostics();

    write(root, "drafts/d.spec", "term d \"D\" {\n}\n");
    write(root, "build/f.spec", "term f \"F\" {\n}\n");
    let update = session.update(SourceChange::Disk(&changed(&[
        "drafts/d.spec",
        "build/f.spec",
    ])));

    assert_eq!(update.kind, UpdateKind::Sources);
    assert!(update.rebuilt_files.is_empty());
    assert_eq!(update.delta, specforge_project::GraphDelta::default());
    assert!(!update.inputs_changed);
    assert_eq!(update.diagnostics[..], before[..]);
}

#[specforge_test(
    behavior = "resolve_use_imports",
    verify = "files matching an exclude entry are not compiled"
)]
fn a_glob_exclude_entry_leaves_its_files_out_of_the_compile() {
    let dir = project(
        r#"{"name":"s","version":"0.1.0","extensions":[],"exclude":["drafts/**","**/*.draft.spec"]}"#,
        &[
            ("drafts/a.spec", "term a \"A\" {\n}\n"),
            ("sub/b.draft.spec", "term b \"B\" {\n}\n"),
            ("main.spec", "term m \"M\" {\n}\n"),
        ],
    );
    let root = dir.path();
    let compiled =
        CompiledProject::compile(&specforge_common::ProjectRoot::of(root).unwrap(), None);
    let mut files: Vec<String> = compiled.source_texts().into_keys().collect();
    files.sort();
    assert_eq!(files, ["main.spec"]);

    let session = ProjectSession::open(&specforge_common::ProjectRoot::of(root).unwrap());
    write(root, "sub/c.draft.spec", "term c \"C\" {\n}\n");
    assert!(
        session
            .changes([root.join("sub/c.draft.spec").as_path()])
            .is_empty()
    );
}

#[specforge_test(
    behavior = "resolve_use_imports",
    verify = "symlink pointing outside spec_root is rejected"
)]
#[cfg(unix)]
fn a_symlinked_directory_is_no_source() {
    let outer = TempDir::new().unwrap();
    let outside = outer.path().join("outside");
    write(&outside, "secret.spec", "term secret \"S\" {\n}\n");
    let dir = project(
        BARE,
        &[("a.spec", "use \"escape/secret\"\nterm a \"A\" {\n}\n")],
    );
    std::os::unix::fs::symlink(&outside, dir.path().join("escape")).unwrap();

    let compiled = CompiledProject::compile(
        &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
        None,
    );

    assert!(compiled.graph().node("secret").is_none());
    assert_eq!(
        e025(&compiled.diagnostics()),
        ["import target not found among the project's sources: escape/secret"]
    );
}

/// Whatever a watcher reports for a step (a file, both directories of a
/// rename, a directory created or removed, a symlink target), the changes
/// of that batch are what the session finds stale: the batch comparison is
/// the full comparison narrowed to what the paths reach, so naming every
/// changed path loses nothing.
#[specforge_test(
    behavior = "classify_project_changes",
    verify = "the changes of a batch naming every changed path are what the session finds stale"
)]
fn a_batch_naming_every_changed_path_changes_what_is_stale() {
    use std::path::PathBuf;

    const PATHS: &[&str] = &[
        "a.spec",
        "b.spec",
        "sub/c.spec",
        "sub/deep/e.spec",
        "drafts/d.spec",
    ];
    let config = |exclude: bool| {
        let exclude = if exclude {
            r#","exclude":["drafts/"]"#
        } else {
            ""
        };
        format!(
            r#"{{"name":"s","version":"0.1.0","extensions":["@specforge/software","@specforge/testing"],"spec_root":"spec"{exclude}}}"#
        )
    };
    for seed in [
        0x9E37_79B9_7F4A_7C15_u64,
        0xD1B5_4A32_D192_ED03,
        0x2545_F491_4F6C_DD1D,
        0x1F83_D9AB_FB41_BD6B,
    ] {
        let mut rng = Rng(seed);
        let dir = project(&config(false), &[]);
        let root = fs::canonicalize(dir.path()).unwrap();
        let spec = root.join("spec");
        let outside = TempDir::new().unwrap();
        let outside_dir = fs::canonicalize(outside.path()).unwrap();
        for path in &PATHS[..3] {
            write(&spec, path, &random_spec(&mut rng));
        }
        let mut session = ProjectSession::open(&specforge_common::ProjectRoot::of(&root).unwrap());
        let mut excluding = false;
        #[cfg_attr(not(unix), allow(unused_mut, unused_variables))]
        let mut links = 0;

        for step in 0..30 {
            let mut paths: Vec<PathBuf> = Vec::new();
            match rng.below(8) {
                0 => {
                    let path = rng.pick(PATHS);
                    write(&spec, path, &random_spec(&mut rng));
                    paths.push(spec.join(path));
                }
                1 => {
                    let path = rng.pick(PATHS);
                    if spec.join(path).is_file() {
                        fs::remove_file(spec.join(path)).unwrap();
                        paths.push(spec.join(path));
                    }
                }
                2 => {
                    let (from, to) = if spec.join("sub").is_dir() {
                        ("sub", "sub2")
                    } else {
                        ("sub2", "sub")
                    };
                    if spec.join(from).is_dir() && !spec.join(to).exists() {
                        fs::rename(spec.join(from), spec.join(to)).unwrap();
                        paths.extend([spec.join(from), spec.join(to)]);
                    }
                }
                3 => {
                    for file in ["made/x.spec", "made/y.spec"] {
                        write(&spec, file, &random_spec(&mut rng));
                    }
                    paths.push(spec.join("made"));
                }
                4 => {
                    if spec.join("made").is_dir() {
                        fs::remove_dir_all(spec.join("made")).unwrap();
                        paths.push(spec.join("made"));
                    }
                }
                #[cfg(unix)]
                5 => {
                    links += 1;
                    let target = outside_dir.join(format!("t{links}.spec"));
                    fs::write(&target, random_spec(&mut rng)).unwrap();
                    let link = spec.join(format!("l{links}.spec"));
                    std::os::unix::fs::symlink(&target, &link).unwrap();
                    paths.push(link);
                }
                #[cfg(unix)]
                6 => {
                    if links > 0 {
                        let target = outside_dir.join(format!("t{}.spec", 1 + rng.below(links)));
                        fs::write(&target, random_spec(&mut rng)).unwrap();
                        paths.push(target);
                    }
                }
                _ => {
                    excluding = !excluding;
                    fs::write(root.join("specforge.json"), config(excluding)).unwrap();
                    paths.push(root.join("specforge.json"));
                }
            }
            let context = format!("seed {seed:#x} step {step}");
            let changes = session.changes(paths.iter().map(PathBuf::as_path));
            let stale = session.stale();
            assert_eq!(changes.sources, stale.sources, "{context}");
            assert_eq!(
                changes.environment.is_empty(),
                stale.environment.is_empty(),
                "{context}"
            );
            assert_eq!(
                changes.check_inputs.is_empty(),
                stale.check_inputs.is_empty(),
                "{context}"
            );

            session.apply(&changes);
            assert!(session.stale().is_empty(), "{context}");
            assert_matches_a_fresh_compile(&session, &root);
        }
    }
}
