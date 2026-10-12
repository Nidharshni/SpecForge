//! The listings: `tools/list`, `resources/list`,
//! `resources/templates/list` and `prompts/list`. Each is the core table
//! (`CORE_TOOLS`, `CORE_RESOURCES`, `CORE_PROMPTS`) then, for tools and
//! resources, the served project's extension surface table (ADR 0017): what
//! is listed is exactly what a call dispatches under that name.

use serde::Serialize;
use serde_json::{Value, json};
use specforge_common::shape::Shape;

use crate::events::{Listing, McpEvent};
use crate::lifecycle::Revision;
use crate::protocol::JsonRpcResponse;
use crate::resources::{CORE_RESOURCES, ResourceSpec};
use crate::state::McpState;
use crate::surface_table::ResourceEntry;
use crate::tool::Category;
use crate::tools::CORE_TOOLS;

/// A tool as `tools/list` describes it (MCP `Tool`, with SpecForge's
/// `category` and `source`).
#[derive(Debug, Clone, Serialize, Shape)]
#[serde(rename_all = "camelCase")]
pub struct ToolDescriptor<'a> {
    pub name: String,
    pub description: String,
    /// The JSON Schema of its arguments: a core tool's derived (ADR 0033),
    /// an extension's as declared. An open value.
    pub input_schema: Value,
    /// Its outputSchema: listed from 2025-06-18, for a tool with one. An
    /// open value.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_schema: Option<&'a Value>,
    /// The tool's role, never where it comes from.
    pub category: Category,
    /// `core` for built-in tools, the extension's name for contributed ones.
    pub source: String,
    pub annotations: ToolAnnotations,
}

/// MCP's `ToolAnnotations`, derived from a core tool's effect; an
/// extension tool's say it only reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Shape)]
#[serde(rename_all = "camelCase")]
pub struct ToolAnnotations {
    pub read_only_hint: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub destructive_hint: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub idempotent_hint: Option<bool>,
    pub open_world_hint: bool,
}

/// A resource as `resources/list` lists it (MCP `Resource`).
#[derive(Debug, Clone, Serialize, Shape)]
#[serde(rename_all = "camelCase")]
pub struct ResourceDescriptor {
    pub uri: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub mime_type: String,
}

/// A templated resource as `resources/templates/list` lists it (MCP
/// `ResourceTemplate`).
#[derive(Debug, Clone, Serialize, Shape)]
#[serde(rename_all = "camelCase")]
pub struct ResourceTemplateDescriptor {
    pub uri_template: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub mime_type: String,
}

/// A prompt as `prompts/list` lists it.
#[derive(Debug, Clone, Serialize, Shape)]
pub struct PromptDescriptor {
    pub name: String,
    pub description: String,
    pub arguments: Vec<PromptArgument>,
}

/// One argument of a listed prompt.
#[derive(Debug, Clone, Serialize, Shape)]
pub struct PromptArgument {
    pub name: String,
    pub description: String,
    pub required: bool,
}

/// A listed resource: plain, or templated when its URI holds a `{`
/// placeholder (an RFC 6570 template).
#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
pub enum Listed {
    Resource(ResourceDescriptor),
    Template(ResourceTemplateDescriptor),
}

impl Listed {
    /// The listing of the resource served at `uri`.
    pub fn of(uri: &str, name: &str, description: Option<&str>, mime_type: &str) -> Self {
        let (name, description, mime_type) = (
            name.to_string(),
            description.map(str::to_string),
            mime_type.to_string(),
        );
        if uri.contains('{') {
            Listed::Template(ResourceTemplateDescriptor {
                uri_template: uri.to_string(),
                name,
                description,
                mime_type,
            })
        } else {
            Listed::Resource(ResourceDescriptor {
                uri: uri.to_string(),
                name,
                description,
                mime_type,
            })
        }
    }
}

/// Every tool the server lists under `revision`, in order: the core tools,
/// then the extension tools. An outputSchema came with structured content,
/// in 2025-06-18: no tool lists one before.
pub fn listed_tools<'s>(
    state: &'s McpState,
    revision: Revision,
) -> impl Iterator<Item = ToolDescriptor<'s>> + 's {
    CORE_TOOLS
        .iter()
        .map(move |tool| tool.descriptor(revision))
        .chain(
            state
                .surfaces()
                .tools()
                .iter()
                .map(move |tool| tool.descriptor(revision)),
        )
}

/// Every resource the server lists (templated or not), in order: the core
/// resources, then the extension resources.
pub fn listed_resources(state: &McpState) -> impl Iterator<Item = Listed> + '_ {
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
    let tools = serde_json::to_value(listed_tools(state, revision).collect::<Vec<_>>()).unwrap();
    let count = tools.as_array().map_or(0, Vec::len);
    state.events.record(McpEvent::McpDiscoveryInvoked {
        discovery_type: Listing::Tools,
        result_count: count,
    });
    JsonRpcResponse::success(id, json!({ "tools": tools }))
}

pub fn handle_list_resources(state: &mut McpState, id: Option<Value>) -> JsonRpcResponse {
    let resources: Vec<Value> = listed_resources(state)
        .filter(|r| matches!(r, Listed::Resource(_)))
        .map(|r| serde_json::to_value(r).unwrap())
        .collect();
    state.events.record(McpEvent::McpDiscoveryInvoked {
        discovery_type: Listing::Resources,
        result_count: resources.len(),
    });
    JsonRpcResponse::success(id, json!({ "resources": resources }))
}

/// MCP `resources/templates/list`: the resources whose URI is a template,
/// as `ResourceTemplate`s.
pub fn handle_list_resource_templates(state: &mut McpState, id: Option<Value>) -> JsonRpcResponse {
    let templates: Vec<Value> = listed_resources(state)
        .filter(|r| matches!(r, Listed::Template(_)))
        .map(|r| serde_json::to_value(r).unwrap())
        .collect();
    state.events.record(McpEvent::McpDiscoveryInvoked {
        discovery_type: Listing::ResourceTemplates,
        result_count: templates.len(),
    });
    JsonRpcResponse::success(id, json!({ "resourceTemplates": templates }))
}

pub fn handle_list_prompts(state: &mut McpState, id: Option<Value>) -> JsonRpcResponse {
    // The core prompts, derived from their table: no extension declares a
    // prompt.
    let prompts: Vec<Value> = crate::prompts::CORE_PROMPTS
        .iter()
        .map(|p| serde_json::to_value(p.descriptor()).unwrap())
        .collect();
    state.events.record(McpEvent::McpDiscoveryInvoked {
        discovery_type: Listing::Prompts,
        result_count: prompts.len(),
    });
    JsonRpcResponse::success(id, json!({ "prompts": prompts }))
}
