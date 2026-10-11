//! How a core tool's arguments are listed and read (plan 08, ADR 0033).
//!
//! The first two tests are snapshots of the core tools' listing, byte for
//! byte as a client receives it, and of what a call answers (and writes)
//! for each way of sending an argument. They were taken before the
//! derivation, bugs included; the commit that changes a line of either
//! re-blesses it, so the snapshot history shows each user-visible change.

use crate::support::*;
use serde_json::{Value, json};
use specforge_test::prelude::*;
use std::path::{Path, PathBuf};

#[test]
fn core_tool_listing_today() {
    let listing = serde_json::to_string_pretty(&core_tools()).expect("a listing");
    insta::assert_snapshot!(listing);
}

/// The defaults of the read views' arguments are the operation's: the query
/// depth and the search limit that `specforge_ops::query` holds, which the
/// CLI's `--depth` default reads too (plan 08 T8).
#[test]
fn the_read_view_defaults_are_the_operations() {
    let tools = core_tools();
    let default_of = |tool: &str, argument: &str| -> Value {
        let tool = tools
            .iter()
            .find(|descriptor| descriptor.name == tool)
            .unwrap_or_else(|| panic!("no tool {tool}"));
        tool.input_schema["properties"][argument]["default"].clone()
    };
    assert_eq!(
        default_of("specforge.query", "depth"),
        json!(specforge_ops::query::DEFAULT_DEPTH)
    );
    assert_eq!(
        default_of("specforge.search", "limit"),
        json!(specforge_ops::query::DEFAULT_SEARCH_LIMIT)
    );
}

/// `main.spec`: `alpha`, misformatted on purpose, and `beta`, which refines it.
const MAIN: &str = concat!(
    "behavior alpha \"Alpha\" {\n",
    "      contract    \"The system MUST work\"\n",
    "}\n",
    "behavior beta \"Beta\" {\n",
    "  refines [alpha]\n",
    "}\n",
);

/// A fresh served project holding [`MAIN`], `@specforge/software` enabled.
fn served() -> Served {
    TestProject::new()
        .enabling(&["@specforge/software"])
        .file("main.spec", MAIN)
        .serve_components()
}

/// The text of a refusal: `refused <code> argument=<a>: <message>`.
fn refusal(error: &Value) -> String {
    format!(
        "refused {} argument={}: {}",
        error["code"].as_str().unwrap_or("?"),
        error["argument"].as_str().unwrap_or("-"),
        error["message"].as_str().unwrap_or("?"),
    )
}

/// The one fact a row records about a successful reply of `tool`.
fn fact(tool: &str, result: &Value, response: &Value) -> String {
    let count = |value: &Value| value.as_array().map_or(0, Vec::len);
    match tool {
        "specforge.format" => format!("check_only={}", result["check_only"]),
        "specforge.rename" => format!("dry_run={}", result["dry_run"]),
        "specforge.migrate" => format!("dry_run={}", result["dry_run"]),
        "specforge.query" | "specforge.export" => format!("nodes={}", count(&result["nodes"])),
        "specforge.search" => format!("results={}", count(&result["results"])),
        "specforge.validate" => {
            let verdict = &response["result"]["_meta"]["specforge/check"];
            format!(
                "ok={} errors={} warnings={}",
                verdict["ok"], verdict["errors"], verdict["warnings"]
            )
        }
        _ => String::new(),
    }
}

/// One tool call from a fresh project: `tool | arguments | outcome`.
fn tool_row(tool: &str, label: &str, arguments: impl FnOnce(&Path) -> Value) -> String {
    let mut served = served();
    let root = served.root().to_path_buf();
    let before = files_under(&root);
    let response = call_tool(&mut served, tool, arguments(&root));
    let after = files_under(&root);
    let wrote: Vec<PathBuf> = changed_files(&root, &before, &after);
    let outcome = if response["result"]["isError"] == true {
        let text = response["result"]["content"][0]["text"]
            .as_str()
            .unwrap_or_default();
        let error: Value = serde_json::from_str(text).unwrap_or_else(|_| json!({"message": text}));
        refusal(&error)
    } else if let Some(error) = response.get("error") {
        format!("refused rpc {error}")
    } else {
        let result = tool_json(&response);
        let fact = fact(tool, &result, &response);
        format!("ok {fact}").trim_end().to_string()
    };
    format!(
        "{} | {label} | {outcome} | wrote {:?}",
        tool.trim_start_matches("specforge."),
        wrote
    )
}

