use specforge_common::{Diagnostic, ProjectConfig, ProjectRoot};
use specforge_graph::Graph;
use specforge_ops::analysis::collect::{CollectorMachine, ProcessMachine};
use specforge_ops::solver::{Solver, Z3};
use specforge_project::{
    DiagnosticsDelta, ProjectSession, RuntimeSource, SharedRuntime, Update, UpdateKind,
};
use specforge_registry::RegistryBuild;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::events::{Discard, EventSink, McpEvent};
use crate::lifecycle::Revision;
use crate::subscriptions::{Changes, Subscriptions};
use crate::surface_table::ExtensionSurfaceTable;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServerPhase {
    Uninitialized,
    Initialized,
    ShuttingDown,
}

pub struct McpState {
    pub(crate) phase: ServerPhase,
    /// The protocol revision `initialize` negotiated (the latest one the
    /// server speaks until then).
    pub(crate) protocol_version: &'static str,
    /// The served project: its root, environment (config, spec root,
    /// registries, rules, manifests, surfaces), graph, diagnostics and
    /// extension runtime: always opened from disk (ADR 0025). Detached
    /// while none is served.
    session: ProjectSession,
    /// How many times the served project changed: every update applied to
    /// it and every replacement bumps it ([`Self::session_generation`]).
    generation: u64,
    /// What MCP serves from the served project's extensions, built from
    /// their declarations whenever its environment loads.
    surfaces: ExtensionSurfaceTable,
    /// The server's root: the path `specforge mcp` was started with, replaced
    /// by a handshake client's `initialize` `projectRoot`. What is served is
    /// the nearest project at or above it; while none is served, every
    /// request looks there again ([`Self::follow_root`], ADR 0077 D3).
    pub(crate) root: Option<PathBuf>,
    /// Where the server's events go (ADR 0057): [`Discard`] unless the host
    /// gave another sink ([`crate::McpServer::with_events`]). Every module
    /// records through it.
    pub(crate) events: Box<dyn EventSink>,
    /// Who hears about what (ADR 0024 D6). Read through
    /// [`Self::subscriptions`]; changed only by the subscription requests,
    /// an update of the served project, cancellation, disconnect and
    /// shutdown.
    pub(crate) subscriptions: Subscriptions,
    /// The Wasm runtime extensions run in, when the host supplies one; by
    /// default the served project's session builds the project's runtime
    /// (`specforge_component::ComponentRuntime::with_user_cache`) each time it loads.
    pub(crate) extension_runtime: Option<SharedRuntime>,
    /// The SMT solver doctor checks: z3 from PATH unless the host gave
    /// another ([`crate::McpServer::with_solver`]).
    pub(crate) solver: Arc<dyn Solver>,
    /// The machine `specforge.collect` runs commands on and keeps approvals in: the user's unless the
    /// host gave another ([`crate::McpServer::with_collector_machine`], ADR 0125).
    pub(crate) collector_machine: Arc<dyn CollectorMachine>,
}

impl McpState {
    pub(crate) fn new() -> Self {
        Self {
            phase: ServerPhase::Uninitialized,
            protocol_version: crate::lifecycle::LATEST_PROTOCOL_VERSION,
            session: ProjectSession::detached(),
            generation: 0,
            surfaces: ExtensionSurfaceTable::empty(),
            root: None,
            events: Box::new(Discard),
            subscriptions: Subscriptions::new(),
            extension_runtime: None,
            solver: Arc::new(Z3::default()),
            collector_machine: Arc::new(ProcessMachine::user()),
        }
    }

    /// The server's root is `root` from now on (`initialize`): the project
    /// at or above it is served, replacing what was (a fresh compile); with
    /// none there, what is served stays.
    pub(crate) fn set_root(&mut self, root: Option<PathBuf>) {
        self.root = root;
        if let Some(project) = self.root_project() {
            self.serve(&project);
        }
    }

    /// While no project is served, serve the one at or above the server's
    /// root if one exists there now (a fresh compile): called once, by the router,
    /// before every session method in both revisions (ADR 0077 D3, ADR 0103
    /// D6), so what a request reads follows disk, not which request came first. Nothing while a project is
    /// served (bringing it up to date is [`Self::ensure_fresh`]'s).
    pub(crate) fn follow_root(&mut self) {
        if self.project_root().is_none()
            && let Some(project) = self.root_project()
        {
            self.serve(&project);
        }
    }

