use crate::answers::ClientSupport;
use crate::document::Document;
use crate::files::{Files, Findings, Publish};
use crate::publish::Publication;
use specforge_graph::Graph;
use specforge_ops::view::ProjectView;
use specforge_project::coverage::RecordedCoverage;
use specforge_project::{Environment, ProjectSession};
use specforge_registry::{FieldRegistry, KindRegistry, RegistryBuild};
use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::Path;
use std::sync::Arc;
use tower_lsp::lsp_types::{TextDocumentContentChangeEvent, Url};

/// Shared LSP server state: the files the editor has open or was sent diagnostics for, the
/// project session (the one `specforge watch` holds: environment, graph, per-file parses and
/// diagnostics).
pub struct LspState {
    /// Every file the editor has open or was sent diagnostics for, by session key, keyed under
    /// the spec root readers see (`LspState::rekey`).
    files: Files,
    project: Project,
    /// [`LspState::token_signature`] as of the last recompile, so a
    /// recompile can tell whether the client's semantic tokens went stale.
    last_token_signature: u64,
    shutdown: bool,
    /// The format configurations the editor was told override its settings
    /// (once per session and configuration, ADR 0021 D1).
    format_notices: HashSet<String>,
    /// What the client declared at initialize that shapes an answer.
    client: ClientSupport,
}

/// The session, or what readers see while it is out for an update.
enum Project {
    Held(Box<ProjectSession>),
    /// The session is being updated off the async runtime: readers keep
    /// its last complete graph and the environment it was built with, so
    /// they never see a half-applied update.
    Out(Box<StandIn>),
}

/// What readers see while the session is out for an update.
struct StandIn {
    graph: Graph,
    env: Arc<Environment>,
    /// The text the graph's spans are positions in
    /// ([`ProjectSession::source_texts`]).
    texts: HashMap<String, Arc<str>>,
    /// The keys of the sources the graph was built from, parsed or
    /// unreadable ([`CompiledProject::source_keys`]).
    source_keys: BTreeSet<String>,
    /// The recorded-coverage memo of this stand-in graph (it records
    /// nothing: no root), seeded with the snapshot the session held for
    /// that graph, so no stand-in reads the memo of another.
    recorded: RecordedCoverage,
}

impl Default for LspState {
    fn default() -> Self {
        Self::new()
    }
}

impl LspState {
    /// A state with no project open (a detached session).
    pub fn new() -> Self {
        let mut state = Self {
            files: Files::default(),
            project: Project::Held(Box::new(ProjectSession::detached())),
            last_token_signature: 0,
            shutdown: false,
            format_notices: HashSet::new(),
            client: ClientSupport::default(),
        };
        state.last_token_signature = state.token_signature();
        state
    }

    /// What the client declared that shapes an answer.
    pub fn client(&self) -> ClientSupport {
        self.client
    }

    /// Record what the client declared at initialize.
    pub fn set_client(&mut self, client: ClientSupport) {
        self.client = client;
    }

    /// Whether the session is out for an update: readers see its last
    /// complete graph and cannot read what only a session holds (the
    /// recorded coverage).
    pub fn rebuilding(&self) -> bool {
        matches!(self.project, Project::Out(_))
    }

    /// Record that the editor is told `configuration` overrides its
    /// settings: true the first time this session, false after.
    pub fn first_format_notice(&mut self, configuration: &str) -> bool {
        self.format_notices.insert(configuration.to_string())
    }

    /// A digest of everything in the graph and registries that semantic
    /// tokens depend on beyond a document's own text: each entity's ID,
    /// kind and title, each kind's `semantic_token` classification, and
    /// each field's declared type (which values are references, enum
    /// members or booleans).
    /// Spans are left out, so an edit that only moves text (whitespace)
    /// keeps the signature.
    pub fn token_signature(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        for node in self.graph().nodes() {
            node.id.raw.as_str().hash(&mut hasher);
            node.kind.raw.as_str().hash(&mut hasher);
            node.title.hash(&mut hasher);
        }
        let mut kinds: Vec<(&String, Option<&String>)> = self
            .kind_registry()
            .iter()
            .map(|(keyword, entry)| (keyword, entry.declared.semantic_token.as_ref()))
            .collect();
        kinds.sort();
        kinds.hash(&mut hasher);
        let mut fields: Vec<(&str, &str, String)> = self
            .field_registry()
            .iter()
            .map(|(kind, field, entry)| (kind, field, format!("{:?}", entry.field_type())))
            .collect();
        fields.sort();
        fields.hash(&mut hasher);
        hasher.finish()
    }

