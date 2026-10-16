//! The infer prompt over a served environment, through `prompts/get`: its
//! scopes, guides, plan paging and refusals.

use crate::support::*;
use serde_json::{Value, json};
use specforge_extension_sdk::prelude::*;
use specforge_mcp::McpServer;
use specforge_test::prelude::*;

/// The infer prompt's reply for `arguments`.
fn infer(server: &mut McpServer, arguments: Value) -> Value {
    get_prompt(server, "specforge://prompts/infer", arguments)
}

/// The test extension, `@specforge/test`, declaring `kind_name` with
/// `guide` and a `description` string field.
fn test_extension(kind_name: &str, guide: Option<&str>) -> TestExtension {
    let (kind_name, guide) = (kind_name.to_string(), guide.map(str::to_string));
    TestExtension::named("@specforge/test").declaring(move |c| {
        c.kind(&kind_name, |k| {
            k.description(&format!("A test {kind_name} entity"));
            if let Some(guide) = &guide {
                k.inference_guide(guide);
            }
            k.field("description", |f| {
                f.field_type(FieldType::String).description("A description");
            });
        });
    })
}

/// An initialized server over an empty project serving the test
/// extension, which declares `kind_name` with `guide`.
fn make_state_with_kind(kind_name: &str, guide: Option<&str>) -> Served {
    TestProject::new().serve(&[test_extension(kind_name, guide)])
}

/// As [`make_state_with_kind`] for `behavior`, the project declaring the
/// behavior `my_behavior` (test.spec).
fn state_with_my_behavior() -> Served {
    TestProject::new()
        .file("test.spec", "behavior my_behavior {\n}\n")
        .serve(&[test_extension("behavior", Some("guide text"))])
}

/// A served project with `count` Rust sources under `src/`, and an
/// extension that analyzes `.rs` files.
fn plan_state_with_sources(count: usize) -> Served {
    let project = (0..count).fold(TestProject::new(), |project, i| {
        project.file(&format!("src/mod_{i:02}.rs"), "fn stub() {}\n")
    });
    project.serve(&[
        test_extension("behavior", Some("guide text")).declaring(|c| {
            c.analyzer("rust", |a| {
                a.file_extensions(&[".rs"]).scan(|_| ScanResponse {
                    items: Vec::new(),
                    language: None,
                });
            });
        }),
    ])
}

fn plan_payload(server: &mut McpServer, arguments: Value) -> Value {
    prompt_payload(&infer(server, arguments))
}

/// Kinds are keywords, matched exactly: `kind:Behavior` is refused, and the
/// suggestion names the kind (ADR 0015 "Query" Q3).
#[specforge_test(
    behavior = "provide_infer_kind_scope",
    verify = "the kind scope renders the kind's guide; an undeclared kind is refused on scope with the closest declared kind"
)]
fn kind_scope_is_exact_and_suggests_the_kind() {
    let mut state = state_with_my_behavior();
    let resp = infer(&mut state, json!({"scope": "kind:Behavior"}));
    let data = &resp["error"]["data"];
    assert_eq!(resp["error"]["code"], -32602, "{resp}");
    assert_eq!(data["message"], "unknown entity kind 'Behavior'", "{resp}");
    assert_eq!(data["argument"], "scope");
    assert_eq!(data["data"]["suggestion"], "did you mean 'behavior'?");
}

/// An extension keyword written with a capital is listed as declared: the
/// entities of that kind are found and the example is written with the
/// keyword the language accepts.
#[specforge_test(
    behavior = "provide_infer_kind_scope",
    verify = "the kind scope renders the kind's guide; an undeclared kind is refused on scope with the closest declared kind"
)]
fn a_capitalized_keyword_is_kept_as_declared() {
    let mut state = TestProject::new()
        .file("d.spec", "ADR pick_db {\n}\n")
        .serve(&[test_extension("ADR", Some("g"))]);
    let resp = infer(&mut state, json!({"scope": "kind:ADR"}));
    let content: Value = prompt_payload(&resp);
    assert_eq!(
        content["existing_entity_ids"],
        json!(["pick_db"]),
        "{content}"
    );
    assert!(
        content["example"]
            .as_str()
            .unwrap()
            .starts_with("ADR example_ADR"),
        "{content}"
    );
}

