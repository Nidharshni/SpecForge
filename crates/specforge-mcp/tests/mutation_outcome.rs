//! What each mutation tool wrote, what it reports and emits, and how the
//! served project follows (architecture plan 04, ADR 0022).
//!
//! Every pin compares what a call left on disk (a snapshot diff of the
//! project) with what it reported.

use std::path::{Path, PathBuf};

use serde_json::{Value, json};
use specforge_mcp::McpServer;

use specforge_test::prelude::*;

use crate::support::*;

/// test.spec: the behavior `alpha` and the feature `beta` that has it.
const TEST_SPEC: &str = concat!(
    "behavior alpha \"Alpha\" {\n",
    "}\n",
    "\n",
    "feature beta \"Beta\" {\n",
    "    behaviors [alpha]\n",
    "}\n",
);

/// A spec file the formatter rewrites.
const UNFORMATTED: &str = "behavior messy \"Messy\" {\ncontract \"The system MUST work\"\n}\n";

/// A spec file in format 0.9: `migrate` rewrites it (and backs it up).
const OLD: &str = "// specforge-format: 0.9\nbehavior gamma \"Gamma\" {\n}\n";

/// The extension blob the build vendors: `@sdk/greet`.
fn greet_blob() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/greet-extension/greet.wasm")
}

fn greet() -> String {
    greet_blob().to_str().expect("a UTF-8 path").to_string()
}

const GREET: &str = "@sdk/greet";
const PRODUCT: &str = "@specforge/product";

/// A project with no source, served with its own component runtime: what
/// an add or a removal changes is what the builtins and installs make of it.
fn empty_components() -> Served {
    TestProject::new().serve_components()
}

/// The project's files, relative to its root, as `files_written` lists them.
fn names(files: &[&str]) -> Vec<PathBuf> {
    files.iter().map(PathBuf::from).collect()
}

/// Call `tool` with `arguments` on `server`; its reply and the files under
/// `root` the call changed.
fn wrote(
    server: &mut McpServer,
    root: &Path,
    tool: &str,
    arguments: Value,
) -> (Value, Vec<PathBuf>) {
    let before = files_under(root);
    let reply = call_tool(server, tool, arguments);
    let changed = changed_files(root, &before, &files_under(root));
    (reply, changed)
}

/// The reply succeeded (no protocol error, no `isError`).
fn assert_ok(reply: &Value) {
    assert_eq!(reply["result"]["isError"], false, "{reply}");
}

/// The last `mcp_mutation_completed` event, if any.
fn last_completed(server: &McpServer) -> Option<Value> {
    events(server, "mcp_mutation_completed").pop()
}

/// The names of the events recorded from index `since` on, among `names`.
fn event_names_since(server: &McpServer, since: usize, names: &[&str]) -> Vec<String> {
    server.state().events[since..]
        .iter()
        .filter(|e| names.contains(&e.name.as_str()))
        .map(|e| e.name.clone())
        .collect()
}

// 1
#[specforge_test(
    behavior = "mcp_mutation_completed",
    verify = "files_changed is the number of files the call wrote, for every mutation tool"
)]
fn enabling_a_builtin_writes_one_file() {
    let mut server = empty_components();
    let root = server.root().to_path_buf();

    let (reply, changed) = wrote(
        &mut server,
        &root,
        "specforge.add_extension",
        json!({"specifiers": [PRODUCT]}),
    );

    assert_ok(&reply);
    assert_eq!(changed, names(&["specforge.json"]));
    assert_eq!(
        events(&server, "mcp_mutation_completed"),
        [completed("specforge.add_extension", 1, 0, true)]
    );
}

// 2
#[specforge_test(
    behavior = "mcp_mutation_completed",
    verify = "files_changed is the number of files the call wrote, for every mutation tool"
)]
fn disabling_a_builtin_writes_one_file() {
    let mut server = empty_components();
    let root = server.root().to_path_buf();
    assert_ok(&call_tool(
        &mut server,
        "specforge.add_extension",
        json!({"specifiers": [PRODUCT]}),
    ));

    let (reply, changed) = wrote(
        &mut server,
        &root,
        "specforge.remove_extension",
        json!({"name": PRODUCT}),
    );

    assert_ok(&reply);
    assert_eq!(changed, names(&["specforge.json"]));
    assert_eq!(
        last_completed(&server),
        Some(completed("specforge.remove_extension", 1, 0, true))
    );
}

