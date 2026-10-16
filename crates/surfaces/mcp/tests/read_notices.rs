// Every tool result and resource read that names the schema's version carries
// what versioning it against `.specforge/schema-cache.json` found (I016 for a
// record that cannot be read), in `_meta.diagnostics` (ADR 0111). MCP never
// records the schema, so a corrupt file is all the notice needs.

use crate::support::*;
use serde_json::{Value, json};
use specforge_test::prelude::*;

/// A served project over the builtin software extension, whose schema record
/// cannot be read.
fn project_with_unreadable_record() -> Served {
    let server = TestProject::new()
        .enabling(&["@specforge/software"])
        .file(
            "main.spec",
            "behavior act \"Act\" {\n  contract \"MUST act\"\n}\n",
        )
        .serve_components();
    server.write(".specforge/schema-cache.json", "{\"schema\": {");
    server
}

/// The codes in a reply's `result._meta.diagnostics`.
fn codes(response: &Value) -> Vec<String> {
    response["result"]["_meta"]["diagnostics"]
        .as_array()
        .map(|list| {
            list.iter()
                .map(|d| d["code"].as_str().unwrap().to_owned())
                .collect()
        })
        .unwrap_or_default()
}

fn assert_tool_carries_i016(server: &mut Served, name: &str, arguments: Value) {
    let response = server.call_tool(name, arguments);
    assert_eq!(codes(&response), ["I016"], "{name}: {response}");
}

#[specforge_test(
    behavior = "compute_schema_version",
    verify = "every reader of the schema reports the notices export reports for the same record"
)]
fn export_render_and_query_carry_an_unreadable_record() {
    let mut server = project_with_unreadable_record();
    assert_tool_carries_i016(&mut server, "specforge.export", json!({"format": "brief"}));
    assert_tool_carries_i016(&mut server, "specforge.render", json!({"format": "brief"}));
    assert_tool_carries_i016(&mut server, "specforge.query", json!({"entity_id": "act"}));
}

#[specforge_test(
    behavior = "compute_schema_version",
    verify = "every reader of the schema reports the notices export reports for the same record"
)]
fn the_schema_tool_carries_an_unreadable_record() {
    let mut server = project_with_unreadable_record();
    assert_tool_carries_i016(&mut server, "specforge.schema", json!({}));
}

#[specforge_test(
    behavior = "compute_schema_version",
    verify = "every reader of the schema reports the notices export reports for the same record"
)]
fn trace_and_model_carry_an_unreadable_record() {
    let mut server = project_with_unreadable_record();
    assert_tool_carries_i016(&mut server, "specforge.trace", json!({"entity_id": "act"}));
    assert_tool_carries_i016(&mut server, "specforge.model", json!({"format": "json"}));
}

#[specforge_test(
    behavior = "compute_schema_version",
    verify = "every reader of the schema reports the notices export reports for the same record"
)]
fn reads_carry_an_unreadable_record() {
    let mut server = project_with_unreadable_record();
    for uri in [
        "specforge://schema",
        "specforge://graph",
        "specforge://context",
        "specforge://brief",
    ] {
        let response = read_resource(&mut server, uri);
        assert_eq!(codes(&response), ["I016"], "{uri}: {response}");
    }
}

#[test]
fn an_unknown_kind_read_carries_i020() {
    let mut server = project_with_unreadable_record();
    let response = read_resource(&mut server, "specforge://entities/ghost");
    assert_eq!(codes(&response), ["I020"], "{response}");
}

#[specforge_test(
    behavior = "compute_schema_version",
    verify = "every reader of the schema reports the notices export reports for the same record"
)]
fn render_into_a_directory_carries_an_unreadable_record() {
    let mut server = project_with_unreadable_record();
    assert_tool_carries_i016(
        &mut server,
        "specforge.render",
        json!({"format": "dot", "out_dir": "out"}),
    );
    assert!(server.root().join("out").join("graph.dot").is_file());
}

#[specforge_test(
    behavior = "compute_schema_version",
    verify = "every reader of the schema reports the notices export reports for the same record"
)]
fn a_scoped_view_carries_an_unreadable_record() {
    let mut server = project_with_unreadable_record();
    for uri in ["specforge://graph/act", "specforge://context/act"] {
        let response = read_resource(&mut server, uri);
        assert_eq!(codes(&response), ["I016"], "{uri}: {response}");
    }
}

/// ADR 0111 D6: the trace prompt's payload has no `_meta`, so it reports no
/// notices, even over a record that cannot be read.
#[test]
fn the_trace_prompt_reports_no_notices() {
    let mut server = project_with_unreadable_record();
    let response = get_prompt(
        &mut server,
        "specforge://prompts/trace",
        json!({"entity_id": "act"}),
    );
    assert!(response["result"]["_meta"].is_null(), "{response}");
    assert!(prompt_payload(&response).get("notices").is_none());
}

/// A list and a search name no version, so they say their own I020 and no I016.
#[test]
fn list_and_search_report_their_own_notices_only() {
    let mut server = project_with_unreadable_record();
    let listed = server.call_tool("specforge.list", json!({"kind": "ghost"}));
    assert_eq!(codes(&listed), ["I020"], "{listed}");
    let searched = server.call_tool(
        "specforge.search",
        json!({"query": "act", "kinds": ["ghost"]}),
    );
    assert_eq!(codes(&searched), ["I020"], "{searched}");
}
