//! Tests for wiring extension surface contributions to MCP tool/resource registries.
//! Verifies that manifest-declared MCP tools and resources appear in the MCP server's
//! discovery responses after initialization with a project containing surface-contributing extensions.
//! Also tests dynamic kind-based tools and resources generated from the graph.

use serde_json::{Value, json};
use specforge_mcp::McpServer;
use specforge_test::prelude::*;
use std::fs;
use tempfile::TempDir;

use crate::fake_extension::{self, EXT, FakeExtension};
use crate::support::*;

/// A server over a project with entities of several kinds, for the
/// dynamic kind-based tools and resources: two planned features and one
/// behavior, `@test/ext` declaring their kinds and `feature`'s `status`.
fn init_server_with_kinds() -> Served {
    TestProject::new()
        .file(
            "test.spec",
            concat!(
                "feature feat_auth \"Authentication\" {\n    status planned\n}\n",
                "feature feat_search \"Search\" {\n    status planned\n}\n",
                "behavior login_behavior \"Login\" {\n    contract \"MUST login\"\n}\n",
            ),
        )
        .serve(&[TestExtension::software().string_field("feature", "status")])
}

/// A server over a project whose one extension declares an MCP tool
/// (`test.list_items`) and an MCP resource (`specforge://test/items`) and
/// no command, served through the runtime seam.
fn init_server_with_surfaces() -> (Logged, std::sync::Arc<FakeExtension>, TempDir) {
    fake_extension::initialized(FakeExtension::declaring(json!({
        "mcp_tools": [{
            "name": "test.list_items",
            "description": "List all items via MCP",
            "export": "mcp__list_items",
            "input_schema": {"type": "object", "properties": {"kind": {"type": "string"}}}
        }],
        "mcp_resources": [{
            "uri_template": "specforge://test/items",
            "name": "test-items",
            "description": "All items resource",
            "export": "mcp__test_items",
            "mime_type": "application/json"
        }]
    })))
}

#[test]
fn extension_mcp_tools_in_registry() {
    let (mut server, _ext, _dir) = init_server_with_surfaces();
    let resp = call(&mut server, "tools/list", json!({}));
    let tools = resp["result"]["tools"].as_array().unwrap();
    let tool_names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();

    // Core tools must still be present
    assert!(
        tool_names.contains(&"specforge.query"),
        "core tool specforge.query must be present"
    );

    // Extension tool must also be present
    assert!(
        tool_names.contains(&"test.list_items"),
        "extension MCP tool 'test.list_items' must appear in registry. Got: {:?}",
        tool_names
    );
}

#[test]
fn extension_mcp_resources_in_registry() {
    let (mut server, _ext, _dir) = init_server_with_surfaces();
    let resp = call(&mut server, "resources/list", json!({}));
    let resources = resp["result"]["resources"].as_array().unwrap();
    let uris: Vec<&str> = resources
        .iter()
        .map(|r| r["uri"].as_str().unwrap())
        .collect();

    // Core resources must still be present
    assert!(
        uris.contains(&"specforge://graph"),
        "core resource must be present"
    );

    // Extension resource must also be present
    assert!(
        uris.contains(&"specforge://test/items"),
        "extension MCP resource 'specforge://test/items' must appear in registry. Got: {:?}",
        uris
    );
}

#[test]
fn capabilities_include_extension_counts() {
    let tool = |name: &str| {
        json!({"name": name, "description": name, "export": format!("mcp__{name}"),
            "input_schema": {"type": "object", "properties": {}}})
    };
    let (mut server, _ext, _dir) = fake_extension::initialized(
        FakeExtension::declaring(json!({}))
            .with_tool(tool("ext.tool_a"))
            .with_tool(tool("ext.tool_b")),
    );

    let resp = call(&mut server, "tools/list", json!({}));
    let tools = resp["result"]["tools"].as_array().unwrap();
    // The core tools, then the 2 extension tools.
    let ext_tools: Vec<_> = tools
        .iter()
        .filter(|t| t["name"].as_str().unwrap().starts_with("ext."))
        .collect();
    assert_eq!(ext_tools.len(), 2, "{ext_tools:?}");
    let initialized = events(&server, "mcp_initialized");
    let [initialized] = initialized.as_slice() else {
        panic!("{initialized:?}")
    };
    assert_eq!(initialized["surface_tools_registered"], 2);
    assert_eq!(initialized["auto_promoted_tools"], 0);
    assert_eq!(
        initialized["tools_registered"],
        specforge_mcp::tools::CORE_TOOLS.len() + 2
    );
}

// --- Dynamic kind-based tools and resources ---

// B:provide_mcp_entities_by_kind — verify unit "specforge.list returns entities filtered by kind"
#[specforge_test(
    behavior = "provide_mcp_entities_by_kind",
    verify = "specforge.list returns entities filtered by kind"
)]
fn list_tool_returns_entities_by_kind() {
    let mut server = init_server_with_kinds();
    let resp = call_tool(&mut server, "specforge.list", json!({"kind": "feature"}));
    assert!(
        resp["error"].is_null(),
        "specforge.list tool must not return error: {:?}",
        resp
    );
    let text = tool_text(&resp);
    let parsed: Value = serde_json::from_str::<Value>(&text).unwrap()["entities"].clone();
    let entities = parsed.as_array().unwrap();

    assert_eq!(
        entities.len(),
        2,
        "expected 2 features, got: {:?}",
        entities
    );
    let ids: Vec<&str> = entities.iter().map(|e| e["id"].as_str().unwrap()).collect();
    assert!(ids.contains(&"feat_auth"));
    assert!(ids.contains(&"feat_search"));
}

/// The ids `specforge.list` returns for `args`.
fn listed_ids(server: &mut McpServer, args: Value) -> Vec<String> {
    let resp = call_tool(server, "specforge.list", args);
    let parsed: Value =
        serde_json::from_str::<Value>(&tool_text(&resp)).unwrap()["entities"].clone();
    parsed
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["id"].as_str().unwrap().to_string())
        .collect()
}

#[specforge_test(
    behavior = "provide_mcp_entities_by_kind",
    verify = "specforge.list keeps the entities whose fields hold the where values"
)]
fn list_tool_filters_by_field_values() {
    let mut server = init_server_with_kinds();
    let planned = json!({"kind": "feature", "where": {"status": "planned"}});
    assert_eq!(
        listed_ids(&mut server, planned),
        ["feat_auth", "feat_search"]
    );
    let by_contract = json!({"where": {"contract": "MUST login"}});
    assert_eq!(listed_ids(&mut server, by_contract), ["login_behavior"]);
    let done = json!({"kind": "feature", "where": {"status": "done"}});
    assert!(listed_ids(&mut server, done).is_empty());
}

#[specforge_test(
    behavior = "provide_mcp_entities_by_kind",
    verify = "specforge.list pages the entities sorted by id with offset and limit"
)]
fn list_tool_pages_sorted_entities() {
    let mut server = init_server_with_kinds();
    assert_eq!(
        listed_ids(&mut server, json!({})),
        ["feat_auth", "feat_search", "login_behavior"]
    );
    let page = json!({"offset": 1, "limit": 1});
    assert_eq!(listed_ids(&mut server, page), ["feat_search"]);
}

#[test]
fn list_tool_refuses_a_malformed_filter_or_page() {
    let mut server = init_server_with_kinds();
    for args in [
        json!({"where": "status=done"}),
        json!({"limit": -1}),
        json!({"offset": "one"}),
        json!({"limit": 1.5}),
    ] {
        let resp = call_tool(&mut server, "specforge.list", args.clone());
        assert_eq!(resp["result"]["isError"], true, "{args}: {resp}");
        let error = crate::tool_errors::mcp_error(&resp);
        assert_eq!(error["code"], "invalid_input", "{args}: {error}");
    }
}

