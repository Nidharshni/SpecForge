//! One pairing table (ADR 0101 D5): each row spells one request twice, as a `specforge` command line and as
//! an MCP tool call over the same project, and both surfaces answer the same document or both refuse. Every
//! relation an operation reads (a sentinel, arguments that exclude or need each other) has a row.

use serde_json::{Value, json};
use std::path::Path;

use crate::coverage_corpus::project;
use crate::support::pairing::{Outcome, cli_outcome, mcp_calls, mcp_outcome, served};
use crate::support::{Cli, TestProject};

struct Pair {
    label: &'static str,
    project: fn() -> TestProject,
    /// The command line; `{root}` in a token is the served project's root.
    cli: &'static [&'static str],
    /// `specforge`'s exit: 0 answered, 1 refused by the operation or a failed verdict, 2 refused by clap.
    exit: i32,
    /// A tool, a prompt (`specforge://prompts/...`), or a resource (`specforge://<view>?<query>`), whose
    /// arguments are none.
    tool: &'static str,
    arguments: fn() -> Value,
}

const PAIRS: &[Pair] = &[
    Pair {
        label: "collect_runner_named",
        project: cargo_test_project,
        cli: &[
            "collect",
            "--path",
            "{root}",
            "--runner",
            "cargo-test",
            "--no-run",
            "--format",
            "json",
        ],
        exit: 0,
        tool: "specforge.collect",
        arguments: || json!({"runner": "cargo-test"}),
    },
    Pair {
        label: "collect_runner_auto",
        project: cargo_test_project,
        cli: &[
            "collect", "--path", "{root}", "--runner", "auto", "--no-run", "--format", "json",
        ],
        exit: 1,
        tool: "specforge.collect",
        arguments: || json!({"runner": "auto"}),
    },
    Pair {
        label: "trace_entity_and_plan",
        project: rv1_with_plan,
        cli: &[
            "trace",
            "alpha",
            "--plan",
            "{root}/agent-plan.json",
            "--path",
            "{root}",
            "--format",
            "json",
        ],
        exit: 2,
        tool: "specforge.trace",
        arguments: || json!({"entity_id": "alpha", "plan": plan()}),
    },
    Pair {
        label: "add_one_string_with_commas",
        project: rv1,
        cli: &[
            "add",
            "@specforge/product,@specforge/governance",
            "--path",
            "{root}",
            "--format",
            "json",
        ],
        exit: 1,
        tool: "specforge.add_extension",
        arguments: || json!({"specifiers": ["@specforge/product,@specforge/governance"]}),
    },
    Pair {
        label: "export_with_and_without_schema",
        project: rv1,
        cli: &[
            "export",
            "--path",
            "{root}",
            "--format",
            "graph",
            "--with-schema",
            "--no-schema",
        ],
        exit: 2,
        tool: "specforge.export",
        arguments: || json!({"format": "graph", "with_schema": true, "no_schema": true}),
    },
    Pair {
        label: "export_depth_without_scope",
        project: rv1,
        cli: &[
            "export", "--path", "{root}", "--format", "graph", "--depth", "2",
        ],
        exit: 2,
        tool: "specforge.export",
        arguments: || json!({"format": "graph", "depth": 2}),
    },
    Pair {
        label: "explore_depth_without_entity",
        project: rv1,
        cli: &[
            "explore", "--path", "{root}", "--depth", "1", "--format", "json",
        ],
        exit: 2,
        tool: "specforge://prompts/explore",
        arguments: || json!({"depth": "1"}),
    },
    Pair {
        label: "review_depth_without_entity",
        project: rv1,
        cli: &[
            "review", "--path", "{root}", "--depth", "1", "--format", "json",
        ],
        exit: 2,
        tool: "specforge://prompts/review",
        arguments: || json!({"depth": "1"}),
    },
    Pair {
        label: "model_depth_without_root",
        project: rv1,
        cli: &["model", "--path", "{root}", "--depth", "2"],
        exit: 2,
        tool: "specforge.model",
        arguments: || json!({"depth": 2}),
    },
    Pair {
        label: "export_context_with_schema",
        project: rv1,
        cli: &[
            "export",
            "--path",
            "{root}",
            "--format",
            "context",
            "--with-schema",
        ],
        exit: 0,
        tool: "specforge.export",
        arguments: || json!({"format": "context", "with_schema": true}),
    },
    Pair {
        label: "export_brief_with_schema",
        project: rv1,
        cli: &[
            "export",
            "--path",
            "{root}",
            "--format",
            "brief",
            "--with-schema",
        ],
        exit: 0,
        tool: "specforge.export",
        arguments: || json!({"format": "brief", "with_schema": true}),
    },
    Pair {
        label: "export_graph_no_schema",
        project: rv1,
        cli: &[
            "export",
            "--path",
            "{root}",
            "--format",
            "graph",
            "--no-schema",
        ],
        exit: 0,
        tool: "specforge.export",
        arguments: || json!({"format": "graph", "no_schema": true}),
    },
    Pair {
        label: "schema_no_edges",
        project: rv1,
        cli: &["schema", "--path", "{root}", "--no-edges"],
        exit: 0,
        tool: "specforge.schema",
        arguments: || json!({"include_edges": false}),
    },
    Pair {
        label: "schema_validation_rules",
        project: rv1,
        cli: &["schema", "--path", "{root}", "--validation-rules"],
        exit: 0,
        tool: "specforge.schema",
        arguments: || json!({"include_validation_rules": true}),
    },
    Pair {
        label: "schema_kind_rules_no_edges",
        project: rv1,
        cli: &[
            "schema",
            "--path",
            "{root}",
            "--kind",
            "behavior",
            "--validation-rules",
            "--no-edges",
        ],
        exit: 0,
        tool: "specforge.schema",
        arguments: || json!({"kind": "behavior", "include_validation_rules": true, "include_edges": false}),
    },
    Pair {
        label: "trace_plan",
        project: fx1_with_plan,
        cli: &[
            "trace",
            "--plan",
            "{root}/agent-plan.json",
            "--path",
            "{root}",
            "--format",
            "json",
        ],
        exit: 1,
        tool: "specforge.trace",
        arguments: || json!({"plan": fx1_plan()}),
    },
    Pair {
        label: "edit_new",
        project: edit_project,
        cli: &[
            "new",
            "behavior",
            "epsilon",
            "Epsilon",
            "--in",
            "main.spec",
            "--contract",
            "The system MUST do epsilon",
            "--invariants",
            "rule_one",
            "--dry-run",
            "--path",
            "{root}",
            "--format",
            "json",
        ],
        exit: 0,
        tool: "specforge.edit",
        arguments: || {
            json!({"dry_run": true, "edits": [{"op": "create", "kind": "behavior", "id": "epsilon",
                "title": "Epsilon", "file": "main.spec",
                "fields": {"contract": "The system MUST do epsilon", "invariants": ["rule_one"]}}]})
        },
    },
    Pair {
        label: "edit_set",
        project: edit_project,
        cli: &[
            "set",
            "delta",
            "category",
            "query",
            "--dry-run",
            "--path",
            "{root}",
            "--format",
            "json",
        ],
        exit: 0,
        tool: "specforge.edit",
        arguments: || json!({"dry_run": true, "edits": [{"op": "set", "entity_id": "delta", "field": "category", "value": "query"}]}),
    },
    Pair {
        label: "edit_set_title",
        project: edit_project,
        cli: &[
            "set",
            "delta",
            "--title",
            "Delta One",
            "--dry-run",
            "--path",
            "{root}",
            "--format",
            "json",
        ],
        exit: 0,
        tool: "specforge.edit",
        arguments: || json!({"dry_run": true, "edits": [{"op": "retitle", "entity_id": "delta", "title": "Delta One"}]}),
    },
    Pair {
        label: "edit_unset",
        project: edit_project,
        cli: &[
            "unset",
            "delta",
            "category",
            "--dry-run",
            "--path",
            "{root}",
            "--format",
            "json",
        ],
        exit: 0,
        tool: "specforge.edit",
        arguments: || json!({"dry_run": true, "edits": [{"op": "unset", "entity_id": "delta", "field": "category"}]}),
    },
    Pair {
        label: "edit_link",
        project: edit_project,
        cli: &[
            "link",
            "delta",
            "invariants",
            "rule_two",
            "--dry-run",
            "--path",
            "{root}",
            "--format",
            "json",
        ],
        exit: 0,
        tool: "specforge.edit",
        arguments: || json!({"dry_run": true, "edits": [{"op": "link", "entity_id": "delta", "field": "invariants", "target": "rule_two"}]}),
    },
    Pair {
        label: "edit_link_verify",
        project: edit_project,
        cli: &[
            "link",
            "delta",
            "--verify",
            "unit:delta handles errors",
            "--dry-run",
            "--path",
            "{root}",
            "--format",
            "json",
        ],
        exit: 0,
        tool: "specforge.edit",
        arguments: || json!({"dry_run": true, "edits": [{"op": "add_verify", "entity_id": "delta", "kind": "unit", "text": "delta handles errors"}]}),
    },
    Pair {
        label: "edit_unlink",
        project: edit_project,
        cli: &[
            "unlink",
            "delta",
            "invariants",
            "rule_one",
            "--dry-run",
            "--path",
            "{root}",
            "--format",
            "json",
        ],
        exit: 0,
        tool: "specforge.edit",
        arguments: || json!({"dry_run": true, "edits": [{"op": "unlink", "entity_id": "delta", "field": "invariants", "target": "rule_one"}]}),
    },
    Pair {
        label: "edit_unlink_verify",
        project: edit_project,
        cli: &[
            "unlink",
            "delta",
            "--verify",
            "unit:delta works",
            "--dry-run",
            "--path",
            "{root}",
            "--format",
            "json",
        ],
        exit: 0,
        tool: "specforge.edit",
        arguments: || json!({"dry_run": true, "edits": [{"op": "remove_verify", "entity_id": "delta", "kind": "unit", "text": "delta works"}]}),
    },
    Pair {
        label: "edit_delete",
        project: edit_project,
        cli: &[
            "delete",
            "rule_two",
            "--dry-run",
            "--path",
            "{root}",
            "--format",
            "json",
        ],
        exit: 0,
        tool: "specforge.edit",
        arguments: || json!({"dry_run": true, "edits": [{"op": "delete", "entity_id": "rule_two"}]}),
    },
    Pair {
        label: "edit_delete_unlinking",
        project: edit_project,
        cli: &[
            "delete",
            "rule_one",
            "--unlink-references",
            "--dry-run",
            "--path",
            "{root}",
            "--format",
            "json",
        ],
        exit: 0,
        tool: "specforge.edit",
        arguments: || json!({"dry_run": true, "edits": [{"op": "delete", "entity_id": "rule_one", "unlink_references": true}]}),
    },
    Pair {
        label: "edit_delete_referenced",
        project: edit_project,
        cli: &[
            "delete",
            "rule_one",
            "--dry-run",
            "--path",
            "{root}",
            "--format",
            "json",
        ],
        exit: 1,
        tool: "specforge.edit",
        arguments: || json!({"dry_run": true, "edits": [{"op": "delete", "entity_id": "rule_one"}]}),
    },
    Pair {
        label: "export_kinds_list_spaced",
        project: rv1,
        cli: &[
            "export",
            "--path",
            "{root}",
            "--format",
            "brief",
            "--kinds",
            "behavior, type",
        ],
        exit: 0,
        tool: "specforge://brief?kinds=behavior,%20type",
        arguments: || Value::Null,
    },
    Pair {
        label: "export_kinds_list_empty_item",
        project: rv1,
        cli: &[
            "export",
            "--path",
            "{root}",
            "--format",
            "brief",
            "--kinds",
            "behavior,",
        ],
        exit: 0,
        tool: "specforge://brief?kinds=behavior,",
        arguments: || Value::Null,
    },
    Pair {
        label: "query_kinds_list",
        project: rv1,
        cli: &[
            "query",
            "Color",
            "--path",
            "{root}",
            "--format",
            "brief",
            "--kinds",
            "type, behavior",
        ],
        exit: 0,
        tool: "specforge.query",
        arguments: || json!({"entity_id": "Color", "format": "brief", "kinds": ["type", "behavior"]}),
    },
];

