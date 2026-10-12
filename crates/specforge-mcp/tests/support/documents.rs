//! The JSON Schema documents MCP sends, for the tests that hold them all to
//! one rule: every core tool's reply, and every other document the server
//! sends ([`specforge_mcp::message::documents`]).

use serde_json::Value;

/// One document's schema and the paths of it that are open values.
pub struct Document {
    /// Whose it is: a tool's name, or the spec type of a message.
    pub owner: String,
    pub schema: &'static Value,
    pub open_values: &'static [&'static str],
}

/// Every document the server sends besides a tool reply: the messages,
/// then each core prompt's payload.
pub fn sent() -> Vec<Document> {
    let messages = specforge_mcp::message::documents()
        .into_iter()
        .map(|document| Document {
            owner: document.spec.to_string(),
            schema: document.schema,
            open_values: document.open_values,
        });
    let prompts = specforge_mcp::prompts::CORE_PROMPTS
        .iter()
        .map(|prompt| Document {
            owner: prompt.spec_payload().to_string(),
            schema: prompt.payload_schema(),
            open_values: prompt.open_values(),
        });
    messages.chain(prompts).collect()
}

/// Every core tool's reply (those with an outputSchema), then every
/// document the server sends besides.
pub fn all() -> Vec<Document> {
    let tools = specforge_mcp::tools::CORE_TOOLS.iter().filter_map(|tool| {
        tool.output_schema().map(|schema| Document {
            owner: tool.name().to_string(),
            schema,
            open_values: tool.open_values(),
        })
    });
    tools.chain(sent()).collect()
}
