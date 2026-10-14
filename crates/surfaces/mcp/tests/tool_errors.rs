//! Reading a failed tool call (ADR 0004 D4-a): an `isError` result whose
//! one text block is the JSON of an `McpError`.

use serde_json::Value;

/// The `McpError` a failed `tools/call` response carries. Panics, naming
/// the response, when the call did not fail that way: a JSON-RPC error, a
/// success, or an error whose text is not an `McpError`.
pub fn mcp_error(resp: &Value) -> Value {
    assert!(
        resp.get("error").is_none(),
        "a tool failure is a result, not a JSON-RPC error: {resp}"
    );
    let result = &resp["result"];
    assert_eq!(
        result["isError"], true,
        "expected an isError result: {resp}"
    );
    let text = result["content"][0]["text"]
        .as_str()
        .unwrap_or_else(|| panic!("an isError result has a text block: {resp}"));
    let error: Value = serde_json::from_str(text)
        .unwrap_or_else(|e| panic!("the isError text is not JSON ({e}): {resp}"));
    assert!(
        error["code"].is_string() && error["message"].is_string(),
        "an McpError has a code and a message: {error}"
    );
    error
}