/// `rv1` with a behavior that has a category, an invariant link and two verify lines, and the invariants.
fn edit_project() -> TestProject {
    rv1().file(
        "spec/more.spec",
        "behavior delta \"Delta\" {\n  category   command\n  invariants [rule_one]\n  contract   \"The system MUST do delta\"\n  verify unit \"delta works\"\n}\n\ninvariant rule_one \"Rule One\" {\n  guarantee \"One MUST hold\"\n}\n\ninvariant rule_two \"Rule Two\" {\n  guarantee \"Two MUST hold\"\n}\n",
    )
}

/// The project of one behavior with a passing recorded test, for the cargo-test collector.
fn cargo_test_project() -> TestProject {
    TestProject::new()
        .raw_config(
            r#"{"name":"demo","version":"0.1.0","extensions":["@specforge/software","@specforge/testing","@specforge/cargo-test"]}"#,
        )
        .file("app.spec", "behavior login \"Login\" {\n  verify unit \"accepts valid credentials\"\n}\n")
        .file("Cargo.toml", "")
        .file(
            "target/specforge/demo.json",
            r#"{"entries":[{"entity_id":"login","test_name":"accepts","verify":"accepts valid credentials","status":"passed"}]}"#,
        )
}

/// A scratch copy of `fixtures/read_views/rv1`.
fn rv1() -> TestProject {
    TestProject::copy_of(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/read_views/rv1"),
    )
}