    /// The project at or above the server's root, canonical (ADR 0014 D6).
    fn root_project(&self) -> Option<ProjectRoot> {
        self.root
            .as_deref()
            .and_then(ProjectRoot::enclosing)
            .map(ProjectRoot::canonicalized)
    }

    /// Who hears about what.
    pub fn subscriptions(&self) -> &Subscriptions {
        &self.subscriptions
    }

    /// The served project's session.
    pub fn session(&self) -> &ProjectSession {
        &self.session
    }

    /// The served project's root: `None` while no project is served.
    pub fn project_root(&self) -> Option<&Path> {
        self.session.project().root()
    }

    /// How many times the served project changed since the server started:
    /// every update applied to it and every replacement counts once. A call
    /// that found nothing changed on disk leaves it as it was.
    pub fn session_generation(&self) -> u64 {
        self.generation
    }

    /// The served project's graph.
    pub fn graph(&self) -> &Graph {
        self.session.project().graph()
    }

    /// The served project's registries, rules, declarations and surfaces.
    /// What a call reads goes through its target's view
    /// ([`crate::target::Call::view`]); this is the served session's, for
    /// the server itself (the surface table, the lifecycle answers).
    pub fn registries(&self) -> &RegistryBuild {
        self.session.project().environment().registries()
    }

    /// The served project's `specforge.json`.
    pub fn config(&self) -> &ProjectConfig {
        self.session.project().environment().config()
    }

    /// Where the served project's `.spec` files live: spans are relative
    /// to it. None while no project on disk is served.
    pub fn spec_root(&self) -> Option<&Path> {
        self.session.inputs().spec_root()
    }

    /// Everything the server reports for the served project: what
    /// `specforge check` reports, then the contributions of its extensions
    /// MCP does not serve under their names (I017).
    pub fn diagnostics(&self) -> Vec<Diagnostic> {
        let mut diagnostics = self.session.project().diagnostics();
        diagnostics.extend(self.surfaces.diagnostics().iter().cloned());
        diagnostics
    }

    /// What MCP serves from the served project's extensions: listed after
    /// the core tools and resources, and looked up by a call that names no
    /// core one.
    pub fn surfaces(&self) -> &ExtensionSurfaceTable {
        &self.surfaces
    }

