//! Each enumerated argument a core tool takes is advertised from its option
//! table (ADR 0027): the input schema's `enum` is every name the table
//! accepts (listed names, then aliases), its `default` the table's, and the
//! description names each choice. The tools that parse a format with a
//! table accept its names and aliases and refuse any other name.

use crate::support::{Serve, Served, TestProject, call_tool};
use serde_json::{Value, json};
use specforge_ops::options::OptionTable;
use specforge_ops::read::coverage::STATUS;
use specforge_ops::read::export::{AGENT_FORMAT, FORMAT};
use specforge_ops::read::infer::{END_STATUS, SESSION_ACTION};
use specforge_ops::read::model::{
    DEPS, GROUP_BY, MODEL_FIELDS, MODEL_FORMAT, OUTLINE_FIELDS, OUTLINE_FORMAT,
};
use specforge_ops::read::navigate::DIRECTION;

/// What a table says an input-schema property must hold.
struct Advertised {
    tool: &'static str,
    argument: &'static str,
    accepted: Vec<&'static str>,
    names: Vec<&'static str>,
    default: Option<&'static str>,
}

fn advertised<T: Copy + PartialEq>(
    tool: &'static str,
    argument: &'static str,
    table: &OptionTable<T>,
) -> Advertised {
    Advertised {
        tool,
        argument,
        accepted: table.accepted().collect(),
        names: table.names().collect(),
        default: table.default_name(),
    }
}

/// [`advertised`] of a required argument: no default.
fn required<T: Copy + PartialEq>(
    tool: &'static str,
    argument: &'static str,
    table: &OptionTable<T>,
) -> Advertised {
    Advertised {
        default: None,
        ..advertised(tool, argument, table)
    }
}

/// Every enumerated argument of a core tool, with the table it reads.
fn enumerated() -> Vec<Advertised> {
    vec![
        advertised("specforge.query", "format", &AGENT_FORMAT),
        advertised("specforge.export", "format", &FORMAT),
        required("specforge.render", "format", &FORMAT),
        advertised("specforge.model", "format", &MODEL_FORMAT),
        advertised("specforge.model", "group_by", &GROUP_BY),
        advertised("specforge.model", "fields", &MODEL_FIELDS),
        advertised("specforge.outline_extensions", "format", &OUTLINE_FORMAT),
        advertised("specforge.outline_extensions", "fields", &OUTLINE_FIELDS),
        advertised("specforge.outline_extensions", "deps", &DEPS),
        advertised("specforge.coverage", "status_filter", &STATUS),
        advertised("specforge.find_references", "direction", &DIRECTION),
        required("specforge.infer_session", "action", &SESSION_ACTION),
        advertised("specforge.infer_session", "status", &END_STATUS),
    ]
}

/// The name lists that are not option tables (ADR 0018's severity and
/// lint profiles): the tool and the argument (`lint`'s items) they list.
fn name_lists() -> Vec<(&'static str, &'static str, &'static [&'static str])> {
    vec![
        (
            "specforge.validate",
            "severity_filter",
            specforge_ops::analysis::check::SEVERITY_NAMES,
        ),
        (
            "specforge.validate",
            "lint",
            specforge_project::LINT_PROFILE_NAMES,
        ),
    ]
}

/// The listed names of a property's `enum` (an array's items' for a list).
fn listed(property: &Value) -> Option<Vec<&str>> {
    let names = property
        .get("enum")
        .or_else(|| property["items"].get("enum"))?;
    Some(
        names
            .as_array()
            .unwrap()
            .iter()
            .map(|name| name.as_str().unwrap())
            .collect(),
    )
}

#[test]
fn every_input_enum_comes_from_a_table_or_a_name_list() {
    let tables = enumerated();
    let lists = name_lists();
    for (tool, argument, names) in &lists {
        assert_eq!(
            listed(&property(tool, argument)).as_deref(),
            Some(*names),
            "{tool}.{argument}"
        );
    }
    for spec in specforge_mcp::tools::CORE_TOOLS {
        let schema = spec.input_schema();
        let Some(properties) = schema["properties"].as_object() else {
            continue;
        };
        for (argument, property) in properties {
            if listed(property).is_none() {
                continue;
            }
            let known = tables
                .iter()
                .any(|t| t.tool == spec.name() && t.argument == argument)
                || lists
                    .iter()
                    .any(|(tool, a, _)| *tool == spec.name() && a == argument);
            assert!(
                known,
                "{}.{argument} lists names no option table or name list holds",
                spec.name()
            );
        }
    }
}

/// The input schema property `argument` of core tool `tool`.
fn property(tool: &str, argument: &str) -> Value {
    let spec = specforge_mcp::tools::CORE_TOOLS
        .iter()
        .find(|spec| spec.name() == tool)
        .unwrap_or_else(|| panic!("no core tool {tool}"));
    spec.input_schema()["properties"][argument].clone()
}

#[specforge_test_macros::test(
    behavior = "name_enumerated_options_once",
    verify = "each enumerated MCP argument advertises the table's names and default"
)]
fn each_enumerated_argument_advertises_its_table() {
    for expected in enumerated() {
        let at = format!("{}.{}", expected.tool, expected.argument);
        let property = property(expected.tool, expected.argument);
        assert_eq!(property["type"], "string", "{at}");
        let listed: Vec<&str> = property["enum"]
            .as_array()
            .unwrap_or_else(|| panic!("{at} lists no enum: {property}"))
            .iter()
            .map(|name| name.as_str().unwrap())
            .collect();
        assert_eq!(listed, expected.accepted, "{at}");
        assert_eq!(
            property.get("default").and_then(Value::as_str),
            expected.default,
            "{at}: one default, the table's, on every surface"
        );
        let description = property["description"].as_str().unwrap_or_default();
        for name in &expected.names {
            assert!(description.contains(name), "{at}: {description}");
        }
    }
}