// 3
#[test]
fn enabling_an_enabled_builtin_writes_nothing() {
    let mut server = empty_components();
    let root = server.root().to_path_buf();
    assert_ok(&call_tool(
        &mut server,
        "specforge.add_extension",
        json!({"specifiers": [PRODUCT]}),
    ));
    assert_eq!(events(&server, "extension_added").len(), 1);

    let (reply, changed) = wrote(
        &mut server,
        &root,
        "specforge.add_extension",
        json!({"specifiers": [PRODUCT]}),
    );

    assert_ok(&reply);
    assert_eq!(changed, Vec::<PathBuf>::new());
    assert_eq!(
        last_completed(&server),
        Some(completed("specforge.add_extension", 0, 0, true))
    );
    // The duplicate add still emits extension_added, a duplicate.
    let added = events(&server, "extension_added");
    assert_eq!(added.len(), 2, "{added:?}");
    assert_eq!(added[1]["wasDuplicate"], true, "{added:?}");
}

/// The extensions `specforge.json` at `root` enables.
fn enabled(root: &Path) -> Vec<Value> {
    let text = std::fs::read_to_string(root.join("specforge.json")).unwrap();
    let config: Value = serde_json::from_str(&text).unwrap();
    config["extensions"].as_array().cloned().unwrap_or_default()
}

#[specforge_test(
    behavior = "extension_added",
    verify = "wasDuplicate is true when extension was already installed"
)]
fn a_duplicate_add_emits_was_duplicate() {
    let mut server = empty_components();
    let root = server.root().to_path_buf();
    for specifier in [PRODUCT.to_string(), greet()] {
        for _ in 0..2 {
            assert_ok(&call_tool(
                &mut server,
                "specforge.add_extension",
                json!({"specifiers": [specifier]}),
            ));
        }
    }

    let total = enabled(&root).len();
    let added = events(&server, "extension_added");
    let duplicates: Vec<(&Value, &Value)> = added
        .iter()
        .map(|e| (&e["extensionSpecifier"], &e["wasDuplicate"]))
        .collect();
    let (product, blob) = (json!(PRODUCT), json!(greet()));
    assert_eq!(
        duplicates,
        [
            (&product, &json!(false)),
            (&product, &json!(true)),
            (&blob, &json!(false)),
            // A local blob already installed with these bytes: AlreadyPresent.
            (&blob, &json!(true)),
        ]
    );
    assert_eq!(added[3]["totalExtensions"], total, "{added:?}");
    // A dry run emits none.
    let since = added.len();
    assert_ok(&call_tool(
        &mut server,
        "specforge.add_extension",
        json!({"specifiers": [PRODUCT], "dry_run": true}),
    ));
    assert_eq!(events(&server, "extension_added").len(), since);
}

// 4
#[test]
fn installing_and_removing_a_local_extension_write_three_files() {
    let mut server = empty_components();
    let root = server.root().to_path_buf();
    let three = names(&[
        ".specforge/extensions/@sdk/greet/extension.wasm",
        "specforge.json",
        "specforge.lock",
    ]);

    let (reply, installed) = wrote(
        &mut server,
        &root,
        "specforge.add_extension",
        json!({"specifiers": [greet()]}),
    );
    assert_ok(&reply);
    assert_eq!(installed, three);
    assert_eq!(
        last_completed(&server),
        Some(completed("specforge.add_extension", 3, 0, true))
    );

    let (reply, removed) = wrote(
        &mut server,
        &root,
        "specforge.remove_extension",
        json!({"name": GREET}),
    );
    assert_ok(&reply);
    assert_eq!(removed, three);
    assert_eq!(
        last_completed(&server),
        Some(completed("specforge.remove_extension", 3, 0, true))
    );
}

// 5
#[specforge_test(
    behavior = "mcp_mutation_completed",
    verify = "files_changed is the number of files the call wrote, for every mutation tool"
)]
fn init_with_a_local_extension_writes_five_files() {
    let mut server = empty_components();
    let scratch = tempfile::TempDir::new().unwrap();
    let dir = scratch.path().join("n1");

    let (reply, changed) = wrote(
        &mut server,
        &dir,
        "specforge.init",
        json!({"path": dir.to_str().unwrap(), "name": "newone", "extensions": [greet()]}),
    );

    assert_ok(&reply);
    assert_eq!(
        changed,
        names(&[
            ".gitignore",
            ".specforge/extensions/@sdk/greet/extension.wasm",
            "spec/hello.spec",
            "specforge.json",
            "specforge.lock",
        ])
    );
    assert_eq!(
        last_completed(&server),
        Some(completed("specforge.init", 5, 0, true))
    );
}

// 6
#[specforge_test(
    behavior = "mcp_mutation_completed",
    verify = "files_changed is the number of files the call wrote, for every mutation tool"
)]
fn init_beside_a_complete_gitignore_writes_two_files() {
    let mut server = empty_components();
    let scratch = tempfile::TempDir::new().unwrap();
    let dir = scratch.path().join("n2");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join(".gitignore"),
        "specforge-infer.json\nspecforge-report.json\n.specforge/\n",
    )
    .unwrap();

    let (reply, changed) = wrote(
        &mut server,
        &dir,
        "specforge.init",
        json!({"path": dir.to_str().unwrap(), "name": "newtwo"}),
    );

    assert_ok(&reply);
    assert_eq!(changed, names(&["spec/hello.spec", "specforge.json"]));
    assert_eq!(
        last_completed(&server),
        Some(completed("specforge.init", 2, 0, true))
    );
}