/// One prompt request from a fresh project.
fn prompt_row(prompt: &str, arguments: Value) -> String {
    let mut served = served();
    let label = arguments.to_string();
    let response = get_prompt(
        &mut served,
        &format!("specforge://prompts/{prompt}"),
        arguments,
    );
    let outcome = match response.get("error") {
        Some(error) => format!(
            "refused {} argument={}: {}",
            error["code"],
            error["data"]["argument"].as_str().unwrap_or("-"),
            error["message"].as_str().unwrap_or("?"),
        ),
        None => "ok".to_string(),
    };
    format!("prompt {prompt} | {label} | {outcome}")
}

#[test]
fn argument_reading_today() {
    let fixed = |value: Value| move |_: &Path| value.clone();
    let rows = [
        tool_row(
            "specforge.format",
            r#"{"check":true}"#,
            fixed(json!({"check": true})),
        ),
        tool_row(
            "specforge.format",
            r#"{"check":true,"write":true}"#,
            fixed(json!({"check": true, "write": true})),
        ),
        tool_row(
            "specforge.format",
            r#"{"check":"true"}"#,
            fixed(json!({"check": "true"})),
        ),
        tool_row(
            "specforge.format",
            r#"{"diff":"true"}"#,
            fixed(json!({"diff": "true"})),
        ),
        tool_row(
            "specforge.rename",
            r#"{"entity_id":"alpha","new_name":"gamma","dry_run":"true"}"#,
            fixed(json!({"entity_id": "alpha", "new_name": "gamma", "dry_run": "true"})),
        ),
        tool_row(
            "specforge.migrate",
            r#"{"dry_run":"true"}"#,
            fixed(json!({"dry_run": "true"})),
        ),
        tool_row(
            "specforge.query",
            r#"{"entity_id":"alpha","depth":"0"}"#,
            fixed(json!({"entity_id": "alpha", "depth": "0"})),
        ),
        tool_row(
            "specforge.query",
            r#"{"entity_id":"alpha","kinds":"behavior"}"#,
            fixed(json!({"entity_id": "alpha", "kinds": "behavior"})),
        ),
        tool_row(
            "specforge.search",
            r#"{"query":"a","limit":"1"}"#,
            fixed(json!({"query": "a", "limit": "1"})),
        ),
        tool_row(
            "specforge.list",
            r#"{"limit":"1"}"#,
            fixed(json!({"limit": "1"})),
        ),
        tool_row(
            "specforge.validate",
            r#"{"strict":"yes"}"#,
            fixed(json!({"strict": "yes"})),
        ),
        tool_row(
            "specforge.export",
            r#"{"format":"brief","scop":"alpha"}"#,
            fixed(json!({"format": "brief", "scop": "alpha"})),
        ),
        tool_row(
            "specforge.stats",
            r#"{"use_cached":true}"#,
            fixed(json!({"use_cached": true})),
        ),
        tool_row(
            "specforge.stats",
            r#"{"path":"<served root>"}"#,
            |root| json!({"path": root.to_str().expect("a UTF-8 root")}),
        ),
        tool_row(
            "specforge.doctor",
            r#"{"use_cached":"true"}"#,
            fixed(json!({"use_cached": "true"})),
        ),
        tool_row("specforge.infer_session", "{}", fixed(json!({}))),
        prompt_row("context", json!({"entity_id": 42})),
        prompt_row("review", json!({"entity_id": "alpha", "depth": "two"})),
        prompt_row("context", json!({"entity_id": "alpha", "bogus": "x"})),
    ];
    insta::assert_snapshot!(rows.join("\n"));
}