/// The agent plan of the trace rows: it names `alpha`.
fn plan() -> Value {
    json!({"plan_id": "p", "entries": [{"entity_id": "alpha", "action": "implement"}]})
}

/// `rv1` with [`plan`] written as `agent-plan.json`.
fn rv1_with_plan() -> TestProject {
    rv1().file("agent-plan.json", plan().to_string())
}

/// The plan of the `trace_plan` row: one entry names an entity, one names none.
fn fx1_plan() -> Value {
    json!({"plan_id": "p", "entries": [
        {"entity_id": "login", "action": "implement"},
        {"entity_id": "ghost", "action": "implement"}]})
}

/// The coverage corpus's `fx1` with [`fx1_plan`] written as `agent-plan.json`.
fn fx1_with_plan() -> TestProject {
    project("fx1").file("agent-plan.json", fx1_plan().to_string())
}

/// Run `label`'s row on both surfaces over one served project: assert the CLI's exit and that the
/// outcomes are equal; return both for a row's own assertions.
fn pair(label: &str) -> (Outcome, Outcome) {
    let row = PAIRS
        .iter()
        .find(|row| row.label == label)
        .unwrap_or_else(|| panic!("no row {label}"));
    let mut served = served((row.project)());
    let root = served.root().to_path_buf();
    let spelled = root.to_str().unwrap();
    let args: Vec<String> = row
        .cli
        .iter()
        .map(|t| t.replace("{root}", spelled))
        .collect();
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let run = Cli::on(served.project()).bare(&args);
    assert_eq!(
        run.code, row.exit,
        "{label}: {args:?}\n{}{}",
        run.stdout, run.stderr
    );
    let call = if row.tool.starts_with("specforge://prompts/") {
        json!({"method": "prompts/get", "params": {"name": row.tool, "arguments": (row.arguments)()}})
    } else if row.tool.starts_with("specforge://") {
        json!({"method": "resources/read", "params": {"uri": row.tool}})
    } else {
        json!({"name": row.tool, "arguments": (row.arguments)()})
    };
    let answer = mcp_calls(&mut served, &[call]).remove(0);
    let (cli, mcp) = (cli_outcome(&run, &root), mcp_outcome(&answer, &root));
    assert_eq!(
        cli, mcp,
        "{label}: the surfaces disagree\ncli: {args:?}\ntool: {} {answer}",
        row.tool
    );
    (cli, mcp)
}