// B:provide_mcp_entities_by_kind — verify unit "specforge.list returns empty array for unknown kind"
#[specforge_test(
    behavior = "provide_mcp_entities_by_kind",
    verify = "specforge.list returns empty for unknown kind"
)]
fn list_tool_empty_for_unknown_kind() {
    let mut server = init_server_with_kinds();
    let resp = call_tool(
        &mut server,
        "specforge.list",
        json!({"kind": "nonexistent"}),
    );
    let text = tool_text(&resp);
    let parsed: Value = serde_json::from_str::<Value>(&text).unwrap()["entities"].clone();
    assert_eq!(parsed.as_array().unwrap().len(), 0);
}

// B:provide_mcp_entities_by_kind — verify unit "specforge://entities/{kind} resource returns entities as JSON"
#[specforge_test(
    behavior = "provide_mcp_entities_by_kind",
    verify = "entity-by-kind resource returns entities"
)]
fn entities_by_kind_resource() {
    let mut server = init_server_with_kinds();
    let resp = read_resource(&mut server, "specforge://entities/behavior");
    let contents = &resp["result"]["contents"];
    assert!(
        contents.is_array(),
        "resource must return contents array, got: {:?}",
        resp
    );
    let text = contents[0]["text"].as_str().unwrap();
    let parsed: Value = serde_json::from_str(text).unwrap();
    let entities = parsed.as_array().unwrap();
    assert_eq!(entities.len(), 1, "expected 1 behavior");
    assert_eq!(entities[0]["id"].as_str().unwrap(), "login_behavior");
}

// B:provide_mcp_entities_by_kind — verify unit "specforge.list tool is registered"
#[specforge_test(
    behavior = "provide_mcp_entities_by_kind",
    verify = "specforge.list tool appears in tool list"
)]
fn list_tool_registered() {
    let mut server = McpServer::new();
    call(&mut server, "initialize", json!({}));
    let resp = call(&mut server, "tools/list", json!({}));
    let tools = resp["result"]["tools"].as_array().unwrap();
    let names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
    assert!(
        names.contains(&"specforge.list"),
        "specforge.list must be in tool list"
    );
}

// --- Extension tool dispatch ---

#[test]
fn extension_tool_dispatches() {
    let (mut server, _ext, _dir) = init_server_with_surfaces();
    let resp = call_tool(&mut server, "test.list_items", json!({"kind": "feature"}));
    // Should NOT be METHOD_NOT_FOUND — extension tools should be recognized
    let error_code = resp["error"]["code"].as_i64();
    assert_ne!(
        error_code,
        Some(-32602),
        "extension tool must not return METHOD_NOT_FOUND, got: {:?}",
        resp
    );
}

// B:handle_mcp_protocol_error — verify unit "unknown tool still returns METHOD_NOT_FOUND"
#[specforge_test(
    behavior = "handle_mcp_protocol_error",
    verify = "truly unknown tool returns -32602 Invalid params (MCP spec example)"
)]
fn unknown_tool_returns_invalid_params() {
    let (mut server, _ext, _dir) = init_server_with_surfaces();
    let resp = call_tool(&mut server, "totally.unknown.tool", json!({}));
    assert_eq!(resp["error"]["code"].as_i64(), Some(-32602));
}

// B:list_mcp_tools — verify unit "re-compilation preserves core tools"
#[specforge_test(
    behavior = "list_mcp_tools",
    verify = "returns core-provided descriptors when no extensions installed"
)]
fn recompilation_refreshes_surfaces() {
    let dir = TempDir::new().unwrap();
    fs::write(
        dir.path().join("core.spec"),
        r#"behavior greet "Greet" {
    status planned
}
"#,
    )
    .unwrap();
    fs::write(
        dir.path().join("specforge.json"),
        r#"{
    "name": "test-recompile",
    "version": "0.1.0",
    "extensions": []
}
"#,
    )
    .unwrap();

    let mut server = McpServer::new();
    call(
        &mut server,
        "initialize",
        json!({"projectRoot": dir.path().to_str().unwrap()}),
    );

    // Exactly the core tools: nothing missing, no extension tool.
    let core_tools = [
        "specforge.add_extension",
        "specforge.analyze",
        "specforge.collect",
        "specforge.coverage",
        "specforge.doctor",
        "specforge.explain",
        "specforge.export",
        "specforge.extensions",
        "specforge.find_definition",
        "specforge.find_implementation",
        "specforge.find_references",
        "specforge.find_spec_for_source",
        "specforge.format",
        "specforge.infer_gaps",
        "specforge.infer_progress",
        "specforge.infer_session",
        "specforge.init",
        "specforge.inspect",
        "specforge.list",
        "specforge.migrate",
        "specforge.model",
        "specforge.outline",
        "specforge.outline_extensions",
        "specforge.providers",
        "specforge.query",
        "specforge.remove_extension",
        "specforge.rename",
        "specforge.render",
        "specforge.schema",
        "specforge.search",
        "specforge.stats",
        "specforge.suggest_fixes",
        "specforge.trace",
        "specforge.validate",
    ];
    let listed = |resp: &Value| -> Vec<String> {
        let tools = resp["result"]["tools"].as_array().unwrap();
        for tool in tools {
            assert_eq!(
                tool["source"], "core",
                "no extension is installed, yet {tool} is listed"
            );
        }
        let mut names: Vec<String> = tools
            .iter()
            .map(|t| t["name"].as_str().unwrap().to_string())
            .collect();
        names.sort();
        names
    };
    let resp1 = call(&mut server, "tools/list", json!({}));
    assert_eq!(listed(&resp1), core_tools);

    // Recompile by calling validate
    let validate_resp = call_tool(
        &mut server,
        "specforge.validate",
        json!({
            "path": dir.path().to_str().unwrap()
        }),
    );
    assert_eq!(validate_resp["result"]["isError"], false, "{validate_resp}");

    // The same core set after recompilation.
    let resp2 = call(&mut server, "tools/list", json!({}));
    assert_eq!(listed(&resp2), core_tools);
}

// B:provide_mcp_entities_by_kind — verify unit "entities resource registered in resource list"
#[specforge_test(
    behavior = "provide_mcp_entities_by_kind",
    verify = "entities resource template in resource template list"
)]
fn entities_resource_registered() {
    let mut server = McpServer::new();
    call(&mut server, "initialize", json!({}));
    let resp = call(&mut server, "resources/templates/list", json!({}));
    let templates = resp["result"]["resourceTemplates"].as_array().unwrap();
    let uris: Vec<&str> = templates
        .iter()
        .map(|t| t["uriTemplate"].as_str().unwrap())
        .collect();
    assert!(
        uris.contains(&"specforge://entities/{kind}"),
        "specforge://entities/{{kind}} must be in the resource template list. Got: {:?}",
        uris
    );
}

// --- Auto-promoted CLI commands (`@test/cmds`, see fake_extension.rs) ---

