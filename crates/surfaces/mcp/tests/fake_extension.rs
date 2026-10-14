//! One extension, `@test/cmds`, served in process (`InProcessRuntime`): it
//! declares, as given (`raw_category`), surfaces — two CLI commands
//! (`report`, `check`), an explicit MCP tool named `specforge.cmds.check`
//! (the name `check` would be auto-promoted to), and one MCP resource —
//! and any commands, MCP tools, MCP resources and compiler passes a test
//! adds. Every export call is recorded; an
//! export answers its configured output, else the guest's own "unknown
//! export" error, so a test sees which export a tool call reached and with
//! what input.

use serde_json::{Value, json};
use specforge_extension_sdk::{ContributionsBuilder, DeclaredCategory, ExtensionMeta};
use specforge_mcp::McpServer;

use crate::support::{Logged, Serve, TestProject, logged};
use specforge_wasm::runtime::{WasmCallResult, WasmRuntime};
use specforge_wasm::testing::InProcessRuntime;
use std::sync::{Arc, OnceLock};

pub const EXT: &str = "@test/cmds";

/// One recorded export call: `(extension, export, input as JSON)`.
pub type Call = (String, String, Value);

pub struct FakeExtension {
    outputs: Vec<(String, Vec<u8>)>,
    /// Exports whose call panics in the runtime, as a broken host function
    /// would.
    panics: Vec<String>,
    /// The compiler passes `@test/cmds` declares (`__describe passes`).
    passes: Value,
    /// Commands declared beside `report` and `check`.
    commands: Vec<Value>,
    /// MCP tools declared beside `specforge.cmds.check`.
    tools: Vec<Value>,
    /// MCP resources declared beside `cmds-summary`.
    resources: Vec<Value>,
    /// The surfaces declared before the added ones: [`Self::surfaces`]
    /// unless [`Self::declaring`] replaced them.
    base: Value,
    /// The runtime serving it, made on first use.
    runtime: OnceLock<Arc<InProcessRuntime>>,
}

impl FakeExtension {
    pub fn new() -> Self {
        Self {
            outputs: Vec::new(),
            panics: Vec::new(),
            passes: json!([]),
            commands: Vec::new(),
            tools: Vec::new(),
            resources: Vec::new(),
            base: Self::surfaces(),
            runtime: OnceLock::new(),
        }
    }

    /// `@test/cmds` declaring `surfaces` (a `SurfaceDescriptor`) instead of
    /// [`Self::surfaces`]: the commands, tools and resources a test adds
    /// come after them.
    pub fn declaring(surfaces: Value) -> Self {
        let mut fake = Self::new();
        fake.base = surfaces;
        for list in ["commands", "mcp_tools", "mcp_resources"] {
            if fake.base.get(list).is_none() {
                fake.base[list] = json!([]);
            }
        }
        fake
    }

    /// Also declare `command` (a `CommandDescriptor`).
    pub fn with_command(mut self, command: Value) -> Self {
        self.commands.push(command);
        self
    }

    /// Also declare the MCP tool `tool` (an `McpToolDescriptor`), after
    /// `specforge.cmds.check`.
    pub fn with_tool(mut self, tool: Value) -> Self {
        self.tools.push(tool);
        self
    }

    /// Also declare the MCP resource `resource` (an
    /// `McpResourceDescriptor`), after `cmds-summary`.
    pub fn with_resource(mut self, resource: Value) -> Self {
        self.resources.push(resource);
        self
    }

    /// Declare the compiler passes named `names`; each runs as the export
    /// `__pass_<name>`, answered from the configured outputs.
    pub fn with_passes(mut self, names: &[&str]) -> Self {
        self.passes = names.iter().map(|name| json!({"name": name})).collect();
        self
    }

    /// Answer calls to `export` with `output`.
    pub fn with_output(mut self, export: &str, output: Value) -> Self {
        self.outputs
            .push((export.into(), serde_json::to_vec(&output).unwrap()));
        self
    }

    /// Make calls to `export` panic in the runtime.
    pub fn with_panic(mut self, export: &str) -> Self {
        self.panics.push(export.into());
        self
    }

    /// How many times an environment loaded the extension so far
    /// (`__handshake` calls).
    pub fn handshakes(&self) -> usize {
        self.runtime()
            .calls()
            .iter()
            .filter(|c| c.export == "__handshake")
            .count()
    }

