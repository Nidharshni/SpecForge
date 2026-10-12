//! Golden outputs of `prompts/list` and `prompts/get` for every core
//! prompt, over one project on disk (plan 07 T0). They pin what the
//! prompts answer; a change to a prompt's output shows here as a snapshot
//! diff, in the commit that makes it.

use serde_json::{Value, json};
use specforge_mcp::McpServer;

use crate::support::{Logged, events, logged};

/// The project the prompts read: product, software and testing loaded,
/// a four-hop chain `alpha - feat_login - beta - inv_session - delta`,
/// `alpha` anchored to `src/login.rs`, and four source files.
fn project() -> tempfile::TempDir {
    let dir = tempfile::TempDir::new().unwrap();
    let root = dir.path();
    std::fs::create_dir_all(root.join("spec")).unwrap();
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(
        root.join("specforge.json"),
        json!({
            "name": "probe",
            "version": "0.1.0",
            "spec_root": "spec",
            "extensions": ["@specforge/product", "@specforge/software", "@specforge/testing"],
        })
        .to_string(),
    )
    .unwrap();
    std::fs::write(
        root.join("spec/main.spec"),
        r#"feature feat_login "Login" {
  problem  "users cannot sign in"
  solution "a login flow"
}
behavior alpha "Alpha" {
  features [feat_login]
  contract "first"
  verify unit "alpha works"
}
behavior beta "Beta" {
  features   [feat_login]
  invariants [inv_session]
  contract "second"
  verify unit "beta works"
}
invariant inv_session "Session" {
  guarantee "sessions expire"
  risk      low
  verify unit "sessions expire"
}
behavior delta "Delta" {
  invariants [inv_session]
  contract "fourth"
}
"#,
    )
    .unwrap();
    std::fs::write(root.join("src/login.rs"), "pub fn login() {}\n").unwrap();
    for file in ["logout.rs", "session.rs", "lib.rs"] {
        std::fs::write(root.join("src").join(file), "pub fn stub() {}\n").unwrap();
    }
    std::fs::write(
        root.join("specforge-anchors.json"),
        json!({
            "version": 1,
            "anchors": [{
                "entity_id": "alpha",
                "file": "src/login.rs",
                "line": 1,
                "symbol_name": "login",
                "item_kind": "fn",
                "scanner": "manual",
            }],
        })
        .to_string(),
    )
    .unwrap();
    dir
}

/// A server for `dir`, as `specforge mcp <dir>` starts it, initialized.
fn server(dir: &tempfile::TempDir) -> Logged {
    let mut server = logged(McpServer::with_project_root(dir.path().to_path_buf()));
    send(&mut server, "initialize", json!({}));
    server
}

fn send(server: &mut McpServer, method: &str, params: Value) -> Value {
    let message = json!({"jsonrpc": "2.0", "id": 1, "method": method, "params": params});
    let reply = server.handle_message(&message.to_string()).unwrap();
    serde_json::from_str(&reply).unwrap()
}

fn get(server: &mut McpServer, prompt: &str, arguments: Value) -> Value {
    send(
        server,
        "prompts/get",
        json!({"name": format!("specforge://prompts/{prompt}"), "arguments": arguments}),
    )
}

/// The reply as the snapshots record it: an error's code, message and
/// data; else the message layout (`turns`: an instruction then the
/// payload as a second message; `inline`: one user message carrying the
/// payload after `## Reference Data`), each message's role, the
/// instruction and the payload parsed as JSON. Inference guides come from
/// the vendored extension manifests, so they read `[guide]`; the project
/// root reads `[root]`.
fn shape(reply: &Value, root: &std::path::Path) -> Value {
    let shaped = if let Some(error) = reply.get("error") {
        json!({"error": error})
    } else {
        let result = &reply["result"];
        let messages = result["messages"].as_array().cloned().unwrap_or_default();
        let roles: Vec<Value> = messages.iter().map(|m| m["role"].clone()).collect();
        let text = |i: usize| messages[i]["content"]["text"].as_str().unwrap_or_default();
        let (layout, instruction, payload) = if messages.len() == 1 {
            let (instruction, data) = text(0)
                .split_once("\n\n## Reference Data\n")
                .unwrap_or((text(0), "null"));
            ("inline", instruction.to_string(), data.to_string())
        } else {
            ("turns", text(0).to_string(), text(1).to_string())
        };
        let mut shaped = json!({
            "layout": layout,
            "roles": roles,
            "instruction": instruction,
            "payload": serde_json::from_str::<Value>(&payload).unwrap_or(Value::String(payload)),
        });
        if let Some(description) = result.get("description") {
            shaped["description"] = description.clone();
        }
        shaped
    };
    redact(shaped, root)
}