/// The listed descriptor called `name`, or `None`.
fn listed_tool(server: &mut McpServer, name: &str) -> Option<Value> {
    let resp = call(server, "tools/list", json!({}));
    resp["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["name"] == name)
        .cloned()
}

#[specforge_test(
    behavior = "auto_promote_commands_to_mcp_tools",
    verify = "CLI command auto-promoted to MCP tool"
)]
fn cli_command_auto_promoted_to_mcp_tool() {
    let output = json!({"exit_code": 0, "stdout": "3 of 4 covered", "stderr": ""});
    let (mut server, ext, _dir) =
        fake_extension::initialized(FakeExtension::new().with_output("cmd__report", output));

    let listed = listed_tool(&mut server, "specforge.cmds.report").expect("report promoted");
    assert_eq!(listed["description"], "Write a coverage report");
    // Its role is core; where it comes from is its source.
    assert_eq!(listed["category"], "core");
    assert_eq!(listed["source"], "@test/cmds");

    // A call reaches the command's cmd__ export with the tool arguments as
    // its args, beside the project root and the served graph, and the
    // command's stdout is the tool result.
    let args = json!({"style": "md", "verbose": true});
    let resp = call_tool(&mut server, "specforge.cmds.report", args.clone());
    assert_eq!(
        resp["result"],
        json!({"content": [{"type": "text", "text": "3 of 4 covered"}], "isError": false})
    );
    let calls = ext.calls();
    let [(extension, export, input)] = calls.as_slice() else {
        panic!("one call: {calls:?}")
    };
    assert_eq!((extension.as_str(), export.as_str()), (EXT, "cmd__report"));
    assert_eq!(input["args"], args);
    assert!(input["cwd"].is_string(), "{input}");
    assert!(input["graph"]["nodes"].is_array(), "{input}");

    // A failing command is a failed tool result carrying its stderr.
    let failing = json!({"exit_code": 2, "stdout": "", "stderr": "no tests found"});
    let (mut server, _ext, _dir) =
        fake_extension::initialized(FakeExtension::new().with_output("cmd__report", failing));
    let resp = call_tool(&mut server, "specforge.cmds.report", json!({"style": "md"}));
    assert_eq!(
        resp["result"],
        json!({"content": [
            {"type": "text", "text": ""},
            {"type": "text", "text": "no tests found"}
        ], "isError": true})
    );
}

/// The `tools/call` result of `specforge.cmds.report` when its export
/// returns `output`, and the input the export got.
fn promoted_report(output: Value) -> (Value, Value) {
    let (mut server, ext, _dir) =
        fake_extension::initialized(FakeExtension::new().with_output("cmd__report", output));
    let resp = call_tool(&mut server, "specforge.cmds.report", json!({"style": "md"}));
    let calls = ext.calls();
    let [(_, _, input)] = calls.as_slice() else {
        panic!("one call: {calls:?}")
    };
    (resp["result"].clone(), input.clone())
}

#[specforge_test(
    behavior = "dispatch_surface_command",
    verify = "over MCP a failure's JSON error object is an isError result carrying it, and output that is not one object is text"
)]
fn over_mcp_a_commands_output_is_structured_only_when_it_is_one_object() {
    let ran = |exit_code: i32, stdout: &str, stderr: &str| {
        promoted_report(json!({"exit_code": exit_code, "stdout": stdout, "stderr": stderr})).0
    };
    let text = |blocks: &[&str]| -> Value {
        blocks
            .iter()
            .map(|t| json!({"type": "text", "text": t}))
            .collect()
    };

    // The command is asked for json, with the host's UTC date.
    let (_, input) = promoted_report(json!({"exit_code": 0, "stdout": "{}", "stderr": ""}));
    assert_eq!(input["format"], "json", "{input}");
    let today = input["today"].as_str().unwrap();
    assert!(
        chrono::NaiveDate::parse_from_str(today, "%Y-%m-%d").is_ok(),
        "{input}"
    );
    // The args the command line sends: an unset flag is false.
    assert_eq!(input["args"], json!({"style": "md", "verbose": false}));

    // One object on stdout is the structured result, beside its text.
    let result = ran(0, r#"{"covered": 3}"#, "");
    assert_eq!(result["isError"], false, "{result}");
    assert_eq!(result["structuredContent"], json!({"covered": 3}));

    // JSON that is not an object, or an object beside a warning, is text.
    for stdout in ["[1, 2]", "3", "\"done\""] {
        let result = ran(0, stdout, "");
        assert_eq!(
            result,
            json!({"content": text(&[stdout]), "isError": false}),
            "{stdout}"
        );
    }
    let result = ran(0, "{}", "warning: stale");
    assert_eq!(
        result,
        json!({"content": text(&["{}", "warning: stale"]), "isError": false})
    );

    // A failure's one error object on stderr is the isError result.
    let error = json!({"code": "ENTITY_NOT_FOUND", "message": "milestone 'm2' not found"});
    let result = ran(1, "", &error.to_string());
    assert_eq!(result["isError"], true, "{result}");
    assert_eq!(result["structuredContent"], error);
    assert_eq!(result["content"], text(&[&error.to_string()]));

    // A failure that wrote prose, or an array, is a failed text result.
    for stderr in ["error: no tests found", "[\"a\"]"] {
        let result = ran(2, "", stderr);
        assert_eq!(
            result,
            json!({"content": text(&["", stderr]), "isError": true}),
            "{stderr}"
        );
    }
    // A failure's object beside stdout is not the error object alone.
    let result = ran(1, "partial", &error.to_string());
    assert_eq!(result["isError"], true, "{result}");
    assert!(result.get("structuredContent").is_none(), "{result}");
}

#[specforge_test(
    behavior = "auto_promote_commands_to_mcp_tools",
    verify = "auto-promoted tool name follows specforge.{ext}.{cmd} pattern"
)]
fn auto_promoted_tool_name_follows_pattern() {
    let (server, _ext, _dir) = fake_extension::initialized(FakeExtension::new());
    // `@test/cmds` has the short name `cmds`; each promoted tool is
    // specforge.cmds.<command id>, dispatched to the command's export.
    let promoted: Vec<(String, String, String)> = server
        .state()
        .surfaces()
        .tools()
        .iter()
        .filter_map(|tool| match &tool.kind {
            specforge_mcp::surface_table::ToolKind::Command(command) => Some((
                tool.name.clone(),
                tool.extension.clone(),
                command.export().to_string(),
            )),
            specforge_mcp::surface_table::ToolKind::McpTool { .. } => None,
        })
        .collect();
    assert_eq!(
        promoted,
        [(
            "specforge.cmds.report".to_string(),
            EXT.to_string(),
            "cmd__report".to_string()
        )]
    );
}

#[specforge_test(
    behavior = "auto_promote_commands_to_mcp_tools",
    verify = "a command the CLI refuses, such as one declaring an arg named format, is not promoted"
)]
fn a_command_the_cli_refuses_is_no_tool() {
    let command = |id: &str, arg: &str| {
        json!({"id": id, "title": id, "description": id, "export": format!("cmd__{id}"),
            "args": [{"name": arg, "arg_type": "string"}]})
    };
    let (mut server, _ext, _dir) = fake_extension::initialized(
        FakeExtension::new()
            .with_command(command("render", "format"))
            .with_command(command("open", "path"))
            .with_command(command("draw", "shape")),
    );
    // The rule the CLI refuses a command line by is the one MCP promotes by,
    // and MCP says why.
    let refused = |arg: &str| {
        let declaration: specforge_protocol_types::CommandDescriptor =
            serde_json::from_value(command("x", arg)).unwrap();
        specforge_ops::command::ExtensionCommand::new(EXT, "cmds", &declaration)
            .refusal()
            .map(ToString::to_string)
    };
    assert!(refused("format").is_some());
    assert!(refused("shape").is_none());
    assert!(listed_tool(&mut server, "specforge.cmds.render").is_none());
    assert!(listed_tool(&mut server, "specforge.cmds.open").is_none());
    assert!(listed_tool(&mut server, "specforge.cmds.draw").is_some());
    let i017: Vec<String> = server
        .state()
        .diagnostics()
        .iter()
        .filter(|d| d.code == "I017" && !d.message.starts_with("command 'check'"))
        .map(|d| d.message.clone())
        .collect();
    assert_eq!(
        i017,
        [
            "command 'render' not auto-promoted: the host refuses it: its arg 'format' takes the host's --format",
            "command 'open' not auto-promoted: the host refuses it: its arg 'path' takes the host's --path",
        ]
    );
}