    /// Record the current [`LspState::token_signature`]; true when it
    /// differs from the one recorded at the previous recompile.
    pub fn record_token_signature(&mut self) -> bool {
        let signature = self.token_signature();
        let changed = signature != self.last_token_signature;
        self.last_token_signature = signature;
        changed
    }

    /// The session key of the file `uri` names ([`Files::key_of`]).
    pub fn key_of(&self, uri: &Url) -> String {
        self.files.key_of(uri)
    }

    /// The open document `uri` names, whatever spelling of it the client sends.
    pub fn document(&self, uri: &Url) -> Option<&Document> {
        self.files.document(&self.key_of(uri))
    }

    /// What the server and the editor share about each file.
    pub fn files(&self) -> &Files {
        &self.files
    }

    /// `didOpen`: `uri` holding `text` at `version` (nothing after shutdown).
    pub fn open_document(&mut self, uri: &Url, text: &str, version: i32) {
        if self.shutdown {
            return;
        }
        self.files.open(uri, text, version);
    }

    /// `didChange`: `changes` applied in order to the open document `uri`, now at
    /// `version`. Nothing for a document that is not open.
    pub fn change_document(
        &mut self,
        uri: &Url,
        version: i32,
        changes: &[TextDocumentContentChangeEvent],
    ) {
        self.files.change(uri, version, changes);
    }

    /// `didClose`.
    pub fn close_document(&mut self, uri: &Url) {
        self.files.close(uri);
    }

    pub fn graph(&self) -> &Graph {
        match &self.project {
            Project::Held(session) => session.project().graph(),
            Project::Out(stand_in) => &stand_in.graph,
        }
    }

    /// The text the project was last compiled from for session file `key`:
    /// what a span of the graph or of a diagnostic is a position in. It is
    /// the open buffer as it was when the compile ran, or the file as the
    /// compile read it; never the buffer now (an edit waiting to be
    /// compiled moves every position after it) nor the disk now. `None`
    /// for a file the compile does not hold.
    pub fn compiled_text(&self, key: &str) -> Option<Arc<str>> {
        match &self.project {
            Project::Held(session) => session.project().source_text(key),
            Project::Out(stand_in) => stand_in.texts.get(key).cloned(),
        }
    }

    /// The project's environment: config, spec root, registries, rules.
    pub fn environment(&self) -> &Environment {
        match &self.project {
            Project::Held(session) => session.project().environment(),
            Project::Out(stand_in) => &stand_in.env,
        }
    }

