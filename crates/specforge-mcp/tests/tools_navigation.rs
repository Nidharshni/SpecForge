use crate::support::*;
use serde_json::{Value, json};
use specforge_extension_sdk::prelude::{PassDiagnostic, PassSpan};
use specforge_mcp::McpServer;
use specforge_test::prelude::*;

/// test.spec: `alpha` on lines 1–4, `beta` on lines 10–12 (its
/// `behaviors [alpha]` on line 11).
const TEST_SPEC: &str = concat!(
    "behavior alpha \"Alpha Behavior\" {\n",
    "    contract \"The system MUST do alpha\"\n",
    "    verify unit \"test alpha\"\n",
    "}\n",
    "\n\n\n\n\n",
    "feature beta \"Beta Feature\" {\n",
    "    behaviors [alpha]\n",
    "}\n",
);

/// The project every in-process test serves, before its own files.
fn project() -> TestProject {
    TestProject::new().file("test.spec", TEST_SPEC)
}

fn test_server() -> Served {
    project().serve(&[TestExtension::software()])
}

/// `text` after `line - 1` line breaks: placed after what precedes it, an
/// entity it declares starts `line - 1` lines below the line it follows
/// (on line `line` of a file it begins).
fn at_line(line: usize, text: &str) -> String {
    format!("{}{text}", "\n".repeat(line - 1))
}

/// The outline of `file`, as entity ids in the order returned.
fn outline_ids(server: &mut McpServer, file: &str) -> Vec<String> {
    let resp = call_tool(server, "specforge.outline", json!({"file": file}));
    let parsed: Value =
        serde_json::from_str::<Value>(&tool_text(&resp)).unwrap()["entries"].clone();
    parsed
        .as_array()
        .unwrap_or_else(|| panic!("no outline in {resp}"))
        .iter()
        .map(|e| e["entity_id"].as_str().unwrap().to_string())
        .collect()
}

/// A server whose order.spec declares `early` (line 5), `middle` (line 12)
/// and `late` (line 20): the graph holds them by id (early, late, middle),
/// not in line order.
fn server_with_unordered_file() -> Served {
    project()
        .file(
            "order.spec",
            &format!(
                "{}{}{}",
                at_line(5, "behavior early \"Early\" {\n}\n"),
                at_line(6, "behavior middle \"Middle\" {\n}\n"),
                at_line(7, "behavior late \"Late\" {\n}\n"),
            ),
        )
        .serve(&[TestExtension::software()])
}

/// A server serving a project of `files` with `@specforge/software`,
/// compiled from disk: spans come from the parser.
fn served(files: &[(&str, &str)]) -> Served {
    files
        .iter()
        .fold(
            TestProject::new().enabling(&["@specforge/software"]),
            |project, (file, text)| project.file(file, text),
        )
        .serve_components()
}

const LIMIT: &str = "invariant session_limit \"Limit\" {\n  guarantee \"x\"\n}\n";
const LOGIN: &str = "behavior login \"Login\" {\n  invariants [session_limit]\n}\n";

/// The result of `tool` with `args`, parsed.
fn result(server: &mut McpServer, tool: &str, args: Value) -> Value {
    let resp = call_tool(server, tool, args);
    let parsed: Value =
        serde_json::from_str(&tool_text(&resp)).unwrap_or_else(|e| panic!("{e}: {resp}"));
    // The array tools answer an object holding the array.
    let key = match tool {
        "specforge.outline" => "entries",
        "specforge.suggest_fixes" => "fixes",
        _ => return parsed,
    };
    parsed.get(key).cloned().unwrap_or(parsed)
}

// --- specforge.inspect ---

/// The statement inspect reports is the field the extension declares
/// headline and normative, not whatever field is named `contract`.
#[test]
fn inspect_reports_no_contract_its_kind_does_not_declare() {
    let mut server = test_server();
    let resp = call_tool(
        &mut server,
        "specforge.inspect",
        json!({"entity_id": "alpha"}),
    );
    let parsed: Value = serde_json::from_str(&tool_text(&resp)).unwrap();
    assert!(parsed["contract"].is_null(), "{parsed}");
    assert!(parsed["fields"]["contract"].is_string(), "{parsed}");
}