// 7
#[cfg(unix)]
#[specforge_test(
    behavior = "mcp_mutation_completed",
    verify = "a mutation that fails after writing reports the files it wrote"
)]
fn a_format_that_fails_on_one_file_reports_what_it_wrote() {
    use std::os::unix::fs::PermissionsExt;
    let mut server = TestProject::new()
        .file("a.spec", UNFORMATTED)
        .file("b.spec", &UNFORMATTED.replace("messy", "other"))
        .serve(&[TestExtension::software()]);
    let root = server.root().to_path_buf();
    let locked = root.join("a.spec");
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o444)).unwrap();
    let generation = server.state().session_generation();

    let (reply, changed) = wrote(&mut server, &root, "specforge.format", json!({}));
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o644)).unwrap();

    let error = crate::tool_errors::mcp_error(&reply);
    assert_eq!(error["code"], "permission_denied", "{error}");
    assert_eq!(changed, names(&["b.spec"]));
    assert_eq!(
        last_completed(&server),
        Some(completed("specforge.format", 1, 0, false))
    );
    assert_eq!(error["data"]["files_written"], json!(["b.spec"]), "{error}");
    // What was written is served.
    assert!(server.state().session_generation() > generation);
}

// 8
#[specforge_test(
    behavior = "mcp_mutation_completed",
    verify = "files_changed is the number of files the call wrote, for every mutation tool"
)]
fn a_migration_writes_the_file_and_its_backup() {
    let mut server = TestProject::new()
        .file("old.spec", OLD)
        .serve(&[TestExtension::software()]);
    let root = server.root().to_path_buf();

    let (reply, changed) = wrote(&mut server, &root, "specforge.migrate", json!({}));

    assert_ok(&reply);
    assert_eq!(
        changed,
        names(&[".specforge/migration.json", "old.spec", "old.spec.bak"])
    );
    assert_eq!(
        last_completed(&server),
        Some(completed("specforge.migrate", 3, 0, true))
    );
}

/// [`TEST_SPEC`] plus other.spec, whose feature also has `alpha`: renaming
/// `alpha` edits both files.
fn rename_server() -> Served {
    TestProject::new()
        .file("test.spec", TEST_SPEC)
        .file(
            "other.spec",
            "feature delta \"Delta\" {\n    behaviors [alpha]\n}\n",
        )
        .serve(&[TestExtension::software()])
}

// 9
#[test]
fn a_rename_reports_its_files_and_one_entity() {
    let mut server = rename_server();
    let root = server.root().to_path_buf();

    let (reply, changed) = wrote(
        &mut server,
        &root,
        "specforge.rename",
        json!({"entity_id": "alpha", "new_name": "omega", "dry_run": true}),
    );
    assert_ok(&reply);
    assert_eq!(changed, Vec::<PathBuf>::new(), "a dry run writes nothing");
    assert_eq!(last_completed(&server), None, "a dry run is no mutation");

    let (reply, changed) = wrote(
        &mut server,
        &root,
        "specforge.rename",
        json!({"entity_id": "alpha", "new_name": "omega"}),
    );
    assert_ok(&reply);
    assert_eq!(changed, names(&["other.spec", "test.spec"]));
    assert_eq!(
        last_completed(&server),
        Some(completed("specforge.rename", 2, 1, true))
    );
    assert!(tool_json(&reply)["diagnostics"].is_array(), "{reply}");
}

// 10
#[test]
fn infer_session_writes_its_manifest() {
    let mut server = TestProject::new()
        .file("src/main.rs", "fn main() {}\n")
        .serve(&[TestExtension::software()]);
    let root = server.root().to_path_buf();
    let tool = "specforge.infer_session";

    let (reply, changed) = wrote(&mut server, &root, tool, json!({"action": "start"}));
    assert_ok(&reply);
    assert_eq!(changed, names(&["specforge-infer.json"]));
    assert_eq!(last_completed(&server), Some(completed(tool, 1, 0, true)));

    let (reply, changed) = wrote(
        &mut server,
        &root,
        tool,
        json!({
            "action": "mark_analyzed",
            "source_file": "src/main.rs",
            "entities_produced": ["alpha", "beta"],
        }),
    );
    assert_ok(&reply);
    assert_eq!(changed, names(&["specforge-infer.json"]));
    assert_eq!(last_completed(&server), Some(completed(tool, 1, 2, true)));

    // A second session while one is active: refused, nothing written.
    let (reply, changed) = wrote(&mut server, &root, tool, json!({"action": "start"}));
    assert_eq!(reply["result"]["isError"], true, "{reply}");
    assert_eq!(changed, Vec::<PathBuf>::new());
    assert_eq!(last_completed(&server), Some(completed(tool, 0, 0, false)));
}

