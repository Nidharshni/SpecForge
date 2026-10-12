//! The listings: `tools/list`, `resources/list`,
//! `resources/templates/list` and `prompts/list`. Each is the core table
//! (`CORE_TOOLS`, `CORE_RESOURCES`, `CORE_PROMPTS`) then, for tools and
//! resources, the served project's extension surface table (ADR 0017): what
//! is listed is exactly what a call dispatches under that name.

use serde_json::{Value, json};

use crate::events::McpEvent;
use crate::lifecycle::Revision;
use crate::protocol::JsonRpcResponse;
use crate::resources::{CORE_RESOURCES, ResourceSpec};
use crate::state::McpState;
use crate::surface_table::{ResourceEntry, ToolEntry};
use crate::tools::CORE_TOOLS;
use crate::types::{McpResourceDescriptor, McpToolDescriptor};

/// Every tool the server lists, in order: the core tools, then the
/// extension tools.
pub fn listed_tools(state: &McpState) -> impl Iterator<Item = McpToolDescriptor> + '_ {
    CORE_TOOLS
        .iter()
        .map(|tool| tool.descriptor())
        .chain(state.surfaces().tools().iter().map(ToolEntry::descriptor))
}

/// Every resource the server lists (templated or not), in order: the core
/// resources, then the extension resources.
pub fn listed_resources(state: &McpState) -> impl Iterator<Item = McpResourceDescriptor> + '_ {
    CORE_RESOURCES.iter().map(ResourceSpec::descriptor).chain(
        state
            .surfaces()
            .resources()
            .iter()
            .map(ResourceEntry::descriptor),
    )
}

pub fn handle_list_tools(
    state: &mut McpState,
    revision: Revision,
    id: Option<Value>,
) -> JsonRpcResponse {
    // outputSchema came with structuredContent, in 2025-06-18.
    let structured = revision.sends_structured_content();
    let tools: Vec<Value> = listed_tools(state)
        .map(|t| {
            let mut tool = serde_json::to_value(t).unwrap();
            if !structured && let Some(listed) = tool.as_object_mut() {
                listed.remove("outputSchema");
            }
            tool
        })
        .collect();
    push_discovery(state, "tools", tools.len());
    JsonRpcResponse::success(id, json!({ "tools": tools }))
}

pub fn handle_list_resources(state: &mut McpState, id: Option<Value>) -> JsonRpcResponse {
    let resources: Vec<Value> = listed_resources(state)
        .filter(|r| !is_template(r))
        .map(|r| serde_json::to_value(r).unwrap())
        .collect();
    push_discovery(state, "resources", resources.len());
    JsonRpcResponse::success(id, json!({ "resources": resources }))
}

/// MCP `resources/templates/list`: the resources whose URI is a template,
/// as `ResourceTemplate`s.
pub fn handle_list_resource_templates(state: &mut McpState, id: Option<Value>) -> JsonRpcResponse {
    let templates: Vec<Value> = listed_resources(state)
        .filter(is_template)
        .map(|r| {
            let mut template = json!({ "uriTemplate": r.uri, "name": r.name });
            if let Some(description) = &r.description {
                template["description"] = json!(description);
            }
            if let Some(mime_type) = &r.mime_type {
                template["mimeType"] = json!(mime_type);
            }
            template
        })
        .collect();
    push_discovery(state, "resource_templates", templates.len());
    JsonRpcResponse::success(id, json!({ "resourceTemplates": templates }))
}

/// Whether a resource's URI is an RFC 6570 template (`{placeholder}`).
fn is_template(resource: &McpResourceDescriptor) -> bool {
    resource.uri.contains('{')
}

pub fn handle_list_prompts(state: &mut McpState, id: Option<Value>) -> JsonRpcResponse {
    // The core prompts, derived from their table: no extension declares a
    // prompt.
    let prompts: Vec<Value> = crate::prompts::descriptors()
        .iter()
        .map(|p| serde_json::to_value(p).unwrap())
        .collect();
    push_discovery(state, "prompts", prompts.len());
    JsonRpcResponse::success(id, json!({ "prompts": prompts }))
}

/// Record a listing: which registry, and how many entries the client got.
fn push_discovery(state: &mut McpState, discovery_type: &str, result_count: usize) {
    state.events.record(McpEvent::new(
        "mcp_discovery_invoked",
        json!({"discoveryType": discovery_type, "resultCount": result_count}),
    ));
}