/// A server serving a project on disk: one behavior `alpha`, the software
/// extension.
fn served() -> Served {
    TestProject::new()
        .enabling(&["@specforge/software"])
        .file(
            "main.spec",
            "behavior alpha \"Alpha\" {\n  contract \"The system MUST work\"\n}\n",
        )
        .serve_components()
}

/// A successful call's result, its text read as JSON.
fn content(response: &Value) -> Value {
    assert_ne!(response["result"]["isError"], true, "{response}");
    let text = response["result"]["content"][0]["text"].as_str().unwrap();
    serde_json::from_str(text).unwrap_or_else(|e| panic!("{e}: {text}"))
}

#[specforge_test_macros::test(
    behavior = "provide_mcp_render_tool",
    verify = "graph and its alias json select the full graph renderer"
)]
fn render_accepts_graph_and_its_json_alias() {
    let mut server = served();
    let mut written = Vec::new();
    for name in ["graph", "json"] {
        let out = server.root().join(name);
        let result = content(&call_tool(
            &mut server,
            "specforge.render",
            json!({"format": name, "out_dir": out.to_str().unwrap()}),
        ));
        assert_eq!(result["format"], "graph", "{name}: {result}");
        let file = out.join("graph.json");
        assert_eq!(
            result["output_files"],
            json!([format!("{name}/graph.json")]),
            "{name}"
        );
        let graph: Value = serde_json::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
        assert_eq!(
            graph["format_version"], "2.0",
            "the full graph export: {graph}"
        );
        written.push(graph);
    }
    assert_eq!(written[0], written[1], "json is graph");
}

#[specforge_test_macros::test(
    behavior = "provide_mcp_query_tool",
    verify = "an unknown format is an invalid-input error naming the expected formats"
)]
fn query_refuses_an_unknown_format() {
    let mut server = served();
    for (name, message) in [
        (
            "yaml",
            "Unknown format: yaml. Expected: graph, context, brief",
        ),
        // dot is the render tool's.
        (
            "dot",
            "Unknown format: dot. Expected: graph, context, brief",
        ),
    ] {
        let response = call_tool(
            &mut server,
            "specforge.query",
            json!({"entity_id": "alpha", "format": name}),
        );
        let error = crate::tool_errors::mcp_error(&response);
        assert_eq!(error["code"], "invalid_input", "{error}");
        assert_eq!(error["argument"], "format", "{error}");
        assert_eq!(error["message"], message);
    }
    // The json alias reads as graph.
    let graph = content(&call_tool(
        &mut server,
        "specforge.query",
        json!({"entity_id": "alpha", "format": "graph"}),
    ));
    let json = content(&call_tool(
        &mut server,
        "specforge.query",
        json!({"entity_id": "alpha", "format": "json"}),
    ));
    assert_eq!(graph, json);
    assert_eq!(graph["nodes"][0]["id"], "alpha", "{graph}");
}

#[test]
fn coverage_and_find_references_refuse_with_the_table_wording() {
    let mut server = served();
    let coverage = call_tool(
        &mut server,
        "specforge.coverage",
        json!({"status_filter": "coverd"}),
    );
    let error = crate::tool_errors::mcp_error(&coverage);
    assert_eq!(error["code"], "invalid_input", "{error}");
    assert_eq!(error["argument"], "status_filter", "{error}");
    assert_eq!(
        error["message"],
        "Unknown coverage status: coverd. Expected: covered, partial, uncovered"
    );
    assert_eq!(error["data"]["suggestion"], "did you mean 'covered'?");

    let references = call_tool(
        &mut server,
        "specforge.find_references",
        json!({"entity_id": "alpha", "direction": "sideways"}),
    );
    let error = crate::tool_errors::mcp_error(&references);
    assert_eq!(error["code"], "invalid_input", "{error}");
    assert_eq!(error["argument"], "direction", "{error}");
    assert_eq!(
        error["message"],
        "Unknown direction: sideways. Expected: incoming, outgoing, both"
    );

    // Absent, the direction is the table's default, echoed by name.
    let answered = content(&call_tool(
        &mut server,
        "specforge.find_references",
        json!({"entity_id": "alpha"}),
    ));
    assert_eq!(answered["direction"], "incoming", "{answered}");
}

#[specforge_test_macros::test(
    behavior = "name_enumerated_options_once",
    verify = "a refusal's available choices are the names its message lists, an alias never among them"
)]
fn a_refusal_offers_one_list_the_tables_names() {
    let mut server = served();
    let response = call_tool(&mut server, "specforge.render", json!({"format": "yaml"}));
    let error = crate::tool_errors::mcp_error(&response);
    let names: Vec<&str> = FORMAT.names().collect();
    // The message and the data are the table's names, in its order; the
    // `json` alias is accepted and in neither.
    assert_eq!(
        error["message"],
        format!("Unknown format: yaml. Expected: {}", names.join(", "))
    );
    assert_eq!(error["data"]["available_renderers"], json!(names));
    assert!(FORMAT.accepted().any(|name| name == "json"));
    assert!(!names.contains(&"json"));
    // One failure vocabulary: the refusal is invalid input, with no code of
    // its own beside the kind.
    assert_eq!(error["code"], "invalid_input", "{error}");
    assert!(error["data"].get("code").is_none(), "{error}");
}