#[test]
fn the_overview_renders_the_guide() {
    let mut state = make_state_with_kind("behavior", Some("Look for public functions"));
    let content = prompt_payload(&infer(&mut state, json!({})));
    assert_eq!(content["installed_extensions"][0], "@specforge/test");
    assert_eq!(
        content["kinds"][0]["inference_guide"],
        "Look for public functions"
    );
    assert_eq!(content["kinds"][0]["extension"], "@specforge/test");
    assert!(
        content["output_format"]
            .as_str()
            .unwrap()
            .contains("Write .spec files in the ./ directory"),
        "{content}"
    );
    assert!(content["validation"].is_string(), "{content}");
}

/// A scope the prompt does not know is refused on `scope`, naming the five it knows.
#[specforge_test(
    behavior = "read_infer_prompt_scope",
    verify = "a scope the prompt does not know is refused on scope, naming the scopes it knows"
)]
fn an_unknown_scope_is_refused_on_scope() {
    let mut state = make_state_with_kind("behavior", Some("guide text"));
    for scope in [
        "unknown:value",
        "kinds:behavior",
        "Kind:behavior",
        "Plan",
        "workflows",
    ] {
        let resp = infer(&mut state, json!({"scope": scope}));
        assert_eq!(resp["error"]["code"], -32602, "{scope}: {resp}");
        let data = &resp["error"]["data"];
        assert_eq!(data["argument"], "scope", "{scope}: {resp}");
        assert_eq!(data["code"], "invalid_input", "{scope}: {resp}");
        let message = resp["error"]["message"].as_str().unwrap();
        assert!(
            message.starts_with(&format!("unknown scope '{scope}'")),
            "{message}"
        );
        for known in ["kind:<name>", "file:<path>", "plan", "workflow"] {
            assert!(message.contains(known), "{known} in {message}");
        }
    }
}

/// No scope, an empty scope and `overview` each answer every kind's guide.
#[specforge_test(
    behavior = "read_infer_prompt_scope",
    verify = "no scope, an empty scope or scope overview renders every kind's guide"
)]
fn the_overview_is_no_scope_or_its_own_name() {
    let mut state = make_state_with_kind("behavior", Some("guide text"));
    for arguments in [
        json!({}),
        json!({"scope": "overview"}),
        json!({"scope": ""}),
    ] {
        let content = prompt_payload(&infer(&mut state, arguments.clone()));
        assert_eq!(content["scope"], "overview", "{arguments}: {content}");
        assert!(content.get("installed_extensions").is_some(), "{content}");
    }
}

#[test]
fn empty_kind_scope_returns_error() {
    let mut state = make_state_with_kind("behavior", Some("guide text"));
    let resp = infer(&mut state, json!({"scope": "kind:"}));
    assert_eq!(resp["error"]["code"], -32602, "{resp}");
    assert_eq!(resp["error"]["data"]["argument"], "scope");
}

#[test]
fn unknown_kind_returns_error() {
    let mut state = make_state_with_kind("behavior", Some("guide text"));
    let resp = infer(&mut state, json!({"scope": "kind:nonexistent"}));
    assert_eq!(resp["error"]["code"], -32602, "{resp}");
    let text = resp["error"]["message"].as_str().unwrap();
    assert!(
        text.contains("nonexistent"),
        "Error should name the unknown kind: {text}"
    );
}

#[test]
fn unknown_kind_names_the_closest_installed_kind() {
    let mut state = make_state_with_kind("behavior", Some("guide text"));
    let resp = infer(&mut state, json!({"scope": "kind:behaviour"}));
    let data = &resp["error"]["data"];
    assert_eq!(data["message"], "unknown entity kind 'behaviour'", "{resp}");
    assert_eq!(data["code"], "invalid_input");
    assert_eq!(data["argument"], "scope");
    assert_eq!(data["data"]["suggestion"], "did you mean 'behavior'?");
}