    /// The runtime serving `@test/cmds` as configured.
    pub fn runtime(&self) -> Arc<InProcessRuntime> {
        Arc::clone(self.runtime.get_or_init(|| Arc::new(self.serve())))
    }

    fn serve(&self) -> InProcessRuntime {
        let mut surfaces = self.base.clone();
        surfaces["commands"]
            .as_array_mut()
            .unwrap()
            .extend(self.commands.iter().cloned());
        surfaces["mcp_tools"]
            .as_array_mut()
            .unwrap()
            .extend(self.tools.iter().cloned());
        surfaces["mcp_resources"]
            .as_array_mut()
            .unwrap()
            .extend(self.resources.iter().cloned());
        let passes = self.passes.clone();
        let mut runtime = InProcessRuntime::new().with(move || {
            let mut c = ContributionsBuilder::new(ExtensionMeta::new(EXT, "0.1.0"));
            c.raw_category(DeclaredCategory::Surfaces, json!([surfaces.clone()]));
            if passes.as_array().is_some_and(|p| !p.is_empty()) {
                c.raw_category(DeclaredCategory::Passes, passes.clone());
            }
            c
        });
        for (export, output) in &self.outputs {
            runtime = runtime.answer_raw(EXT, export, WasmCallResult::Ok(output.clone()));
        }
        for export in &self.panics {
            runtime = runtime.fault(EXT, export);
        }
        runtime
    }

    /// Every non-protocol export call so far, oldest first.
    pub fn calls(&self) -> Vec<Call> {
        self.runtime()
            .calls()
            .into_iter()
            .filter(|c| c.export != "__handshake" && c.export != "__describe")
            .map(|c| (c.extension, c.export, c.input))
            .collect()
    }

    /// The surfaces `@test/cmds` declares.
    pub fn surfaces() -> Value {
        json!({
            "commands": [
                {
                    "id": "report",
                    "title": "Report",
                    "description": "Write a coverage report",
                    "export": "cmd__report",
                    "args": [
                        {"name": "style", "arg_type": {"enum": {"values": ["md", "json"]}}, "required": true, "description": "Output style"},
                        {"name": "verbose", "arg_type": "bool"},
                        {"name": "limit", "arg_type": "integer"},
                        {"name": "out", "arg_type": "path"}
                    ]
                },
                {
                    "id": "check",
                    "title": "Check",
                    "description": "Check the project",
                    "export": "cmd__check",
                    "args": []
                }
            ],
            "mcp_tools": [
                {
                    "name": "specforge.cmds.check",
                    "description": "Explicit check tool",
                    "export": "mcp__check",
                    "input_schema": {"type": "object", "properties": {"strict": {"type": "boolean"}}},
                    "output_schema": {"type": "object", "properties": {"checked": {"type": "boolean"}}}
                }
            ],
            "mcp_resources": [
                {
                    "uri_template": "specforge://ext/cmds/summary",
                    "name": "cmds-summary",
                    "export": "mcp__summary",
                    "mime_type": "application/json"
                }
            ]
        })
    }
}

/// A project on disk that enables `@test/cmds`, with one spec file.
pub fn project() -> tempfile::TempDir {
    let dir = cmds_project().into_dir();
    // The runtime serves it; the project has it installed.
    specforge_installed::testing::install(dir.path(), &[EXT]);
    dir
}

/// [`project`], as the fixture serves it.
fn cmds_project() -> TestProject {
    TestProject::new()
        .config(|c| c["name"] = json!("cmds"))
        .enabling(&[EXT])
        .file("main.spec", "")
}

/// A server whose extensions run in `ext`, its events logged; not yet
/// initialized.
pub fn server_with(ext: &Arc<FakeExtension>) -> Logged {
    logged(McpServer::new().with_extension_runtime(ext.runtime() as Arc<dyn WasmRuntime>))
}

/// A server initialized over [`project`] with `ext` as its runtime.
pub fn initialized(ext: FakeExtension) -> (Logged, Arc<FakeExtension>, tempfile::TempDir) {
    let ext = Arc::new(ext);
    let (server, log, project) = cmds_project()
        .serve_in(ext.runtime() as Arc<dyn WasmRuntime>)
        .into_parts();
    (Logged { server, log }, ext, project.into_dir())
}