// B:provide_mcp_inspect_tool — verify unit "returns entity details"
#[specforge_test(
    behavior = "provide_mcp_inspect_tool",
    verify = "specforge.inspect returns full entity details"
)]
fn inspect_returns_details() {
    let mut server = project().serve(&[TestExtension::software().headline("behavior")]);
    let resp = call_tool(
        &mut server,
        "specforge.inspect",
        json!({"entity_id": "alpha"}),
    );
    let text = tool_text(&resp);
    let parsed: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(parsed["entity_id"], "alpha");
    assert_eq!(parsed["kind"], "behavior");
    assert!(parsed["source_span"].is_object());
    assert!(parsed["contract"].is_string());
    assert!(parsed["verify_declarations"].is_array());
}

// B:provide_mcp_inspect_tool — verify unit "includes reference count"
#[specforge_test(
    behavior = "provide_mcp_inspect_tool",
    verify = "response includes referenced_by, refers_to and verify declarations"
)]
fn inspect_includes_referenced_by_and_refers_to() {
    let mut server = test_server();
    let parsed = result(
        &mut server,
        "specforge.inspect",
        json!({"entity_id": "alpha"}),
    );
    // beta -> alpha is alpha's only edge: beta references alpha, and alpha
    // refers to nothing.
    assert_eq!(parsed["referenced_by"], json!(["beta"]));
    assert_eq!(parsed["refers_to"], json!([]));
    assert_eq!(parsed["verify_declarations"], json!(["unit test alpha"]));
    // The deprecated aliases are gone.
    assert!(parsed.get("references").is_none(), "{parsed}");
    assert!(parsed.get("reference_count").is_none(), "{parsed}");

    let parsed = result(
        &mut server,
        "specforge.inspect",
        json!({"entity_id": "beta"}),
    );
    assert_eq!(parsed["referenced_by"], json!([]));
    assert_eq!(parsed["refers_to"], json!(["alpha"]));
}

// B:provide_mcp_inspect_tool — verify unit "unknown entity returns error"
#[specforge_test(
    behavior = "provide_mcp_inspect_tool",
    verify = "non-existent entity returns error response"
)]
fn inspect_unknown_entity() {
    let mut server = test_server();
    let resp = call_tool(
        &mut server,
        "specforge.inspect",
        json!({"entity_id": "nonexistent"}),
    );
    // C9-00/C9-12: entity-not-found is a tool execution error — the tool ran
    // and the domain state did not match — so it surfaces as an isError
    // result, not a -32602 protocol error.
    assert!(
        resp["result"]["isError"] == true,
        "domain failure must be an isError tool result"
    );
}

#[specforge_test(
    behavior = "provide_mcp_inspect_tool",
    verify = "response includes every field, like an invariant's guarantee"
)]
fn inspect_returns_every_field() {
    let mut server = project()
        .file(
            "ids.spec",
            "invariant unique_ids \"Unique ids\" {\n    guarantee \"Ids MUST be unique\"\n    risk medium\n}\n",
        )
        .serve(&[TestExtension::software().string_field("invariant", "risk")]);
    let resp = call_tool(
        &mut server,
        "specforge.inspect",
        json!({"entity_id": "unique_ids"}),
    );
    let parsed: Value = serde_json::from_str(&tool_text(&resp)).unwrap();
    assert_eq!(parsed["fields"]["guarantee"], "Ids MUST be unique");
    assert_eq!(parsed["fields"]["risk"], "medium");
    assert!(parsed["contract"].is_null(), "an invariant has no contract");
}