#[specforge_test_macros::test(
    behavior = "parse_extension_specifier",
    verify = "a string holding commas is one extension specifier, for add and init alike"
)]
fn add_reads_one_string_with_commas_as_one_specifier() {
    assert_eq!(pair("add_one_string_with_commas").0, Outcome::Refused);
}

#[test]
fn every_row_has_a_unique_label() {
    let mut labels: Vec<&str> = PAIRS.iter().map(|row| row.label).collect();
    labels.sort_unstable();
    let rows = labels.len();
    labels.dedup();
    assert_eq!(labels.len(), rows, "a label names two rows");
}

#[test]
fn collect_with_a_named_runner_is_one_request() {
    let (cli, _) = pair("collect_runner_named");
    assert!(matches!(cli, Outcome::Answered(_)), "{cli:?}");
}

#[specforge_test_macros::test(
    behavior = "provide_mcp_collect_tool",
    verify = "specforge.collect reads runner as specforge collect --runner reads it"
)]
fn collect_runner_auto_is_one_request() {
    assert_eq!(pair("collect_runner_auto").0, Outcome::Refused);
}

#[specforge_test_macros::test(
    behavior = "validate_agent_plan",
    verify = "specforge trace and specforge.trace both refuse an entity with a plan"
)]
fn trace_entity_and_plan_is_one_request() {
    assert_eq!(pair("trace_entity_and_plan").0, Outcome::Refused);
}

#[specforge_test_macros::test(
    behavior = "provide_mcp_export_tool",
    verify = "specforge export and specforge.export both refuse with_schema with no_schema"
)]
fn export_both_schema_flags_is_one_request() {
    assert_eq!(pair("export_with_and_without_schema").0, Outcome::Refused);
}

#[test]
fn export_depth_without_scope_is_refused_on_both() {
    assert_eq!(pair("export_depth_without_scope").0, Outcome::Refused);
}

#[test]
fn explore_depth_without_entity_is_refused_on_both() {
    assert_eq!(pair("explore_depth_without_entity").0, Outcome::Refused);
}

