//! The compiled project a test assembles in memory (ADR 0108).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

use specforge_common::{ConfigProblem, ConfigRead, Diagnostic};
use specforge_graph::{Graph, GraphBuild};
use specforge_installed::{Installed, LockState};
use specforge_protocol_types::{ExtensionDeclaration, HandshakeResponse};
use specforge_registry::RegistryBuild;
use tempfile::TempDir;

use crate::sources::SourceCache;
use crate::{CompiledProject, EnabledExtension, Environment, ExtensionLoad, Loaded, SharedRuntime};

/// What the compile read and loaded, from which the environment is assembled at every builder call
/// ([`Environment::assemble`]), so the order of the calls does not matter and the environment is one `check` could
/// make.
struct Parts {
    read: ConfigRead,
    installed: Installed,
    /// What the compile read each entry as enabling, in place of what the config's entries name.
    enabled: Option<Vec<EnabledExtension>>,
    declarations: Vec<ExtensionDeclaration>,
    runtime: Option<SharedRuntime>,
}

impl Parts {
    fn environment(&self) -> Environment {
        let load =
            if self.runtime.is_none() && self.enabled.is_none() && self.declarations.is_empty() {
                ExtensionLoad::NoRuntime
            } else {
                ExtensionLoad::In {
                    runtime: self.runtime.clone().unwrap_or_else(|| {
                        Arc::new(specforge_wasm::testing::InProcessRuntime::new())
                    }),
                    loaded: Loaded {
                        enabled: self.enabled.clone().unwrap_or_else(|| {
                            self.installed
                                .unloaded(&self.read.config.extensions, &crate::builtins())
                                .enabled
                        }),
                        declarations: self.declarations.clone(),
                    },
                }
            };
        Environment::assemble(self.read.clone(), self.installed.clone(), load)
    }
}

/// A compiled project a test assembles in memory: a graph (parsed from one source, or built node by node), an
/// environment, and what its compile reported (the test's list; no check runs). Rooted at a temp directory it owns
/// (or a directory the test owns); only what the test writes is on disk. The in-memory counterpart of
/// [`super::TestProject`].
///
/// Invariants: `compiled().root() == Some(root())`, `compiled().environment().root() == root()`
/// and `compiled().environment().project_root() == None`;
/// `detached().root() == None`; `compiled().diagnostics()` is exactly the `reporting` list (empty by default); its
/// entity snapshot is taken over its own graph, in its own environment; a source is built as `check` builds it,
/// in the environment the project ends with (ADR 0123).
///
/// A parts setter after [`Self::over`], [`Self::over_in`] or [`Self::assembling`] panics: the environment was made
/// whole. A setter called after [`Self::compiled`] starts a new project (builders take `self`).
pub struct Assembled {
    /// The directory `root` is, while the value owns it.
    _dir: Option<TempDir>,
    root: PathBuf,
    /// `None` after `over`, `over_in` and `assembling`.
    parts: Option<Parts>,
    env: Arc<Environment>,
    /// A graph the test made ([`Self::graph`], [`Self::graph_mut`]); `None`: the project's graph is built from
    /// `texts` in its environment when the project is compiled (ADR 0123).
    made: Option<Graph>,
    reported: Vec<Diagnostic>,
    /// The text each source was read as, by key: what `source` parsed, none for a graph made elsewhere.
    texts: HashMap<String, Arc<str>>,
    rooted: OnceLock<CompiledProject>,
    detached: OnceLock<CompiledProject>,
}

impl Default for Assembled {
    fn default() -> Self {
        Self::new()
    }
}

impl Assembled {
    /// An empty project rooted at a fresh temp directory: the compile found a `specforge.json` there (not written
    /// to disk) with the default config, nothing enabled, no extension, nothing reported.
    pub fn new() -> Self {
        let dir = TempDir::new().expect("a temp dir");
        Self::in_parts(dir.path().to_path_buf(), Some(dir))
    }

    /// [`Self::new`], rooted at `root`, a directory the test owns.
    pub fn at(root: &Path) -> Self {
        Self::in_parts(root.to_path_buf(), None)
    }

    /// A project over a registry build made in memory ([`Environment::bare`]), rooted at a fresh temp directory.
    pub fn over(registries: RegistryBuild) -> Self {
        Self::bare(registries, None)
    }

