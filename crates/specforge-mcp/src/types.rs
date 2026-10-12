use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpToolDescriptor {
    pub name: String,
    pub description: String,
    #[serde(rename = "inputSchema")]
    pub input_schema: Value,
    /// The schema the tool's `structuredContent` conforms to (listed from
    /// protocol 2025-06-18 on).
    #[serde(
        rename = "outputSchema",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub output_schema: Option<Value>,
    /// The tool's role: one of the spec's `McpToolCategory` values.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    /// Where the tool comes from: `core`, or the contributing extension's
    /// name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    /// MCP `ToolAnnotations` (`readOnlyHint`, ...): what the tool does to
    /// its environment. A core tool's derive from its effect; an extension
    /// tool's say it only reads, since the host grants an extension no
    /// capability.
    pub annotations: Value,
}

/// A resource as `resources/list` lists it (MCP `Resource`: `mimeType` on
/// the wire).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpResourceDescriptor {
    pub uri: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpPromptDescriptor {
    pub name: String,
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arguments: Option<Vec<McpPromptArgument>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpPromptArgument {
    pub name: String,
    pub description: String,
    pub required: bool,
}