#[test]
fn empty_file_scope_returns_error() {
    let mut state = make_state_with_kind("behavior", Some("guide text"));
    let resp = infer(&mut state, json!({"scope": "file:"}));
    assert_eq!(resp["error"]["code"], -32602, "{resp}");
    assert_eq!(resp["error"]["data"]["argument"], "scope");
}

#[test]
fn plan_scope_returns_kind_priorities() {
    let mut state = state_with_my_behavior();
    let resp = infer(&mut state, json!({"scope": "plan"}));
    let content: Value = prompt_payload(&resp);
    assert_eq!(content["scope"], "plan");
    let priorities = content["kind_priorities"].as_array().unwrap();
    assert!(!priorities.is_empty());
    // Kinds with no entity come first: the one kind of the test extension
    // that has an entity is last.
    let last = priorities.last().unwrap();
    assert_eq!(last["kind"], "behavior");
    assert_eq!(last["existing_count"], 1);
}

#[test]
fn plan_scope_respects_target_directory() {
    let mut state = make_state_with_kind("behavior", Some("guide text"));
    let resp = infer(
        &mut state,
        json!({"scope": "plan", "target_spec_directory": "specs/"}),
    );
    let content: Value = prompt_payload(&resp);
    assert_eq!(content["target_spec_directory"], "specs/");
}

#[test]
fn plan_scope_includes_progress() {
    let mut state = make_state_with_kind("behavior", Some("guide text"));
    let resp = infer(&mut state, json!({"scope": "plan"}));
    let content: Value = prompt_payload(&resp);
    assert!(content["progress"]["files_total"].is_number());
}

#[specforge_test(
    behavior = "provide_infer_workflow_scope",
    verify = "workflow returns step-by-step protocol"
)]
#[specforge_test(
    behavior = "provide_infer_workflow_scope",
    verify = "workflow documents retry pattern"
)]
fn workflow_scope_returns_protocol() {
    let mut state = make_state_with_kind("behavior", Some("guide text"));
    let resp = infer(&mut state, json!({"scope": "workflow"}));
    let instruction = resp["result"]["messages"][0]["content"]["text"]
        .as_str()
        .unwrap_or_else(|| panic!("{resp}"));
    assert!(instruction.contains("Start Session"));
    assert!(instruction.contains("mark_analyzed"));
    assert!(instruction.contains("End Session"));
    assert!(instruction.contains("Retry Pattern"));
    assert!(
        instruction.contains("`specforge.validate`"),
        "{instruction}"
    );
}

#[specforge_test(
    behavior = "provide_infer_workflow_scope",
    verify = "the workflow lists the tools it names, as the tool table names them"
)]
fn workflow_scope_lists_tools_and_kinds() {
    let mut state = make_state_with_kind("behavior", Some("guide text"));
    let resp = infer(&mut state, json!({"scope": "workflow"}));
    let content: Value = prompt_payload(&resp);
    assert_eq!(
        content["tools"],
        json!([
            "specforge.infer_session",
            "specforge.infer_progress",
            "specforge.validate",
            "specforge.query",
            "specforge.search",
            "specforge.schema"
        ])
    );
    let kinds = content["installed_kinds"].as_array().unwrap();
    assert!(kinds.contains(&Value::from("behavior")));
}

#[specforge_test(
    behavior = "provide_infer_plan_scope",
    verify = "plan pages the file lists 50 at a time from the cursor"
)]
fn plan_scope_caps_file_lists_at_50() {
    let mut state = plan_state_with_sources(60);
    let content = plan_payload(&mut state, json!({"scope": "plan"}));
    let files = content["unanalyzed_files"].as_array().unwrap();
    assert_eq!(files.len(), 50, "50 files, files only");
    assert!(
        files.iter().all(|f| f.as_str().unwrap().ends_with(".rs")),
        "a page lists only files: {files:?}"
    );
    assert_eq!(content["unanalyzed_total"], 60);
    assert_eq!(content["next_cursor"], 50);
}

