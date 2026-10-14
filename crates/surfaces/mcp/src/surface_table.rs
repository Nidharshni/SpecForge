//! The extension surface table: what MCP serves from the served project's
//! extensions (ADR 0017).
//!
//! Built once whenever the project's environment loads, from the
//! extensions' declarations (`RegistryBuild::declarations`), it is the only
//! authority on what MCP lists and dispatches beside the core tables:
//! `tools/list` is [`CORE_TOOLS`](crate::tools::CORE_TOOLS) then
//! [`ExtensionSurfaceTable::tools`], and a `tools/call` naming no core tool
//! is one lookup in it ([`ExtensionSurfaceTable::tool`]); resources alike.
//! Each name is served once, the first in this order: core tools, explicit
//! extension tools in extension load order, then extension commands
//! (`specforge_ops::command::ExtensionCommands`, load then declaration
//! order). A contribution not served under its name is reported with I017
//! saying why, and so is a command the host refuses or shadows.
//!
//! Nothing here knows how a command's schema is derived or how its tool is
//! named: that is `specforge_ops::command::ExtensionCommand`'s. Nothing
//! here calls an extension either: dispatch is `extension_dispatch`'s,
//! over the `WasmRuntime` seam.

use serde_json::Value;
use specforge_common::{Code, Diagnostic, codes};
use specforge_ops::command::{ExtensionCommand, ExtensionCommands};
use specforge_registry::RegistryBuild;

use crate::lifecycle::Revision;
use crate::registry::{Listed, ToolDescriptor};
use crate::resources::ResourceSpec;
use crate::tool::{Category, ToolGroup, ToolSpec, read_only_annotations};

/// The code reporting an extension contribution MCP does not serve under
/// its name.
const NOT_SERVED: Code = codes::I017;

/// What MCP serves from the served project's extensions.
#[derive(Debug, Clone, Default)]
pub struct ExtensionSurfaceTable {
    /// In listing order.
    tools: Vec<ToolEntry>,
    /// In listing order: extension load order.
    resources: Vec<ResourceEntry>,
    /// I017 for each contribution not served under its name, saying why.
    diagnostics: Vec<Diagnostic>,
    stats: PromotionStats,
}

/// How the project's commands became tools: the `commands_auto_promoted`
/// event's counts.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PromotionStats {
    /// Commands the extensions declare (shadowed ones included).
    pub declared: usize,
    /// Commands served as tools.
    pub promoted: usize,
    /// Commands not served as tools (I017): a name taken, refused, shadowed.
    pub conflicts: usize,
}

/// One extension tool MCP serves.
#[derive(Debug, Clone, PartialEq)]
pub struct ToolEntry {
    pub name: String,
    /// The contributing extension: the tool's `source`.
    pub extension: String,
    pub description: String,
    /// The group it is listed in: an explicit tool's declared category when
    /// it names one (`core`, `navigation`, `management`), else core; a
    /// command's is core (its own category is a CLI grouping). Never
    /// `mutation`: an extension holds no capability to write.
    pub group: ToolGroup,
    pub kind: ToolKind,
}

/// What a tool call runs.
#[derive(Debug, Clone, PartialEq)]
pub enum ToolKind {
    /// An explicit MCP tool: its `mcp__` export, with the schemas it
    /// declares (its input checked before the export runs, its output
    /// after).
    McpTool {
        export: String,
        input_schema: Value,
        output_schema: Option<Value>,
    },
    /// An extension command: its `cmd__` export, with the args its
    /// derivation normalizes.
    Command(Box<ExtensionCommand>),
}

/// One extension resource MCP serves.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResourceEntry {
    /// The URIs it serves: an RFC 6570 template, or one URI.
    pub uri_template: String,
    pub name: String,
    pub description: Option<String>,
    pub mime_type: String,
    pub extension: String,
    pub export: String,
}

impl ToolEntry {
    /// The tool as `tools/list` describes it.
    pub fn descriptor(&self, revision: Revision) -> ToolDescriptor<'_> {
        let input_schema = match &self.kind {
            ToolKind::McpTool { input_schema, .. } => input_schema.clone(),
            ToolKind::Command(command) => command.input_schema(),
        };
        ToolDescriptor {
            name: self.name.clone(),
            description: self.description.clone(),
            input_schema,
            output_schema: self
                .output_schema()
                .filter(|_| revision.sends_structured_content()),
            category: Category::from(self.group),
            source: self.extension.clone(),
            annotations: read_only_annotations(),
        }
    }

    /// The schema its structured output conforms to: an explicit tool's
    /// declared one; a command declares none.
    pub fn output_schema(&self) -> Option<&Value> {
        match &self.kind {
            ToolKind::McpTool { output_schema, .. } => output_schema.as_ref(),
            ToolKind::Command(_) => None,
        }
    }
}

impl ResourceEntry {
    /// The resource as `resources/list` (or `resources/templates/list`)
    /// describes it.
    pub fn descriptor(&self) -> Listed {
        Listed::of(
            &self.uri_template,
            &self.name,
            self.description.as_deref(),
            &self.mime_type,
        )
    }

    /// Whether its template names `uri`, by the one rule core resources
    /// follow too ([`crate::uri_template::names`]), whatever the scheme.
    pub fn names(&self, uri: &str) -> bool {
        crate::uri_template::names(&self.uri_template, uri)
    }
}