#[specforge_test(
    behavior = "auto_promote_commands_to_mcp_tools",
    verify = "derived input_schema computed from command args"
)]
fn a_commands_input_schema_is_derived_from_its_args() {
    let (mut server, _ext, _dir) = fake_extension::initialized(FakeExtension::new());
    let listed = listed_tool(&mut server, "specforge.cmds.report").unwrap();
    assert_eq!(
        listed["inputSchema"],
        json!({
            "type": "object",
            "properties": {
                "style": {"type": "string", "enum": ["md", "json"], "description": "Output style"},
                "verbose": {"type": "boolean", "default": false},
                "limit": {"type": "integer"},
                "out": {"type": "string"}
            },
            "required": ["style"],
            "additionalProperties": false
        })
    );
}

#[specforge_test(
    behavior = "auto_promote_commands_to_mcp_tools",
    verify = "explicit MCP tool wins over auto-promoted tool with I017"
)]
fn explicit_mcp_tool_wins_over_auto_promoted() {
    let (mut server, ext, _dir) = fake_extension::initialized(
        FakeExtension::new().with_output("mcp__check", json!({"checked": true})),
    );
    // One descriptor named specforge.cmds.check: the explicit one.
    let resp = call(&mut server, "tools/list", json!({}));
    let named: Vec<&Value> = resp["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|t| t["name"] == "specforge.cmds.check")
        .collect();
    assert_eq!(
        named,
        [&json!({
            "name": "specforge.cmds.check",
            "description": "Explicit check tool",
            "inputSchema": {"type": "object", "properties": {"strict": {"type": "boolean"}}},
            "outputSchema": {"type": "object", "properties": {"checked": {"type": "boolean"}}},
            "category": "core",
            "source": "@test/cmds",
            "annotations": {"readOnlyHint": true, "openWorldHint": false}
        })]
    );
    let i017: Vec<_> = server
        .state()
        .diagnostics()
        .iter()
        .filter(|d| d.code == "I017")
        .map(|d| (d.severity, d.message.clone()))
        .collect();
    assert_eq!(
        i017,
        [(
            specforge_common::Severity::Info,
            "command 'check' not auto-promoted: explicit MCP tool 'specforge.cmds.check' already exists"
                .to_string()
        )]
    );

    // A call reaches the explicit tool's mcp__ export, not cmd__check.
    let resp = call_tool(&mut server, "specforge.cmds.check", json!({"strict": true}));
    assert_eq!(resp["result"]["isError"], false, "{resp}");
    assert_eq!(
        ext.calls(),
        [(
            EXT.to_string(),
            "mcp__check".to_string(),
            json!({"strict": true})
        )]
    );
}

#[specforge_test(
    behavior = "notify_diagnostics_delta_via_mcp",
    verify = "the I017 the extension surfaces add and remove is notified with the update's delta"
)]
fn the_i017_the_surfaces_drop_is_notified() {
    let (mut server, _ext, dir) = fake_extension::initialized(
        FakeExtension::new().with_output("mcp__check", json!({"checked": true})),
    );
    let resp = call(
        &mut server,
        "resources/subscribe",
        json!({"uri": "specforge://diagnostics"}),
    );
    assert_eq!(resp["result"], json!({}), "{resp}");
    server.take_notifications();

    // Without the extension the explicit tool no longer shadows the command:
    // the surface table drops its I017 and the project reports I002.
    fs::write(
        dir.path().join("specforge.json"),
        r#"{"name":"cmds","version":"0.1.0","extensions":[]}"#,
    )
    .unwrap();
    let resp = call_tool(&mut server, "specforge.stats", json!({}));
    assert!(resp["error"].is_null(), "{resp}");

    let notifications = server.take_notifications();
    let changed: Vec<&Value> = notifications
        .iter()
        .filter(|n| n["method"] == "specforge/diagnosticsChanged")
        .collect();
    assert_eq!(changed.len(), 1, "{notifications:?}");
    let codes = |side: &str| -> Vec<(String, String)> {
        changed[0]["params"][side]
            .as_array()
            .unwrap()
            .iter()
            .map(|d| {
                (
                    d["code"].as_str().unwrap().to_string(),
                    d["message"].as_str().unwrap().to_string(),
                )
            })
            .collect()
    };
    let removed = codes("removed");
    assert!(
        removed
            .iter()
            .any(|(code, message)| code == "I017" && message.starts_with("command 'check'")),
        "{removed:?}"
    );
    assert!(codes("added").iter().any(|(code, _)| code == "I002"));
}

#[specforge_test(
    behavior = "commands_auto_promoted",
    verify = "emits commands_auto_promoted with correct promoted and conflict counts"
)]
fn event_commands_auto_promoted() {
    let (server, _ext, _dir) = fake_extension::initialized(FakeExtension::new());
    let promoted: Vec<Value> = server
        .log()
        .recorded()
        .iter()
        .filter(|recorded| recorded.name() == "commands_auto_promoted")
        .map(|recorded| {
            let mut params = recorded.payload();
            assert!(params["timestamp"].is_string(), "{params}");
            params.as_object_mut().unwrap().remove("timestamp");
            params
        })
        .collect();
    assert_eq!(promoted, [json!({"promotedCount": 1, "conflictCount": 1})]);

    // A project whose extensions contribute no command promotes nothing
    // and says nothing.
    let (server, _ext, _dir) = init_server_with_surfaces();
    assert!(events(&server, "commands_auto_promoted").is_empty());
}

#[specforge_test(
    behavior = "dispatch_surface_mcp_tool",
    verify = "the served project's runtime is the one its compile loaded and serves later calls until the project reloads"
)]
fn one_runtime_serves_extension_calls_until_the_next_compile() {
    let dir = TempDir::new().unwrap();
    let config = json!({"name": "rt", "version": "0.1.0",
        "extensions": ["@specforge/software", "@specforge/testing"]});
    fs::write(dir.path().join("specforge.json"), config.to_string()).unwrap();
    fs::write(
        dir.path().join("main.spec"),
        "behavior greet \"Greet\" {\n  category command\n  contract \"MUST greet\"\n}\n",
    )
    .unwrap();
    let mut server = McpServer::new();
    call(
        &mut server,
        "initialize",
        json!({"projectRoot": dir.path().to_str().unwrap()}),
    );
    let runtime = |server: &McpServer| {
        std::sync::Arc::clone(
            server
                .state()
                .session()
                .runtime()
                .expect("the served session runs its extensions"),
        )
    };
    let compiled = runtime(&server);

    // analyze runs the extensions' passes in it, call after call, and a
    // validate of the unchanged project keeps it.
    let analyze = json!({});
    call_tool(&mut server, "specforge.analyze", analyze.clone());
    call_tool(&mut server, "specforge.analyze", analyze);
    call_tool(&mut server, "specforge.validate", json!({}));
    assert!(
        std::sync::Arc::ptr_eq(&compiled, &runtime(&server)),
        "later calls reuse it while the project's environment is unchanged"
    );

    // The environment changes on disk: the reload may load other modules,
    // so later calls run in its runtime.
    let config = json!({"name": "rt", "version": "0.1.0",
        "extensions": ["@specforge/software"]});
    fs::write(dir.path().join("specforge.json"), config.to_string()).unwrap();
    call_tool(&mut server, "specforge.validate", json!({}));
    assert!(!std::sync::Arc::ptr_eq(&compiled, &runtime(&server)));
}

#[specforge_test(
    behavior = "list_mcp_tools",
    verify = "an extension tool's declared output_schema is listed as its outputSchema"
)]
fn an_extension_tool_lists_its_declared_output_schema() {
    let (mut server, _ext, _dir) = fake_extension::initialized(FakeExtension::new());
    let resp = call(&mut server, "tools/list", json!({}));
    let tools = resp["result"]["tools"].as_array().unwrap();
    let check = tools
        .iter()
        .find(|t| t["name"] == "specforge.cmds.check")
        .unwrap();
    assert_eq!(
        check["outputSchema"],
        json!({"type": "object", "properties": {"checked": {"type": "boolean"}}})
    );
    // An auto-promoted command declares none.
    let report = tools
        .iter()
        .find(|t| t["name"] == "specforge.cmds.report")
        .unwrap();
    assert!(report.get("outputSchema").is_none(), "{report}");
}

