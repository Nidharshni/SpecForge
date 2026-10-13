use assert_cmd::Command;
use std::fs;
use std::io::Write;
use std::process::Output;
use tempfile::TempDir;

pub fn setup_project(files: &[(&str, &str)]) -> TempDir {
    let dir = TempDir::new().unwrap();
    // A real project: specforge.json is what makes the directory a project
    // (ProjectRoot) — management ops require it.
    if !files.iter().any(|(p, _)| *p == "specforge.json") {
        fs::write(
            dir.path().join("specforge.json"),
            r#"{"name":"test","version":"0.1.0","extensions":[]}"#,
        )
        .unwrap();
    }
    for (path, content) in files {
        let full = dir.path().join(path);
        if let Some(parent) = full.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(&full, content).unwrap();
    }
    dir
}

pub fn setup_project_with_config(config: &str, spec_files: &[(&str, &str)]) -> TempDir {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("specforge.json"), config).unwrap();
    let spec_dir = dir.path().join("spec");
    fs::create_dir_all(&spec_dir).unwrap();
    for (path, content) in spec_files {
        let full = spec_dir.join(path);
        if let Some(parent) = full.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(&full, content).unwrap();
    }
    dir
}

pub fn specforge_cmd() -> Command {
    assert_cmd::cargo_bin_cmd!("specforge")
}

pub fn parse_json_stdout(output: &Output) -> serde_json::Value {
    let stdout = String::from_utf8_lossy(&output.stdout);
    serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("invalid JSON: {}\noutput: {}", e, stdout))
}

// --- Fixture Constants ---

/// 5 software entity kinds: behavior, invariant, event, type, port
pub const SOFTWARE_SPEC: &str = r#"
behavior parse_input "Parse Input" {
    contract "The system MUST parse all valid input"
    status done
}

behavior emit_output "Emit Output" {
    contract "The system MUST emit structured output"
}

invariant graph_acyclic "Graph Acyclicity" {
    guarantee "The dependency graph MUST be acyclic"
}

event input_parsed "Input Parsed" {
    trigger parse_input
}

type parse_result "Parse Result" {
    description "Result of a parse operation"
}

port file_reader "File Reader" {
    direction inbound
}
"#;

/// 6 product entity kinds: feature, journey, deliverable, milestone, module, term
pub const PRODUCT_SPEC: &str = r#"
feature fast_parsing "Fast Parsing" {
    problem "Users need quick feedback"
    solution "Incremental parsing"
    behaviors [parse_input]
}

journey developer_workflow "Developer Workflow" {
    description "A developer writes and validates specs"
}

deliverable cli_tool "CLI Tool" {
    description "The specforge CLI binary"
}

milestone v1_release "V1 Release" {
    status planned
    description "First stable release"
}

module parser_module "Parser Module" {
    description "Handles .spec file parsing"
}

term spec_file "Spec File" {
    definition "A .spec file containing entity declarations"
}
"#;

/// 3 governance entity kinds: decision, constraint, failure_mode
pub const GOVERNANCE_SPEC: &str = r#"
decision use_treesitter "Use Tree-Sitter" {
    status accepted
    context "Need a robust parser"
    decision_text "We will use tree-sitter for parsing"
    consequences "Fast parsing, good error recovery"
}

constraint sub_second_check "Sub-Second Check" {
    category performance
    priority high
    description "Check must complete in under 1 second for 100 files"
}

failure_mode parser_crash "Parser Crash" {
    severity high
    occurrence unlikely
    detection likely
    cause "Malformed input"
    effect "CLI exits with panic"
    mitigations [parse_input]
}
"#;

/// All 14 entity kinds combined with cross-references
pub const MULTI_EXTENSION_SPEC: &str = r#"
behavior parse_input "Parse Input" {
    contract "The system MUST parse all valid input"
    status done
}

behavior emit_output "Emit Output" {
    contract "The system MUST emit structured output"
}

invariant graph_acyclic "Graph Acyclicity" {
    guarantee "The dependency graph MUST be acyclic"
}

event input_parsed "Input Parsed" {
    trigger parse_input
}

type parse_result "Parse Result" {
    description "Result of a parse operation"
}

port file_reader "File Reader" {
    direction inbound
}

feature fast_parsing "Fast Parsing" {
    problem "Users need quick feedback"
    solution "Incremental parsing"
    behaviors [parse_input]
}

journey developer_workflow "Developer Workflow" {
    description "A developer writes and validates specs"
}

deliverable cli_tool "CLI Tool" {
    description "The specforge CLI binary"
}

milestone v1_release "V1 Release" {
    status planned
    description "First stable release"
}

module parser_module "Parser Module" {
    description "Handles .spec file parsing"
}

term spec_file "Spec File" {
    definition "A .spec file containing entity declarations"
}

decision use_treesitter "Use Tree-Sitter" {
    status accepted
    context "Need a robust parser"
    decision_text "We will use tree-sitter for parsing"
    consequences "Fast parsing, good error recovery"
}

constraint sub_second_check "Sub-Second Check" {
    category performance
    priority high
    description "Check must complete in under 1 second for 100 files"
}

failure_mode parser_crash "Parser Crash" {
    severity high
    occurrence unlikely
    detection likely
    cause "Malformed input"
    effect "CLI exits with panic"
    mitigations [parse_input]
}
"#;