impl ExtensionSurfaceTable {
    /// The table of `registries`' declarations, beside `core_tools` and
    /// `core_resources`: explicit tools by extension load order, then
    /// commands; a name a core tool or an earlier entry has is not served,
    /// nor is a resource whose URIs a core resource or an earlier one
    /// serves; each with an I017 saying why.
    pub fn build(
        registries: &RegistryBuild,
        core_tools: &[&dyn ToolSpec],
        core_resources: &[ResourceSpec],
    ) -> Self {
        let mut table = ExtensionSurfaceTable::default();
        for declaration in registries.declarations() {
            let extension = declaration.name();
            for tool in &declaration.surfaces.mcp_tools {
                if let Some(why) = table.tool_name_taken(&tool.name, core_tools) {
                    table.not_served(format!(
                        "MCP tool '{}' of '{extension}' not served: {why}",
                        tool.name
                    ));
                    continue;
                }
                table.tools.push(ToolEntry {
                    name: tool.name.clone(),
                    extension: extension.to_string(),
                    description: tool.description.clone(),
                    group: tool
                        .category
                        .as_deref()
                        .and_then(ToolGroup::parse)
                        .unwrap_or(ToolGroup::Core),
                    kind: ToolKind::McpTool {
                        export: tool.export.clone(),
                        input_schema: tool.input_schema.clone(),
                        output_schema: tool.output_schema.clone(),
                    },
                });
            }
            for resource in &declaration.surfaces.mcp_resources {
                let entry = ResourceEntry {
                    uri_template: resource.uri_template.clone(),
                    name: resource.name.clone(),
                    description: resource.description.clone(),
                    mime_type: resource.mime_type.clone(),
                    extension: extension.to_string(),
                    export: resource.export.clone(),
                };
                let probe = crate::uri_template::probe(&entry.uri_template);
                let shadow = core_resources
                    .iter()
                    .find(|core| core.names(&probe))
                    .map(|core| format!("the core resource '{}' serves its URIs", core.uri))
                    .or_else(|| {
                        table
                            .resources
                            .iter()
                            .find(|earlier| earlier.names(&probe))
                            .map(|earlier| {
                                format!(
                                    "'{}' of '{}' serves its URIs",
                                    earlier.name, earlier.extension
                                )
                            })
                    });
                match shadow {
                    Some(why) => table.not_served(format!(
                        "MCP resource '{}' ({}) of '{extension}' not served: {why}",
                        entry.name, entry.uri_template
                    )),
                    None => table.resources.push(entry),
                }
            }
        }
        let commands = ExtensionCommands::build(registries);
        table.stats.declared = commands.all().len() + commands.shadowed().len();
        for command in commands.all() {
            let name = command.tool_name();
            let why = match command.refusal() {
                Some(refusal) => Some(format!("the host refuses it: {refusal}")),
                None => table.command_name_taken(&name, core_tools),
            };
            if let Some(why) = why {
                table.not_served(format!(
                    "command '{}' not auto-promoted: {why}",
                    command.id()
                ));
                table.stats.conflicts += 1;
                continue;
            }
            table.tools.push(ToolEntry {
                name,
                extension: command.extension().to_string(),
                description: command.description().to_string(),
                group: ToolGroup::Core,
                kind: ToolKind::Command(Box::new(command.clone())),
            });
            table.stats.promoted += 1;
        }
        for shadowed in commands.shadowed() {
            let winner = commands
                .all()
                .iter()
                .find(|c| c.short() == shadowed.short() && c.cli_name() == shadowed.cli_name())
                .map_or("", |c| c.extension());
            table.not_served(format!(
                "command '{}' of '{}' not auto-promoted: '{winner}' routes a command of that name first",
                shadowed.id(),
                shadowed.extension()
            ));
            table.stats.conflicts += 1;
        }
        table
    }

    /// The table of a server serving no extension.
    pub fn empty() -> Self {
        ExtensionSurfaceTable::default()
    }

    /// The extension tools, in listing order.
    pub fn tools(&self) -> &[ToolEntry] {
        &self.tools
    }

    /// The extension resources, in listing order.
    pub fn resources(&self) -> &[ResourceEntry] {
        &self.resources
    }

    /// The extension tool named `name`.
    pub fn tool(&self, name: &str) -> Option<&ToolEntry> {
        self.tools.iter().find(|tool| tool.name == name)
    }

    /// The first extension resource whose template names `uri` (core
    /// resources are matched before, by the caller).
    pub fn resource(&self, uri: &str) -> Option<&ResourceEntry> {
        self.resources.iter().find(|resource| resource.names(uri))
    }

    /// I017 for each contribution not served under its name.
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    /// How the project's commands became tools.
    pub fn stats(&self) -> PromotionStats {
        self.stats
    }

    /// Why an explicit tool named `name` is not served: a core tool or an
    /// earlier extension tool has the name.
    fn tool_name_taken(&self, name: &str, core_tools: &[&dyn ToolSpec]) -> Option<String> {
        if core_tools.iter().any(|core| core.name() == name) {
            return Some("a core tool has that name".to_string());
        }
        self.tool(name)
            .map(|earlier| format!("'{}' serves a tool of that name", earlier.extension))
    }

    /// Why a command whose tool would be `name` is not served: the name is
    /// a core tool's, or an explicit tool's.
    fn command_name_taken(&self, name: &str, core_tools: &[&dyn ToolSpec]) -> Option<String> {
        if core_tools.iter().any(|core| core.name() == name) {
            return Some(format!("core tool '{name}' already exists"));
        }
        self.tool(name)
            .map(|_| format!("explicit MCP tool '{name}' already exists"))
    }

    fn not_served(&mut self, message: String) {
        self.diagnostics.push(Diagnostic::new(NOT_SERVED, message));
    }
}