#[specforge_test(
    behavior = "provide_mcp_inspect_tool",
    verify = "coverage status matches specforge.coverage obligation by obligation"
)]
fn inspect_coverage_matches_the_coverage_tool() {
    let mut server = test_server();
    let statuses = |server: &mut McpServer| {
        let resp = call_tool(server, "specforge.inspect", json!({"entity_id": "alpha"}));
        let inspect: Value = serde_json::from_str(&tool_text(&resp)).unwrap();
        let resp = call_tool(server, "specforge.coverage", json!({"entity_id": "alpha"}));
        let coverage: Value =
            serde_json::from_str::<Value>(&tool_text(&resp)).unwrap()["entities"].clone();
        (
            inspect["coverage_status"].as_str().unwrap().to_string(),
            coverage[0]["status"].as_str().unwrap().to_string(),
        )
    };
    // The recorded report is a check input: written to disk, it is read
    // by the next call (ADR 0014 D8).
    let report = |server: &Served, tests: &str| {
        server.write(
            "specforge-report.json",
            &format!(r#"{{"results":{{"alpha":{{"tests":[{tests}]}}}}}}"#),
        );
    };

    // A passing test that names no obligation proves none of them.
    report(&server, r#"{"name":"t","status":"passed"}"#);
    let (inspect, coverage) = statuses(&mut server);
    assert_eq!(inspect, "uncovered");
    assert_eq!(inspect, coverage);

    report(
        &server,
        r#"{"name":"t","status":"passed","verify":"test alpha"}"#,
    );
    let (inspect, coverage) = statuses(&mut server);
    assert_eq!(inspect, "covered");
    assert_eq!(inspect, coverage);
}

#[specforge_test(
    behavior = "provide_mcp_inspect_tool",
    verify = "diagnostics are the entity's own, not those of an entity whose ID contains it"
)]
fn inspect_diagnostics_are_the_entitys_own() {
    // tasks.spec: `task` on lines 20–22, `task_id_uniqueness` on 30–34.
    let tasks = format!(
        "{}{}",
        at_line(
            20,
            "invariant task \"Task\" {\n    guarantee \"a task exists\"\n}\n"
        ),
        at_line(
            8,
            "invariant task_id_uniqueness \"Unique\" {\n    guarantee \"ids are unique\"\n    // one\n    // two\n}\n"
        ),
    );
    let at = |start_line, end_line| PassSpan {
        file: "tasks.spec".into(),
        start_line,
        start_col: 1,
        end_line,
        end_col: 2,
    };
    // Two findings placed by their spans (task_id_uniqueness's block, a
    // line inside task's), two by the entity they name.
    let mut server = project()
        .file("tasks.spec", &tasks)
        .serve(&[TestExtension::software()
            .reporting(PassDiagnostic::warning("W003", "a finding").with_span(at(30, 34)))
            .reporting(PassDiagnostic::warning("W100", "a finding").with_span(at(21, 21)))
            .reporting(PassDiagnostic::warning("W101", "a finding").with_entity("task"))
            .reporting(
                PassDiagnostic::warning("W102", "a finding").with_entity("task_id_uniqueness"),
            )]);
    let codes = |server: &mut McpServer, id: &str| {
        let resp = call_tool(server, "specforge.inspect", json!({"entity_id": id}));
        let parsed: Value = serde_json::from_str(&tool_text(&resp)).unwrap();
        parsed["diagnostics"]
            .as_array()
            .unwrap()
            .iter()
            .map(|d| d["code"].as_str().unwrap().to_string())
            .collect::<Vec<_>>()
    };
    assert_eq!(codes(&mut server, "task"), vec!["W100", "W101"]);
    assert_eq!(
        codes(&mut server, "task_id_uniqueness"),
        vec!["W003", "W102"]
    );
}

#[specforge_test(
    behavior = "provide_mcp_inspect_tool",
    verify = "a spanless diagnostic belongs to the entities its data names, never to one its message quotes"
)]
fn inspect_attributes_spanless_diagnostics_by_data() {
    // A reference cycle: no span, its entities in its data.
    // and a spanless diagnostic whose message quotes `gamma` but whose
    // data names nothing, a check-phase pass's.
    let mut server = TestProject::new()
        .file(
            "a.spec",
            "behavior alpha \"A\" {\n  depends_on [beta]\n}\nbehavior beta \"B\" {\n  depends_on [alpha]\n}\nbehavior gamma \"G\" {\n}\n",
        )
        .serve(&[TestExtension::software()
            .reference("behavior", "depends_on", "behavior")
            .reporting(PassDiagnostic::warning(
                "W900",
                "behavior 'gamma' is mentioned here",
            ))]);
    let codes = |server: &mut McpServer, id: &str| -> Vec<String> {
        let parsed = result(server, "specforge.inspect", json!({"entity_id": id}));
        parsed["diagnostics"]
            .as_array()
            .unwrap()
            .iter()
            .map(|d| d["code"].as_str().unwrap().to_string())
            .collect()
    };
    assert!(codes(&mut server, "alpha").contains(&"W061".to_string()));
    assert!(codes(&mut server, "beta").contains(&"W061".to_string()));
    assert!(!codes(&mut server, "gamma").contains(&"W061".to_string()));

    // A spanless diagnostic whose message quotes an ID but whose data
    // names none belongs to nobody.
    let w900 = server
        .state()
        .diagnostics()
        .into_iter()
        .find(|d| d.code == "W900")
        .expect("the pass reports W900");
    assert!(w900.span.is_none() && w900.data.is_none(), "{w900:?}");
    assert!(!codes(&mut server, "gamma").contains(&"W900".to_string()));
}