    /// The protocol revision `initialize` negotiated (the latest the server speaks until then).
    pub fn protocol_version(&self) -> &'static str {
        self.protocol_version
    }

    /// The revision a request without `_meta` is served under: the one
    /// `initialize` negotiated.
    pub fn negotiated(&self) -> Revision {
        Revision::Negotiated(self.protocol_version)
    }

    /// Whether `initialize` was answered (and shutdown not asked for).
    pub fn is_initialized(&self) -> bool {
        self.phase == ServerPhase::Initialized
    }

    /// Whether the session accepts JSON-RPC batches: only a 2025-03-26
    /// session does, since later revisions removed batching.
    pub fn accepts_batches(&self) -> bool {
        self.phase == ServerPhase::Initialized
            && self.protocol_version == crate::lifecycle::BATCHING_PROTOCOL_VERSION
    }

    /// Serve the project at `root` as it is on disk now, its config and
    /// extensions included: the served session is reopened at `root`
    /// (`ProjectSession::reopen`), a fresh compile of what is on disk,
    /// whether it was the project served or another. The graph,
    /// diagnostics, registries, config and runtime change together, with
    /// the session; the extension surface table is built again from this
    /// project's declarations, so nothing a previous load contributed
    /// survives, and nothing is listed twice.
    /// Subscribed clients learn what changed. The one place the served
    /// project is replaced (the server's root, adopting a call's path, the
    /// directory `init` created). Only a project (a directory holding
    /// `specforge.json`) is ever served.
    pub(crate) fn serve(&mut self, root: &ProjectRoot) {
        debug_assert!(
            root.is_configured(),
            "only a project is served: {} holds no specforge.json",
            root.dir().display()
        );
        let update = self.session.reopen(root, self.source(), |_| {});
        self.applied(update);
    }

    /// Where a session MCP opens gets its extension runtime: the host's
    /// when it gave one, else the project's own, built for each environment
    /// load (ADR 0030 D3).
    fn source(&self) -> RuntimeSource {
        match &self.extension_runtime {
            Some(runtime) => RuntimeSource::Fixed(Some(Arc::clone(runtime))),
            None => RuntimeSource::project(),
        }
    }

    /// A session of the project at `root`, its extensions running as
    /// [`Self::source`] says: how another project a call names is opened.
    pub(crate) fn open(&self, root: &ProjectRoot) -> ProjectSession {
        ProjectSession::open_from(root, self.source())
    }

    /// Bring the served project up to date with disk (behavior
    /// `bring_session_up_to_date`): exactly what changed since it was last
    /// built is applied, an update for changed sources, an environment
    /// reload (its extension tools and resources registered again) for a
    /// changed `specforge.json`, `specforge.lock` or extension module, a
    /// re-check for a changed check input. Subscribed clients learn what
    /// changed. `None` when nothing did.
    pub(crate) fn ensure_fresh(&mut self) {
        if let Some(update) = self.session.ensure_fresh() {
            self.applied(update);
        }
    }

    /// Record `update`, applied to the served session: the extension
    /// surface table is built again when its environment loaded again, and
    /// subscribed clients learn what changed: the update's diagnostics
    /// delta, then the I017 the rebuilt table added and removed, computed
    /// only when someone hears the diagnostics (ADR 0024 D6, ADR 0074).
    fn applied(&mut self, update: Update) {
        // Every session verifies its updates in a debug build (ADR 0035):
        // a divergence from a cold rebuild is a bug, and this is the one
        // place every update of the served project passes.
        assert_converged(&update, "the served project");
        self.generation += 1;
        let replaced = (update.kind == UpdateKind::Environment).then(|| {
            let rebuilt = ExtensionSurfaceTable::build(
                self.registries(),
                crate::tools::CORE_TOOLS,
                crate::resources::CORE_RESOURCES,
            );
            std::mem::replace(&mut self.surfaces, rebuilt)
        });
        if replaced.is_some() {
            let stats = self.surfaces.stats();
            if stats.declared > 0 {
                self.events.record(McpEvent::CommandsAutoPromoted {
                    promoted_count: stats.promoted,
                    conflict_count: stats.conflicts,
                });
            }
        }
        // Subscribers hear what changed (ADR 0024 D6), here because every
        // update of the served project passes here (ADR 0035 D3).
        let diagnostics = self.subscriptions.hears_diagnostics().then(|| {
            let mut delta = update.diagnostics_delta();
            if let Some(previous) = &replaced {
                delta.append(DiagnosticsDelta::between(
                    previous.diagnostics(),
                    self.surfaces.diagnostics(),
                ));
            }
            delta
        });
        self.subscriptions
            .updated(&Changes::of(&update, diagnostics), &mut *self.events);
    }

    /// Every subscription and stream ends, each recorded; the session
    /// detaches. How many subscriptions ended.
    pub(crate) fn shutdown(&mut self) -> usize {
        self.phase = ServerPhase::ShuttingDown;
        // The queue stays: the host sends it after the shutdown response.
        let released = self.subscriptions.disconnect(&mut *self.events);
        self.session = ProjectSession::detached();
        self.generation += 1;
        self.surfaces = ExtensionSurfaceTable::empty();
        released
    }
}

/// An update that diverged from a cold rebuild is a bug (ADR 0035): a
/// debug build stops on it. Every update MCP applies passes here.
pub(crate) fn assert_converged(update: &Update, project: &str) {
    if let Some(divergence) = update.divergence() {
        debug_assert!(
            false,
            "an update of {project} diverged from a cold rebuild: {divergence}"
        );
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use crate::McpServer;
    use crate::testing::EventLog;

    /// The replies to `initialize`, `ping` and a listing.
    fn replies(server: &mut McpServer) -> Vec<String> {
        [
            json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {}}),
            json!({"jsonrpc": "2.0", "id": 2, "method": "ping"}),
            json!({"jsonrpc": "2.0", "id": 3, "method": "prompts/list"}),
        ]
        .iter()
        .map(|request| {
            server
                .handle_message(&request.to_string())
                .expect("a request has a reply")
        })
        .collect()
    }

    #[test]
    fn a_server_records_into_discard_unless_given_a_sink() {
        let log = EventLog::new();
        let mut logging = McpServer::new().with_events(log.clone());
        let mut call = |method: &str| {
            let request = json!({"jsonrpc": "2.0", "id": 1, "method": method, "params": {}});
            logging.handle_message(&request.to_string());
        };
        call("initialize");
        assert_eq!(log.names(), ["mcp_initialized"]);
        call("prompts/list");
        assert_eq!(log.names(), ["mcp_initialized", "mcp_discovery_invoked"]);

        // A server given no sink answers the same, and keeps nothing.
        let mut discarding = McpServer::new();
        let mut logged = McpServer::new().with_events(EventLog::new());
        assert_eq!(replies(&mut discarding), replies(&mut logged));
    }
}
