//! The read views on both surfaces (architecture plan 2026-10 02).
//!
//! Stats, trace, the coverage views, the schema, the model and the outline
//! are each one operation over the project view (`specforge_ops::view`),
//! shared by the CLI and MCP. The `*_today` tests pin what each surface
//! answers on two small projects, `fixtures/coverage/fx1` and
//! `fixtures/read_views/rv1`, as insta snapshots
//! (`tests/snapshots/tests__read_views__*.snap`). They prove no spec
//! obligation (they pin current behavior, bugs included), so they carry no
//! `specforge_test` link; a change that alters what they pin re-blesses the
//! snapshot in the same commit, where the diff shows it.
//!
//! rv1 holds a verified behavior `alpha`, an unverified `beta`, a union
//! `Color = Red | Green`, and, under `spec/sub/`, a behavior `gamma`, so a
//! compile of the sub-path has an entity.

use assert_cmd::Command;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use tempfile::TempDir;

use crate::coverage_corpus::{copy_tree, mcp_calls, mcp_responses, project};
use crate::parity::normalized;

/// A scratch copy of `fixtures/read_views/rv1`.
fn rv1() -> TempDir {
    let tmp = TempDir::new().unwrap();
    copy_tree(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/read_views/rv1"),
        tmp.path(),
    );
    tmp
}

fn s(path: &Path) -> &str {
    path.to_str().unwrap()
}

/// What a CLI command printed, and its exit code.
struct Run {
    code: Option<i32>,
    stdout: String,
    stderr: String,
}

fn cli(args: &[&str]) -> Run {
    let out = Command::new(env!("CARGO_BIN_EXE_specforge"))
        .args(args)
        .output()
        .unwrap();
    Run {
        code: out.status.code(),
        stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
    }
}

/// A command's stdout as JSON.
fn cli_json(args: &[&str]) -> Value {
    let run = cli(args);
    serde_json::from_str(&run.stdout)
        .unwrap_or_else(|e| panic!("{args:?} is not JSON ({e}): {}{}", run.stdout, run.stderr))
}

/// `text` with the machine-specific parts of `root` replaced.
fn normalized_text(text: &str, root: &Path) -> String {
    let canonical = root.canonicalize().unwrap();
    text.replace(s(&canonical), "[ROOT]")
        .replace(s(root), "[ROOT]")
}

/// The text content of each MCP tool call's result, as the server wrote it.
fn mcp_texts(root: &Path, calls: &[Value]) -> Vec<String> {
    mcp_responses(root, calls)
        .into_iter()
        .map(|resp| {
            resp["result"]["content"][0]["text"]
                .as_str()
                .unwrap_or_else(|| panic!("no text content: {resp}"))
                .to_string()
        })
        .collect()
}

/// A schema-shaped document reduced to what does not move whenever an
/// extension adds a field: its version, the kinds and edge types by name,
/// how many validation rules it lists. Other documents are kept whole.
fn schema_digest(value: &Value) -> Value {
    let names = |items: &Value, key: &str| -> Value {
        items
            .as_array()
            .map(|items| items.iter().map(|i| i[key].clone()).collect())
            .unwrap_or(Value::Null)
    };
    if value.get("entity_kinds").is_some() {
        let mut digest = json!({
            "schema_version": value["schema_version"],
            "extensions": value["extensions"],
            "entity_kinds": names(&value["entity_kinds"], "name"),
            "edge_types": names(&value["edge_types"], "label"),
        });
        if value.get("edge_types").is_none() {
            digest.as_object_mut().unwrap().remove("edge_types");
        }
        if let Some(rules) = value.get("validation_rules") {
            digest["validation_rules"] = json!(rules.as_array().map(Vec::len));
        }
        return digest;
    }
    if value.get("fields").is_some() && value.get("name").is_some() {
        return json!({
            "name": value["name"],
            "testable": value["testable"],
            "fields": names(&value["fields"], "name"),
        });
    }
    if value.get("$schema").is_some() {
        let mut properties: Vec<&String> = value["properties"]
            .as_object()
            .map(|p| p.keys().collect())
            .unwrap_or_default();
        properties.sort();
        return json!({
            "$id": value["$id"],
            "title": value["title"],
            "required": value["required"],
            "properties": properties,
        });
    }
    value.clone()
}

/// What `specforge stats` prints for `root`, and what `specforge.stats`
/// answers, pinned under `stats_<name>_*`.
fn pin_stats(name: &str, root: &Path) {
    let json = cli_json(&["stats", "--format", "json", s(root)]);
    insta::assert_snapshot!(format!("stats_{name}_cli"), normalized(&json, root));
    let human = cli(&["stats", s(root)]);
    insta::assert_snapshot!(
        format!("stats_{name}_cli_human"),
        normalized_text(&human.stdout, root)
    );
    let mcp = &mcp_calls(root, &[json!({"name": "specforge.stats", "arguments": {}})])[0];
    insta::assert_snapshot!(format!("stats_{name}_mcp"), normalized(mcp, root));
}

#[test]
fn stats_today() {
    pin_stats("fx1", project("fx1").path());
    pin_stats("rv1", rv1().path());
}

/// `specforge stats --format json` and `specforge.stats` on `root` answer one
/// document (`McpStatsResult`, ADR 0052).
fn assert_stats_agree(root: &Path) {
    let cli = cli_json(&["stats", "--format", "json", s(root)]);
    let mcp = &mcp_calls(root, &[json!({"name": "specforge.stats", "arguments": {}})])[0];
    assert_eq!(cli, *mcp, "{root:?}");
}

#[specforge_test_macros::test(
    behavior = "read_views_over_the_project_view",
    verify = "specforge stats --format json and specforge.stats answer the same document"
)]
fn cli_and_mcp_stats_report_the_same_numbers() {
    assert_stats_agree(project("fx1").path());
    assert_stats_agree(rv1().path());
    let example = TempDir::new().unwrap();
    copy_tree(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/todo-app"),
        example.path(),
    );
    assert_stats_agree(example.path());
}

