//! The wire text of the refusals of validate, analyze and the trace tool and
//! prompt, pinned once before any of them moves (round 13, plan 10): one
//! block per call, the request then the McpError (a tool) or the JSON-RPC
//! error (a prompt), the project root redacted.

use crate::support::*;
use serde_json::{Value, json};
use specforge_test::prelude::*;

const BEHAVIORS: &str = "behavior login \"Log in\" {\n  contract \"MUST log in\"\n}\n\nbehavior logout \"Log out\" {\n  contract \"MUST log out\"\n}\n";

/// A served project: `@specforge/software` and `@specforge/testing`.
fn served_with(extensions: &[&str], files: &[(&str, &str)]) -> Served {
    let mut project = TestProject::new()
        .enabling(extensions)
        .file("main.spec", BEHAVIORS);
    for (path, text) in files {
        project = project.file(path, text);
    }
    project.serve_components()
}

/// `text` with the root and its parent redacted, canonical spellings first.
fn redact(text: String, root: &std::path::Path) -> String {
    let parent = root.parent().unwrap();
    let mut spellings: Vec<(String, &str)> = Vec::new();
    for (path, name) in [(root, "<root>"), (parent, "<parent>")] {
        spellings.push((path.display().to_string(), name));
        if let Ok(canonical) = std::fs::canonicalize(path) {
            spellings.push((canonical.display().to_string(), name));
        }
    }
    spellings.sort_by_key(|(path, _)| std::cmp::Reverse(path.len()));
    spellings
        .iter()
        .fold(text, |text, (path, name)| text.replace(path.as_str(), name))
}

/// One block: the request, then the error the reply carries.
fn block(served: &mut Served, label: &str, request: Value, prompt: bool) -> String {
    let reply = if prompt {
        get_prompt(served, label, request.clone())
    } else {
        call_tool(served, label, request.clone())
    };
    let error = if prompt {
        reply["error"].clone()
    } else {
        crate::tool_errors::mcp_error(&reply)
    };
    let text = format!(
        "== {label} {request}\n{}\n",
        serde_json::to_string_pretty(&error).unwrap()
    );
    redact(text, served.root())
}

#[specforge_test(
    invariant = "mcp_structured_error_responses",
    verify = "an operation states the request field each refusal is about, and a refusal of the project or the environment is about none"
)]
fn validate_analyze_and_trace_refusals_answer_as_pinned() {
    let mut out = String::new();
    let mut p = served_with(
        &["@specforge/software", "@specforge/testing"],
        &[("bad.json", "{not json")],
    );
    let tool = |p: &mut Served, out: &mut String, name: &str, args: Value| {
        out.push_str(&block(p, name, args, false));
    };
    tool(
        &mut p,
        &mut out,
        "specforge.validate",
        json!({"severity_filter": "errors"}),
    );
    tool(
        &mut p,
        &mut out,
        "specforge.validate",
        json!({"lint": ["pedantik"]}),
    );
    tool(
        &mut p,
        &mut out,
        "specforge.analyze",
        json!({"pass": "contrakts"}),
    );
    tool(
        &mut p,
        &mut out,
        "specforge.analyze",
        json!({"pass": "zzzzzz"}),
    );
    tool(&mut p, &mut out, "specforge.analyze", json!({"min": 150}));
    tool(&mut p, &mut out, "specforge.analyze", json!({"min": 50}));
    tool(
        &mut p,
        &mut out,
        "specforge.analyze",
        json!({"test_results": "none.json"}),
    );
    tool(
        &mut p,
        &mut out,
        "specforge.analyze",
        json!({"test_results": "bad.json"}),
    );
    tool(
        &mut p,
        &mut out,
        "specforge.trace",
        json!({"entity_id": "logn"}),
    );
    tool(
        &mut p,
        &mut out,
        "specforge.trace",
        json!({"entity_id": "zzzzzzzz"}),
    );
    out.push_str(&block(
        &mut p,
        "specforge://prompts/trace",
        json!({"entity_id": "logn"}),
        true,
    ));
    tool(
        &mut p,
        &mut out,
        "specforge.analyze",
        json!({"test_results": "../r/specforge-report.json"}),
    );
    tool(
        &mut p,
        &mut out,
        "specforge.analyze",
        json!({"pass": "contrakts", "min": 150}),
    );
    tool(
        &mut p,
        &mut out,
        "specforge.analyze",
        json!({"min": 150, "test_results": "none.json"}),
    );

    // No coverage pass: the gate cannot be answered (E068).
    let mut q = served_with(&["@specforge/software"], &[]);
    tool(&mut q, &mut out, "specforge.analyze", json!({"min": 50}));

    // An unusable recorded report (E045).
    let mut r = served_with(
        &["@specforge/software", "@specforge/testing"],
        &[("specforge-report.json", "{not json")],
    );
    tool(&mut r, &mut out, "specforge.analyze", json!({"min": 50}));

    insta::assert_snapshot!("validate_analyze_and_trace_refusals", out);
}
