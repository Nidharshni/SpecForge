//! An initialized server over a test project.

use std::ops::{Deref, DerefMut};
use std::path::Path;
use std::sync::Arc;

use serde_json::{Value, json};
use specforge_project::SharedRuntime;
use specforge_project::testing::TestProject;
use specforge_wasm::testing::RecordedCall;

use super::{EventLog, rpc};
use crate::McpServer;

/// The solver every served test project's doctor sees: z3 runs (ADR 0079 D5), whatever is on the PATH.
fn z3_ready() -> Arc<dyn specforge_ops::solver::Solver> {
    Arc::new(specforge_ops::testing::ScriptedSolver::ready())
}

/// An initialized server over a [`TestProject`]: what a client of `specforge mcp <root>` talks to. Dereferences
/// to the server.
pub struct Served {
    server: McpServer,
    /// What the server recorded.
    log: EventLog,
    project: TestProject,
}

impl Served {
    /// As `specforge mcp <root>` serves the project: the server `specforge_cli::mcp::Args::run` builds
    /// (`McpServer::with_project_root(root)`), then the client's `initialize` with no `projectRoot`. Its extensions
    /// load in the project's own component runtime. The cross-surface tests compare the CLI with this.
    pub fn rooted(project: TestProject) -> Self {
        Self::rooted_with(project, json!({}))
    }

    /// [`Self::rooted`], the client's `initialize` naming `protocol_version` (`"2025-06-18"`: the first revision
    /// with structured content).
    pub fn rooted_at(project: TestProject, protocol_version: &str) -> Self {
        Self::rooted_with(project, json!({"protocolVersion": protocol_version}))
    }

    fn rooted_with(project: TestProject, params: Value) -> Self {
        let server =
            McpServer::with_project_root(project.root().to_path_buf()).with_solver(z3_ready());
        Self::initialize(project, server, params, false)
    }

    /// A server whose `initialize` names the project (`projectRoot`), its extensions in the project's served
    /// runtime ([`TestProject::serving`]), else its own component runtime. MCP's own tests start here.
    pub fn initialized(project: TestProject) -> Self {
        let server = Self::server_for(&project);
        Self::over(project, server)
    }

    /// [`Self::initialized`], the `initialize` also naming `protocol_version`.
    pub fn initialized_at(project: TestProject, protocol_version: &str) -> Self {
        let server = Self::server_for(&project);
        let params = json!({"protocolVersion": protocol_version});
        Self::initialize(project, server, params, true)
    }

    /// [`Self::initialized`], its extensions running in `runtime` (`McpServer::with_extension_runtime`). What an
    /// in-process runtime serves, the project has installed: it loads through the production path.
    pub fn initialized_in(project: TestProject, runtime: SharedRuntime) -> Self {
        let project = project.installing_enabled();
        Self::over(
            project,
            McpServer::new()
                .with_solver(z3_ready())
                .with_extension_runtime(runtime),
        )
    }

    fn server_for(project: &TestProject) -> McpServer {
        let server = McpServer::new().with_solver(z3_ready());
        match project.served() {
            Some(served) => server.with_extension_runtime(Arc::clone(served) as SharedRuntime),
            None => server,
        }
    }

    /// `server` initialized over `project`, which the `initialize` names (`projectRoot`).
    pub fn over(project: TestProject, server: McpServer) -> Self {
        Self::initialize(project, server, json!({}), true)
    }

    fn initialize(project: TestProject, server: McpServer, params: Value, name_root: bool) -> Self {
        let log = EventLog::new();
        let mut server = server.with_events(log.clone());
        let mut params = params;
        if name_root {
            params["projectRoot"] = json!(project.root().to_str().expect("a UTF-8 root"));
        }
        let reply = rpc::call(&mut server, "initialize", params);
        assert!(reply["error"].is_null(), "initialize: {reply}");
        assert!(server.state().session().project().root().is_some());
        Served {
            server,
            log,
            project,
        }
    }

    pub fn project(&self) -> &TestProject {
        &self.project
    }

    pub fn root(&self) -> &Path {
        self.project.root()
    }

    /// Write `text` at `path` under the root: the next request that reads the project brings the served session
    /// up to date (ADR 0014), as an edit in a client's editor would.
    pub fn write(&self, path: &str, text: &str) {
        self.project.write(path, text);
    }

    /// Remove `path` under the root.
    pub fn remove(&self, path: &str) {
        self.project.remove(path);
    }

    /// What the server recorded.
    pub fn log(&self) -> &EventLog {
        &self.log
    }

    /// The server, its log, and the project, apart (the surface tests' `fake_extension::initialized`).
    pub fn into_parts(self) -> (McpServer, EventLog, TestProject) {
        (self.server, self.log, self.project)
    }

    /// The extension calls the served runtime answered, oldest first, handshakes and describes left out (empty
    /// when the project serves none).
    pub fn extension_calls(&self) -> Vec<RecordedCall> {
        self.project
            .served()
            .map(|runtime| runtime.calls())
            .unwrap_or_default()
            .into_iter()
            .filter(|c| c.export != "__handshake" && c.export != "__describe")
            .collect()
    }

    /// `tools/call` `name` with `arguments`: the reply.
    pub fn call_tool(&mut self, name: &str, arguments: Value) -> Value {
        rpc::call_tool(&mut self.server, name, arguments)
    }

    /// [`Self::call_tool`], its text content parsed as JSON.
    pub fn tool(&mut self, name: &str, arguments: Value) -> Value {
        rpc::tool(&mut self.server, name, arguments)
    }
}

impl Deref for Served {
    type Target = McpServer;

    fn deref(&self) -> &McpServer {
        &self.server
    }
}

impl DerefMut for Served {
    fn deref_mut(&mut self) -> &mut McpServer {
        &mut self.server
    }
}