#[test]
fn trace_today() {
    let tmp = project("fx1");
    let root = tmp.path();
    let snap = |name: &str, value: &Value| {
        insta::assert_snapshot!(format!("trace_fx1_{name}"), normalized(value, root));
    };

    snap(
        "login_cli",
        &cli_json(&["trace", "login", "--path", s(root), "--format", "json"]),
    );
    snap(
        "every_cli",
        &cli_json(&["trace", "--path", s(root), "--format", "json"]),
    );
    let human = cli(&["trace", "login", "--path", s(root), "--format", "human"]);
    insta::assert_snapshot!("trace_fx1_login_cli_human", human.stdout);
    let ghost = cli(&["trace", "ghost", "--path", s(root)]);
    insta::assert_snapshot!(
        "trace_fx1_ghost_cli",
        format!(
            "exit: {:?}\nstdout:\n{}stderr:\n{}",
            ghost.code, ghost.stdout, ghost.stderr
        )
    );

    let mcp = mcp_calls(
        root,
        &[
            json!({"name": "specforge.trace", "arguments": {"entity_id": "login"}}),
            json!({"name": "specforge.trace", "arguments": {"plan": {"entries": [
                {"entity_id": "login"}, {"entity_id": "ghost"}]}}}),
            json!({"name": "specforge.trace", "arguments": {"entity_id": "ghost"}}),
        ],
    );
    snap("login_mcp", &mcp[0]);
    snap("plan_mcp", &mcp[1]);
    snap("ghost_mcp", &mcp[2]);
}

/// For every entity of `root`, `specforge.trace` answers the document
/// `specforge trace <id> --format json` prints.
fn assert_traces_agree(root: &Path) {
    let every = cli_json(&["trace", "--path", s(root), "--format", "json"]);
    let ids: Vec<String> = every["traces"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["entity_id"].as_str().unwrap().to_string())
        .collect();
    assert!(!ids.is_empty(), "{root:?}");
    let calls: Vec<Value> = ids
        .iter()
        .map(|id| json!({"name": "specforge.trace", "arguments": {"entity_id": id}}))
        .collect();
    for (id, mcp) in ids.iter().zip(mcp_calls(root, &calls)) {
        let cli = cli_json(&["trace", id, "--path", s(root), "--format", "json"]);
        assert_eq!(mcp, cli, "{id} on {root:?}");
    }
}

#[specforge_test_macros::test(
    behavior = "read_views_over_the_project_view",
    verify = "specforge trace and specforge.trace return the same chain for an entity"
)]
fn cli_and_mcp_trace_are_the_same_chain() {
    assert_traces_agree(project("fx1").path());
    assert_traces_agree(rv1().path());
}

#[test]
fn coverage_views_today() {
    let tmp = project("fx1");
    let root = tmp.path();
    let prompt = |name: &str, arguments: Value| {
        json!({"method": "prompts/get", "params": {
            "name": format!("specforge://prompts/{name}"), "arguments": arguments}})
    };
    let calls = [
        (
            "coverage_all",
            json!({"name": "specforge.coverage", "arguments": {}}),
        ),
        (
            "coverage_status_union",
            json!({"name": "specforge.coverage", "arguments": {"entity_id": "Status"}}),
        ),
        (
            "coverage_kind_behavior",
            json!({"name": "specforge.coverage", "arguments": {"kind": "behavior"}}),
        ),
        (
            "coverage_uncovered",
            json!({"name": "specforge.coverage", "arguments": {"status_filter": "uncovered"}}),
        ),
        (
            "inspect_payload",
            json!({"name": "specforge.inspect", "arguments": {"entity_id": "Payload"}}),
        ),
        (
            "query_login",
            json!({"name": "specforge.query",
                   "arguments": {"entity_id": "login", "include_coverage": true}}),
        ),
        ("prompt_review", prompt("review", json!({}))),
        (
            "prompt_trace_login",
            prompt("trace", json!({"entity_id": "login"})),
        ),
        (
            "prompt_trace_plan",
            prompt(
                "trace",
                json!({"plan": {"entries": [{"entity_id": "login"}, {"entity_id": "ghost"}]}}),
            ),
        ),
    ];
    let requests: Vec<Value> = calls.iter().map(|(_, call)| call.clone()).collect();
    for ((name, _), result) in calls.iter().zip(mcp_calls(root, &requests)) {
        insta::assert_snapshot!(format!("coverage_fx1_{name}"), normalized(&result, root));
    }
}

#[test]
fn schema_today() {
    let tmp = rv1();
    let root = tmp.path();
    let snap = |name: &str, value: &Value| {
        insta::assert_snapshot!(
            format!("schema_rv1_{name}"),
            normalized(&schema_digest(value), root)
        );
    };

    snap("cli", &cli_json(&["schema", s(root)]));
    snap(
        "cli_kind_behavior",
        &cli_json(&["schema", s(root), "--kind", "behavior"]),
    );
    snap(
        "cli_publish_context",
        &cli_json(&["schema", s(root), "--publish", "--format", "context"]),
    );
    for (name, args) in [
        (
            "cli_publish_kind_behavior",
            ["schema", s(root), "--publish", "--kind", "behavior"].as_slice(),
        ),
        (
            "cli_format_without_publish",
            ["schema", s(root), "--format", "brief"].as_slice(),
        ),
    ] {
        let refused = cli(args);
        insta::assert_snapshot!(
            format!("schema_rv1_{name}"),
            format!(
                "exit: {:?}\nstderr:\n{}",
                refused.code,
                normalized_text(&refused.stderr, root)
            )
        );
    }
    let nosuch = cli(&["schema", s(root), "--kind", "nosuch"]);
    insta::assert_snapshot!(
        "schema_rv1_cli_kind_nosuch",
        format!("exit: {:?}\nstderr:\n{}", nosuch.code, nosuch.stderr)
    );

    let mcp = mcp_calls(
        root,
        &[
            json!({"name": "specforge.schema", "arguments": {}}),
            json!({"name": "specforge.schema", "arguments": {"kind": "behavior"}}),
            json!({"name": "specforge.schema", "arguments": {"include_edges": false}}),
            json!({"name": "specforge.schema", "arguments": {"include_validation_rules": true}}),
            json!({"name": "specforge.schema", "arguments": {"kind": "nosuch"}}),
        ],
    );
    snap("mcp", &mcp[0]);
    snap("mcp_kind_behavior", &mcp[1]);
    snap("mcp_no_edges", &mcp[2]);
    snap("mcp_validation_rules", &mcp[3]);
    snap("mcp_kind_nosuch", &mcp[4]);
    let resource = &mcp_responses(
        root,
        &[json!({"method": "resources/read", "params": {"uri": "specforge://schema"}})],
    )[0];
    let text = resource["result"]["contents"][0]["text"].as_str().unwrap();
    snap("resource", &serde_json::from_str(text).unwrap());
}