    /// [`Self::over`], whose extension calls go to `runtime`.
    pub fn over_in(registries: RegistryBuild, runtime: SharedRuntime) -> Self {
        Self::bare(registries, Some(runtime))
    }

    /// A project whose environment the test makes at its root (a fresh temp directory).
    pub fn assembling(env: impl FnOnce(&Path) -> Environment) -> Self {
        let dir = TempDir::new().expect("a temp dir");
        let root = dir.path().to_path_buf();
        let env = env(&root);
        Self::made(root, Some(dir), env)
    }

    /// [`Self::assembling`], at `root`, a directory the test owns.
    pub fn assembling_at(root: &Path, env: impl FnOnce(&Path) -> Environment) -> Self {
        Self::made(root.to_path_buf(), None, env(root))
    }

    fn in_parts(root: PathBuf, dir: Option<TempDir>) -> Self {
        let parts = Parts {
            read: ConfigRead {
                found: true,
                ..Default::default()
            },
            installed: Installed::unread(&root),
            enabled: None,
            declarations: Vec::new(),
            runtime: None,
        };
        let env = Arc::new(parts.environment());
        Assembled {
            _dir: dir,
            root,
            parts: Some(parts),
            env,
            made: None,
            reported: Vec::new(),
            texts: HashMap::new(),
            rooted: OnceLock::new(),
            detached: OnceLock::new(),
        }
    }

    fn bare(registries: RegistryBuild, runtime: Option<SharedRuntime>) -> Self {
        let dir = TempDir::new().expect("a temp dir");
        let root = dir.path().to_path_buf();
        let env = Environment::bare(Installed::unread(&root), registries, runtime);
        Self::made(root, Some(dir), env)
    }

    fn made(root: PathBuf, dir: Option<TempDir>, env: Environment) -> Self {
        Assembled {
            _dir: dir,
            root,
            parts: None,
            env: Arc::new(env),
            made: None,
            reported: Vec::new(),
            texts: HashMap::new(),
            rooted: OnceLock::new(),
            detached: OnceLock::new(),
        }
    }

    /// A new project: nothing made from the old one is kept.
    fn changed(mut self) -> Self {
        self.rooted = OnceLock::new();
        self.detached = OnceLock::new();
        self
    }

    /// Edit what the compile read and loaded, and assemble the environment again.
    fn with_parts(mut self, edit: impl FnOnce(&mut Parts, &Path)) -> Self {
        let parts = self.parts.as_mut().expect(
            "a parts setter on a project whose environment was made whole (over, over_in or assembling)",
        );
        edit(parts, &self.root);
        self.env = Arc::new(parts.environment());
        self.changed()
    }

    /// The compile loaded its extensions in `runtime`, which the environment holds.
    pub fn runtime(self, runtime: SharedRuntime) -> Self {
        self.with_parts(|parts, _| parts.runtime = Some(runtime))
    }

    /// The compile read `config` as `specforge.json` (not written to disk): its `extensions` entries, each enabling
    /// what its text names, and its `providers`, registered against the loaded declarations.
    pub fn config_json(self, config: serde_json::Value) -> Self {
        self.with_parts(|parts, root| {
            parts.read = specforge_common::parse_project_config(
                &root.join("specforge.json"),
                &config.to_string(),
            );
        })
    }

    /// [`Self::config_json`] of a config enabling `entries`.
    pub fn config(self, entries: &[&str]) -> Self {
        self.config_json(serde_json::json!({ "extensions": entries }))
    }

    /// What the compile read each entry as enabling, in place of what the config's entries name.
    pub fn enabled(self, enabled: Vec<EnabledExtension>) -> Self {
        self.with_parts(|parts, _| parts.enabled = Some(enabled))
    }

    /// The compile loaded `declarations`, in this order.
    pub fn declarations(self, declarations: Vec<ExtensionDeclaration>) -> Self {
        self.with_parts(|parts, _| parts.declarations = declarations)
    }

    /// An extension `name` at `version` that contributes nothing.
    pub fn declaration(name: &str, version: &str) -> ExtensionDeclaration {
        ExtensionDeclaration {
            handshake: HandshakeResponse {
                name: name.into(),
                version: version.into(),
                ..Default::default()
            },
            ..Default::default()
        }
    }

    /// `<root>/specforge.lock` locks each `(name, version, source)`; the compile read it.
    pub fn lock(self, entries: &[(&str, &str, &str)]) -> Self {
        self.with_parts(|parts, root| {
            let lock = specforge_installed::testing::write_lock(root, entries);
            parts.installed = Installed::with_lock(root, LockState::Read(lock));
        })
    }