#[specforge_test(
    behavior = "dispatch_surface_mcp_tool",
    verify = "input validated against declared input_schema"
)]
fn extension_tool_input_is_checked_against_its_schema() {
    let (mut server, ext, _dir) = fake_extension::initialized(
        FakeExtension::new()
            .with_output("mcp__check", json!({"checked": true}))
            .with_output("cmd__report", json!("")),
    );
    // strict is a boolean; a string never reaches the module.
    let resp = call_tool(
        &mut server,
        "specforge.cmds.check",
        json!({"strict": "yes"}),
    );
    let error = crate::tool_errors::mcp_error(&resp);
    assert_eq!(error["code"], "invalid_input", "{error}");
    assert!(
        error["message"].as_str().unwrap().contains("$.strict"),
        "{error}"
    );
    assert!(ext.calls().is_empty(), "no export ran: {:?}", ext.calls());

    // Valid input reaches the export.
    let resp = call_tool(&mut server, "specforge.cmds.check", json!({"strict": true}));
    assert_eq!(resp["result"]["isError"], false, "{resp}");
    assert_eq!(ext.calls().len(), 1);
}

#[specforge_test(
    behavior = "dispatch_surface_mcp_tool",
    verify = "output that does not match the declared output_schema is a schema_mismatch error"
)]
fn extension_tool_output_is_checked_against_its_schema() {
    // check declares {"checked": boolean}; this module returns a string.
    let (mut server, ext, _dir) = fake_extension::initialized(
        FakeExtension::new().with_output("mcp__check", json!({"checked": "yes"})),
    );
    let resp = call_tool(&mut server, "specforge.cmds.check", json!({"strict": true}));
    assert_eq!(ext.calls().len(), 1, "the module ran");
    assert_eq!(resp["result"]["isError"], true, "{resp}");
    assert!(resp["result"].get("structuredContent").is_none(), "{resp}");
    let error = crate::tool_errors::mcp_error(&resp);
    assert_eq!(error["code"], "schema_mismatch", "{error}");
    assert_eq!(
        error["message"],
        "tool 'specforge.cmds.check' of '@test/cmds' answered a reply its output schema refuses: $.checked: expected boolean, got string"
    );
    assert_eq!(
        error["data"]["violations"],
        json!(["$.checked: expected boolean, got string"]),
        "{error}"
    );

    // An output that matches is the result.
    let (mut server, _ext, _dir) = fake_extension::initialized(
        FakeExtension::new().with_output("mcp__check", json!({"checked": true})),
    );
    let resp = call_tool(&mut server, "specforge.cmds.check", json!({"strict": true}));
    assert_eq!(resp["result"]["isError"], false, "{resp}");
    assert_eq!(
        resp["result"]["structuredContent"],
        json!({"checked": true})
    );
}

#[specforge_test(
    behavior = "dispatch_surface_mcp_tool",
    verify = "an output with a key its declared output_schema does not allow is a schema_mismatch error"
)]
fn extension_tool_output_with_an_undeclared_key_is_refused() {
    let closed = FakeExtension::declaring(json!({
        "mcp_tools": [{
            "name": "test.closed",
            "description": "A tool whose output schema is closed",
            "export": "mcp__closed",
            "input_schema": {"type": "object"},
            "output_schema": {
                "type": "object",
                "properties": {"checked": {"type": "boolean"}},
                "additionalProperties": false
            }
        }]
    }));
    let (mut server, _ext, _dir) = fake_extension::initialized(
        closed.with_output("mcp__closed", json!({"checked": true, "extra": 1})),
    );
    let resp = call_tool(&mut server, "test.closed", json!({}));
    assert_eq!(resp["result"]["isError"], true, "{resp}");
    let error = crate::tool_errors::mcp_error(&resp);
    assert_eq!(error["code"], "schema_mismatch", "{error}");
    assert_eq!(
        error["message"],
        "tool 'test.closed' of '@test/cmds' answered a reply its output schema refuses: $.extra: undeclared key"
    );
    assert_eq!(
        error["data"]["violations"],
        json!(["$.extra: undeclared key"]),
        "{error}"
    );
}

#[specforge_test(
    behavior = "dispatch_surface_mcp_tool",
    verify = "an output that breaks a definition its declared output_schema refers to by $ref is a schema_mismatch error"
)]
fn extension_tool_output_breaking_a_referenced_definition_is_refused() {
    let referencing = FakeExtension::declaring(json!({
        "mcp_tools": [{
            "name": "test.referencing",
            "description": "A tool whose output schema refers to a definition",
            "export": "mcp__referencing",
            "input_schema": {"type": "object"},
            "output_schema": {
                "type": "object",
                "properties": {"span": {"$ref": "#/$defs/Span"}},
                "additionalProperties": false,
                "$defs": {"Span": {
                    "type": "object",
                    "properties": {"line": {"type": "integer"}},
                    "required": ["line"],
                    "additionalProperties": false
                }}
            }
        }]
    }));
    let (mut server, _ext, _dir) = fake_extension::initialized(
        referencing.with_output("mcp__referencing", json!({"span": {"line": "x"}})),
    );
    let resp = call_tool(&mut server, "test.referencing", json!({}));
    assert_eq!(resp["result"]["isError"], true, "{resp}");
    let error = crate::tool_errors::mcp_error(&resp);
    assert_eq!(error["code"], "schema_mismatch", "{error}");
    assert_eq!(
        error["data"]["violations"],
        json!(["$.span.line: expected integer, got string"]),
        "{error}"
    );
}

#[test]
fn the_schema_check_finds_type_enum_and_required_violations() {
    let schema = json!({
        "type": "object",
        "properties": {
            "format": {"type": "string", "enum": ["md", "json"]},
            "paths": {"type": "array", "items": {"type": "string"}},
        },
        "required": ["format"],
    });
    let check = |value: Value| specforge_common::shape::violations(&schema, &value);
    assert!(check(json!({"format": "md", "paths": ["a"]})).is_empty());
    assert_eq!(check(json!({})), ["$: missing required format"]);
    assert_eq!(check(json!({"format": "xml"})).len(), 1);
    assert_eq!(
        check(json!({"format": "md", "paths": [1]})),
        ["$.paths[0]: expected string, got integer"]
    );
    assert_eq!(check(json!(3)), ["$: expected object, got integer"]);
}

#[specforge_test(
    behavior = "dispatch_surface_mcp_tool",
    verify = "input JSON passed to mcp__ export"
)]
fn an_extension_tool_export_gets_its_arguments_as_json() {
    let (mut server, ext, _dir) = fake_extension::initialized(
        FakeExtension::new().with_output("mcp__check", json!({"checked": true})),
    );
    call_tool(&mut server, "specforge.cmds.check", json!({"strict": true}));
    assert_eq!(
        ext.calls(),
        [(
            EXT.to_string(),
            "mcp__check".to_string(),
            json!({"strict": true})
        )]
    );
}

#[specforge_test(
    behavior = "dispatch_surface_mcp_tool",
    verify = "Wasm trap returned as structured MCP error"
)]
fn a_trapping_extension_tool_is_a_structured_error() {
    // No output for mcp__check: the guest routes no such export.
    let (mut server, ext, _dir) = fake_extension::initialized(FakeExtension::new());
    let resp = call_tool(&mut server, "specforge.cmds.check", json!({}));
    let error = crate::tool_errors::mcp_error(&resp);
    assert_eq!(error["code"], "internal_error", "{error}");
    assert_eq!(error["diagnostic"]["code"], "E028", "{error}");
    assert!(
        error["message"]
            .as_str()
            .unwrap()
            .starts_with("MCP tool mcp__check() of '@test/cmds' trapped: guest_error: unknown export 'mcp__check'"),
        "{error}"
    );
    assert_eq!(error["tool"], "specforge.cmds.check", "{error}");
    assert_eq!(ext.calls().len(), 1, "the export was called");
    // The server keeps serving.
    assert!(call(&mut server, "ping", json!({}))["result"].is_object());
}

