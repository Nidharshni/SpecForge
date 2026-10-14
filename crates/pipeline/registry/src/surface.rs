//! The surfaces registry: which extension owns each declared command, MCP
//! tool and MCP resource (first registration wins).

use specforge_common::{Diagnostic, codes};
use specforge_protocol_types::SurfaceDescriptor;
use std::collections::HashMap;

/// The protocol's own type: one wire shape (`"string"`, `{"enum":
/// {"values": [..]}}`, ...) for every declared command argument.
pub use specforge_protocol_types::CommandArgType;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SurfaceType {
    Command,
    McpTool,
    McpResource,
}

#[derive(Debug, Clone)]
pub struct SurfaceRegistryEntry {
    pub surface_type: SurfaceType,
    pub contribution_name: String,
    pub extension_name: String,
    pub export_name: String,
}

/// Refuse each explicit MCP tool of `ext_name` whose `input_schema` or
/// `output_schema` is not a JSON object (E055): it is removed from
/// `surfaces`, so it is neither registered nor listed.
pub fn refuse_malformed_tool_schemas(
    ext_name: &str,
    surfaces: &mut SurfaceDescriptor,
) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    surfaces.mcp_tools.retain(|tool| {
        let malformed: Vec<&str> = [
            ("input_schema", Some(&tool.input_schema)),
            ("output_schema", tool.output_schema.as_ref()),
        ]
        .into_iter()
        .filter(|(_, schema)| schema.is_some_and(|s| !s.is_object()))
        .map(|(name, _)| name)
        .collect();
        for name in &malformed {
            diagnostics.push(
                Diagnostic::new(
                    codes::E055,
                    format!(
                        "MCP tool '{}' of extension '{}': {} must be a JSON object; the tool is not registered",
                        tool.name, ext_name, name
                    ),
                )
                .with_suggestion("declare the schema as a JSON Schema object".to_string()),
            );
        }
        malformed.is_empty()
    });
    diagnostics
}

/// Register the declared surfaces, extension by extension in load order
/// (first registration wins). Detects duplicate command IDs, MCP tool
/// names and MCP resource names across extensions (E039).
pub fn register_surface_contributions(
    declared: &[(String, SurfaceDescriptor)],
) -> (Vec<SurfaceRegistryEntry>, Vec<Diagnostic>) {
    let mut entries = Vec::new();
    let mut diagnostics = Vec::new();
    let mut seen_commands: HashMap<String, String> = HashMap::new();
    let mut seen_tools: HashMap<String, String> = HashMap::new();
    let mut seen_resources: HashMap<String, String> = HashMap::new();

    for (ext_name, surfaces) in declared {
        for cmd in &surfaces.commands {
            if let Some(first_ext) = seen_commands.get(&cmd.id) {
                diagnostics.push(Diagnostic::new(
                    codes::E039,
                    format!(
                        "duplicate surface command ID '{}': extension '{}' conflicts with '{}'",
                        cmd.id, ext_name, first_ext
                    ),
                ));
            } else {
                seen_commands.insert(cmd.id.clone(), ext_name.clone());
                entries.push(SurfaceRegistryEntry {
                    surface_type: SurfaceType::Command,
                    contribution_name: cmd.id.clone(),
                    extension_name: ext_name.clone(),
                    export_name: cmd.export.clone(),
                });
            }
        }

        for tool in &surfaces.mcp_tools {
            if let Some(first_ext) = seen_tools.get(&tool.name) {
                diagnostics.push(Diagnostic::new(
                    codes::E039,
                    format!(
                        "duplicate MCP tool name '{}': extension '{}' conflicts with '{}'",
                        tool.name, ext_name, first_ext
                    ),
                ));
            } else {
                seen_tools.insert(tool.name.clone(), ext_name.clone());
                entries.push(SurfaceRegistryEntry {
                    surface_type: SurfaceType::McpTool,
                    contribution_name: tool.name.clone(),
                    extension_name: ext_name.clone(),
                    export_name: tool.export.clone(),
                });
            }
        }

        for resource in &surfaces.mcp_resources {
            if let Some(first_ext) = seen_resources.get(&resource.name) {
                diagnostics.push(Diagnostic::new(
                    codes::E039,
                    format!(
                        "duplicate MCP resource name '{}': extension '{}' conflicts with '{}'",
                        resource.name, ext_name, first_ext
                    ),
                ));
            } else {
                seen_resources.insert(resource.name.clone(), ext_name.clone());
                entries.push(SurfaceRegistryEntry {
                    surface_type: SurfaceType::McpResource,
                    contribution_name: resource.name.clone(),
                    extension_name: ext_name.clone(),
                    export_name: resource.export.clone(),
                });
            }
        }
    }

    (entries, diagnostics)
}