fn format_changes(arguments: Value) -> Vec<PathBuf> {
    let mut served = served();
    let root = served.root().to_path_buf();
    let before = files_under(&root);
    let reply = call_tool(&mut served, "specforge.format", arguments);
    assert_eq!(reply["result"]["isError"], false, "{reply}");
    changed_files(&root, &before, &files_under(&root))
}

#[specforge_test(
    behavior = "provide_mcp_format_tool",
    verify = "the format tool advertises no default for write, and check or diff without write writes nothing"
)]
fn format_states_the_write_rule_not_a_default() {
    let format = core_tools()
        .into_iter()
        .find(|tool| tool.name == "specforge.format")
        .expect("the format tool");
    let write = &format.input_schema["properties"]["write"];
    assert_eq!(write["type"], "boolean");
    assert!(write.get("default").is_none(), "{write}");

    let none: Vec<PathBuf> = Vec::new();
    assert_eq!(format_changes(json!({"check": true})), none);
    assert_eq!(format_changes(json!({"diff": true})), none);
    assert_eq!(
        format_changes(json!({})),
        [PathBuf::from("main.spec")],
        "an absent write writes"
    );
}

#[specforge_test(
    behavior = "read_mcp_arguments_as_declared",
    verify = "an argument neither the tool nor its target declares is refused naming it, with the declared one it is close to"
)]
fn an_undeclared_argument_is_refused_naming_it() {
    use crate::tool_errors::mcp_error;

    let mut served = served();
    let root = served.root().to_path_buf();

    let reply = call_tool(
        &mut served,
        "specforge.export",
        json!({"format": "brief", "scop": "alpha"}),
    );
    let error = mcp_error(&reply);
    assert_eq!(error["code"], "invalid_input");
    assert_eq!(error["argument"], "scop");
    assert_eq!(error["message"], "unknown argument 'scop'");
    assert_eq!(error["data"]["suggestion"], "did you mean 'scope'?");

    // The target's names are its own: use_cached only where the target reads it.
    let reply = call_tool(&mut served, "specforge.stats", json!({"use_cached": true}));
    assert_eq!(mcp_error(&reply)["argument"], "use_cached");
    let reply = call_tool(&mut served, "specforge.doctor", json!({"use_cached": true}));
    assert_eq!(reply["result"]["isError"], false, "{reply}");
    // A Served entry accepts its own project's root, though it lists no path.
    let own = root.to_str().expect("a UTF-8 root");
    let reply = call_tool(&mut served, "specforge.stats", json!({"path": own}));
    assert_eq!(reply["result"]["isError"], false, "{reply}");

    // A refused mutation is a failed one: it writes nothing.
    let before = files_under(&root);
    let reply = call_tool(
        &mut served,
        "specforge.rename",
        json!({"entity_id": "alpha", "new_name": "gamma", "extra": "x"}),
    );
    assert_eq!(mcp_error(&reply)["argument"], "extra");
    assert_eq!(
        changed_files(&root, &before, &files_under(&root)),
        Vec::<PathBuf>::new()
    );

    // A prompt refuses it as invalid params, naming the argument.
    let reply = get_prompt(
        &mut served,
        "specforge://prompts/context",
        json!({"entity_id": "alpha", "bogus": "x"}),
    );
    assert_eq!(reply["error"]["code"], -32602, "{reply}");
    assert_eq!(reply["error"]["data"]["argument"], "bogus");

    // Every core input schema says so.
    for tool in core_tools() {
        assert_eq!(
            tool.input_schema["additionalProperties"], false,
            "{}",
            tool.name
        );
    }
}

/// The reply of a successful call of `tool`, parsed.
fn answer(tool: &str, arguments: Value) -> Value {
    let mut served = served();
    let reply = call_tool(&mut served, tool, arguments);
    assert_eq!(reply["result"]["isError"], false, "{tool}: {reply}");
    tool_json(&reply)
}

