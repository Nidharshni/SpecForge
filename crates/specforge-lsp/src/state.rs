use crate::answers::ClientSupport;
use crate::document::Document;
use specforge_common::Diagnostic;
use specforge_graph::Graph;
use specforge_ops::view::ProjectView;
use specforge_project::coverage::RecordedCoverage;
use specforge_project::{Environment, ProjectSession};
use specforge_registry::{EdgeRegistry, FieldRegistry, KindRegistry, RegistryBuild};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Shared LSP server state: open documents, the project session (the one
/// `specforge watch` holds: environment, graph, per-file parses and
/// diagnostics), and the diagnostics last published per URI.
pub struct LspState {
    documents: HashMap<String, Document>,
    diagnostics: HashMap<String, Vec<Diagnostic>>,
    project: Project,
    /// Where diagnostics without a span were last published.
    anchor: Option<String>,
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
            documents: HashMap::new(),
            diagnostics: HashMap::new(),
            project: Project::Held(Box::new(ProjectSession::detached())),
            anchor: None,
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

    pub fn open_document(&mut self, uri: &str, content: &str) {
        if self.shutdown {
            return;
        }
        self.documents.insert(
            uri.to_string(),
            Document::new(uri.to_string(), content.to_string()),
        );
    }

    pub fn close_document(&mut self, uri: &str) {
        self.documents.remove(uri);
        self.diagnostics.remove(uri);
    }

    pub fn is_open(&self, uri: &str) -> bool {
        self.documents.contains_key(uri)
    }

    pub fn document(&self, uri: &str) -> Option<&Document> {
        self.documents.get(uri)
    }

    pub fn document_mut(&mut self, uri: &str) -> Option<&mut Document> {
        self.documents.get_mut(uri)
    }

    pub fn open_uris(&self) -> Vec<&str> {
        let mut uris: Vec<&str> = self.documents.keys().map(|s| s.as_str()).collect();
        uris.sort();
        uris
    }

    /// Apply one content change to an open document: `range` (UTF-16
    /// positions) replaced by `new_text`, the whole text when `None`.
    pub fn apply_change(
        &mut self,
        uri: &str,
        range: Option<tower_lsp::lsp_types::Range>,
        new_text: &str,
    ) {
        if let Some(doc) = self.documents.get_mut(uri) {
            doc.apply_change(range, new_text);
        }
    }

    pub fn set_diagnostics(&mut self, uri: &str, diags: Vec<Diagnostic>) {
        self.diagnostics.insert(uri.to_string(), diags);
    }

    pub fn diagnostics(&self, uri: &str) -> &[Diagnostic] {
        self.diagnostics
            .get(uri)
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }

    /// Every diagnostic last published, URI by URI in URI order, each list
    /// in published order: the copies the editor shows (a spanless one
    /// placed at its first subject's name, its data kept), what the hover
    /// reports about an entity.
    pub fn published_diagnostics(&self) -> impl Iterator<Item = &Diagnostic> {
        let mut uris: Vec<&String> = self.diagnostics.keys().collect();
        uris.sort();
        uris.into_iter()
            .flat_map(|uri| self.diagnostics[uri].iter())
    }

    /// The URIs diagnostics were last published for.
    pub fn published_uris(&self) -> Vec<String> {
        self.diagnostics.keys().cloned().collect()
    }

    /// Forget what was published for `uri` (it was cleared).
    pub fn clear_diagnostics(&mut self, uri: &str) {
        self.diagnostics.remove(uri);
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
                recorded,
            };
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
    }

    pub fn kind_registry(&self) -> &KindRegistry {
        self.registries().kinds()
    }

    pub fn field_registry(&self) -> &FieldRegistry {
        self.registries().fields()
    }

    pub fn edge_registry(&self) -> &EdgeRegistry {
        self.registries().edges()
    }

    /// Everything built from the loaded extensions.
    pub fn registries(&self) -> &RegistryBuild {
        self.environment().registries()
    }

    /// Where the project's `.spec` files live (empty with no project).
    pub fn spec_root(&self) -> &Path {
        self.environment().spec_root()
    }

    /// The session's key for a file's absolute path
    /// ([`Environment::source_key`]): relative to the spec root when the
    /// file is under it, else the path itself.
    pub fn source_key(&self, path: &str) -> String {
        self.environment().source_key(Path::new(path))
    }

    /// The absolute path of a session file key.
    pub fn file_path(&self, key: &str) -> PathBuf {
        self.spec_root().join(key)
    }

    pub fn shutdown(&mut self) {
        self.shutdown = true;
        self.documents.clear();
        self.diagnostics.clear();
        // Dropping the session frees its graph and its Wasm runtime.
        self.project = Project::Held(Box::new(ProjectSession::detached()));
    }

    pub fn is_shutdown(&self) -> bool {
        self.shutdown
    }

    /// The open document diagnostics without a span were last published
    /// on (`None` once it is closed).
    pub fn anchor(&self) -> Option<&str> {
        self.anchor.as_deref().filter(|uri| self.is_open(uri))
    }

    pub fn set_anchor(&mut self, uri: Option<String>) {
        self.anchor = uri;
    }

    /// Keep what `publication` sends: each file's placed diagnostics (code
    /// actions read them back; an empty list forgets the file), and where
    /// diagnostics about no entity went, when any did.
    pub fn record(&mut self, publication: &crate::publish::Publication) {
        if let Some(anchor) = &publication.anchor {
            self.anchor = Some(anchor.to_string());
        }
        for (uri, file) in &publication.files {
            if file.placed.is_empty() {
                self.diagnostics.remove(uri.as_str());
            } else {
                self.diagnostics
                    .insert(uri.to_string(), file.placed.clone());
            }
        }
    }
}