// 11
#[test]
fn previews_are_no_mutation() {
    let mut server = TestProject::new()
        .enabling(&["@specforge/software"])
        .file("test.spec", TEST_SPEC)
        .file("a.spec", UNFORMATTED)
        .file("old.spec", OLD)
        .serve_components();
    let root = server.root().to_path_buf();
    let before = files_under(&root);
    let generation = server.state().session_generation();

    for (tool, arguments) in [
        ("specforge.format", json!({"check": true})),
        ("specforge.format", json!({"diff": true})),
        ("specforge.format", json!({"write": false})),
        (
            "specforge.rename",
            json!({"entity_id": "alpha", "new_name": "omega", "dry_run": true}),
        ),
        (
            "specforge.add_extension",
            json!({"specifiers": [PRODUCT], "dry_run": true}),
        ),
        (
            "specforge.add_extension",
            json!({"specifiers": [greet()], "dry_run": true}),
        ),
        (
            "specforge.remove_extension",
            json!({"name": "@specforge/software", "dry_run": true}),
        ),
        ("specforge.migrate", json!({"dry_run": true})),
    ] {
        let reply = call_tool(&mut server, tool, arguments.clone());
        assert!(reply["result"].is_object(), "{tool} {arguments}: {reply}");
        assert_eq!(
            last_completed(&server),
            None,
            "{tool} {arguments} previewed: {reply}"
        );
    }
    assert_eq!(files_under(&root), before, "a preview writes nothing");
    assert_eq!(server.state().session_generation(), generation);

    // An extension already installed from a registry answers "already
    // present", and its dry run is still a preview.
    let mut installed = registry_installed_greet();
    let reply = call_tool(
        &mut installed,
        "specforge.add_extension",
        json!({"specifiers": ["@sdk/greet@0.0.0-test"], "dry_run": true}),
    );
    assert_eq!(
        tool_json(&reply)["added"][0]["action"],
        "already_present",
        "{reply}"
    );
    assert_eq!(last_completed(&installed), None, "{reply}");
    assert!(events(&installed, "extension_added").is_empty());
}

// 12
#[test]
fn a_mutation_refused_for_its_arguments_is_a_failed_mutation() {
    let mut server = rename_server();

    let reply = call_tool(&mut server, "specforge.rename", json!({"entity_id": 3}));
    assert_eq!(reply["result"]["isError"], true, "{reply}");
    assert_eq!(
        events(&server, "mcp_mutation_completed"),
        [completed("specforge.rename", 0, 0, false)]
    );

    let reply = call_tool(
        &mut server,
        "specforge.rename",
        json!({"entity_id": 3, "dry_run": true}),
    );
    assert_eq!(reply["result"]["isError"], true, "{reply}");
    // Arguments that do not parse cannot say they asked for a preview
    // (ADR 0022): a failed mutation.
    assert_eq!(
        events(&server, "mcp_mutation_completed"),
        [
            completed("specforge.rename", 0, 0, false),
            completed("specforge.rename", 0, 0, false)
        ]
    );
}

// 13
#[test]
fn the_domain_event_precedes_the_mutation_event() {
    let mut server = empty_components();

    let since = server.state().events.len();
    assert_ok(&call_tool(
        &mut server,
        "specforge.add_extension",
        json!({"specifiers": [PRODUCT]}),
    ));
    assert_eq!(
        event_names_since(
            &server,
            since,
            &[
                "mcp_tool_invoked",
                "extension_added",
                "mcp_mutation_completed"
            ]
        ),
        [
            "mcp_tool_invoked",
            "extension_added",
            "mcp_mutation_completed"
        ]
    );

    let scratch = tempfile::TempDir::new().unwrap();
    let dir = scratch.path().join("fresh");
    let since = server.state().events.len();
    assert_ok(&call_tool(
        &mut server,
        "specforge.init",
        json!({"path": dir.to_str().unwrap(), "name": "fresh"}),
    ));
    assert_eq!(
        event_names_since(
            &server,
            since,
            &[
                "mcp_tool_invoked",
                "project_initialized",
                "mcp_mutation_completed"
            ]
        ),
        [
            "mcp_tool_invoked",
            "project_initialized",
            "mcp_mutation_completed"
        ]
    );
}

