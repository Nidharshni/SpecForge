//! The product extension's commands over MCP: `specforge.product.<command>`, the tool each command declared by
//! `@specforge/product` is (ADR 0008, ADR 0011). The CLI's `product_commands.rs` runs the same commands through the
//! binary.

use crate::support::*;
use serde_json::{Value, json};
use specforge_mcp::testing::rpc;
use specforge_project::testing::specs;
use specforge_test::prelude::*;

/// The `specforge product` tests' project: product enabled, `spec.spec` the one source.
fn product_project() -> TestProject {
    TestProject::new()
        .config(|c| c["name"] = "test-project".into())
        .enabling(&["@specforge/product"])
        .file("spec.spec", specs::PRODUCT_COMMANDS)
}

#[specforge_test(
    behavior = "surface_list_features",
    verify = "status filter reduces result set"
)]
fn the_features_command_is_an_mcp_tool_with_its_filters() {
    let mut served = Served::rooted(product_project());

    let all = served.tool("specforge.product.features", json!({}));
    assert_eq!(all["total"], 2, "{all}");
    let done = served.tool("specforge.product.features", json!({"status": "done"}));
    assert_eq!(done["total"], 1, "{done}");
    assert_eq!(done["features"][0]["id"], "f2");
}

#[specforge_test(
    behavior = "dispatch_surface_command",
    verify = "over MCP a command is asked for json and its JSON output is the tool's structured content"
)]
fn over_mcp_a_command_answers_json_as_structured_content() {
    // MCP 2025-06-18 is the first revision with structured content.
    let mut served = Served::rooted_at(product_project(), "2025-06-18");

    // The tool has no format argument: the host always asks for json.
    let listed = rpc::call(&mut served, "tools/list", json!({}));
    let tool = listed["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["name"] == "specforge.product.milestone_completion")
        .unwrap();
    assert!(
        tool["inputSchema"]["properties"].get("format").is_none(),
        "{tool}"
    );

    let response = served.call_tool(
        "specforge.product.milestone_completion",
        json!({"milestone": "m1"}),
    );
    let result = &response["result"];
    assert_eq!(result["isError"], false, "{response}");
    let structured = &result["structuredContent"];
    assert_eq!(structured["milestone_id"], "m1", "{response}");
    assert_eq!(rpc::tool_json(&response), *structured);
}

#[specforge_test(
    behavior = "surface_format_conventions",
    verify = "an MCP tool call returns the json payload"
)]
fn an_mcp_tool_call_returns_the_json_payload() {
    let project = product_project();
    // What `specforge product health --format json` prints: the command run over the compiled project.
    let compiled = project.compile();
    let view = specforge_ops::view::ProjectView::of(&compiled);
    let commands = specforge_ops::extension::command::ExtensionCommands::build(view.registries());
    let health = commands
        .of("product")
        .find(|command| command.id() == "health")
        .expect("product health");
    let printed = specforge_ops::extension::command::run(
        &view,
        health,
        &serde_json::Map::new(),
        specforge_ops::extension::command::CommandFormat::Json,
    )
    .expect("health runs");
    let cli: Value = serde_json::from_str(&printed.stdout).unwrap();

    // Asked for nothing, the tool answers what --format json prints.
    let mut served = Served::rooted(project);
    assert_eq!(served.tool("specforge.product.health", json!({})), cli);
}

#[specforge_test(
    behavior = "surface_error_handling",
    verify = "MCP tool errors are isError results carrying the error object"
)]
fn over_mcp_an_error_is_an_is_error_result_with_the_object() {
    let mut served = Served::rooted_at(product_project(), "2025-06-18");

    let response = served.call_tool(
        "specforge.product.journey_coverage",
        json!({"journey": "j2"}),
    );
    let result = &response["result"];
    assert_eq!(result["isError"], true, "{response}");
    let expected = json!({"code": "ENTITY_NOT_FOUND",
        "message": "journey 'j2' not found", "entity_id": "j2", "suggestion": "j1"});
    assert_eq!(rpc::tool_json(&response), expected);
    assert_eq!(result["structuredContent"], expected);
}