fn redact(value: Value, root: &std::path::Path) -> Value {
    let roots: Vec<String> = [
        root.to_path_buf(),
        std::fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf()),
    ]
    .iter()
    .map(|p| p.display().to_string())
    .collect();
    fn walk(value: Value, key: Option<&str>, roots: &[String]) -> Value {
        match value {
            Value::String(_) if key == Some("inference_guide") => Value::from("[guide]"),
            Value::String(text) => {
                let mut text = text;
                for root in roots {
                    text = text.replace(root.as_str(), "[root]");
                }
                Value::String(text)
            }
            Value::Array(items) => {
                Value::Array(items.into_iter().map(|v| walk(v, None, roots)).collect())
            }
            Value::Object(map) => Value::Object(
                map.into_iter()
                    .map(|(k, v)| {
                        let v = walk(v, Some(&k), roots);
                        (k, v)
                    })
                    .collect(),
            ),
            other => other,
        }
    }
    let mut roots = roots;
    // The canonical root first: it is the longer one on macOS (/private/var).
    roots.sort_by_key(|r| std::cmp::Reverse(r.len()));
    walk(value, None, &roots)
}

/// Snapshot each `(name, prompt, arguments)` case against one server.
fn pin(cases: &[(&str, &str, Value)]) {
    let dir = project();
    let mut server = server(&dir);
    for (name, prompt, arguments) in cases {
        let reply = get(&mut server, prompt, arguments.clone());
        insta::assert_json_snapshot!(*name, shape(&reply, dir.path()));
    }
}

/// Every prompt's payload is valid against the schema its type derives
/// (unlinked: the reflection test holds that schema to the spec).
#[test]
fn every_rendered_payload_conforms_to_its_derived_schema() {
    let dir = project();
    let mut server = server(&dir);
    let cases: &[(&str, Value)] = &[
        (
            "context",
            json!({"entity_id": "alpha", "structural_constraints": "beta"}),
        ),
        ("review", json!({})),
        ("review", json!({"entity_id": "alpha"})),
        ("trace", json!({"entity_id": "alpha"})),
        ("explore", json!({})),
        ("infer", json!({})),
        ("infer", json!({"scope": "kind:behavior"})),
        ("infer", json!({"scope": "file:src/login.rs"})),
        ("infer", json!({"scope": "plan"})),
        ("infer", json!({"scope": "workflow"})),
    ];
    for (prompt, arguments) in cases {
        let spec = specforge_mcp::prompts::core_prompt(&format!("specforge://prompts/{prompt}"))
            .expect("a core prompt");
        let reply = get(&mut server, prompt, arguments.clone());
        let text = reply["result"]["messages"][1]["content"]["text"]
            .as_str()
            .unwrap_or_else(|| panic!("{prompt} {arguments}: {reply}"));
        let payload: Value = serde_json::from_str(text).unwrap();
        let violations = specforge_common::shape::violations(spec.payload_schema(), &payload);
        assert!(
            violations.is_empty(),
            "{prompt} {arguments}: {violations:#?}"
        );
    }
}

#[test]
fn prompt_golden_list() {
    let dir = project();
    let mut server = server(&dir);
    let reply = send(&mut server, "prompts/list", json!({}));
    insta::assert_json_snapshot!("01_list", reply["result"]);
}

#[test]
fn prompt_golden_context() {
    pin(&[
        ("02_context_alpha", "context", json!({"entity_id": "alpha"})),
        (
            "03_context_constraints_csv",
            "context",
            json!({"entity_id": "alpha", "structural_constraints": "inv_session, beta"}),
        ),
        (
            "04_context_constraints_array",
            "context",
            json!({"entity_id": "alpha", "structural_constraints": ["inv_session"]}),
        ),
        (
            "05_context_constraint_ghost",
            "context",
            json!({"entity_id": "alpha", "structural_constraints": "ghost"}),
        ),
        ("06_context_no_arguments", "context", json!({})),
        (
            "07_context_entity_id_number",
            "context",
            json!({"entity_id": 42}),
        ),
    ]);
}

#[test]
fn prompt_golden_review() {
    pin(&[
        ("08_review_whole_graph", "review", json!({})),
        ("09_review_alpha", "review", json!({"entity_id": "alpha"})),
        (
            "10_review_alpha_depth_number",
            "review",
            json!({"entity_id": "alpha", "depth": 2}),
        ),
        (
            "11_review_alpha_depth_string",
            "review",
            json!({"entity_id": "alpha", "depth": "2"}),
        ),
        ("12_review_ghost", "review", json!({"entity_id": "ghost"})),
    ]);
}

