use specforge_common::{Diagnostic, ProjectConfig};
use specforge_graph::Graph;
use specforge_project::{ProjectSession, RuntimeSource, SharedRuntime, Update, UpdateKind};
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
    pub phase: ServerPhase,
    /// The protocol revision `initialize` negotiated (the latest one the
    /// server speaks until then).
    pub protocol_version: &'static str,
    /// Whether a project (or the empty default surface) is being served:
    /// set by `initialize`, or by the first stateless request.
    pub served: bool,
    /// The served project: its root, environment (config, spec root,
    /// registries, rules, manifests, surfaces), graph, diagnostics and
    /// extension runtime: always opened from disk (ADR 0025). Detached
    /// while none is served.
    session: ProjectSession,
    /// How many times the served project changed: every update applied to
    /// it and every replacement bumps it ([`Self::session_generation`]).
    generation: u64,
    /// The last update [`Self::ensure_fresh`] applied.
    last_update: Option<Update>,
    /// What MCP serves from the served project's extensions, built from
    /// their declarations whenever its environment loads.
    surfaces: ExtensionSurfaceTable,
    /// Project compiled when the client's `initialize` names no `projectRoot`
    /// (the `specforge mcp <path>` argument).
    pub default_project_root: Option<PathBuf>,
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
    pub extension_runtime: Option<SharedRuntime>,
}

impl Default for McpState {
    fn default() -> Self {
        Self::new()
    }
}

impl McpState {
    pub fn new() -> Self {
        Self {
            phase: ServerPhase::Uninitialized,
            protocol_version: crate::lifecycle::LATEST_PROTOCOL_VERSION,
            served: false,
            session: ProjectSession::detached(),
            generation: 0,
            last_update: None,
            surfaces: ExtensionSurfaceTable::empty(),
            default_project_root: None,
            events: Box::new(Discard),
            subscriptions: Subscriptions::new(),
            extension_runtime: None,
        }
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
    /// extensions included: the served session reloads its environment
    /// and rebuilds from the sources (a fresh compile), or a session is
    /// opened when `root` is not the project served. The graph,
    /// diagnostics, registries, config and runtime change together, with
    /// the session; the extension surface table is built again from this
    /// project's declarations, so nothing a previous load contributed
    /// survives, and nothing is listed twice.
    /// Subscribed clients learn what changed. The one place the served
    /// project is replaced (initialize, adopting a call's path, the
    /// directory `init` created). Only a project (a directory holding
    /// `specforge.json` or `specforge.spec`) is ever served.
    pub fn serve(&mut self, root: &Path) {
        debug_assert!(
            specforge_common::is_project_root(root),
            "only a project is served: {} holds no specforge.json or specforge.spec",
            root.display()
        );
        let before = self.heard_diagnostics();
        // The served session reloads when it is the project on disk at
        // `root`; any other root is opened.
        let canonical = |p: &Path| std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
        let reloads = self
            .project_root()
            .is_some_and(|served| canonical(served) == canonical(root));
        let update = if reloads {
            self.session.reload_environment()
        } else {
            let next = self.open(root);
            let previous = std::mem::replace(&mut self.session, next);
            self.session.replaced(&previous)
        };
        self.applied(update, before);
    }

    /// A session of the project at `root`, its extensions running in the
    /// host's runtime when the server has one, else in the project's own,
    /// built for each environment load (ADR 0030 D3): how the served project
    /// and another project a call names are opened.
    pub(crate) fn open(&self, root: &Path) -> ProjectSession {
        let source = match &self.extension_runtime {
            Some(runtime) => RuntimeSource::Fixed(Some(Arc::clone(runtime))),
            None => RuntimeSource::project(),
        };
        ProjectSession::open_from(root, source)
    }

    /// Bring the served project up to date with disk (behavior
    /// `bring_session_up_to_date`): exactly what changed since it was last
    /// built is applied, an update for changed sources, an environment
    /// reload (its extension tools and resources registered again) for a
    /// changed `specforge.json`, `specforge.lock` or extension module, a
    /// re-check for a changed check input. Subscribed clients learn what
    /// changed. `None` when nothing did.
    pub fn ensure_fresh(&mut self) -> Option<&Update> {
        let before = self.heard_diagnostics();
        let update = self.session.ensure_fresh()?;
        self.applied(update, before);
        self.last_update.as_ref()
    }

    /// What the server reports now, when someone hears about the
    /// diagnostics: an update compares it with what it reports after.
    fn heard_diagnostics(&self) -> Option<Vec<Diagnostic>> {
        self.subscriptions
            .hears_diagnostics()
            .then(|| self.diagnostics())
    }

    /// Record `update`, applied to the served session: the extension
    /// surface table is built again when its environment loaded again, and
    /// subscribed clients learn what changed.
    fn applied(&mut self, update: Update, before: Option<Vec<Diagnostic>>) {
        // Every session verifies its updates in a debug build (ADR 0035):
        // a divergence from a cold rebuild is a bug, and this is the one
        // place every update of the served project passes.
        assert_converged(&update, "the served project");
        self.generation += 1;
        if update.kind == UpdateKind::Environment {
            self.surfaces = ExtensionSurfaceTable::build(
                self.registries(),
                crate::tools::CORE_TOOLS,
                crate::resources::CORE_RESOURCES,
            );
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
        let after = before.as_ref().map(|_| self.diagnostics());
        let diagnostics = before.as_deref().zip(after.as_deref());
        self.subscriptions
            .updated(&Changes::of(&update, diagnostics), &mut *self.events);
        self.last_update = Some(update);
    }

    /// Every subscription and stream ends, each recorded; the session
    /// detaches. How many subscriptions ended.
    pub fn shutdown(&mut self) -> usize {
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
    use crate::events::testing::EventLog;

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