// --- specforge.find_definition ---

// B:provide_mcp_find_definition_tool — verify unit "returns source location"
#[specforge_test(
    behavior = "provide_mcp_find_definition_tool",
    verify = "specforge.find_definition returns file, line, and column"
)]
fn find_definition_returns_location() {
    let mut server = served(&[("limit.spec", LIMIT), ("login.spec", LOGIN)]);
    let parsed = result(
        &mut server,
        "specforge.find_definition",
        json!({"entity_id": "session_limit"}),
    );
    // The position is the entity's name, where a cursor goes.
    assert_eq!(parsed["entity_id"], "session_limit");
    assert_eq!(parsed["file_path"], "limit.spec");
    assert_eq!(
        (&parsed["line"], &parsed["column"]),
        (&json!(1), &json!(11))
    );
    assert_eq!(
        parsed["name_span"],
        json!({"file": "limit.spec", "start_line": 1, "start_col": 11, "end_line": 1, "end_col": 24})
    );
    assert_eq!(
        parsed["source_span"],
        json!({"file": "limit.spec", "start_line": 1, "start_col": 1, "end_line": 3, "end_col": 2})
    );
    assert_eq!(parsed["precision"], "token");

    // An indented declaration: nested.spec line 7 is
    // `    behavior indented "Indented" {`, its name at column 14.
    let mut server = project()
        .file(
            "nested.spec",
            &at_line(7, "    behavior indented \"Indented\" {\n    }\n"),
        )
        .serve(&[TestExtension::software()]);
    let parsed = result(
        &mut server,
        "specforge.find_definition",
        json!({"entity_id": "indented"}),
    );
    assert_eq!(
        (&parsed["file_path"], &parsed["line"], &parsed["column"]),
        (&json!("nested.spec"), &json!(7), &json!(14))
    );
    // The block starts at the keyword, column 5.
    assert_eq!(parsed["source_span"]["start_col"], 5, "{parsed}");
    assert_eq!(parsed["precision"], "token");
}

// B:provide_mcp_find_definition_tool — verify unit "unknown entity returns error"
#[specforge_test(
    behavior = "provide_mcp_find_definition_tool",
    verify = "non-existent entity returns error response"
)]
fn find_definition_unknown_entity() {
    let mut server = test_server();
    let resp = call_tool(
        &mut server,
        "specforge.find_definition",
        json!({"entity_id": "nonexistent"}),
    );
    // C9-00/C9-12: entity-not-found is a tool execution error — the tool ran
    // and the domain state did not match — so it surfaces as an isError
    // result, not a -32602 protocol error.
    assert!(
        resp["result"]["isError"] == true,
        "domain failure must be an isError tool result"
    );
}

// --- specforge.find_references ---

// B:provide_mcp_find_references_tool — verify unit "returns referencing entities"
#[specforge_test(
    behavior = "provide_mcp_find_references_tool",
    verify = "specforge.find_references returns all reference locations"
)]
fn find_references_returns_refs() {
    let mut server = served(&[("limit.spec", LIMIT), ("login.spec", LOGIN)]);
    let parsed = result(
        &mut server,
        "specforge.find_references",
        json!({"entity_id": "session_limit"}),
    );
    assert_eq!(parsed["entity_id"], "session_limit");
    // One location per occurrence: the token as written, its field.
    assert_eq!(
        parsed["locations"],
        json!([{
            "referencing_entity_id": "login",
            "referenced_entity_id": "session_limit",
            "field": "invariants",
            "role": "reference",
            "precision": "token",
            "source_span": {"file": "login.spec", "start_line": 2, "start_col": 15, "end_line": 2, "end_col": 28}
        }])
    );
}