// 14 (split: the spec's payloads)
#[specforge_test(
    behavior = "extension_added",
    verify = "emits extension_added with correct extensionSpecifier after specforge add"
)]
fn extension_added_names_the_specifier_and_the_total() {
    let mut server = empty_components();
    let root = server.root().to_path_buf();
    assert_ok(&call_tool(
        &mut server,
        "specforge.add_extension",
        json!({"specifiers": [PRODUCT]}),
    ));
    // The product builtin and the builtin peers it requires.
    let total = enabled(&root).len();
    assert!(total >= 1);
    assert_eq!(
        events(&server, "extension_added"),
        [json!({"extensionSpecifier": PRODUCT, "totalExtensions": total, "wasDuplicate": false})]
    );

    // A local blob: the specifier as the call gave it, not the name it
    // declares.
    assert_ok(&call_tool(
        &mut server,
        "specforge.add_extension",
        json!({"specifiers": [greet()]}),
    ));
    assert_eq!(
        events(&server, "extension_added")[1],
        json!({"extensionSpecifier": greet(), "totalExtensions": total + 1, "wasDuplicate": false})
    );
}

#[specforge_test(
    behavior = "provide_mcp_add_extension_tool",
    verify = "specforge.add_extension adds every specifier as one change, one extension_added per specifier"
)]
fn add_extension_adds_every_specifier_as_one_change() {
    let mut server = empty_components();
    let root = server.root().to_path_buf();

    let (reply, changed) = wrote(
        &mut server,
        &root,
        "specforge.add_extension",
        json!({"specifiers": [PRODUCT, greet(), PRODUCT]}),
    );

    assert_ok(&reply);
    let payload = tool_json(&reply);
    let added = payload["added"].as_array().unwrap();
    let actions: Vec<&str> = added
        .iter()
        .map(|a| a["action"].as_str().unwrap())
        .collect();
    assert_eq!(actions, ["enabled", "installed", "duplicate"], "{payload}");
    let specifiers: Vec<Value> = added.iter().map(|a| a["specifier"].clone()).collect();
    assert_eq!(specifiers, [json!(PRODUCT), json!(greet()), json!(PRODUCT)]);
    assert_eq!(added[2]["duplicate_of"], PRODUCT, "{payload}");
    let three = [
        ".specforge/extensions/@sdk/greet/extension.wasm",
        "specforge.json",
        "specforge.lock",
    ];
    assert_eq!(payload["files_written"], json!(three), "{payload}");
    assert_eq!(changed, names(&three));
    let total = enabled(&root).len();
    assert_eq!(
        events(&server, "extension_added"),
        [
            json!({"extensionSpecifier": PRODUCT, "totalExtensions": total, "wasDuplicate": false}),
            json!({"extensionSpecifier": greet(), "totalExtensions": total, "wasDuplicate": false}),
            json!({"extensionSpecifier": PRODUCT, "totalExtensions": total, "wasDuplicate": true}),
        ]
    );
    assert_eq!(events(&server, "mcp_mutation_completed").len(), 1);
    assert_eq!(last_completed(&server).unwrap()["files_changed"], 3);

    // One specifier refused: nothing written, no extension added, the refusal naming it.
    let mut server = empty_components();
    let root = server.root().to_path_buf();
    let missing = root.join("missing.wasm");
    let missing = missing.to_str().unwrap();
    let (reply, changed) = wrote(
        &mut server,
        &root,
        "specforge.add_extension",
        json!({"specifiers": [PRODUCT, missing]}),
    );
    let error = crate::tool_errors::mcp_error(&reply);
    assert!(
        error["message"]
            .as_str()
            .unwrap()
            .contains(&format!("adding '{missing}': file not found")),
        "{reply}"
    );
    assert!(changed.is_empty(), "{changed:?}");
    assert!(events(&server, "extension_added").is_empty());

    // No specifier, or the one-specifier argument of old: refused before anything runs.
    for arguments in [json!({"specifiers": []}), json!({"specifier": PRODUCT})] {
        let reply = call_tool(&mut server, "specforge.add_extension", arguments.clone());
        crate::tool_errors::mcp_error(&reply);
        assert!(events(&server, "extension_added").is_empty(), "{arguments}");
    }
}

/// Init `name` in a new directory of `scratch` with `extensions`; the
/// `project_initialized` events recorded.
fn initialized(name: &str, extensions: Value) -> Vec<Value> {
    let mut server = empty_components();
    let scratch = tempfile::TempDir::new().unwrap();
    let dir = scratch.path().join(name);
    assert_ok(&call_tool(
        &mut server,
        "specforge.init",
        json!({"path": dir.to_str().unwrap(), "name": name, "extensions": extensions}),
    ));
    events(&server, "project_initialized")
}