#[specforge_test(
    behavior = "dispatch_surface_mcp_tool",
    verify = "tool output returned as MCP tool result"
)]
fn an_extension_tools_output_is_its_result() {
    let (mut server, _ext, _dir) = fake_extension::initialized(
        FakeExtension::new().with_output("mcp__check", json!({"checked": true})),
    );
    let resp = call_tool(&mut server, "specforge.cmds.check", json!({}));
    let result = &resp["result"];
    assert_eq!(result["isError"], false, "{resp}");
    assert_eq!(result["structuredContent"], json!({"checked": true}));
    let text: Value = serde_json::from_str(&tool_text(&resp)).unwrap();
    assert_eq!(text, json!({"checked": true}));
}

const SUMMARY: &str = "specforge://ext/cmds/summary";

fn with_summary() -> FakeExtension {
    FakeExtension::new().with_output(
        "mcp__summary",
        json!({"content": "{\"commands\":2}", "mime_type": "application/json"}),
    )
}

#[specforge_test(
    behavior = "dispatch_surface_mcp_resource",
    verify = "URI matched against registered templates"
)]
fn an_extension_resource_is_found_by_its_uri_template() {
    let (mut server, ext, _dir) = fake_extension::initialized(with_summary());
    let resp = read_resource(&mut server, SUMMARY);
    assert!(resp["error"].is_null(), "{resp}");
    // A URI no template of an extension matches reaches no export: the
    // template has no placeholder, so it names itself only.
    for other in [
        "specforge://ext/other/summary",
        "specforge://ext/cmds/summary/more",
    ] {
        let resp = read_resource(&mut server, other);
        assert_eq!(resp["error"]["code"], -32002, "{resp}");
        assert_eq!(
            resp["error"]["message"],
            format!("Unknown resource URI: {other}")
        );
        assert_eq!(resp["error"]["data"], json!({ "uri": other }), "{resp}");
    }
    assert_eq!(ext.calls().len(), 1, "{:?}", ext.calls());
}

#[specforge_test(
    behavior = "dispatch_surface_mcp_resource",
    verify = "URI passed to mcp__ export"
)]
fn an_extension_resource_export_gets_the_uri() {
    let (mut server, ext, _dir) = fake_extension::initialized(with_summary());
    read_resource(&mut server, SUMMARY);
    assert_eq!(
        ext.calls(),
        [(
            EXT.to_string(),
            "mcp__summary".to_string(),
            json!({"uri": SUMMARY})
        )]
    );
}

#[specforge_test(
    behavior = "dispatch_surface_mcp_resource",
    verify = "Wasm trap returned as structured MCP error"
)]
fn a_trapping_extension_resource_is_a_structured_error() {
    // No output for mcp__summary: the guest routes no such export.
    let (mut server, _ext, _dir) = fake_extension::initialized(FakeExtension::new());
    let resp = read_resource(&mut server, SUMMARY);
    let error = &resp["error"];
    // An internal error: the extension's fault, not the client's params.
    assert_eq!(error["code"], -32603, "{resp}");
    let message = "MCP resource mcp__summary() of '@test/cmds' trapped: guest_error: unknown export 'mcp__summary'";
    assert!(
        error["message"].as_str().unwrap().starts_with(message),
        "{resp}"
    );
    // The diagnostic is in the McpError, never only in the message.
    assert_eq!(error["data"]["code"], "internal_error", "{resp}");
    assert_eq!(error["data"]["diagnostic"]["code"], "E028", "{resp}");
    assert_eq!(error["data"]["message"], error["message"], "{resp}");
    assert!(call(&mut server, "ping", json!({}))["result"].is_object());
}

#[specforge_test(
    behavior = "dispatch_surface_mcp_resource",
    verify = "a resource whose answer is not its content and mime type is a structured MCP error"
)]
fn a_resource_answering_no_content_is_a_structured_error() {
    for answer in [
        json!("oops"),
        json!({"text": "t"}),
        json!({"content": "c"}),
        json!([1, 2]),
    ] {
        let (mut server, _ext, _dir) = fake_extension::initialized(
            FakeExtension::new().with_output("mcp__summary", answer.clone()),
        );
        let resp = read_resource(&mut server, SUMMARY);
        assert!(resp["result"].is_null(), "{answer}: {resp}");
        assert_eq!(resp["error"]["code"], -32603, "{resp}");
        assert!(
            resp["error"]["message"].as_str().unwrap().starts_with(
                "MCP resource mcp__summary() of '@test/cmds' answered output that is not a McpResourceContent: "
            ),
            "{resp}"
        );
        assert_eq!(
            resp["error"]["data"]["diagnostic"]["code"], "E028",
            "{resp}"
        );
    }
}

#[specforge_test(
    behavior = "dispatch_surface_mcp_resource",
    verify = "resource content and mime_type returned to client"
)]
fn an_extension_resources_content_and_mime_type_are_returned() {
    let (mut server, _ext, _dir) = fake_extension::initialized(with_summary());
    let resp = read_resource(&mut server, SUMMARY);
    let content = &resp["result"]["contents"][0];
    assert_eq!(content["uri"], SUMMARY, "{resp}");
    assert_eq!(content["mimeType"], "application/json", "{resp}");
    assert_eq!(content["text"], "{\"commands\":2}", "{resp}");
}

// --- What MCP serves from `@test/cmds`, pinned (plan 06 T1) ---
//
// Characterization: these describe today's listings, the args a command's
// export receives over MCP and the errors a command tool answers. Later
// changes flip them on purpose.

/// `ordered`: an enum arg with a declared default, and a flag (R2).
fn ordered_command() -> Value {
    json!({"id": "ordered", "title": "Ordered", "description": "List in order",
        "export": "cmd__ordered",
        "args": [{"name": "order", "arg_type": {"enum": {"values": ["asc", "desc"]}},
                  "default_value": "desc"},
                 {"name": "all", "arg_type": "bool"}]})
}

/// `strict`: a required flag (R3).
fn strict_command() -> Value {
    json!({"id": "strict", "title": "Strict", "description": "Check strictly",
        "export": "cmd__strict",
        "args": [{"name": "strict", "arg_type": "bool", "required": true}]})
}

/// An explicit tool named as a core tool (R5).
fn core_named_tool() -> Value {
    json!({"name": "specforge.validate", "description": "Validate, the extension's way",
        "export": "mcp__v", "input_schema": {"type": "object"}})
}

/// A resource outside `specforge://ext/` (R6).
fn acme_resource() -> Value {
    json!({"uri_template": "acme://doc/{id}", "name": "doc", "export": "mcp__doc",
        "mime_type": "text/plain"})
}

/// A resource whose template a core resource's names (R6).
fn core_shadowed_resource() -> Value {
    json!({"uri_template": "specforge://graph/ext/{id}", "name": "graph-ext",
        "export": "mcp__graph_ext", "mime_type": "application/json"})
}