#[test]
fn prompt_golden_review_malformed_report() {
    let dir = project();
    let mut server = server(&dir);
    std::fs::write(dir.path().join("specforge-report.json"), "{not json").unwrap();
    let reply = get(&mut server, "review", json!({"entity_id": "alpha"}));
    insta::assert_json_snapshot!("13_review_malformed_report", shape(&reply, dir.path()));
}

#[test]
fn prompt_golden_trace() {
    pin(&[
        ("14_trace_alpha", "trace", json!({"entity_id": "alpha"})),
        (
            "15_trace_plan",
            "trace",
            json!({"plan": {"entries": [{"entity_id": "beta"}, {"entity_id": "ghost"}]}}),
        ),
        (
            "16_trace_plan_not_json",
            "trace",
            json!({"plan": "{not json"}),
        ),
        ("17_trace_no_arguments", "trace", json!({})),
        ("18_trace_ghost", "trace", json!({"entity_id": "ghost"})),
    ]);
}

#[test]
fn prompt_golden_explore() {
    pin(&[
        ("19_explore_no_arguments", "explore", json!({})),
        ("20_explore_alpha", "explore", json!({"entity_id": "alpha"})),
        (
            "21_explore_kind_behavior",
            "explore",
            json!({"kind": "behavior"}),
        ),
        (
            "22_explore_alpha_kind_invariant",
            "explore",
            json!({"entity_id": "alpha", "kind": "invariant"}),
        ),
        ("23_explore_ghost", "explore", json!({"entity_id": "ghost"})),
    ]);
}

#[test]
fn prompt_golden_infer() {
    pin(&[
        ("24_infer_overview", "infer", json!({})),
        (
            "25_infer_kind_behavior",
            "infer",
            json!({"scope": "kind:behavior"}),
        ),
        (
            "26_infer_kind_capitalized",
            "infer",
            json!({"scope": "kind:Behavior"}),
        ),
        ("27_infer_kind_empty", "infer", json!({"scope": "kind:"})),
        (
            "28_infer_kind_unknown",
            "infer",
            json!({"scope": "kind:nope"}),
        ),
        (
            "29_infer_file_source",
            "infer",
            json!({"scope": "file:src/login.rs"}),
        ),
        (
            "30_infer_file_spec",
            "infer",
            json!({"scope": "file:main.spec"}),
        ),
        ("31_infer_file_empty", "infer", json!({"scope": "file:"})),
        ("32_infer_plan", "infer", json!({"scope": "plan"})),
        (
            "33_infer_plan_cursor_string",
            "infer",
            json!({"scope": "plan", "cursor": "50"}),
        ),
        ("34_infer_workflow", "infer", json!({"scope": "workflow"})),
    ]);
}

#[test]
fn prompt_golden_protocol_errors() {
    let dir = project();
    let mut server = server(&dir);
    let unknown = send(
        &mut server,
        "prompts/get",
        json!({"name": "specforge://prompts/nope"}),
    );
    insta::assert_json_snapshot!("35_unknown_prompt", shape(&unknown, dir.path()));
    let no_name = send(&mut server, "prompts/get", json!({"arguments": {}}));
    insta::assert_json_snapshot!("36_no_name", shape(&no_name, dir.path()));
    let not_an_object = send(
        &mut server,
        "prompts/get",
        json!({"name": "specforge://prompts/context", "arguments": "x"}),
    );
    insta::assert_json_snapshot!(
        "37_arguments_not_an_object",
        shape(&not_an_object, dir.path())
    );

    let mut fresh = McpServer::with_project_root(dir.path().to_path_buf());
    let uninitialized = send(
        &mut fresh,
        "prompts/get",
        json!({"name": "specforge://prompts/context", "arguments": {"entity_id": "alpha"}}),
    );
    insta::assert_json_snapshot!("38_before_initialize", shape(&uninitialized, dir.path()));
}

#[test]
fn prompt_golden_events() {
    let dir = project();
    let mut server = server(&dir);
    get(&mut server, "context", json!({"entity_id": "alpha"}));
    get(&mut server, "explore", json!({}));
    send(
        &mut server,
        "prompts/get",
        json!({"name": "specforge://prompts/nope"}),
    );
    let invoked = events(&server, "mcp_prompt_invoked");
    insta::assert_json_snapshot!("39_invocation_events", invoked);
}