const MODEL_FORMATS: [&str; 5] = ["markdown", "mermaid", "dot", "json", "dbml"];

#[test]
fn model_and_outline_today() {
    let tmp = rv1();
    let root = tmp.path();
    let calls: Vec<Value> = MODEL_FORMATS
        .iter()
        .map(|format| json!({"name": "specforge.model", "arguments": {"format": format}}))
        .chain(std::iter::once(
            json!({"name": "specforge.outline_extensions", "arguments": {"format": "json"}}),
        ))
        .collect();
    let mcp = mcp_texts(root, &calls);
    for (format, text) in MODEL_FORMATS.iter().zip(&mcp) {
        let run = cli(&["model", s(root), "--format", format]);
        assert_eq!(run.code, Some(0), "{}", run.stderr);
        assert!(!run.stdout.is_empty(), "model --format {format}");
        assert_eq!(&run.stdout, text, "model --format {format}");
    }
    let outline = cli(&["outline", s(root), "--format", "json"]);
    assert_eq!(outline.code, Some(0), "{}", outline.stderr);
    assert_eq!(&outline.stdout, &mcp[5]);
}

#[specforge_test_macros::test(
    behavior = "expose_model_mcp_tool",
    verify = "MCP tool produces same output as CLI command"
)]
fn cli_and_mcp_model_render_the_same_text() {
    let tmp = project("fx1");
    let root = tmp.path();
    // (CLI flags, MCP arguments): every format, then each filter.
    let mut cases: Vec<(Vec<&str>, Value)> = MODEL_FORMATS
        .iter()
        .map(|format| (vec!["--format", *format], json!({"format": format})))
        .collect();
    cases.extend([
        (
            vec!["--extension", "@specforge/software"],
            json!({"extension": "@specforge/software"}),
        ),
        (
            vec!["--kinds", "behavior,type"],
            json!({"kinds": ["behavior", "type"]}),
        ),
        (
            vec!["--format", "mermaid", "--root", "behavior", "--depth", "1"],
            json!({"format": "mermaid", "root": "behavior", "depth": 1}),
        ),
        (vec!["--group-by", "none"], json!({"group_by": "none"})),
        (
            vec!["--fields", "all", "--format", "dbml"],
            json!({"fields": "all", "format": "dbml"}),
        ),
    ]);
    let calls: Vec<Value> = cases
        .iter()
        .map(|(_, arguments)| json!({"name": "specforge.model", "arguments": arguments}))
        .collect();
    for ((flags, arguments), text) in cases.iter().zip(mcp_texts(root, &calls)) {
        let mut args = vec!["model", s(root)];
        args.extend(flags);
        let run = cli(&args);
        assert_eq!(run.code, Some(0), "{args:?}: {}", run.stderr);
        assert!(!run.stdout.is_empty(), "{args:?}");
        assert_eq!(run.stdout, text, "{args:?} vs {arguments}");
    }
}

#[test]
fn a_model_refuses_what_the_project_does_not_have() {
    let tmp = project("fx1");
    let root = s(tmp.path());

    let run = cli(&["model", root, "--root", "behaviour"]);
    assert_eq!(run.code, Some(1), "{}", run.stderr);
    assert_eq!(run.stdout, "");
    assert_eq!(
        run.stderr,
        "error[unknown_kind]: unknown entity kind 'behaviour'\n  hint: did you mean 'behavior'?\n"
    );

    let run = cli(&["model", root, "--extension", "software"]);
    assert_eq!(run.code, Some(1), "{}", run.stderr);
    assert_eq!(run.stdout, "");
    assert_eq!(
        run.stderr,
        "error[extension_not_found]: extension 'software' is not loaded by this project\n  hint: did you mean '@specforge/software'?\n"
    );

    // A kind the project does not know is reported, and selects nothing.
    let run = cli(&["model", root, "--kinds", "behaviour", "--format", "json"]);
    assert_eq!(run.code, Some(0), "{}", run.stderr);
    let model: Value = serde_json::from_str(&run.stdout).unwrap();
    assert!(model["entities"].as_array().unwrap().is_empty());
    assert!(
        run.stderr
            .contains("info[I020]: unknown entity kind 'behaviour'"),
        "{}",
        run.stderr
    );
    assert!(
        run.stderr.contains("did you mean 'behavior'?"),
        "{}",
        run.stderr
    );
}