#[specforge_test(
    behavior = "provide_mcp_find_references_tool",
    verify = "direction and include_declaration select which occurrences are returned"
)]
fn find_references_direction_and_declaration() {
    let mut server = served(&[("limit.spec", LIMIT), ("login.spec", LOGIN)]);
    let mut spans = |args: Value| -> Vec<String> {
        let parsed = result(&mut server, "specforge.find_references", args);
        parsed["locations"]
            .as_array()
            .unwrap()
            .iter()
            .map(|l| {
                let s = &l["source_span"];
                format!(
                    "{} {}:{} {} {}",
                    s["file"].as_str().unwrap(),
                    s["start_line"],
                    s["start_col"],
                    l["role"].as_str().unwrap(),
                    l["referencing_entity_id"].as_str().unwrap()
                )
            })
            .collect()
    };
    assert_eq!(
        spans(json!({"entity_id": "session_limit", "include_declaration": true})),
        [
            "limit.spec 1:11 declaration session_limit",
            "login.spec 2:15 reference login"
        ]
    );
    // Incoming is the default: nothing references login.
    assert!(spans(json!({"entity_id": "login"})).is_empty());
    assert_eq!(
        spans(json!({"entity_id": "login", "direction": "outgoing"})),
        ["login.spec 2:15 reference login"]
    );
    assert_eq!(
        spans(json!({"entity_id": "login", "direction": "both", "include_declaration": true})),
        [
            "login.spec 1:10 declaration login",
            "login.spec 2:15 reference login"
        ]
    );
    let resp = call_tool(
        &mut server,
        "specforge.find_references",
        json!({"entity_id": "login", "direction": "sideways"}),
    );
    assert_eq!(resp["result"]["isError"], true, "{resp}");
    assert!(tool_text(&resp).contains("direction"), "{resp}");
}

// B:provide_mcp_find_references_tool — verify unit "unknown entity returns error"
#[specforge_test(
    behavior = "provide_mcp_find_references_tool",
    verify = "non-existent entity returns error response"
)]
fn find_references_unknown_entity() {
    let mut server = test_server();
    let resp = call_tool(
        &mut server,
        "specforge.find_references",
        json!({"entity_id": "nonexistent"}),
    );
    // C9-00/C9-12: entity-not-found is a tool execution error — the tool ran
    // and the domain state did not match — so it surfaces as an isError
    // result, not a -32602 protocol error.
    assert!(
        resp["result"]["isError"] == true,
        "domain failure must be an isError tool result"
    );
}

// --- specforge.outline ---

// B:provide_mcp_outline_tool — verify unit "returns entities in file"
#[specforge_test(
    behavior = "provide_mcp_outline_tool",
    verify = "specforge.outline returns all entities defined in file"
)]
fn outline_returns_entities_in_file() {
    let mut server = test_server();
    let resp = call_tool(
        &mut server,
        "specforge.outline",
        json!({"file": "test.spec"}),
    );
    let text = tool_text(&resp);
    let parsed: Value = serde_json::from_str::<Value>(&text).unwrap()["entries"].clone();
    let entries = parsed.as_array().unwrap();
    for entry in entries {
        assert_eq!(entry["range"]["file"], "test.spec");
    }
    // Both of test.spec's entities, and none from another file.
    server.write(
        "other.spec",
        &at_line(3, "behavior elsewhere \"Elsewhere\" {\n}\n"),
    );
    assert_eq!(outline_ids(&mut server, "test.spec"), vec!["alpha", "beta"]);
    assert_eq!(outline_ids(&mut server, "other.spec"), vec!["elsewhere"]);
}