#[specforge_test(
    behavior = "project_initialized",
    verify = "emits project_initialized with correct projectName and extensionCount"
)]
fn project_initialized_counts_the_extensions() {
    let none = initialized("newone", json!([]));
    assert_eq!(none.len(), 1, "{none:?}");
    assert_eq!(none[0]["projectName"], "newone");
    assert_eq!(none[0]["extensionCount"], 0);

    // A builtin and a local blob: product (and the builtin peers it
    // requires), and @sdk/greet.
    let some = initialized("newtwo", json!([PRODUCT, greet()]));
    assert_eq!(some[0]["projectName"], "newtwo");
    let count = some[0]["extensionCount"].as_u64().unwrap();
    assert!(count >= 2, "{some:?}");
}

#[specforge_test(
    behavior = "project_initialized",
    verify = "specFilePath refers to the starter entity spec file, not specforge.json"
)]
fn project_initialized_names_the_starter_file() {
    let events = initialized("starter", json!([]));
    assert_eq!(
        events,
        [json!({"projectName": "starter", "extensionCount": 0, "specFilePath": "spec/hello.spec"})]
    );
}

// 15
#[test]
fn a_write_brings_the_served_project_up_to_date() {
    let mut server = TestProject::new()
        .enabling(&["@specforge/software"])
        .file("test.spec", TEST_SPEC)
        .file("a.spec", UNFORMATTED)
        .file("old.spec", OLD)
        .serve_components();

    for (tool, arguments) in [
        ("specforge.format", json!({})),
        (
            "specforge.rename",
            json!({"entity_id": "alpha", "new_name": "omega"}),
        ),
        ("specforge.add_extension", json!({"specifiers": [PRODUCT]})),
        ("specforge.remove_extension", json!({"name": PRODUCT})),
        ("specforge.migrate", json!({})),
    ] {
        let generation = server.state().session_generation();
        let reply = call_tool(&mut server, tool, arguments.clone());
        assert_ok(&reply);
        assert!(
            server.state().session_generation() > generation,
            "{tool} {arguments} wrote the served project, which was not brought up to date"
        );
    }
    // What is served is what is on disk.
    assert!(server.state().graph().node("omega").is_some());
    assert!(server.state().graph().node("alpha").is_none());
}

// 16
#[test]
fn infer_session_leaves_the_served_project_as_it_is() {
    let mut server = TestProject::new()
        .file("test.spec", TEST_SPEC)
        .file("src/main.rs", "fn main() {}\n")
        .serve(&[TestExtension::software()]);
    let generation = server.state().session_generation();

    assert_ok(&call_tool(
        &mut server,
        "specforge.infer_session",
        json!({"action": "start"}),
    ));

    // specforge-infer.json is no project input.
    assert_eq!(server.state().session_generation(), generation);
}

/// The `files_written` a mutation reply lists: in its result (and its
/// `structuredContent`, when sent), or in a refusal's `data`.
fn files_written(reply: &Value) -> Vec<String> {
    let text = tool_json(reply);
    let listed = match reply["result"]["isError"] == true {
        true => &text["data"]["files_written"],
        false => &text["files_written"],
    };
    let structured = &reply["result"]["structuredContent"];
    if structured.is_object() {
        assert_eq!(&structured["files_written"], listed, "{reply}");
    }
    listed
        .as_array()
        .unwrap_or_else(|| panic!("no files_written in {reply}"))
        .iter()
        .map(|f| f.as_str().expect("a file name").to_string())
        .collect()
}

/// `changed` as the names `files_written` lists.
fn shown(changed: &[PathBuf]) -> Vec<String> {
    changed.iter().map(|p| p.display().to_string()).collect()
}

/// Call `tool`; the files it wrote on disk equal its reply's
/// `files_written`, whose length is its event's `files_changed`.
fn lists_what_it_wrote(server: &mut McpServer, root: &Path, tool: &str, arguments: Value) -> Value {
    let (reply, changed) = wrote(server, root, tool, arguments.clone());
    let listed = files_written(&reply);
    assert_eq!(listed, shown(&changed), "{tool} {arguments}: {reply}");
    let event = last_completed(server).expect("a mutation event");
    assert_eq!(event["toolName"], tool);
    assert_eq!(event["files_changed"], listed.len(), "{tool} {arguments}");
    reply
}