#[specforge_test_macros::test(
    behavior = "read_views_over_the_project_view",
    verify = "specforge outline and specforge.outline_extensions render the same text"
)]
fn cli_and_mcp_outline_render_the_same_text() {
    // fx1 loads four extensions: direct and transitive dependencies.
    let tmp = project("fx1");
    let root = tmp.path();
    let mut cases = Vec::new();
    for format in ["markdown", "mermaid", "dot", "json"] {
        for fields in ["none", "keys", "all"] {
            for deps in ["direct", "effective", "full"] {
                cases.push((format, fields, deps));
            }
        }
    }
    let calls: Vec<Value> = cases
        .iter()
        .map(|(format, fields, deps)| {
            json!({"name": "specforge.outline_extensions",
                   "arguments": {"format": format, "fields": fields, "deps": deps}})
        })
        .collect();
    for ((format, fields, deps), text) in cases.iter().zip(mcp_texts(root, &calls)) {
        let args = [
            "outline",
            s(root),
            "--format",
            format,
            "--fields",
            fields,
            "--deps",
            deps,
        ];
        let run = cli(&args);
        assert_eq!(run.code, Some(0), "{args:?}: {}", run.stderr);
        assert_eq!(run.stdout, text, "{args:?}");
    }

    // With no arguments both surfaces take the tables' defaults (ADR 0027 D3).
    let defaults = mcp_texts(
        root,
        &[json!({"name": "specforge.outline_extensions", "arguments": {}})],
    );
    let run = cli(&["outline", s(root)]);
    assert_eq!(run.code, Some(0), "{}", run.stderr);
    assert_eq!(run.stdout, defaults[0], "outline with no arguments");
}

#[specforge_test_macros::test(
    behavior = "read_views_over_the_project_view",
    verify = "specforge schema --kind and specforge.schema with a kind return the same document"
)]
fn cli_and_mcp_schema_kind_are_one_document() {
    let tmp = rv1();
    let root = tmp.path();
    let mcp = mcp_calls(
        root,
        &[json!({"name": "specforge.schema", "arguments": {"kind": "behavior"}})],
    );
    let answered = cli_json(&["schema", s(root), "--kind", "behavior"]);
    assert_eq!(answered, mcp[0]);
    assert_eq!(
        answered["entity_kinds"][0]["name"], "behavior",
        "{answered}"
    );
    assert!(
        answered["edge_types"]
            .as_array()
            .is_some_and(|edges| !edges.is_empty()),
        "the edge types that touch the kind: {answered}"
    );

    // Unfiltered, the CLI prints the schema itself, in its own key order.
    let printed = cli(&["schema", s(root)]);
    assert_eq!(printed.code, Some(0), "{}", printed.stderr);
    let schema: specforge_emitter::GraphProtocolSchema =
        serde_json::from_str(&printed.stdout).unwrap();
    assert_eq!(
        printed.stdout.trim_end(),
        serde_json::to_string_pretty(&schema).unwrap()
    );
}

#[specforge_test_macros::test(
    behavior = "serve_schema_resource",
    verify = "--no-edges and --validation-rules select what include_edges and include_validation_rules select"
)]
fn cli_and_mcp_schema_requests_agree() {
    let tmp = rv1();
    let root = tmp.path();
    let cases: [(&[&str], Value); 3] = [
        (&["--no-edges"], json!({"include_edges": false})),
        (
            &["--validation-rules"],
            json!({"include_validation_rules": true}),
        ),
        (
            &["--kind", "behavior", "--validation-rules", "--no-edges"],
            json!({"kind": "behavior", "include_validation_rules": true, "include_edges": false}),
        ),
    ];
    let calls: Vec<Value> = cases
        .iter()
        .map(|(_, arguments)| json!({"name": "specforge.schema", "arguments": arguments}))
        .collect();
    for ((flags, arguments), answered) in cases.iter().zip(mcp_calls(root, &calls)) {
        let mut args = vec!["schema", s(root)];
        args.extend(*flags);
        let printed = cli_json(&args);
        assert_eq!(printed, answered, "{flags:?} vs {arguments}");
        assert_eq!(
            printed.get("edge_types").is_some(),
            !flags.contains(&"--no-edges"),
            "{flags:?}"
        );
        assert_eq!(
            printed.get("validation_rules").is_some(),
            flags.contains(&"--validation-rules"),
            "{flags:?}"
        );
    }
}

/// `rv1` with a `specforge-report.json` that does not parse.
fn rv1_with_a_malformed_report() -> TempDir {
    let tmp = rv1();
    std::fs::write(tmp.path().join("specforge-report.json"), "{not json").unwrap();
    tmp
}

#[specforge_test_macros::test(
    behavior = "read_views_over_the_project_view",
    verify = "the recorded test report is read at the view's root, never an ancestor's"
)]
fn analyze_and_stats_read_the_report_at_the_compiled_root() {
    let tmp = rv1_with_a_malformed_report();
    let sub: PathBuf = tmp.path().join("spec");

    // At the root, and from a sub-path (which is the project it is in),
    // both refuse the report: exit 2, and under JSON output the error
    // document on stdout and nothing on stderr.
    for args in [
        vec!["stats", s(tmp.path()), "--format", "json"],
        vec!["analyze", "--path", s(tmp.path()), "--format", "json"],
        vec!["stats", s(&sub), "--format", "json"],
        vec!["analyze", "--path", s(&sub), "--format", "json"],
    ] {
        let run = cli(&args);
        assert_eq!(run.code, Some(2), "{args:?}: {}", run.stdout);
        let document: Value = serde_json::from_str(&run.stdout)
            .unwrap_or_else(|e| panic!("{args:?}: not a document ({e}): {}", run.stdout));
        assert_eq!(document["code"], "E045", "{args:?}: {document}");
        assert!(
            document["error"]
                .as_str()
                .is_some_and(|e| e.contains("invalid test results")),
            "{document}"
        );
        assert_eq!(run.stderr, "", "{args:?}");
    }
}

/// The kinds `<dir>/.specforge/schema-cache.json` holds.
fn cached_kinds(dir: &Path) -> usize {
    let raw = std::fs::read_to_string(dir.join(".specforge/schema-cache.json")).unwrap();
    let cache: Value = serde_json::from_str(&raw).unwrap();
    cache["schema"]["entity_kinds"].as_array().unwrap().len()
}