#[specforge_test(
    behavior = "provide_mcp_outline_tool",
    verify = "non-existent file returns error response"
)]
fn outline_of_a_missing_file_is_an_error() {
    let mut server = test_server();
    let resp = call_tool(
        &mut server,
        "specforge.outline",
        json!({"file": "nonexistent.spec"}),
    );
    let error = crate::tool_errors::mcp_error(&resp);
    assert_eq!(error["code"], "file_not_found", "{error}");
    let message = error["message"].as_str().unwrap_or_default();
    assert!(message.contains("nonexistent.spec"), "{resp}");
}

#[test]
fn outline_of_an_existing_file_without_entities_is_empty() {
    let mut server = project()
        .file("empty.spec", "// nothing yet\n")
        .serve(&[TestExtension::software()]);

    let resp = call_tool(
        &mut server,
        "specforge.outline",
        json!({"file": "empty.spec"}),
    );
    let parsed: Value =
        serde_json::from_str::<Value>(&tool_text(&resp)).unwrap()["entries"].clone();
    assert_eq!(parsed, json!([]));
}

// B:provide_mcp_outline_tool — verify unit "sorted by line number"
#[specforge_test(
    behavior = "provide_mcp_outline_tool",
    verify = "sorted by line number"
)]
fn outline_sorted_by_line() {
    let mut server = server_with_unordered_file();
    assert_eq!(
        outline_ids(&mut server, "order.spec"),
        vec!["early", "middle", "late"]
    );
}

// --- specforge.suggest_fixes ---

/// `nav`: logout names `sesion_limit`, which no entity declares and is
/// close to `session_limit`.
const NAV_LOGIN: &str = "behavior login \"Login\" {\n  invariants [session_limit]\n}\n\n\
                         behavior logout \"Logout\" {\n  invariants [sesion_limit]\n}\n";

/// The fixes `args` asks for, as `"title kind code | file L:C-L:C new_text…"`.
fn fixes(server: &mut McpServer, args: Value) -> Vec<String> {
    let parsed = result(server, "specforge.suggest_fixes", args);
    parsed
        .as_array()
        .unwrap()
        .iter()
        .map(|f| {
            let edits: Vec<String> = f["edits"]
                .as_array()
                .unwrap()
                .iter()
                .map(|e| {
                    let r = &e["range"];
                    format!(
                        "{} {}:{}-{}:{} {:?}",
                        e["file_path"].as_str().unwrap(),
                        r["start_line"],
                        r["start_col"],
                        r["end_line"],
                        r["end_col"],
                        e["new_text"].as_str().unwrap()
                    )
                })
                .collect();
            format!(
                "{} {} {} | {}",
                f["title"].as_str().unwrap(),
                f["kind"].as_str().unwrap(),
                f["diagnostic_code"].as_str().unwrap_or("-"),
                edits.join(" | ")
            )
        })
        .collect()
}

const REPLACE: &str =
    "Replace with 'session_limit' quickfix E003 | login.spec 6:15-6:27 \"session_limit\"";
const CREATE: &str = "Create invariant stub for sesion_limit refactor E003 | login.spec 8:1-8:1 \"\\ninvariant sesion_limit \\\"sesion_limit\\\" {\\n  // TODO: fill in fields\\n}\\n\"";

// B:provide_mcp_suggest_fixes_tool — verify unit "returns suggestions from diagnostics"
#[specforge_test(
    behavior = "provide_mcp_suggest_fixes_tool",
    verify = "specforge.suggest_fixes returns applicable fix suggestions"
)]
fn suggest_fixes_returns_suggestions() {
    let mut server = served(&[("limit.spec", LIMIT), ("login.spec", NAV_LOGIN)]);
    // Each fix carries the edits that apply it.
    assert_eq!(
        fixes(&mut server, json!({"entity_id": "logout"})),
        [CREATE, REPLACE]
    );
}

#[specforge_test(
    behavior = "provide_mcp_suggest_fixes_tool",
    verify = "clean entity with no diagnostics returns empty list"
)]
fn suggest_fixes_for_a_clean_entity_is_empty() {
    let mut server = served(&[("limit.spec", LIMIT), ("login.spec", NAV_LOGIN)]);
    let inspected = result(
        &mut server,
        "specforge.inspect",
        json!({"entity_id": "session_limit"}),
    );
    assert_eq!(inspected["diagnostics"], json!([]), "{inspected}");
    assert!(fixes(&mut server, json!({"entity_id": "session_limit"})).is_empty());
    // A diagnostic whose data names no fix offers none: login's W006 and
    // E006 have suggestion text, not edits.
    assert!(fixes(&mut server, json!({"entity_id": "login"})).is_empty());
}