/// The refusal of a call of `tool`: its `McpError`.
fn refused(tool: &str, arguments: Value) -> Value {
    let mut served = served();
    let reply = call_tool(&mut served, tool, arguments);
    crate::tool_errors::mcp_error(&reply)
}

#[specforge_test(
    behavior = "read_mcp_arguments_as_declared",
    verify = "a boolean or count sent as a string is read as one, as an extension command reads it; any other value of the wrong type is refused naming the argument"
)]
fn a_core_tool_reads_a_value_by_its_type() {
    // A count sent as a string is that count: depth "0" is the entity alone.
    let query = answer(
        "specforge.query",
        json!({"entity_id": "alpha", "depth": "0", "format": "brief"}),
    );
    assert_eq!(query["nodes"].as_array().map(Vec::len), Some(1), "{query}");
    let deeper = answer(
        "specforge.query",
        json!({"entity_id": "alpha", "depth": 1, "format": "brief"}),
    );
    assert_eq!(
        deeper["nodes"].as_array().map(Vec::len),
        Some(2),
        "{deeper}"
    );

    // search and list read a limit the same way.
    let found = answer("specforge.search", json!({"query": "a", "limit": "1"}));
    assert_eq!(
        found["results"].as_array().map(Vec::len),
        Some(1),
        "{found}"
    );
    let listed = answer("specforge.list", json!({"limit": "1"}));
    assert_eq!(
        listed["entities"].as_array().map(Vec::len),
        Some(1),
        "{listed}"
    );

    // Anything else of the wrong type is refused naming the argument.
    let error = refused("specforge.validate", json!({"strict": "yes"}));
    assert_eq!(error["code"], "invalid_input");
    assert_eq!(error["argument"], "strict");
    assert_eq!(error["message"], "strict must be true or false, got 'yes'");
    let error = refused(
        "specforge.query",
        json!({"entity_id": "alpha", "kinds": "behavior"}),
    );
    assert_eq!(error["argument"], "kinds");
    assert_eq!(
        error["message"],
        "kinds must be a list of strings, got 'behavior'"
    );
    let error = refused("specforge.search", json!({"query": "a", "limit": -1}));
    assert_eq!(error["argument"], "limit");
    let error = refused("specforge.infer_session", json!({}));
    assert_eq!(error["argument"], "action");
    assert_eq!(error["message"], "Missing required parameter: action");
}

#[specforge_test(
    invariant = "dry_run_side_effect_freedom",
    verify = "an MCP dry run, check or diff asked for with the string true writes nothing"
)]
fn a_preview_asked_for_with_a_string_writes_nothing() {
    for (tool, arguments) in [
        ("specforge.format", json!({"check": "true"})),
        ("specforge.format", json!({"diff": "true"})),
        (
            "specforge.rename",
            json!({"entity_id": "alpha", "new_name": "gamma", "dry_run": "true"}),
        ),
        (
            "specforge.remove_extension",
            json!({"name": "@specforge/software", "dry_run": "true"}),
        ),
        ("specforge.migrate", json!({"dry_run": "true"})),
    ] {
        let mut served = served();
        let root = served.root().to_path_buf();
        let before = files_under(&root);
        let reply = call_tool(&mut served, tool, arguments.clone());
        assert_eq!(
            changed_files(&root, &before, &files_under(&root)),
            Vec::<PathBuf>::new(),
            "{tool} {arguments}: {reply}"
        );
        // It answered as the preview it was asked for.
        let result = tool_json(&reply);
        let previewed = result["check_only"] == true
            || result["dry_run"] == true
            || result["action"] == "planned"
            || result["diffs"].is_array();
        assert!(previewed, "{tool} {arguments}: {result}");
    }
}