#[specforge_test_macros::test(
    behavior = "read_views_over_the_project_view",
    verify = "the schema cache is the view root's, never an ancestor's"
)]
fn export_of_a_sub_path_leaves_the_project_cache_alone() {
    let tmp = rv1();
    let root = tmp.path();
    let first = cli(&["export", s(root), "--format", "graph"]);
    assert_eq!(first.code, Some(0), "{}", first.stderr);
    assert_eq!(cached_kinds(root), 5);

    // The sub-path is the project it is in: it versions against, and records
    // in, the project's cache, and writes none of its own.
    let sub = root.join("spec");
    let run = cli(&["export", s(&sub), "--format", "graph"]);
    assert_eq!(run.code, Some(0), "{}", run.stderr);
    assert!(!run.stderr.contains("W053"), "{}", run.stderr);
    assert_eq!(cached_kinds(root), 5);
    assert!(
        !sub.join(".specforge").exists(),
        "nothing is cached under the sub-path"
    );
}

#[specforge_test_macros::test(
    behavior = "read_views_over_the_project_view",
    verify = "specforge schema and specforge.schema carry the same version"
)]
fn cli_and_mcp_schema_carry_the_same_version() {
    let tmp = rv1();
    let root = tmp.path();
    // A CLI export caches its schema. Make the cache an older 1.2.3 that
    // also had a kind the project no longer has: a breaking change since.
    let export = cli(&["export", s(root), "--format", "graph"]);
    assert_eq!(export.code, Some(0), "{}", export.stderr);
    let cache_path = root.join(".specforge/schema-cache.json");
    let mut cache: Value =
        serde_json::from_str(&std::fs::read_to_string(&cache_path).unwrap()).unwrap();
    cache["schema"]["schema_version"] = json!({"major": 1, "minor": 2, "patch": 3});
    cache["schema"]["entity_kinds"]
        .as_array_mut()
        .unwrap()
        .push(json!({"name": "legacy", "source_extension": "x", "testable": false, "fields": []}));
    std::fs::write(&cache_path, cache.to_string()).unwrap();

    let expected = json!({"major": 2, "minor": 0, "patch": 0});
    let cli_schema = cli_json(&["schema", s(root)]);
    assert_eq!(cli_schema["schema_version"], expected);
    let mcp = &mcp_calls(
        root,
        &[json!({"name": "specforge.schema", "arguments": {}})],
    )[0];
    assert_eq!(mcp["schema_version"], expected, "{mcp}");
    let resource = &mcp_responses(
        root,
        &[json!({"method": "resources/read", "params": {"uri": "specforge://schema"}})],
    )[0];
    let text = resource["result"]["contents"][0]["text"].as_str().unwrap();
    let served: Value = serde_json::from_str(text).unwrap();
    assert_eq!(served["schema_version"], expected);
    // The three are one document.
    assert_eq!(cli_schema, *mcp);
    assert_eq!(cli_schema, served);
    // Neither surface wrote the cache.
    assert_eq!(
        std::fs::read_to_string(&cache_path).unwrap(),
        cache.to_string()
    );
}

/// rv1's report: alpha's one obligation proven, or not.
fn record_alpha(root: &Path, status: &str) {
    std::fs::write(
        root.join("specforge-report.json"),
        json!({"runner": "fixture", "results": {"alpha": {"tests": [
            {"name": "alpha_test", "status": status, "verify": "alpha works"}
        ]}}})
        .to_string(),
    )
    .unwrap();
}

#[specforge_test_macros::test(
    behavior = "read_views_over_the_project_view",
    verify = "Read Views over the Project View: read views hold — project_compiled, one_report_rule, one_coverage_per_state, surfaces_agree"
)]
fn contract_read_views() {
    let tmp = rv1();
    let root = tmp.path();

    // project_compiled: the CLI's compiled project and MCP's session each
    // supply a view, and answer.
    let stats = cli_json(&["stats", "--format", "json", s(root)]);
    assert_eq!(stats["entity_count"], 6, "{stats}");

    // surfaces_agree: the same numbers, chains, schema version and rows.
    assert_stats_agree(root);
    assert_traces_agree(root);
    let mcp = mcp_calls(
        root,
        &[
            json!({"name": "specforge.schema", "arguments": {}}),
            json!({"name": "specforge.coverage", "arguments": {}}),
        ],
    );
    assert_eq!(cli_json(&["schema", s(root)]), mcp[0]);
    assert_eq!(
        json!(mcp[1]["entities"].as_array().unwrap().len()),
        stats["testable_count"],
        "{}",
        mcp[1]
    );

    // one_coverage_per_state: within one MCP session the coverage follows
    // the report's content, read again as soon as it changes.
    let mut server = specforge_mcp::McpServer::with_project_root(root.to_path_buf());
    server.handle_message(
        &json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {}}).to_string(),
    );
    let mut alpha_status = || {
        let request = json!({"jsonrpc": "2.0", "id": 2, "method": "tools/call", "params": {
            "name": "specforge.coverage", "arguments": {"entity_id": "alpha"}}});
        let response: Value =
            serde_json::from_str(&server.handle_message(&request.to_string()).unwrap()).unwrap();
        let text = response["result"]["content"][0]["text"].as_str().unwrap();
        let rows: Value = serde_json::from_str(text).unwrap();
        rows["entities"][0]["status"].as_str().unwrap().to_string()
    };
    assert_eq!(alpha_status(), "uncovered");
    record_alpha(root, "failed");
    assert_eq!(alpha_status(), "partial");
    assert_eq!(alpha_status(), "partial");
    record_alpha(root, "passed");
    assert_eq!(alpha_status(), "covered");
    assert_eq!(
        cli_json(&["stats", "--format", "json", s(root)])["proof_pct"],
        20.0
    );

    // one_report_rule: a report at the root that cannot be read is an
    // error on every view of the project, from its root or from a sub-path.
    std::fs::write(root.join("specforge-report.json"), "{not json").unwrap();
    assert_eq!(cli(&["stats", s(root)]).code, Some(2));
    let refused = mcp_calls(
        root,
        &[
            json!({"name": "specforge.stats", "arguments": {}}),
            json!({"name": "specforge.coverage", "arguments": {}}),
        ],
    );
    for result in &refused {
        assert_eq!(result["isError"]["diagnostic"]["code"], "E045", "{result}");
    }
    let sub = root.join("spec");
    assert_eq!(cli(&["stats", s(&sub)]).code, Some(2));
    assert_eq!(
        cli(&["analyze", "--path", s(&sub), "--format", "json"]).code,
        Some(2)
    );
    // ...and the schema cache likewise: the sub-path's export is the
    // project's, and keeps the root's cache.
    assert_eq!(cli(&["export", s(root)]).code, Some(0));
    assert_eq!(cli(&["export", s(&sub)]).code, Some(0));
    assert_eq!(cached_kinds(root), 5);
}