#[specforge_test(
    behavior = "provide_mcp_suggest_fixes_tool",
    verify = "diagnostic_code filter restricts to matching diagnostics"
)]
fn suggest_fixes_diagnostic_code_filter() {
    let mut server = served(&[("limit.spec", LIMIT), ("login.spec", NAV_LOGIN)]);
    assert_eq!(
        fixes(&mut server, json!({"diagnostic_code": "E003"})),
        [CREATE, REPLACE]
    );
    assert!(fixes(&mut server, json!({"diagnostic_code": "W006"})).is_empty());
}

#[test]
fn suggest_fixes_entity_and_file_filters() {
    let mut server = served(&[("limit.spec", LIMIT), ("login.spec", NAV_LOGIN)]);
    assert_eq!(
        fixes(&mut server, json!({"file_path": "login.spec"})),
        [CREATE, REPLACE]
    );
    assert!(fixes(&mut server, json!({"file_path": "limit.spec"})).is_empty());
    assert!(fixes(&mut server, json!({"entity_id": "login"})).is_empty());
    let resp = call_tool(
        &mut server,
        "specforge.suggest_fixes",
        json!({"entity_id": "nope"}),
    );
    assert_eq!(resp["result"]["isError"], true, "{resp}");
}

#[specforge_test(
    behavior = "provide_mcp_find_references_tool",
    verify = "entity with no references returns empty list"
)]
fn find_references_empty_list() {
    let mut server = project()
        .file("orphan.spec", "behavior orphan_node \"Orphan\" {\n}\n")
        .serve(&[TestExtension::software()]);
    let resp = call_tool(
        &mut server,
        "specforge.find_references",
        json!({"entity_id": "orphan_node"}),
    );
    let text = tool_text(&resp);
    let parsed: Value = serde_json::from_str(&text).unwrap();
    assert!(parsed["locations"].as_array().unwrap().is_empty());
}

#[specforge_test(
    behavior = "provide_mcp_outline_tool",
    verify = "nested entries included for complex entities"
)]
fn outline_nests_an_entitys_methods() {
    // store.spec: the port `store`, its method `load` on line 3.
    let mut server = project()
        .file(
            "store.spec",
            "port store \"Store\" {\n    // what it stores\n    method load(path: Path) -> Store\n\n}\n",
        )
        .serve(&[TestExtension::software().kind("port", false)]);

    let resp = call_tool(
        &mut server,
        "specforge.outline",
        json!({"file": "store.spec"}),
    );
    let parsed: Value =
        serde_json::from_str::<Value>(&tool_text(&resp)).unwrap()["entries"].clone();

    let children = parsed[0]["children"].as_array().unwrap();
    assert_eq!(children.len(), 1, "{parsed}");
    assert_eq!(children[0]["entity_id"], "store.load");
    assert_eq!(children[0]["kind"], "method");
    assert_eq!(children[0]["title"], "load(path: Path) -> Store");
    assert_eq!(children[0]["range"]["start_line"], 3);
    // An entity without members has no children key.
    let flat = call_tool(
        &mut server,
        "specforge.outline",
        json!({"file": "test.spec"}),
    );
    let flat: Value = serde_json::from_str(&tool_text(&flat)).unwrap();
    assert!(flat[0].get("children").is_none(), "{flat}");
}

// B:provide_mcp_find_references_tool — verify unit "each reference includes source span"
#[specforge_test(
    behavior = "provide_mcp_find_references_tool",
    verify = "specforge.find_references returns all reference locations"
)]
fn find_references_returns_source_spans() {
    let mut server = test_server();
    let parsed = result(
        &mut server,
        "specforge.find_references",
        json!({"entity_id": "alpha"}),
    );
    // The token as written: test.spec line 11 is `    behaviors [alpha]`,
    // `alpha` at columns 16–21.
    assert_eq!(
        parsed["locations"],
        json!([{
            "referencing_entity_id": "beta",
            "referenced_entity_id": "alpha",
            "field": "behaviors",
            "role": "reference",
            "precision": "token",
            "source_span": {
                "file": "test.spec",
                "start_line": 11,
                "start_col": 16,
                "end_line": 11,
                "end_col": 21
            }
        }])
    );
}