#[specforge_test(
    behavior = "read_mcp_arguments_as_declared",
    verify = "a prompt reads its arguments by the same rule its listing states"
)]
fn a_prompt_reads_its_arguments_by_the_listed_rule() {
    // alpha <- beta <- gamma: depth 1 from alpha reaches beta, depth 2 gamma.
    let mut served = TestProject::new()
        .enabling(&["@specforge/software", "@specforge/testing"])
        .file(
            "main.spec",
            concat!(
                "behavior alpha \"Alpha\" {\n  contract \"a\"\n  verify unit \"a\"\n}\n",
                "behavior beta \"Beta\" {\n  refines [alpha]\n  contract \"b\"\n  verify unit \"b\"\n}\n",
                "behavior gamma \"Gamma\" {\n  refines [beta]\n  contract \"c\"\n  verify unit \"c\"\n}\n",
            ),
        )
        .serve_components();
    let summary = |served: &mut Served, depth: Value| -> usize {
        let reply = get_prompt(
            served,
            "specforge://prompts/review",
            json!({"entity_id": "alpha", "depth": depth}),
        );
        prompt_payload(&reply)["coverage_summary"]
            .as_array()
            .map_or(0, Vec::len)
    };
    assert_eq!(summary(&mut served, json!("2")), 3, "a count from a string");
    assert_eq!(summary(&mut served, json!(2)), 3);
    assert_eq!(summary(&mut served, json!(1)), 2);

    // A value of the wrong type is -32602 naming the argument.
    for (prompt, arguments, argument, message) in [
        (
            "context",
            json!({"entity_id": 42}),
            "entity_id",
            "entity_id must be a string, got 42",
        ),
        (
            "infer",
            json!({"scope": "plan", "cursor": "-1"}),
            "cursor",
            "cursor must be a non-negative integer, got -1",
        ),
        (
            "explore",
            json!({"depth": "x"}),
            "depth",
            "depth must be a non-negative integer, got 'x'",
        ),
    ] {
        let reply = get_prompt(
            &mut served,
            &format!("specforge://prompts/{prompt}"),
            arguments,
        );
        assert_eq!(reply["error"]["code"], -32602, "{prompt}: {reply}");
        assert_eq!(reply["error"]["message"], message, "{prompt}");
        assert_eq!(reply["error"]["data"]["argument"], argument, "{prompt}");
    }
}