#[specforge_test(
    behavior = "mcp_mutation_completed",
    verify = "a mutation's reply lists in files_written the files files_changed counts"
)]
fn every_mutation_reply_lists_what_it_wrote() {
    // Enable a builtin, enable it again, disable it.
    let mut server = empty_components();
    let root = server.root().to_path_buf();
    let add = "specforge.add_extension";
    let enabled = lists_what_it_wrote(&mut server, &root, add, json!({"specifiers": [PRODUCT]}));
    assert_eq!(files_written(&enabled), ["specforge.json"]);
    let again = lists_what_it_wrote(&mut server, &root, add, json!({"specifiers": [PRODUCT]}));
    assert_eq!(files_written(&again), Vec::<String>::new());
    let remove = "specforge.remove_extension";
    lists_what_it_wrote(&mut server, &root, remove, json!({"name": PRODUCT}));

    // Install a local extension, then remove it: module, lock, config.
    let installed = lists_what_it_wrote(&mut server, &root, add, json!({"specifiers": [greet()]}));
    assert_eq!(files_written(&installed).len(), 3);
    lists_what_it_wrote(&mut server, &root, remove, json!({"name": GREET}));

    // Init with a local extension elsewhere, and beside a complete
    // .gitignore: named from the new project's root.
    let scratch = tempfile::TempDir::new().unwrap();
    let n1 = scratch.path().join("n1");
    let init = "specforge.init";
    let first = lists_what_it_wrote(
        &mut server,
        &n1,
        init,
        json!({"path": n1.to_str().unwrap(), "name": "newone", "extensions": [greet()]}),
    );
    assert_eq!(files_written(&first).len(), 5);
    let n2 = scratch.path().join("n2");
    std::fs::create_dir_all(&n2).unwrap();
    std::fs::write(
        n2.join(".gitignore"),
        "specforge-infer.json\nspecforge-report.json\n.specforge/\n",
    )
    .unwrap();
    lists_what_it_wrote(
        &mut server,
        &n2,
        init,
        json!({"path": n2.to_str().unwrap(), "name": "newtwo"}),
    );

    // A migration with backups.
    let mut migrating = TestProject::new()
        .file("old.spec", OLD)
        .serve(&[TestExtension::software()]);
    let root = migrating.root().to_path_buf();
    let migrated = lists_what_it_wrote(&mut migrating, &root, "specforge.migrate", json!({}));
    assert_eq!(
        files_written(&migrated),
        [".specforge/migration.json", "old.spec", "old.spec.bak"]
    );

    // A rename.
    let mut renaming = rename_server();
    let root = renaming.root().to_path_buf();
    lists_what_it_wrote(
        &mut renaming,
        &root,
        "specforge.rename",
        json!({"entity_id": "alpha", "new_name": "omega"}),
    );

    // An inference step.
    let mut inferring = TestProject::new()
        .file("src/main.rs", "fn main() {}\n")
        .serve(&[TestExtension::software()]);
    let root = inferring.root().to_path_buf();
    lists_what_it_wrote(
        &mut inferring,
        &root,
        "specforge.infer_session",
        json!({"action": "start"}),
    );

    // A format that fails on one file: the refusal's data names the one it
    // wrote.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut formatting = TestProject::new()
            .file("a.spec", UNFORMATTED)
            .file("b.spec", &UNFORMATTED.replace("messy", "other"))
            .serve(&[TestExtension::software()]);
        let root = formatting.root().to_path_buf();
        let locked = root.join("a.spec");
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o444)).unwrap();
        let failed = lists_what_it_wrote(&mut formatting, &root, "specforge.format", json!({}));
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o644)).unwrap();
        assert_eq!(failed["result"]["isError"], true, "{failed}");
        assert_eq!(files_written(&failed), ["b.spec"]);
    }
}

#[test]
fn a_preview_reply_has_no_files_written() {
    let mut server = TestProject::new()
        .enabling(&["@specforge/software"])
        .file("test.spec", TEST_SPEC)
        .file("a.spec", UNFORMATTED)
        .file("old.spec", OLD)
        .serve_components();

    for (tool, arguments) in [
        ("specforge.format", json!({"check": true})),
        ("specforge.format", json!({"diff": true})),
        (
            "specforge.rename",
            json!({"entity_id": "alpha", "new_name": "omega", "dry_run": true}),
        ),
        (
            "specforge.add_extension",
            json!({"specifiers": [PRODUCT], "dry_run": true}),
        ),
        (
            "specforge.remove_extension",
            json!({"name": "@specforge/software", "dry_run": true}),
        ),
        ("specforge.migrate", json!({"dry_run": true})),
    ] {
        let reply = call_tool(&mut server, tool, arguments.clone());
        let text = tool_json(&reply);
        assert!(
            text.get("files_written").is_none(),
            "{tool} {arguments}: {reply}"
        );
        assert!(
            text["data"].get("files_written").is_none(),
            "{tool} {arguments}: {reply}"
        );
        assert!(
            reply["result"]["structuredContent"]
                .get("files_written")
                .is_none(),
            "{tool} {arguments}: {reply}"
        );
    }

    let mut installed = registry_installed_greet();
    let reply = call_tool(
        &mut installed,
        "specforge.add_extension",
        json!({"specifiers": ["@sdk/greet@0.0.0-test"], "dry_run": true}),
    );
    assert!(tool_json(&reply).get("files_written").is_none(), "{reply}");
    assert!(
        reply["result"]["structuredContent"]
            .get("files_written")
            .is_none(),
        "{reply}"
    );
}