    /// The compile read the lock at the root as it is on disk now.
    pub fn read_lock(self) -> Self {
        self.with_parts(|parts, root| parts.installed = Installed::at(root))
    }

    /// The compile found no `specforge.json` at the root.
    pub fn without_config_file(self) -> Self {
        self.with_parts(|parts, _| parts.read.found = false)
    }

    /// The compile could not use `specforge.json` as written, for `problems` (what the surface reports for them is
    /// the test's to set).
    pub fn config_problems(self, problems: Vec<ConfigProblem>) -> Self {
        self.with_parts(|parts, _| parts.read.problems = problems)
    }

    /// The project's graph, made by the test: read as if built in the project's environment.
    pub fn graph(mut self, graph: Graph) -> Self {
        self.made = Some(graph);
        self.texts.clear();
        self.changed()
    }

    /// The project's graph, to add nodes and edges to: what a test that builds its graph node by node edits. What
    /// was made from the old graph ([`Self::compiled`], [`Self::detached`]) is made again. A source is built in the
    /// environment first and its text dropped.
    pub fn graph_mut(&mut self) -> &mut Graph {
        self.rooted = OnceLock::new();
        self.detached = OnceLock::new();
        let built = match self.made.take() {
            Some(graph) => graph,
            None => self.graph_build().into_parts().0,
        };
        self.texts.clear();
        self.made.insert(built)
    }

    /// The project's graph is `text` as the one file `main.spec`, built in the project's environment as a compile
    /// builds it ([`Environment::graph_of`]) when the project is compiled, so a setter called after it still
    /// applies. The compile read `text` as `main.spec`'s text.
    pub fn source(mut self, text: &str) -> Self {
        self.made = None;
        self.texts.clear();
        self.texts.insert("main.spec".to_string(), Arc::from(text));
        self.changed()
    }

    /// The project's graph build: the test's graph held in its environment's empty build, or the texts built there.
    fn graph_build(&self) -> GraphBuild {
        match &self.made {
            Some(graph) => self.env.graph_of([]).holding(graph.clone()),
            None => self.env.graph_of(
                self.texts
                    .iter()
                    .map(|(key, text)| specforge_parser::parse(text, key)),
            ),
        }
    }

    /// The project's compile reported `diagnostics`.
    pub fn reporting(mut self, diagnostics: Vec<Diagnostic>) -> Self {
        self.reported = diagnostics;
        self.changed()
    }

    /// The root: a temp directory the project owns, or the one the test gave.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Write `contents` at `path` (relative to the root), its directories created.
    pub fn write(&self, path: &str, contents: impl AsRef<[u8]>) {
        let path = self.root.join(path);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap_or_else(|e| panic!("{}: {e}", parent.display()));
        }
        std::fs::write(&path, contents).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    }

    /// Record, as `specforge collect` would, one passing test per `(entity, obligation)`.
    pub fn record_passing(&self, proofs: &[(&str, &str)]) {
        let mut results = serde_json::Map::new();
        for (entity, obligation) in proofs {
            results.insert(
                entity.to_string(),
                serde_json::json!({"tests": [
                    {"name": format!("{entity}_test"), "status": "passed", "verify": obligation}
                ]}),
            );
        }
        self.write(
            "specforge-report.json",
            serde_json::json!({"runner": "fixture", "results": results}).to_string(),
        );
    }

    /// The compiled project, rooted at [`Self::root`]: made once, so its memo is shared across calls.
    pub fn compiled(&self) -> &CompiledProject {
        self.rooted.get_or_init(|| {
            CompiledProject::assembled(
                Arc::clone(&self.env),
                self.graph_build(),
                self.reported.clone(),
                SourceCache::holding(self.texts.clone()),
                true,
            )
        })
    }

    /// The same graph, environment and report with no root (ADR 0047 D4): no recorded report, no schema record,
    /// and an operation that reads the project on disk refuses.
    pub fn detached(&self) -> &CompiledProject {
        self.detached.get_or_init(|| {
            CompiledProject::assembled(
                Arc::clone(&self.env),
                self.graph_build(),
                self.reported.clone(),
                SourceCache::holding(self.texts.clone()),
                false,
            )
        })
    }
}