#[specforge_test(
    behavior = "read_mcp_arguments_as_declared",
    verify = "a boolean or count sent as a string is read as one, as an extension command reads it; any other value of the wrong type is refused naming the argument"
)]
fn a_target_argument_is_read_by_its_type() {
    // use_cached "true" is the last compile: an extension enabled since,
    // which is not installed, is not reported until a fresh call.
    let mut served = served();
    served.write(
        "specforge.json",
        r#"{"name":"t","version":"0.1.0","extensions":["@specforge/software","@acme/missing"]}"#,
    );
    let load_failures = |served: &mut Served, arguments: Value| -> Vec<Value> {
        tool("specforge.doctor", served, arguments)["findings"]
            .as_array()
            .map(|findings| {
                findings
                    .iter()
                    .filter(|f| f["about"] == "load")
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    };
    let cached = load_failures(&mut served, json!({"use_cached": "true"}));
    assert!(cached.is_empty(), "{cached:?}");
    let fresh = load_failures(&mut served, json!({}));
    assert!(fresh.iter().any(|f| f["code"] == "E028"), "{fresh:?}");

    // A path that is not a string is refused, not ignored.
    let error = refused("specforge.validate", json!({"path": 42}));
    assert_eq!(error["code"], "invalid_input");
    assert_eq!(error["argument"], "path");
    assert_eq!(error["message"], "path must be a string, got 42");
    let error = refused("specforge.doctor", json!({"use_cached": "yes"}));
    assert_eq!(error["argument"], "use_cached");
    assert_eq!(
        error["message"],
        "use_cached must be true or false, got 'yes'"
    );
}

/// `tools/call` of `name`, the reply's JSON.
fn tool(name: &str, served: &mut Served, arguments: Value) -> Value {
    crate::support::tool(served, name, arguments)
}

#[specforge_test(
    behavior = "read_mcp_arguments_as_declared",
    verify = "a listing is derived from the typed arguments: each argument's type, description, default, enumerated values and whether it is required"
)]
fn every_core_tool_lists_its_typed_arguments() {
    for tool in specforge_mcp::tools::CORE_TOOLS {
        let schema = tool.input_schema();
        let properties = schema["properties"].as_object().expect("properties");
        for argument in tool.arguments() {
            let name = format!("{}.{}", tool.name(), argument.name);
            let property = &properties[argument.name];
            assert_eq!(property, &argument.schema, "{name}");
            // Its description is the field's doc comment (an enumerated
            // argument's goes on to name each choice).
            let text = property["description"].as_str().expect(&name);
            assert!(text.starts_with(argument.description), "{name}: {text}");
            // Its type is its Rust type's: a flag states its default,
            // a count `minimum: 0`; a required argument has no default.
            match property["type"].as_str() {
                // An optional flag (format's write) states no default.
                Some("boolean") if name == "specforge.format.write" => {
                    assert!(property.get("default").is_none(), "{name}")
                }
                Some("boolean") => assert!(property["default"].is_boolean(), "{name}"),
                Some("integer") => assert_eq!(property["minimum"], 0, "{name}"),
                Some("number") => assert!(property.get("default").is_none(), "{name}"),
                Some("string" | "array" | "object") => {}
                other => panic!("{name}: type {other:?}"),
            }
            if argument.required {
                assert!(property.get("default").is_none(), "{name}");
            }
        }
        // Nothing is listed that no field declares, but the target's own.
        let declared: Vec<&str> = tool.arguments().iter().map(|a| a.name).collect();
        for listed in properties.keys() {
            assert!(
                declared.contains(&listed.as_str())
                    || tool.target().fields().contains(&listed.as_str()),
                "{}: lists {listed}",
                tool.name()
            );
        }
    }
}

// --- the derive, over probe structs ---

mod derived {
    use super::*;
    use specforge_mcp::args::{
        AgentPlan, Arg, Argument, Arguments, EntityIds, choice_schema, input_schema, read,
    };
    use specforge_mcp::target::TargetSpec;
    use specforge_ops::model::ModelFormat;
    use specforge_project::coverage::Status;
    use specforge_protocol_types::command_args::normalize_arg;
    use specforge_protocol_types::{CommandArgDescriptor, CommandArgType};

    #[derive(Debug, Arguments)]
    struct Probe {
        /// A name
        name: String,
        /// Hops
        #[arg(default = 3)]
        hops: usize,
        /// A flag
        flag: bool,
        /// Maybe
        maybe: Option<String>,
        /// Kinds
        kinds: Vec<String>,
        /// Format
        #[arg(choice = specforge_ops::model::MODEL_FORMAT)]
        format: ModelFormat,
        /// Status
        #[arg(choice = specforge_ops::coverage::STATUS)]
        status: Option<Status>,
        /// Profiles
        #[arg(names = &["a", "b"])]
        profiles: Vec<String>,
    }

    fn properties(declared: &[Argument]) -> Vec<(&'static str, Value)> {
        declared
            .iter()
            .map(|argument| (argument.name, argument.schema.clone()))
            .collect()
    }