#[specforge_test(
    behavior = "provide_infer_plan_scope",
    verify = "the last page of a plan has no next cursor, and a page lists only files"
)]
fn the_last_plan_page_has_no_next_cursor() {
    let mut state = plan_state_with_sources(60);
    let first = plan_payload(&mut state, json!({"scope": "plan"}));
    assert_eq!(first["scope"], "plan");
    assert_eq!(first["next_cursor"], 50);
    let last = plan_payload(&mut state, json!({"scope": "plan", "cursor": 50}));
    assert_eq!(last["scope"], "plan");
    assert!(last.get("next_cursor").is_none(), "{last}");
    assert_eq!(last["unanalyzed_files"].as_array().unwrap().len(), 10);
    assert_eq!(last["unanalyzed_total"], 60);
    for page in [&first, &last] {
        assert!(
            page["unanalyzed_files"]
                .as_array()
                .unwrap()
                .iter()
                .all(|f| f.as_str().unwrap().ends_with(".rs")),
            "a page lists only files: {page}"
        );
    }
}

#[test]
fn file_scope_lists_the_entities_anchored_to_the_file() {
    // auth_login (auth.spec) is anchored to src/auth.rs:3; cache_get, which
    // no spec declares, to src/cache.rs.
    let mut state = TestProject::new()
        .file("auth.spec", "behavior auth_login {\n}\n")
        .file("src/auth.rs", "\n\npub fn login() {}\n")
        .file(
            "specforge-anchors.json",
            json!({"version": 1, "anchors": [
                {"entity_id": "auth_login", "file": "src/auth.rs", "line": 3,
                 "symbol_name": "login", "item_kind": "fn", "scanner": "manual"},
                {"entity_id": "cache_get", "file": "src/cache.rs", "line": 1,
                 "symbol_name": "get", "item_kind": "fn", "scanner": "manual"},
            ]})
            .to_string(),
        )
        .serve(&[test_extension("behavior", Some("guide text"))]);

    let content = prompt_payload(&infer(&mut state, json!({"scope": "file:src/auth.rs"})));
    assert_eq!(content["match_mode"], "exact");
    let refs = content["existing_entities_referencing_file"]
        .as_array()
        .unwrap();
    assert_eq!(refs.len(), 1, "{content}");
    assert_eq!(refs[0]["entity_id"], "auth_login");
    assert_eq!(refs[0]["kind"], "behavior");
    assert_eq!(refs[0]["line"], 3);
    assert_eq!(refs[0]["symbol_name"], "login");

    // A directory lists the files under it; a substring is no match.
    let content = prompt_payload(&infer(&mut state, json!({"scope": "file:src"})));
    assert_eq!(content["match_mode"], "directory");
    assert_eq!(
        content["existing_entities_referencing_file"]
            .as_array()
            .map(Vec::len),
        Some(2)
    );
    let content = prompt_payload(&infer(&mut state, json!({"scope": "file:e.rs"})));
    assert_eq!(content["match_mode"], "none");
}

/// A project on disk, compiled with `@specforge/software`: behavior
/// `alpha` declared in `spec/main.spec` and anchored to `src/login.rs:1`;
/// `src/other.rs` anchors nothing.
fn anchored_project() -> (McpServer, tempfile::TempDir) {
    let dir = tempfile::TempDir::new().unwrap();
    let root = dir.path();
    std::fs::create_dir_all(root.join("spec")).unwrap();
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(
        root.join("specforge.json"),
        json!({"name": "probe", "version": "0.1.0", "spec_root": "spec",
               "extensions": ["@specforge/software"]})
        .to_string(),
    )
    .unwrap();
    std::fs::write(
        root.join("spec/main.spec"),
        "behavior alpha \"Alpha\" {\n  contract \"first\"\n}\n",
    )
    .unwrap();
    std::fs::write(root.join("src/login.rs"), "pub fn login() {}\n").unwrap();
    std::fs::write(root.join("src/other.rs"), "pub fn other() {}\n").unwrap();
    std::fs::write(
        root.join("specforge-anchors.json"),
        json!({"version": 1, "anchors": [{"entity_id": "alpha", "file": "src/login.rs",
               "line": 1, "symbol_name": "login", "item_kind": "fn", "scanner": "manual"}]})
        .to_string(),
    )
    .unwrap();
    let mut server = McpServer::with_project_root(root.to_path_buf());
    server.handle_message(
        &json!({"jsonrpc": "2.0", "id": 0, "method": "initialize", "params": {}}).to_string(),
    );
    (server, dir)
}

