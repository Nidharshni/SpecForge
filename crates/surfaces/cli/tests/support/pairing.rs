//! What the surfaces' harnesses share to pair a command line with an MCP call (ADR 0101 D5): the in-process
//! server and its calls, and the normalization of an answer's machine-specific parts.

use serde_json::{Value, json};
use specforge_mcp::testing::{Served, rpc};
use std::path::Path;

use crate::support::TestProject;

/// The project as `specforge mcp <root>` serves it, its `initialize` negotiating MCP 2025-03-26.
pub fn served(project: TestProject) -> Served {
    Served::rooted_at(project, "2025-03-26")
}

/// The JSON-RPC responses to `calls` on `served`, one per call, in order. A call is a tool's `{name, arguments}`,
/// or `{method, params}` for any other request.
pub fn mcp_responses(served: &mut Served, calls: &[Value]) -> Vec<Value> {
    calls
        .iter()
        .map(|call| match call.get("method") {
            Some(method) => rpc::call(served, method.as_str().unwrap(), call["params"].clone()),
            None => rpc::call(served, "tools/call", call.clone()),
        })
        .collect()
}

/// Call MCP tools on `served`; one result per call, in order: the tool's JSON
/// content, `{"isError": content}` for an error result, or `{"error": ...}`
/// for a JSON-RPC error ([`mcp_responses`]).
pub fn mcp_calls(served: &mut Served, calls: &[Value]) -> Vec<Value> {
    mcp_responses(served, calls)
        .into_iter()
        .map(|resp| {
            if !resp["error"].is_null() {
                return json!({"error": resp["error"]});
            }
            let Some(text) = resp["result"]["content"][0]["text"].as_str() else {
                return resp["result"].clone();
            };
            let content = serde_json::from_str(text).unwrap_or_else(|_| json!({"text": text}));
            if resp["result"]["isError"] == true {
                return json!({ "isError": content });
            }
            content
        })
        .collect()
}

/// `value` with the parts that churn for reasons outside this harness
/// redacted: the embedded Graph Protocol schema (any extension change moves
/// it).
fn redacted(value: &Value) -> Value {
    match value {
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(k, v)| {
                    let v = match k.as_str() {
                        "schema" if v.is_object() => json!("[SCHEMA]"),
                        _ => redacted(v),
                    };
                    (k.clone(), v)
                })
                .collect(),
        ),
        Value::Array(items) => Value::Array(items.iter().map(redacted).collect()),
        other => other.clone(),
    }
}

/// `text` with every 64-digit hex run (a SHA-256: blob hashes move whenever
/// an extension is rebuilt) replaced by `[SHA256]`.
pub fn without_hashes(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut run = String::new();
    for c in text.chars().chain(std::iter::once(' ')) {
        if c.is_ascii_hexdigit() {
            run.push(c);
            continue;
        }
        if run.len() == 64 {
            out.push_str("[SHA256]");
        } else {
            out.push_str(&run);
        }
        run.clear();
        out.push(c);
    }
    out.pop();
    out
}

/// `text` with the machine-specific parts of `root` replaced: its canonical spelling when it exists, then as given.
pub fn normalized_text(text: &str, root: &Path) -> String {
    let text = match root.canonicalize() {
        Ok(canonical) => text.replace(canonical.to_str().unwrap(), "[ROOT]"),
        Err(_) => text.to_string(),
    };
    text.replace(root.to_str().unwrap(), "[ROOT]")
}

/// `value` as pretty JSON with the machine-specific parts replaced.
pub fn normalized(value: &Value, root: &Path) -> String {
    let text = without_hashes(&serde_json::to_string_pretty(&redacted(value)).unwrap());
    normalized_text(&text, root)
}