/// What a CLI run printed, for a snapshot: its exit code and both streams,
/// the project's machine-specific path replaced.
fn run_text(run: &Run, root: &Path) -> String {
    format!(
        "exit: {:?}\nstdout:\n{}stderr:\n{}",
        run.code,
        normalized_text(&run.stdout, root),
        normalized_text(&run.stderr, root)
    )
}

/// The node ids of a graph-shaped document, in the document's order.
fn node_ids(document: &str) -> Vec<String> {
    let value: Value = serde_json::from_str(document).expect("a query prints JSON");
    value["nodes"]
        .as_array()
        .expect("a graph document has nodes")
        .iter()
        .map(|n| n["id"].as_str().unwrap().to_string())
        .collect()
}

/// `text` with every run of 64 hexadecimal digits (a SHA-256) replaced by
/// `[SHA256]`.
fn without_hashes(text: &str) -> String {
    let mut out = String::new();
    let mut run = String::new();
    let flush = |run: &mut String, out: &mut String| {
        out.push_str(if run.len() == 64 { "[SHA256]" } else { run });
        run.clear();
    };
    for ch in text.chars() {
        if ch.is_ascii_hexdigit() {
            run.push(ch);
        } else {
            flush(&mut run, &mut out);
            out.push(ch);
        }
    }
    flush(&mut run, &mut out);
    out
}

fn mcp_query(arguments: Value) -> Value {
    json!({"name": "specforge.query", "arguments": arguments})
}

/// What `specforge query` and `specforge.query` answer on fx1 today
/// (architecture plan 2026-10 05): two implementations, two documents
/// (Graph Protocol 1.0 on both tools, 2.0 from the resource), a raw
/// `E003: ...` line from the CLI, no kind report from the CLI.
#[test]
fn query_today() {
    let tmp = project("fx1");
    let root = tmp.path();
    // A schema's content hash moves whenever a builtin extension's
    // declaration does: it is not what these snapshots pin.
    let snap = |name: &str, text: String| {
        insta::assert_snapshot!(format!("query_today_{name}"), without_hashes(&text));
    };

    let login = cli(&["query", "login", "--path", s(root)]);
    snap("cli_login", run_text(&login, root));
    snap(
        "cli_ghost",
        run_text(&cli(&["query", "logn", "--path", s(root)]), root),
    );
    let filtered = cli(&["query", "login", "--path", s(root), "--kind", "behaviour"]);
    snap(
        "cli_unknown_kind",
        format!(
            "exit: {:?}\nstderr: {:?}\nnodes: {:?}\n",
            filtered.code,
            filtered.stderr,
            node_ids(&filtered.stdout)
        ),
    );

    let calls = [
        mcp_query(json!({"entity_id": "login"})),
        mcp_query(json!({"entity_id": "login", "kinds": ["behaviour"]})),
        mcp_query(json!({"entity_id": "logn"})),
        mcp_query(json!({"entity_id": "login", "include_coverage": true, "format": "brief"})),
        json!({"method": "resources/read", "params": {"uri": "specforge://graph/login"}}),
    ];
    let responses = mcp_responses(root, &calls);
    for (name, response) in [
        "mcp_login",
        "mcp_unknown_kind",
        "mcp_ghost",
        "mcp_coverage_brief",
        "resource_login",
    ]
    .iter()
    .zip(&responses)
    {
        snap(name, normalized(&response["result"], root).to_string());
    }
}

/// `specforge.list` and `specforge.search` answer today from MCP-only
/// handlers: list reports no unknown kind, search reports `Behavior` as
/// I020 and ignores a lone `field`.
#[test]
fn list_and_search_today() {
    let tmp = project("fx1");
    let root = tmp.path();
    let calls = [
        json!({"name": "specforge.list", "arguments": {"kind": "behaviour"}}),
        json!({"name": "specforge.list", "arguments": {"kind": "behavior", "limit": 1, "offset": 1}}),
        json!({"name": "specforge.search", "arguments": {"query": "log", "kinds": ["Behavior"]}}),
        json!({"name": "specforge.search", "arguments": {"query": "log", "field": "title"}}),
        json!({"name": "specforge.search", "arguments": {"query": "log"}}),
    ];
    let responses = mcp_responses(root, &calls);
    let results: Vec<Value> = responses.iter().map(|r| r["result"].clone()).collect();
    for (name, result) in [
        "list_unknown_kind",
        "list_page",
        "search_capitalized_kind",
        "search_lone_field",
    ]
    .iter()
    .zip(&results)
    {
        insta::assert_snapshot!(
            format!("list_and_search_today_{name}"),
            normalized(result, root).to_string()
        );
    }
    assert_ne!(
        results[3], results[4],
        "a lone `field` is refused, not ignored"
    );
}