/// 4-entity linear chain: feature → behavior → invariant → type (via ref lists)
pub const DEEP_CHAIN_SPEC: &str = r#"
feature feat_root "Feature Root" {
    problem "needs parsing"
    solution "parse it"
    behaviors [beh_middle]
}

behavior beh_middle "Behavior Middle" {
    contract "The system MUST validate"
    features [feat_root]
}

invariant inv_deep "Invariant Deep" {
    guarantee "The graph MUST be acyclic"
    enforced_by [beh_middle]
}

type typ_leaf "Type Leaf" {
    description "A leaf type"
}
"#;

/// Graph with a cycle: a→b→c→a via reference-list fields
pub const CYCLE_SPEC: &str = r#"
behavior cycle_a "Cycle A" {
    contract "first in cycle"
    features [cycle_c]
}

feature cycle_b "Cycle B" {
    problem "second"
    solution "cycle"
    behaviors [cycle_a]
}

feature cycle_c "Cycle C" {
    problem "third"
    solution "cycle"
    behaviors [cycle_a]
}
"#;

/// Single entity with no references (isolated node)
pub const ISOLATED_SPEC: &str = r#"
behavior isolated_node "Isolated Node" {
    contract "The system MUST be isolated"
}
"#;

/// Build a JSON-RPC request string
pub fn mcp_request(id: u64, method: &str, params: serde_json::Value) -> String {
    serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": method,
        "params": params
    })
    .to_string()
}

/// The `initialize` a standard MCP client sends first: no `projectRoot`, so
/// the server compiles the `specforge mcp <path>` argument.
pub fn mcp_initialize(id: u64) -> String {
    mcp_request(
        id,
        "initialize",
        serde_json::json!({
            "protocolVersion": "2025-03-26",
            "capabilities": {},
            "clientInfo": {"name": "e2e", "version": "0"}
        }),
    )
}

/// A `specforge.json` enabling the builtins whose kinds the fixtures use.
pub const BUILTINS_CONFIG: &str = r#"{"name":"test","version":"0.1.0","extensions":["@specforge/software","@specforge/product","@specforge/governance"]}"#;

/// Send requests to MCP after a standard `initialize` (id 0) in an existing
/// project, e.g. one made with [`setup_project_with_config`] to enable
/// extensions; every line it wrote, parsed.
pub fn mcp_session_in(dir: &TempDir, requests: &[String]) -> Vec<serde_json::Value> {
    let mut all = vec![mcp_initialize(0)];
    all.extend_from_slice(requests);
    mcp_raw_session_in(dir, &all)
}

/// `specforge mcp <dir>` fed exactly `requests`, no `initialize` first (a
/// stateless MCP 2026-07-28 client), and every line it wrote, parsed.
pub fn mcp_raw_session_in(dir: &TempDir, requests: &[String]) -> Vec<serde_json::Value> {
    let mut child = std::process::Command::new(assert_cmd::cargo_bin!("specforge"))
        .args(["mcp"])
        .arg(dir.path())
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("failed to start specforge mcp");

    let stdin = child.stdin.as_mut().unwrap();
    for req in requests {
        writeln!(stdin, "{}", req).unwrap();
    }
    stdin.flush().unwrap();
    drop(child.stdin.take());

    let output = child.wait_with_output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);

    stdout
        .lines()
        .filter(|l| !l.trim().is_empty())
        .filter_map(|l| serde_json::from_str(l).ok())
        .collect()
}

/// Find a JSON-RPC response by its `id` field in a list of responses.
pub fn find_response(responses: &[serde_json::Value], id: u64) -> Option<&serde_json::Value> {
    responses.iter().find(|r| r["id"] == id)
}

/// The `McpError` of a failed MCP tool call: an `isError` result whose
/// text is the error's JSON (ADR 0004 D4-a).
pub fn tool_error(response: &serde_json::Value) -> serde_json::Value {
    assert!(
        response.get("error").is_none(),
        "a tool failure is a result, not a JSON-RPC error: {response}"
    );
    assert_eq!(response["result"]["isError"], true, "{response}");
    let error = parse_tool_content(response);
    assert!(error["code"].is_string(), "an McpError: {error}");
    error
}

/// Parse the `content[0].text` from an MCP tool result as JSON.
pub fn parse_tool_content(response: &serde_json::Value) -> serde_json::Value {
    let text = response["result"]["content"][0]["text"]
        .as_str()
        .unwrap_or_else(|| panic!("no content[0].text in response: {}", response));
    serde_json::from_str(text)
        .unwrap_or_else(|e| panic!("content text is not valid JSON: {}\ntext: {}", e, text))
}

/// Entities with cross-kind references for testing resolution
pub const CROSS_REF_SPEC: &str = r#"
behavior validate_graph "Validate Graph" {
    contract "The system MUST validate the graph"
    features [graph_validation]
}

behavior resolve_refs "Resolve References" {
    contract "The system MUST resolve cross-entity references"
    features [graph_validation]
}

feature graph_validation "Graph Validation" {
    problem "Graph must be valid before export"
    solution "Multi-pass validation"
    behaviors [validate_graph, resolve_refs]
}

invariant refs_resolved "References Resolved" {
    guarantee "All references MUST resolve to existing entities"
    enforced_by [validate_graph]
}

event validation_complete "Validation Complete" {
    trigger validate_graph
}

failure_mode unresolved_ref "Unresolved Reference" {
    severity medium
    occurrence occasional
    detection moderate
    cause "Typo in entity ID"
    effect "Broken graph edges"
    mitigations [resolve_refs]
}
"#;