// B:provide_mcp_outline_tool — verify unit "outline entries sorted by line number"
#[specforge_test(
    behavior = "provide_mcp_outline_tool",
    verify = "outline entries sorted by line number"
)]
fn outline_sorted_by_line_extended() {
    let mut server = server_with_unordered_file();
    let resp = call_tool(
        &mut server,
        "specforge.outline",
        json!({"file": "order.spec"}),
    );
    let parsed: Value =
        serde_json::from_str::<Value>(&tool_text(&resp)).unwrap()["entries"].clone();
    let lines: Vec<u64> = parsed
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["range"]["start_line"].as_u64().unwrap())
        .collect();
    assert_eq!(lines, vec![5, 12, 20]);
}

// --- the anchors manifest (plan 06 T0 pins) ---

/// A server over `project()` whose root holds `anchors` as
/// `specforge-anchors.json`.
fn server_with_anchors(anchors: &str) -> Served {
    project()
        .file("specforge-anchors.json", anchors)
        .serve(&[TestExtension::software()])
}

/// `find_implementation` lists the anchors of the entity it is asked
/// about, in manifest order, and nothing for another entity or without a
/// manifest.
#[specforge_test(
    behavior = "provide_mcp_find_implementation_tool",
    verify = "find_implementation lists every anchor of the entity, in manifest order"
)]
fn find_implementation_lists_every_anchor_of_the_entity() {
    let mut server = server_with_anchors(
        r#"{"version":1,"anchors":[
            {"entity_id":"alpha","file":"src/lib.rs","line":1,"symbol_name":"a","item_kind":"function","scanner":"rust"},
            {"entity_id":"beta","file":"src/lib.rs","line":2,"symbol_name":"b","item_kind":"function","scanner":"rust"},
            {"entity_id":"alpha","file":"src/net.rs","line":4,"symbol_name":"n","item_kind":"function","scanner":"rust","confidence":0.5}
        ]}"#,
    );
    let found = result(
        &mut server,
        "specforge.find_implementation",
        json!({"entity_id": "alpha"}),
    );
    assert_eq!(
        found,
        json!({
            "entity_id": "alpha",
            "count": 2,
            "implementations": [
                {"file": "src/lib.rs", "line": 1, "symbol_name": "a", "item_kind": "function", "scanner": "rust"},
                {"file": "src/net.rs", "line": 4, "symbol_name": "n", "item_kind": "function", "scanner": "rust"},
            ],
        })
    );

    let none = result(
        &mut server,
        "specforge.find_implementation",
        json!({"entity_id": "nope"}),
    );
    assert_eq!(none["count"], 0);
    assert_eq!(none["implementations"], json!([]));

    let mut bare = test_server();
    let absent = result(
        &mut bare,
        "specforge.find_implementation",
        json!({"entity_id": "alpha"}),
    );
    assert_eq!(absent["count"], 0);
}

/// An anchors manifest with an anchor missing a field is a
/// `schema_mismatch` for both readers, with E071 as its diagnostic.
#[test]
fn an_unusable_anchors_manifest_is_schema_mismatch() {
    let mut server = server_with_anchors(r#"{"version":1,"anchors":[{"entity_id":"x"}]}"#);
    for (tool, args) in [
        ("specforge.find_implementation", json!({"entity_id": "x"})),
        (
            "specforge.find_spec_for_source",
            json!({"file_path": "a.rs"}),
        ),
    ] {
        let resp = call_tool(&mut server, tool, args);
        let error = crate::tool_errors::mcp_error(&resp);
        assert_eq!(error["code"], "schema_mismatch", "{tool}: {error}");
        assert_eq!(error["diagnostic"]["code"], "E071", "{tool}: {error}");
        assert!(
            error["message"]
                .as_str()
                .unwrap()
                .starts_with("failed to parse specforge-anchors.json: missing field `file`"),
            "{tool}: {error}"
        );
    }
}