/// What the server lists beside the core surface: extension tools (in
/// listed order), resources and templates, and the diagnostics it reports.
fn extension_listings(server: &mut McpServer) -> Value {
    let core_uris: Vec<&str> = specforge_mcp::resources::CORE_RESOURCES
        .iter()
        .map(|r| r.uri)
        .collect();
    let tools: Vec<Value> = call(server, "tools/list", json!({}))["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|t| t["source"] != "core")
        .cloned()
        .collect();
    let resources: Vec<Value> = call(server, "resources/list", json!({}))["result"]["resources"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| !core_uris.contains(&r["uri"].as_str().unwrap()))
        .cloned()
        .collect();
    let templates: Vec<Value> =
        call(server, "resources/templates/list", json!({}))["result"]["resourceTemplates"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|r| !core_uris.contains(&r["uriTemplate"].as_str().unwrap()))
            .cloned()
            .collect();
    let diagnostics: Vec<Value> = server
        .state()
        .diagnostics()
        .iter()
        .map(|d| json!({"code": d.code, "message": d.message}))
        .collect();
    json!({"tools": tools, "resources": resources, "resourceTemplates": templates,
        "diagnostics": diagnostics})
}

#[test]
fn pinned_fake_extension_listings() {
    let fakes: [(&str, FakeExtension); 5] = [
        ("plain", FakeExtension::new()),
        (
            "r2_default",
            FakeExtension::new().with_command(ordered_command()),
        ),
        (
            "r3_required_flag",
            FakeExtension::new().with_command(strict_command()),
        ),
        (
            "r5_core_named_tool",
            FakeExtension::new().with_tool(core_named_tool()),
        ),
        (
            "r6_resources",
            FakeExtension::new()
                .with_resource(acme_resource())
                .with_resource(core_shadowed_resource()),
        ),
    ];
    for (name, fake) in fakes {
        let (mut server, _ext, _dir) = fake_extension::initialized(fake);
        insta::assert_json_snapshot!(
            format!("fake_extension_listings_{name}"),
            extension_listings(&mut server)
        );
    }
}

/// The args the export of `tool` received for each of `calls`, in order:
/// `None` for a call that reached no export.
fn args_reaching(fake: FakeExtension, tool: &str, calls: &[Value]) -> Vec<Option<Value>> {
    let export = format!("cmd__{}", tool.rsplit('.').next().unwrap());
    let fake = fake.with_output(
        &export,
        json!({"exit_code": 0, "stdout": "{}", "stderr": ""}),
    );
    let (mut server, ext, _dir) = fake_extension::initialized(fake);
    calls
        .iter()
        .map(|arguments| {
            let before = ext.calls().len();
            call_tool(&mut server, tool, arguments.clone());
            ext.calls()
                .get(before)
                .map(|(_, _, input)| input["args"].clone())
        })
        .collect()
}

#[specforge_test(
    behavior = "dispatch_surface_command",
    verify = "the CLI and MCP send a command's export the same args for the same input, its declared defaults applied by the host"
)]
fn mcp_sends_a_commands_export_the_args_its_derivation_normalizes() {
    // What `ExtensionCommand::normalize` gives for the same declaration and
    // input: what the command line sends too (extension_command.rs,
    // `the_cli_sends_the_args_the_derivation_normalizes`).
    let normalized = |declaration: Value, given: Value| -> Value {
        let declaration: specforge_protocol_types::CommandDescriptor =
            serde_json::from_value(declaration).unwrap();
        let command = specforge_ops::command::ExtensionCommand::new(EXT, "cmds", &declaration);
        Value::Object(command.normalize(given.as_object().unwrap()).unwrap())
    };
    // R2: the declared default, and an unset flag false.
    let ordered = args_reaching(
        FakeExtension::new().with_command(ordered_command()),
        "specforge.cmds.ordered",
        &[json!({}), json!({"order": "asc", "all": true})],
    );
    assert_eq!(
        ordered,
        [
            Some(json!({"order": "desc", "all": false})),
            Some(json!({"order": "asc", "all": true}))
        ]
    );
    assert_eq!(ordered[0], Some(normalized(ordered_command(), json!({}))));
    // R3: a required flag is a flag, false unless set.
    let strict = args_reaching(
        FakeExtension::new().with_command(strict_command()),
        "specforge.cmds.strict",
        &[json!({}), json!({"strict": true})],
    );
    assert_eq!(
        strict,
        [
            Some(json!({"strict": false})),
            Some(json!({"strict": true}))
        ]
    );
    assert_eq!(strict[0], Some(normalized(strict_command(), json!({}))));
    // Values come typed: an integer or a flag as a string is converted.
    let report = args_reaching(
        FakeExtension::new(),
        "specforge.cmds.report",
        &[json!({"style": "md", "limit": "3", "verbose": "true"})],
    );
    assert_eq!(
        report,
        [Some(json!({"style": "md", "limit": 3, "verbose": true}))]
    );
}

#[specforge_test(
    behavior = "dispatch_surface_command",
    verify = "over MCP an argument the command's declaration refuses is the INVALID_INPUT error object the CLI writes, and the export is not called"
)]
fn over_mcp_a_refused_argument_is_the_commands_invalid_input_object() {
    let (mut server, ext, _dir) = fake_extension::initialized(FakeExtension::new());
    for (arguments, error) in [
        (
            json!({}),
            json!({"code": "INVALID_INPUT", "message": "missing required arg 'style'"}),
        ),
        (
            json!({"style": "xml"}),
            json!({"code": "INVALID_INPUT", "message": "style must be one of md, json, got 'xml'"}),
        ),
        (
            json!({"style": "jsno"}),
            json!({"code": "INVALID_INPUT", "message": "style must be one of md, json, got 'jsno'",
                "suggestion": "json"}),
        ),
        (
            json!({"style": "md", "limit": "x"}),
            json!({"code": "INVALID_INPUT", "message": "limit must be an integer, got 'x'"}),
        ),
        (
            json!({"style": "md", "bogus": 1}),
            json!({"code": "INVALID_INPUT", "message": "unknown argument 'bogus'"}),
        ),
        (
            json!({"style": "md", "verbos": true}),
            json!({"code": "INVALID_INPUT", "message": "unknown argument 'verbos'",
                "suggestion": "verbose"}),
        ),
    ] {
        let resp = call_tool(&mut server, "specforge.cmds.report", arguments.clone());
        let result = &resp["result"];
        assert_eq!(result["isError"], true, "{arguments}: {resp}");
        assert_eq!(result["structuredContent"], error, "{arguments}");
        assert_eq!(
            serde_json::from_str::<Value>(&tool_text(&resp)).unwrap(),
            error,
            "{arguments}"
        );
    }
    assert!(ext.calls().is_empty(), "no export ran: {:?}", ext.calls());
    assert!(events(&server, "surface_command_dispatched").is_empty());
}