    /// The project view reads take (navigation among them): the
    /// session's graph and registries, rooted at its project root; while
    /// the session is out for an update, its last complete graph, with no
    /// root.
    pub fn view(&self) -> ProjectView<'_> {
        match &self.project {
            Project::Held(session) => ProjectView::of(session.project()),
            Project::Out(stand_in) => {
                ProjectView::new(&stand_in.graph, &stand_in.env, None, &stand_in.recorded)
            }
        }
    }

    /// The project session, unless it is out for an update.
    pub fn session(&self) -> Option<&ProjectSession> {
        match &self.project {
            Project::Held(session) => Some(session),
            Project::Out(_) => None,
        }
    }

    /// The version of the buffer the project was compiled from for session
    /// file `key` ([`ProjectSession::buffer`]); `None` for a file compiled
    /// from disk and while the session is out for an update.
    pub fn compiled_version(&self, key: &str) -> Option<i32> {
        self.session()?.buffer(key)?.version
    }

    /// The project session, unless it is out for an update.
    pub fn session_mut(&mut self) -> Option<&mut ProjectSession> {
        match &mut self.project {
            Project::Held(session) => Some(session),
            Project::Out(_) => None,
        }
    }

    /// Take the session out (to update it off the async runtime). Until
    /// [`LspState::set_session`], readers see its last complete graph and
    /// environment. `None` when it is already out.
    pub fn take_session(&mut self) -> Option<ProjectSession> {
        let stand_in = match &self.project {
            Project::Held(session) => Project::Out(Box::new(StandIn {
                graph: session.project().graph().clone(),
                env: session.project().shared_environment(),
                texts: session.project().source_texts(),
                source_keys: session
                    .project()
                    .source_keys()
                    .into_iter()
                    .map(str::to_string)
                    .collect(),
                recorded: RecordedCoverage::of(Arc::clone(session.project().recorded().entities())),
            })),
            Project::Out(_) => return None,
        };
        match std::mem::replace(&mut self.project, stand_in) {
            Project::Held(session) => Some(*session),
            Project::Out(_) => None,
        }
    }

    /// While the session is out for a project's first open: let readers see
    /// its loaded `environment` (no graph yet), so what needs only the
    /// environment (the kinds a keyword completion offers) is served before
    /// its sources are read. Nothing when the session is held.
    pub fn show_environment(&mut self, environment: Arc<Environment>) {
        if let Project::Out(stand_in) = &mut self.project {
            let graph = Graph::new();
            let recorded = RecordedCoverage::over(&graph, &environment);
            **stand_in = StandIn {
                graph,
                env: environment,
                texts: HashMap::new(),
                source_keys: BTreeSet::new(),
                recorded,
            };
            self.rekey();
        }
    }

    /// Put a session in: one taken out, or a newly opened project. After
    /// shutdown it is dropped (its runtime freed) for an empty one.
    pub fn set_session(&mut self, session: ProjectSession) {
        self.project = Project::Held(Box::new(if self.shutdown {
            ProjectSession::detached()
        } else {
            session
        }));
        self.rekey();
    }

    pub fn kind_registry(&self) -> &KindRegistry {
        self.registries().kinds()
    }

    pub fn field_registry(&self) -> &FieldRegistry {
        self.registries().fields()
    }

    /// Everything built from the loaded extensions.
    pub fn registries(&self) -> &RegistryBuild {
        self.environment().registries()
    }

    /// The keys of the sources the project was last compiled from (each a
    /// `.spec` file's path relative to the spec root, parsed or unreadable,
    /// open buffers included): what a `use` path may name. While the session
    /// is out for an update, its last complete compile's.
    pub fn source_keys(&self) -> BTreeSet<&str> {
        match &self.project {
            Project::Held(session) => session.project().source_keys(),
            Project::Out(stand_in) => stand_in.source_keys.iter().map(String::as_str).collect(),
        }
    }

    /// Where the project's `.spec` files live (empty with no project).
    pub fn spec_root(&self) -> &Path {
        self.environment().spec_root()
    }

    pub fn shutdown(&mut self) {
        self.shutdown = true;
        self.files.clear();
        // Dropping the session frees its graph and its Wasm runtime.
        self.project = Project::Held(Box::new(ProjectSession::detached()));
        self.rekey();
    }

    pub fn is_shutdown(&self) -> bool {
        self.shutdown
    }

    /// Record what the project reports now and answer what to send
    /// ([`Files::publish_compile`]).
    pub fn publish(&mut self, publication: Publication) -> Vec<Publish> {
        self.files.publish_compile(publication)
    }

    /// Record what formatting reported and answer what to send now
    /// ([`Files::publish_formatter`]).
    pub fn publish_formatting(&mut self, findings: Findings) -> Option<Publish> {
        self.files.publish_formatter(findings)
    }

    /// Keep `files` keyed under the spec root readers see: called by `show_environment`,
    /// `set_session` and `shutdown`, the three places the environment changes.
    fn rekey(&mut self) {
        let spec_root = self.spec_root().to_path_buf();
        self.files.rekey(&spec_root);
    }
}