/// `specforge.find_spec_for_source`'s structured result for `file_path`.
fn find_spec_for_source(server: &mut McpServer, file_path: &str) -> Value {
    tool(
        server,
        "specforge.find_spec_for_source",
        json!({"file_path": file_path}),
    )
}

fn entity_ids(entities: &Value) -> Vec<String> {
    entities
        .as_array()
        .unwrap_or_else(|| panic!("{entities}"))
        .iter()
        .map(|e| e["entity_id"].as_str().unwrap().to_string())
        .collect()
}

#[specforge_test(
    behavior = "provide_infer_file_scope",
    verify = "file scope lists the entities find_spec_for_source finds for the same file"
)]
fn infer_file_scope_agrees_with_find_spec_for_source() {
    let (mut server, _dir) = anchored_project();
    for spelling in [
        "src/login.rs",
        "./src/login.rs",
        "src\\login.rs",
        "src",
        "login.rs",
    ] {
        let infer = prompt_payload(&infer(
            &mut server,
            json!({"scope": format!("file:{spelling}")}),
        ));
        let tool = find_spec_for_source(&mut server, spelling);
        assert_eq!(
            infer["existing_entities_referencing_file"], tool["entities"],
            "{spelling}"
        );
        assert_eq!(infer["match_mode"], tool["match_mode"], "{spelling}");
        assert_eq!(entity_ids(&tool["entities"]), ["alpha"], "{spelling}");
    }
    let tool = find_spec_for_source(&mut server, "src/login.rs");
    let alpha = &tool["entities"][0];
    assert_eq!(alpha["kind"], "behavior");
    assert_eq!(alpha["line"], 1);
    assert_eq!(alpha["symbol_name"], "login");
}

#[specforge_test(
    behavior = "provide_infer_file_scope",
    verify = "an unanchored file lists no existing entities"
)]
fn infer_file_scope_lists_nothing_for_an_unanchored_file() {
    let (mut server, _dir) = anchored_project();
    // main.spec declares alpha, but a spec file anchors no source.
    for file in ["src/other.rs", "main.spec", "spec/main.spec"] {
        let infer = prompt_payload(&infer(
            &mut server,
            json!({"scope": format!("file:{file}")}),
        ));
        assert_eq!(
            infer["existing_entities_referencing_file"],
            json!([]),
            "{file}"
        );
        assert_eq!(infer["match_mode"], "none", "{file}");
    }
}

/// Over a `specforge-infer.json` that cannot be used, the plan refuses with
/// E071 instead of listing every source file as unanalyzed (plan 06 R6),
/// which every `mark_analyzed` would then refuse.
#[specforge_test(
    behavior = "provide_infer_plan_scope",
    verify = "plan refuses a specforge-infer.json it cannot use with E071"
)]
fn the_plan_refuses_an_unusable_manifest() {
    let mut state = TestProject::new()
        .file("src/lib.rs", "fn stub() {}\n")
        .file("specforge-infer.json", "{ nope")
        .serve(&[
            test_extension("behavior", Some("guide text")).declaring(|c| {
                c.analyzer("rust", |a| {
                    a.file_extensions(&[".rs"]).scan(|_| ScanResponse {
                        items: Vec::new(),
                        language: None,
                    });
                });
            }),
        ]);
    let resp = infer(&mut state, json!({"scope": "plan"}));
    assert!(resp.get("result").is_none(), "{resp}");
    let data = &resp["error"]["data"];
    assert_eq!(data["code"], "schema_mismatch", "{resp}");
    assert_eq!(data["diagnostic"]["code"], "E071", "{resp}");
    assert!(
        data["message"]
            .as_str()
            .unwrap()
            .starts_with("failed to parse specforge-infer.json"),
        "{resp}"
    );
}