/// The one wording of "unknown entity kind '<k>'" and where each surface
/// carries its suggestion: the schema tool's refusal in `data.suggestion`,
/// the infer prompt's in `data.data.suggestion`, search's I020 notice in
/// `diagnostic.suggestion`.
#[test]
fn unknown_kind_wordings_today() {
    let tmp = rv1();
    let root = tmp.path();
    let calls = [
        json!({"name": "specforge.schema", "arguments": {"kind": "behaviour"}}),
        json!({"method": "prompts/get", "params": {
            "name": "specforge://prompts/infer", "arguments": {"scope": "kind:behaviour"}}}),
        json!({"name": "specforge.search", "arguments": {"query": "", "kinds": ["behaviour"]}}),
    ];
    for (name, response) in ["schema", "infer", "search"]
        .iter()
        .zip(mcp_responses(root, &calls))
    {
        let answer = if response["error"].is_null() {
            response["result"].clone()
        } else {
            response["error"].clone()
        };
        insta::assert_snapshot!(
            format!("unknown_kind_wordings_today_{name}"),
            normalized(&answer, root).to_string()
        );
    }
}

/// Every entity of `root`, by id.
fn entity_ids(root: &Path) -> Vec<String> {
    let rows = &mcp_calls(root, &[json!({"name": "specforge.list", "arguments": {}})])[0];
    rows["entities"]
        .as_array()
        .expect("specforge.list answers an object holding the entities")
        .iter()
        .map(|row| row["id"].as_str().unwrap().to_string())
        .collect()
}

/// For every entity of `root`, `specforge query` prints the document
/// `specforge.query` answers for the same arguments: the depths, formats and
/// coverage flag each take both values across the four combinations.
fn assert_queries_agree(root: &Path) {
    let combinations = [
        (0, "graph", false),
        (2, "graph", true),
        (2, "context", false),
        (0, "context", true),
    ];
    let ids = entity_ids(root);
    assert!(!ids.is_empty(), "{root:?}");
    for (depth, format, include_coverage) in combinations {
        let calls: Vec<Value> = ids
            .iter()
            .map(|id| {
                mcp_query(json!({
                    "entity_id": id, "depth": depth, "format": format,
                    "include_coverage": include_coverage
                }))
            })
            .collect();
        for (id, mcp) in ids.iter().zip(mcp_calls(root, &calls)) {
            let mut args = vec![
                "query".to_string(),
                id.clone(),
                "--path".to_string(),
                s(root).to_string(),
                "--depth".to_string(),
                depth.to_string(),
                "--format".to_string(),
                format.to_string(),
            ];
            if include_coverage {
                args.push("--include-coverage".to_string());
            }
            let args: Vec<&str> = args.iter().map(String::as_str).collect();
            let cli = cli_json(&args);
            assert_eq!(
                cli, mcp,
                "{id} at depth {depth}, {format}, coverage {include_coverage} on {root:?}"
            );
        }
    }
}

#[specforge_test_macros::test(
    behavior = "read_views_over_the_project_view",
    verify = "specforge query and specforge.query return the same document for an entity"
)]
fn cli_and_mcp_query_are_one_document() {
    assert_queries_agree(project("fx1").path());
    assert_queries_agree(rv1().path());
}

// --- explore, review and infer-guide (plan 04) ---

/// The payload of the prompt `name` (its second message, as JSON) for
/// `arguments`, one `specforge mcp` session each.
fn prompt_payloads(root: &Path, name: &str, arguments: &[Value]) -> Vec<Value> {
    let calls: Vec<Value> = arguments
        .iter()
        .map(|arguments| {
            json!({"method": "prompts/get", "params": {
                "name": format!("specforge://prompts/{name}"), "arguments": arguments}})
        })
        .collect();
    mcp_responses(root, &calls)
        .into_iter()
        .map(|response| {
            let text = response["result"]["messages"][1]["content"]["text"]
                .as_str()
                .unwrap_or_else(|| panic!("no prompt payload in {response}"));
            serde_json::from_str(text).unwrap()
        })
        .collect()
}

/// `value` less the keys the infer prompt adds to the guide's data.
fn without(mut value: Value, keys: &[&str]) -> Value {
    for key in keys {
        value.as_object_mut().unwrap().remove(*key);
    }
    value
}

#[specforge_test_macros::test(
    behavior = "provide_explore_cli",
    verify = "specforge explore --format json is the explore prompt's payload for the same arguments"
)]
fn cli_and_prompt_explore_are_one_document() {
    let tmp = project("fx1");
    let root = tmp.path();
    let cases: Vec<(Vec<&str>, Value)> = vec![
        (vec![], json!({})),
        (vec!["login"], json!({"entity_id": "login"})),
        (
            vec!["login", "--depth", "0"],
            json!({"entity_id": "login", "depth": 0}),
        ),
        (vec!["--kind", "behavior"], json!({"kind": "behavior"})),
        (vec!["--kind", "behaviour"], json!({"kind": "behaviour"})),
    ];
    let arguments: Vec<Value> = cases
        .iter()
        .map(|(_, arguments)| arguments.clone())
        .collect();
    for ((flags, _), prompt) in cases
        .iter()
        .zip(prompt_payloads(root, "explore", &arguments))
    {
        let mut args = vec!["explore"];
        args.extend(flags);
        args.extend(["--path", s(root), "--format", "json"]);
        assert_eq!(cli_json(&args), prompt, "{args:?}");
    }
}

#[specforge_test_macros::test(
    behavior = "provide_explore_cli",
    verify = "an unknown entity is E003 naming the closest entity, exit 1"
)]
fn explore_refuses_an_unknown_entity() {
    let tmp = project("fx1");
    let run = cli(&["explore", "logn", "--path", s(tmp.path())]);
    assert_eq!(run.code, Some(1), "{}", run.stderr);
    assert_eq!(run.stdout, "");
    assert_eq!(
        run.stderr,
        "error[E003]: unresolved entity 'logn' — not found in graph\n  hint: did you mean 'login'?\n"
    );
}

