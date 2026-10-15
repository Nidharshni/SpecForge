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

/// What one surface answered for a request: its document, normalized, or a refusal.
#[derive(Debug, PartialEq)]
pub enum Outcome {
    /// The surface answered with this document.
    Answered(Value),
    /// The surface refused the request (a usage error, an operation's refusal, an `isError` result).
    Refused,
}

/// A CLI run's outcome: exit 2 (clap), or a nonzero exit that prints `{error, code}` or nothing parseable, is a
/// refusal; otherwise the document on stdout, normalized under `root`.
pub fn cli_outcome(run: &crate::support::Run, root: &Path) -> Outcome {
    if run.code == 2 {
        return Outcome::Refused;
    }
    match serde_json::from_str::<Value>(&run.stdout) {
        Ok(doc) if doc.get("error").is_some() && doc.get("code").is_some() => Outcome::Refused,
        Ok(doc) => Outcome::Answered(normalized_json(&doc, root)),
        Err(_) if run.code != 0 => Outcome::Refused,
        Err(e) => panic!(
            "a command that answered printed no JSON ({e}): {}",
            run.stdout
        ),
    }
}

/// An MCP result as [`mcp_calls`] returns it: an `isError` or `error` result is a refusal; otherwise the content,
/// normalized under `root`.
pub fn mcp_outcome(result: &Value, root: &Path) -> Outcome {
    if result.get("isError").is_some() || result.get("error").is_some() {
        return Outcome::Refused;
    }
    Outcome::Answered(normalized_json(result, root))
}

fn normalized_json(value: &Value, root: &Path) -> Value {
    serde_json::from_str(&normalized(value, root)).expect("normalizing keeps the JSON")
}