#[test]
fn review_depth_without_entity_is_refused_on_both() {
    assert_eq!(pair("review_depth_without_entity").0, Outcome::Refused);
}

#[test]
fn model_depth_without_root_is_refused_on_both() {
    assert_eq!(pair("model_depth_without_root").0, Outcome::Refused);
}

/// The document a row's surfaces both answered.
fn answered(label: &str) -> Value {
    match pair(label).0 {
        Outcome::Answered(document) => document,
        Outcome::Refused => panic!("{label}: both surfaces refused"),
    }
}

#[specforge_test_macros::test(
    behavior = "provide_mcp_export_tool",
    verify = "with_schema embeds the schema in a context, brief or budgeted export, and no_schema leaves it out of a graph export"
)]
fn export_schema_flags_select_the_same_export() {
    for label in ["export_context_with_schema", "export_brief_with_schema"] {
        let with = answered(label);
        // The pairing's normalization redacts an embedded schema to a marker.
        assert_eq!(with["schema"], "[SCHEMA]", "{label}: {with}");
    }
    let without = answered("export_graph_no_schema");
    assert_eq!(without["format_version"], "1.0", "{without}");
    assert!(without.get("schema").is_none(), "{without}");

    let mut served = served(rv1());
    let call = json!({"name": "specforge.export", "arguments": {
        "format": "graph", "max_tokens": 100000, "with_schema": true}});
    let budgeted = mcp_calls(&mut served, &[call]).remove(0);
    assert!(budgeted["schema"].is_object(), "{budgeted}");
}

#[specforge_test_macros::test(
    behavior = "serve_schema_resource",
    verify = "--no-edges and --validation-rules select what include_edges and include_validation_rules select"
)]
fn schema_flags_select_the_same_document() {
    let no_edges = answered("schema_no_edges");
    assert!(no_edges.get("edge_types").is_none(), "{no_edges}");
    let rules = answered("schema_validation_rules");
    assert!(rules.get("edge_types").is_some(), "{rules}");
    assert!(rules.get("validation_rules").is_some(), "{rules}");
    let both = answered("schema_kind_rules_no_edges");
    assert!(both.get("edge_types").is_none(), "{both}");
    assert!(both.get("validation_rules").is_some(), "{both}");
}

#[specforge_test_macros::test(
    behavior = "validate_agent_plan",
    verify = "specforge trace --plan and specforge.trace answer the same document for a plan"
)]
fn a_plan_check_is_one_document() {
    assert_eq!(answered("trace_plan")["ok"], false);
}

#[specforge_test_macros::test(
    behavior = "validate_typed_edit",
    verify = "a preview refuses what the write would"
)]
fn every_edit_verb_is_one_request_on_both_surfaces() {
    for label in [
        "edit_new",
        "edit_set",
        "edit_set_title",
        "edit_unset",
        "edit_link",
        "edit_link_verify",
        "edit_unlink",
        "edit_unlink_verify",
        "edit_delete",
        "edit_delete_unlinking",
    ] {
        let document = answered(label);
        assert_eq!(document["dry_run"], true, "{label}: {document}");
        assert!(!document["diff"].as_str().unwrap().is_empty(), "{label}");
    }
    assert_eq!(pair("edit_delete_referenced").0, Outcome::Refused);
}

/// The kinds of the nodes a row's surfaces both answered.
fn kinds_of(label: &str) -> std::collections::BTreeSet<String> {
    answered(label)["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|node| node["kind"].as_str().unwrap().to_string())
        .collect()
}

#[specforge_test_macros::test(
    behavior = "export_agent_graph_format",
    verify = "a kinds list written as text names the same kinds on the command line and in a graph resource's query: the items between commas, each trimmed, an empty item a kind the project does not know"
)]
fn a_kinds_list_written_as_text_is_one_filter_on_every_surface() {
    assert_eq!(
        kinds_of("export_kinds_list_spaced"),
        ["behavior", "type"].map(String::from).into()
    );
    assert_eq!(
        kinds_of("export_kinds_list_empty_item"),
        ["behavior"].map(String::from).into()
    );
}

#[specforge_test_macros::test(
    behavior = "query_graph_multi_resolution",
    verify = "specforge query --kinds reads a kinds list as specforge export --kinds does, and answers what specforge.query answers for its items"
)]
fn query_kinds_is_a_kinds_list_as_exports_is() {
    let ids: Vec<String> = answered("query_kinds_list")["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|node| node["id"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(ids.len(), 3, "Color and its two variants: {ids:?}");
}