/// The greet module installed from a registry (a `source: registry` lock
/// entry) and enabled: what `specforge add @sdk/greet@0.0.0-test` leaves.
fn registry_installed_greet() -> Served {
    let project = TestProject::new().enabling(&[GREET]);
    specforge_installed::testing::install_module(
        project.root(),
        GREET,
        &std::fs::read(greet_blob()).unwrap(),
    );
    project.serve_components()
}

// P1
#[specforge_test(
    behavior = "mcp_mutation_completed",
    verify = "a call whose arguments ask for a preview records nothing and lists no files_written, whatever its operation answered"
)]
fn a_dry_run_is_a_preview_whatever_the_operation_answers() {
    let mut server = registry_installed_greet();
    let since = server.state().events.len();
    let reply = call_tool(
        &mut server,
        "specforge.add_extension",
        json!({"specifiers": ["@sdk/greet@0.0.0-test"], "dry_run": true}),
    );
    let payload = tool_json(&reply);
    assert_eq!(payload["added"][0]["action"], "already_present", "{reply}");
    assert!(payload.get("files_written").is_none(), "{reply}");
    assert_eq!(
        event_names_since(
            &server,
            since,
            &["extension_added", "mcp_mutation_completed"]
        ),
        Vec::<String>::new()
    );
}

// P2
#[specforge_test(
    behavior = "mcp_mutation_completed",
    verify = "a mutation's reply lists in files_written the files files_changed counts"
)]
fn a_refused_preview_lists_nothing_and_a_refused_mutation_lists_no_file() {
    let mut server = rename_server();
    for (tool, preview, real) in [
        (
            "rename",
            json!({"entity_id": "ghost", "new_name": "ghost2", "dry_run": true}),
            json!({"entity_id": "ghost", "new_name": "ghost2"}),
        ),
        (
            "migrate",
            json!({"target_version": "x.y", "dry_run": true}),
            json!({"target_version": "x.y"}),
        ),
        (
            "format",
            json!({"paths": ["missing.spec"], "check": true}),
            json!({"paths": ["missing.spec"]}),
        ),
    ] {
        let tool = format!("specforge.{tool}");
        let since = server.state().events.len();
        let reply = call_tool(&mut server, &tool, preview.clone());
        assert_eq!(
            reply["result"]["isError"], true,
            "{tool} {preview}: {reply}"
        );
        assert!(
            tool_json(&reply)["data"].get("files_written").is_none(),
            "{tool} {preview}: {reply}"
        );
        assert_eq!(
            event_names_since(&server, since, &["mcp_mutation_completed"]),
            Vec::<String>::new(),
            "{tool} {preview}"
        );

        let reply = call_tool(&mut server, &tool, real.clone());
        assert_eq!(reply["result"]["isError"], true, "{tool} {real}: {reply}");
        assert_eq!(files_written(&reply), Vec::<String>::new(), "{tool} {real}");
        assert_eq!(
            last_completed(&server),
            Some(completed(&tool, 0, 0, false)),
            "{tool} {real}"
        );
    }
}

// P3
#[specforge_test(
    behavior = "provide_mcp_rename_tool",
    verify = "rename's updated diagnostics follow a successful write only: a preview and a failure carry none"
)]
fn rename_returns_diagnostics_after_a_successful_write_only() {
    let mut server = rename_server();
    let tool = "specforge.rename";

    let reply = call_tool(
        &mut server,
        tool,
        json!({"entity_id": "alpha", "new_name": "omega", "dry_run": true}),
    );
    assert_ok(&reply);
    assert!(tool_json(&reply).get("diagnostics").is_none(), "{reply}");

    let reply = call_tool(
        &mut server,
        tool,
        json!({"entity_id": "ghost", "new_name": "ghost2"}),
    );
    assert_eq!(reply["result"]["isError"], true, "{reply}");
    let text = tool_json(&reply);
    assert!(text.get("diagnostics").is_none(), "{reply}");
    assert!(text["data"].get("diagnostics").is_none(), "{reply}");

    let reply = call_tool(
        &mut server,
        tool,
        json!({"entity_id": "alpha", "new_name": "omega"}),
    );
    assert_ok(&reply);
    let payload = tool_json(&reply);
    let codes: Vec<&str> = payload["diagnostics"]
        .as_array()
        .unwrap_or_else(|| panic!("no diagnostics in {reply}"))
        .iter()
        .map(|d| d["code"].as_str().expect("a code"))
        .collect();
    let served: Vec<String> = server
        .state()
        .diagnostics()
        .iter()
        .map(|d| d.code.to_string())
        .collect();
    assert_eq!(codes, served, "{reply}");
    assert!(payload["files_written"].is_array(), "{reply}");
}