    #[specforge_test(
        behavior = "read_mcp_arguments_as_declared",
        verify = "a listing is derived from the typed arguments: each argument's type, description, default, enumerated values and whether it is required"
    )]
    fn a_listing_is_derived_from_the_typed_arguments() {
        let declared = Probe::declared();
        assert_eq!(
            properties(&declared),
            [
                ("name", json!({"type": "string", "description": "A name"})),
                (
                    "hops",
                    json!({"type": "integer", "minimum": 0, "default": 3, "description": "Hops"})
                ),
                (
                    "flag",
                    json!({"type": "boolean", "default": false, "description": "A flag"})
                ),
                ("maybe", json!({"type": "string", "description": "Maybe"})),
                (
                    "kinds",
                    json!({"type": "array", "items": {"type": "string"}, "description": "Kinds"})
                ),
                (
                    "format",
                    choice_schema(&specforge_ops::model::MODEL_FORMAT, "Format")
                ),
                (
                    "status",
                    choice_schema(&specforge_ops::coverage::STATUS, "Status")
                ),
                (
                    "profiles",
                    json!({
                        "type": "array",
                        "items": {"type": "string", "enum": ["a", "b"]},
                        "description": "Profiles",
                    })
                ),
            ]
        );
        let required: Vec<&str> = declared
            .iter()
            .filter(|argument| argument.required)
            .map(|argument| argument.name)
            .collect();
        assert_eq!(required, ["name"]);

        let schema = input_schema(&declared, TargetSpec::SERVED_VIEW);
        assert_eq!(schema["required"], json!(["name"]));
        assert_eq!(schema["additionalProperties"], false);
        assert_eq!(
            schema["properties"].as_object().expect("properties").len(),
            8
        );
        assert_eq!(
            schema["properties"]["format"]["default"], "markdown",
            "a table's default is the argument's"
        );
        assert!(schema["properties"]["status"].get("default").is_none());
    }

    fn refusal(arguments: Value) -> specforge_mcp::tool::McpError {
        *read::<Probe>(&arguments).expect_err("a refusal")
    }

    #[specforge_test(
        behavior = "read_mcp_arguments_as_declared",
        verify = "an absent or null argument reads as the default the listing advertises, and a missing required one is refused naming it"
    )]
    fn an_absent_argument_is_its_default() {
        for given in [json!({"name": "x"}), json!({"name": "x", "hops": null})] {
            let probe: Probe = read(&given).expect("a read");
            assert_eq!(probe.name, "x");
            assert_eq!(probe.hops, 3);
            assert!(!probe.flag);
            assert_eq!(probe.maybe, None);
            assert!(probe.kinds.is_empty());
            assert_eq!(probe.format, ModelFormat::Markdown);
            assert_eq!(probe.status, None);
            assert!(probe.profiles.is_empty());
        }

        let error = refusal(json!({}));
        assert_eq!(error.code.as_str(), "invalid_input");
        assert_eq!(error.message, "Missing required parameter: name");
        assert_eq!(error.argument.as_deref(), Some("name"));
    }

    #[specforge_test(
        behavior = "read_mcp_arguments_as_declared",
        verify = "a boolean or count sent as a string is read as one, as an extension command reads it; any other value of the wrong type is refused naming the argument"
    )]
    fn a_value_is_read_by_its_type() {
        let probe: Probe = read(&json!({
            "name": "x", "hops": "2", "flag": "true", "kinds": ["a"], "maybe": "m",
            "format": "dot", "status": "partial", "profiles": ["a"],
        }))
        .expect("a read");
        assert_eq!((probe.hops, probe.flag), (2, true));
        assert_eq!(probe.format, ModelFormat::Dot);
        assert_eq!(probe.status, Some(Status::Partial));
        assert_eq!(probe.kinds, ["a"]);

        for (given, argument, message) in [
            (
                json!({"name": "x", "flag": "yes"}),
                "flag",
                "flag must be true or false, got 'yes'",
            ),
            (
                json!({"name": "x", "hops": -1}),
                "hops",
                "hops must be a non-negative integer, got -1",
            ),
            (
                json!({"name": "x", "hops": "two"}),
                "hops",
                "hops must be a non-negative integer, got 'two'",
            ),
            (json!({"name": 42}), "name", "name must be a string, got 42"),
            (
                json!({"name": "x", "kinds": "k"}),
                "kinds",
                "kinds must be a list of strings, got 'k'",
            ),
        ] {
            let error = refusal(given.clone());
            assert_eq!(error.code.as_str(), "invalid_input", "{given}");
            assert_eq!(error.message, message, "{given}");
            assert_eq!(error.argument.as_deref(), Some(argument), "{given}");
        }
    }

    /// `T`'s reading of each value is `normalize_arg`'s, value and wording.
    fn parity<T: Arg + std::fmt::Debug>(
        arg_type: CommandArgType,
        minimum: Option<i64>,
        values: &[Value],
        same: impl Fn(&T, &Value) -> bool,
    ) {
        let arg = CommandArgDescriptor {
            name: "x".into(),
            arg_type,
            required: false,
            default_value: None,
            description: None,
            minimum,
        };
        for value in values {
            match (normalize_arg(&arg, value), T::read("x", value)) {
                (Ok(normalized), Ok(read)) => {
                    assert!(
                        same(&read, &normalized),
                        "{value}: {read:?} vs {normalized}"
                    )
                }
                (Err(error), Err(message)) => assert_eq!(error.message(), message, "{value}"),
                (rule, read) => panic!("{value}: rule {rule:?}, adapter {read:?}"),
            }
        }
    }

    #[test]
    fn the_flag_count_and_string_adapters_are_the_commands_rule() {
        let values = [
            json!(true),
            json!(false),
            json!("true"),
            json!("false"),
            json!("yes"),
            json!(""),
            json!(0),
            json!(1),
            json!(2),
            json!(-1),
            json!("2"),
            json!("-1"),
            json!("two"),
            json!(1.5),
            json!([]),
            json!({}),
        ];
        parity::<bool>(CommandArgType::Bool, None, &values, |read, normalized| {
            normalized == &Value::Bool(*read)
        });
        parity::<usize>(
            CommandArgType::Integer,
            Some(0),
            &values,
            |read, normalized| normalized == &Value::from(*read),
        );
        parity::<String>(CommandArgType::String, None, &values, |read, normalized| {
            normalized == &Value::from(read.as_str())
        });
    }

    #[test]
    fn a_table_refusal_names_the_argument() {
        let error = refusal(json!({"name": "x", "format": "svg"}));
        assert_eq!(error.code.as_str(), "invalid_input");
        assert!(
            error.message.starts_with("Unknown format: svg. Expected: "),
            "{}",
            error.message
        );
        assert_eq!(error.argument.as_deref(), Some("format"));
        let close = refusal(json!({"name": "x", "format": "marcdown"}));
        assert_eq!(
            close.data.as_ref().map(|data| &data["suggestion"]),
            Some(&json!("did you mean 'markdown'?"))
        );
        let wrong_type = refusal(json!({"name": "x", "format": 5}));
        assert_eq!(wrong_type.argument.as_deref(), Some("format"));
    }

    #[derive(Debug, Arguments)]
    struct Raw {
        /// The where
        r#where: Option<serde_json::Map<String, Value>>,
        /// A plan
        plan: Option<AgentPlan>,
        /// Ids
        ids: EntityIds,
        /// A required renderer
        #[arg(choice = specforge_ops::export::FORMAT)]
        renderer: String,
    }

    #[test]
    fn the_derive_reads_fields_in_order_and_unraws_names() {
        let declared = Raw::declared();
        let names: Vec<&str> = declared.iter().map(|argument| argument.name).collect();
        assert_eq!(names, ["where", "plan", "ids", "renderer"]);
        assert_eq!(
            declared[0].schema,
            json!({"type": "object", "additionalProperties": true, "description": "The where"})
        );
        assert!(!declared[0].required && !declared[2].required && declared[3].required);

        let raw: Raw = read(&json!({
            "where": {"status": "done"}, "ids": "a, b,,c ", "renderer": "markdown",
        }))
        .expect("a read");
        assert_eq!(raw.r#where.expect("a where")["status"], "done");
        assert!(raw.plan.is_none());
        assert_eq!(raw.ids.0, ["a", "b", "c"]);
        assert_eq!(raw.renderer, "markdown");
        let missing = *read::<Raw>(&json!({})).expect_err("a refusal");
        assert_eq!(missing.message, "Missing required parameter: renderer");
    }
}