#[specforge_test_macros::test(
    behavior = "provide_review_cli",
    verify = "specforge review --format json is the review prompt's payload for the same arguments"
)]
fn cli_and_prompt_review_are_one_document() {
    let tmp = project("fx1");
    let root = tmp.path();
    let cases: Vec<(Vec<&str>, Value)> = vec![
        (vec![], json!({})),
        (vec!["login"], json!({"entity_id": "login"})),
        (
            vec!["login", "--depth", "2"],
            json!({"entity_id": "login", "depth": 2}),
        ),
    ];
    let arguments: Vec<Value> = cases
        .iter()
        .map(|(_, arguments)| arguments.clone())
        .collect();
    for ((flags, _), prompt) in cases
        .iter()
        .zip(prompt_payloads(root, "review", &arguments))
    {
        let mut args = vec!["review"];
        args.extend(flags);
        args.extend(["--path", s(root), "--format", "json"]);
        assert_eq!(cli_json(&args), prompt, "{args:?}");
    }
}

#[specforge_test_macros::test(
    behavior = "provide_review_cli",
    verify = "a recorded report that cannot be read exits 2 with E045"
)]
fn review_refuses_an_unreadable_report() {
    let tmp = rv1();
    std::fs::write(tmp.path().join("specforge-report.json"), "{not json").unwrap();
    let run = cli(&["review", "--path", s(tmp.path())]);
    assert_eq!(run.code, Some(2), "{}", run.stderr);
    assert!(run.stderr.starts_with("error[E045]"), "{}", run.stderr);

    let ghost = cli(&["review", "ghost", "--path", s(rv1().path())]);
    assert_eq!(ghost.code, Some(1), "{}", ghost.stderr);
    assert!(ghost.stderr.starts_with("error[E003]"), "{}", ghost.stderr);
}

#[specforge_test_macros::test(
    behavior = "provide_infer_guide_cli",
    verify = "specforge infer-guide --format json is the infer prompt's guide data"
)]
fn cli_and_prompt_infer_guide_are_one_document() {
    let tmp = project("fx1");
    let root = tmp.path();
    let prompts = prompt_payloads(
        root,
        "infer",
        &[json!({}), json!({"scope": "kind:behavior"})],
    );
    assert_eq!(
        cli_json(&["infer-guide", "--path", s(root), "--format", "json"]),
        without(
            prompts[0].clone(),
            &["scope", "output_format", "validation"]
        )
    );
    assert_eq!(
        cli_json(&[
            "infer-guide",
            "behavior",
            "--path",
            s(root),
            "--format",
            "json"
        ]),
        without(prompts[1].clone(), &["scope", "validation"])
    );
}

#[specforge_test_macros::test(
    behavior = "provide_infer_guide_cli",
    verify = "an undeclared kind is unknown_kind naming the closest declared kind, exit 1"
)]
fn infer_guide_refuses_an_undeclared_kind() {
    let tmp = project("fx1");
    let run = cli(&["infer-guide", "behaviour", "--path", s(tmp.path())]);
    assert_eq!(run.code, Some(1), "{}", run.stderr);
    assert_eq!(run.stdout, "");
    assert_eq!(
        run.stderr,
        "error[unknown_kind]: unknown entity kind 'behaviour'\n  hint: did you mean 'behavior'?\n"
    );
}

#[specforge_test_macros::test(
    behavior = "compute_inference_guide",
    verify = "every builtin kind's example parses and holds no value of the wrong type"
)]
fn builtin_examples_are_well_typed() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    std::fs::create_dir_all(root.join("spec")).unwrap();
    std::fs::write(
        root.join("specforge.json"),
        json!({"name": "all", "version": "0.1.0", "spec_root": "spec",
               "extensions": ["@specforge/product", "@specforge/software",
                              "@specforge/testing", "@specforge/governance",
                              "@specforge/formal"]})
        .to_string(),
    )
    .unwrap();

    let overview = cli_json(&["infer-guide", "--path", s(root), "--format", "json"]);
    let kinds: Vec<&str> = overview["kinds"]
        .as_array()
        .unwrap()
        .iter()
        .map(|kind| kind["kind"].as_str().unwrap())
        .collect();
    assert!(kinds.len() >= 20, "{kinds:?}");
    let examples: Vec<String> = kinds
        .iter()
        .map(|kind| {
            let guide = cli_json(&["infer-guide", kind, "--path", s(root), "--format", "json"]);
            guide["example"].as_str().unwrap().to_string()
        })
        .collect();
    std::fs::write(root.join("spec/examples.spec"), examples.join("\n\n")).unwrap();

    let check = cli(&["check", s(root), "--format", "json"]);
    let report: Value = serde_json::from_str(&check.stdout)
        .unwrap_or_else(|e| panic!("check is not JSON ({e}): {}{}", check.stdout, check.stderr));
    let codes: Vec<&str> = report
        .as_array()
        .unwrap_or_else(|| panic!("{report}"))
        .iter()
        .map(|d| d["code"].as_str().unwrap())
        .collect();
    // The placeholders (`ref_id`, `ref_1`) name no entity: E003 and E010.
    // Nothing else is wrong: no syntax error (E001), no value of the wrong
    // type (E061).
    for code in &codes {
        assert!(
            matches!(*code, "E003" | "E010") || !code.starts_with('E'),
            "an example is wrong ({code}): {report}"
        );
    }
}

#[test]
fn explore_review_and_infer_guide_human_output() {
    let tmp = project("fx1");
    let root = tmp.path();
    for (name, args) in [
        ("explore_fx1_human", vec!["explore", "login"]),
        ("review_fx1_human", vec!["review"]),
        ("infer_guide_fx1_human", vec!["infer-guide"]),
        (
            "infer_guide_fx1_behavior_human",
            vec!["infer-guide", "behavior"],
        ),
    ] {
        let mut args = args;
        args.extend(["--path", s(root)]);
        let run = cli(&args);
        assert_eq!(run.code, Some(0), "{args:?}: {}", run.stderr);
        insta::assert_snapshot!(name, normalized_text(&run.stdout, root));
    }
}