#[specforge_test(
    behavior = "auto_promote_commands_to_mcp_tools",
    verify = "an explicit extension tool named as a core tool is not listed, with I017"
)]
fn a_core_named_tool_is_not_served() {
    let (mut server, ext, _dir) =
        fake_extension::initialized(FakeExtension::new().with_tool(core_named_tool()));
    let resp = call(&mut server, "tools/list", json!({}));
    let named: Vec<&Value> = resp["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|t| t["name"] == "specforge.validate")
        .collect();
    assert_eq!(named.len(), 1, "listed once: {named:?}");
    assert_eq!(named[0]["source"], "core");
    let i017: Vec<String> = server
        .state()
        .diagnostics()
        .iter()
        .filter(|d| d.code == "I017")
        .map(|d| d.message.clone())
        .collect();
    assert!(
        i017.contains(
            &"MCP tool 'specforge.validate' of '@test/cmds' not served: a core tool has that name"
                .to_string()
        ),
        "{i017:?}"
    );
    call_tool(&mut server, "specforge.validate", json!({}));
    assert!(ext.calls().is_empty(), "the core tool ran, not mcp__v");
}

#[specforge_test(
    behavior = "list_mcp_tools",
    verify = "every listed extension tool is the one dispatched under its name, listed once"
)]
fn every_listed_extension_tool_is_the_one_dispatched() {
    // The explicit check takes the name its command would have, and an
    // explicit tool is named as a core tool.
    let fake = FakeExtension::new()
        .with_tool(core_named_tool())
        .with_command(ordered_command());
    let fake = ["mcp__check", "mcp__v"]
        .into_iter()
        .fold(fake, |fake, export| {
            fake.with_output(export, json!({"checked": true}))
        })
        .with_output(
            "cmd__report",
            json!({"exit_code": 0, "stdout": "{}", "stderr": ""}),
        )
        .with_output(
            "cmd__ordered",
            json!({"exit_code": 0, "stdout": "{}", "stderr": ""}),
        );
    let (mut server, ext, _dir) = fake_extension::initialized(fake);
    let resp = call(&mut server, "tools/list", json!({}));
    let listed: Vec<String> = resp["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap().to_string())
        .collect();
    let mut unique = listed.clone();
    unique.sort();
    unique.dedup();
    assert_eq!(unique.len(), listed.len(), "each name once: {listed:?}");

    let declared = |name: &str| -> Option<&'static str> {
        match name {
            "specforge.cmds.check" => Some("mcp__check"),
            "specforge.cmds.report" => Some("cmd__report"),
            "specforge.cmds.ordered" => Some("cmd__ordered"),
            _ => None,
        }
    };
    let extension_tools: Vec<&Value> = resp["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|t| t["source"] == EXT)
        .collect();
    assert_eq!(extension_tools.len(), 3, "{extension_tools:?}");
    for tool in extension_tools {
        let name = tool["name"].as_str().unwrap();
        let arguments = if name == "specforge.cmds.report" {
            json!({"style": "md"})
        } else {
            json!({})
        };
        let before = ext.calls().len();
        call_tool(&mut server, name, arguments);
        let calls = ext.calls();
        let reached: Vec<&str> = calls[before..]
            .iter()
            .map(|(_, export, _)| export.as_str())
            .collect();
        assert_eq!(reached, [declared(name).unwrap()], "{name}");
    }
}

#[specforge_test(
    behavior = "dispatch_surface_mcp_resource",
    verify = "an extension resource template outside specforge://ext/ is read through its export"
)]
fn a_resource_outside_specforge_ext_is_read_through_its_export() {
    let (mut server, ext, _dir) = fake_extension::initialized(
        FakeExtension::new()
            .with_resource(acme_resource())
            .with_output(
                "mcp__doc",
                json!({"content": "doc 1", "mime_type": "text/plain"}),
            ),
    );
    let resp = read_resource(&mut server, "acme://doc/1");
    let content = &resp["result"]["contents"][0];
    assert_eq!(content["uri"], "acme://doc/1", "{resp}");
    assert_eq!(content["mimeType"], "text/plain", "{resp}");
    assert_eq!(content["text"], "doc 1", "{resp}");
    assert_eq!(
        ext.calls(),
        [(
            EXT.to_string(),
            "mcp__doc".to_string(),
            json!({"uri": "acme://doc/1"})
        )]
    );
    let dispatched: Vec<Value> = events(&server, "surface_mcp_resource_dispatched")
        .into_iter()
        .map(|e| e["uriTemplate"].clone())
        .collect();
    assert_eq!(dispatched, [json!("acme://doc/{id}")]);
    // Another scheme's URI no template names is still unknown.
    let resp = read_resource(&mut server, "acme://other/1");
    assert_eq!(resp["error"]["code"], -32002, "{resp}");
    assert_eq!(
        resp["error"]["message"],
        "Unknown resource URI: acme://other/1"
    );
    assert_eq!(
        resp["error"]["data"],
        json!({ "uri": "acme://other/1" }),
        "{resp}"
    );
}

#[specforge_test(
    behavior = "dispatch_surface_mcp_resource",
    verify = "a URI names a core or extension resource by one rule: a template's text before its first placeholder and anything after it, a plain URI alone or followed by a query"
)]
fn an_extension_resource_template_names_an_empty_placeholder() {
    let (mut server, ext, _dir) = fake_extension::initialized(
        FakeExtension::new()
            .with_resource(acme_resource())
            .with_output(
                "mcp__doc",
                json!({"content": "doc", "mime_type": "text/plain"}),
            ),
    );
    let resp = read_resource(&mut server, "acme://doc/");
    assert!(resp["error"].is_null(), "{resp}");
    assert_eq!(resp["result"]["contents"][0]["text"], "doc", "{resp}");
    assert_eq!(
        ext.calls(),
        [(
            EXT.to_string(),
            "mcp__doc".to_string(),
            json!({"uri": "acme://doc/"})
        )]
    );
    assert_eq!(
        crate::support::events(&server, "surface_mcp_resource_dispatched").len(),
        1
    );
}

#[specforge_test(
    behavior = "dispatch_surface_mcp_resource",
    verify = "a URI names a core or extension resource by one rule: a template's text before its first placeholder and anything after it, a plain URI alone or followed by a query"
)]
fn an_extension_resource_names_its_uri_with_a_query() {
    let uri = "specforge://ext/cmds/summary?x=1";
    let (mut server, ext, _dir) = fake_extension::initialized(with_summary());
    let resp = read_resource(&mut server, uri);
    assert!(resp["error"].is_null(), "{resp}");
    assert_eq!(
        ext.calls(),
        [(
            EXT.to_string(),
            "mcp__summary".to_string(),
            json!({"uri": uri})
        )]
    );
    // Subscribing to it is honoured too: the same lookup answers it.
    let resp = call(&mut server, "resources/subscribe", json!({"uri": uri}));
    assert!(resp["error"].is_null(), "{resp}");
}

#[specforge_test(
    behavior = "dispatch_surface_mcp_resource",
    verify = "an extension resource template a core resource serves is not listed, with I017"
)]
fn a_resource_template_a_core_resource_serves_is_not_served() {
    let (mut server, ext, _dir) = fake_extension::initialized(
        FakeExtension::new()
            .with_resource(core_shadowed_resource())
            .with_output(
                "mcp__graph_ext",
                json!({"content": "{}", "mime_type": "application/json"}),
            ),
    );
    let templates = call(&mut server, "resources/templates/list", json!({}));
    let listed: Vec<&str> = templates["result"]["resourceTemplates"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["uriTemplate"].as_str().unwrap())
        .collect();
    assert!(
        !listed.contains(&"specforge://graph/ext/{id}"),
        "{listed:?}"
    );
    let i017: Vec<String> = server
        .state()
        .diagnostics()
        .iter()
        .filter(|d| d.code == "I017" && d.message.starts_with("MCP resource"))
        .map(|d| d.message.clone())
        .collect();
    assert_eq!(
        i017,
        [
            "MCP resource 'graph-ext' (specforge://graph/ext/{id}) of '@test/cmds' not served: the core resource 'specforge://graph/{entity_id}' serves its URIs"
        ]
    );
    // Its URIs are the core entity resource's: the export never runs.
    read_resource(&mut server, "specforge://graph/ext/1");
    assert!(ext.calls().is_empty(), "{:?}", ext.calls());
}

#[test]
fn a_declared_short_name_names_the_commands_tools() {
    // `@acme/widgets` declares the short name `w`: its command `list_all`
    // is the tool `specforge.w.list_all`, as it is `specforge w list-all`.
    let declaration = specforge_protocol_types::ExtensionDeclaration {
        handshake: specforge_protocol_types::HandshakeResponse {
            name: "@acme/widgets".into(),
            version: "1.0.0".into(),
            ext_short: Some("w".into()),
            ..Default::default()
        },
        surfaces: serde_json::from_value(json!({"commands": [{"id": "list_all",
            "title": "List", "description": "List all", "export": "cmd__list_all"}]}))
        .unwrap(),
        ..Default::default()
    };
    let registries = specforge_registry::build_registries(vec![declaration]);
    let table = specforge_mcp::surface_table::ExtensionSurfaceTable::build(
        &registries,
        specforge_mcp::tools::CORE_TOOLS,
        specforge_mcp::resources::CORE_RESOURCES,
    );
    let names: Vec<&str> = table.tools().iter().map(|t| t.name.as_str()).collect();
    assert_eq!(names, ["specforge.w.list_all"]);
}
