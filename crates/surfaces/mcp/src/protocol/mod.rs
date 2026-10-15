pub mod error_codes;
pub(crate) mod method;
pub mod router;

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    pub id: Option<Value>,
    pub method: String,
    #[serde(default)]
    pub params: Value,
}

#[derive(Debug, Serialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: &'static str,
    /// Always present: JSON-RPC 2.0 requires `null` when the request's id
    /// could not be read (a parse error).
    pub id: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

#[derive(Debug, Clone, Serialize)]
pub struct JsonRpcError {
    pub code: i64,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

impl JsonRpcError {
    pub fn new(code: i64, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            data: None,
        }
    }

    pub fn with_data(mut self, data: Value) -> Self {
        self.data = Some(data);
        self
    }
}

impl JsonRpcResponse {
    pub fn success(id: Option<Value>, result: Value) -> Self {
        Self {
            jsonrpc: "2.0",
            id,
            result: Some(result),
            error: None,
        }
    }

    /// A successful response whose result is `message`.
    pub fn sent<M: crate::message::Message>(id: Option<Value>, message: &M) -> Self {
        Self::success(
            id,
            serde_json::to_value(message).expect("a message serializes"),
        )
    }

    pub fn error(id: Option<Value>, code: i64, message: impl Into<String>) -> Self {
        Self::from_error(id, JsonRpcError::new(code, message))
    }

    pub fn from_error(id: Option<Value>, error: JsonRpcError) -> Self {
        Self {
            jsonrpc: "2.0",
            id,
            result: None,
            error: Some(error),
        }
    }

    pub fn error_with_data(
        id: Option<Value>,
        code: i64,
        message: impl Into<String>,
        data: Value,
    ) -> Self {
        Self::from_error(id, JsonRpcError::new(code, message).with_data(data))
    }
}

/// A JSON-RPC id as text: a string as it is, `null` as nothing, a number as
/// its JSON text.
pub fn id_text(id: &Value) -> String {
    match id {
        Value::String(text) => text.clone(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

pub fn parse_request(input: &str) -> Result<JsonRpcRequest, JsonRpcResponse> {
    let value: Value = serde_json::from_str(input)
        .map_err(|_| JsonRpcResponse::error(None, error_codes::PARSE_ERROR, "Parse error"))?;
    parse_request_value(value)
}

/// Read one JSON-RPC request object (a whole message, or one member of a
/// batch).
pub fn parse_request_value(value: Value) -> Result<JsonRpcRequest, JsonRpcResponse> {
    // Validate jsonrpc field
    if value.get("jsonrpc").and_then(|v| v.as_str()) != Some("2.0") {
        let id = value.get("id").cloned();
        return Err(JsonRpcResponse::error(
            id,
            error_codes::INVALID_REQUEST,
            "Invalid Request: missing or invalid jsonrpc version",
        ));
    }

    // Validate method field
    if value.get("method").and_then(|v| v.as_str()).is_none() {
        let id = value.get("id").cloned();
        return Err(JsonRpcResponse::error(
            id,
            error_codes::INVALID_REQUEST,
            "Invalid Request: missing method",
        ));
    }

    serde_json::from_value(value).map_err(|e| {
        JsonRpcResponse::error(
            None,
            error_codes::INVALID_REQUEST,
            format!("Invalid Request: {}", e),
        )
    })
}
