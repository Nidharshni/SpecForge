//! The project a test builds on disk, and the in-process driver over it.
//!
//! A [`TestProject`] is a directory holding `specforge.json` and the files a test writes, plus a second, empty
//! directory a test's processes use as `HOME` ([`TestProject::home`]). It compiles and opens exactly as the
//! surfaces do. [`TestProject::compile`] is what every CLI command holds (the component runtime with the per-user
//! compile cache), and [`TestProject::session`] is what MCP and the LSP open ([`ProjectSession::open`]). With
//! [`TestProject::serving`], both run the extensions an in-process runtime serves instead, installed as a project
//! that enables them has them. Both directories live as long as the value.
//!
//! The CLI driver (`specforge-cli/tests/support`) and the MCP driver (`specforge_mcp::testing::Served`) take a
//! `TestProject`; nothing else in a test writes `specforge.json`, picks a runtime or calls
//! `CompiledProject::compile`.

pub mod specs;

use std::path::Path;
use std::sync::Arc;

use serde_json::{Value, json};
use specforge_common::{ExtensionEntry, ProjectRoot};
use specforge_wasm::testing::InProcessRuntime;

use crate::{CompiledProject, ProjectSession, RuntimeSource, SharedRuntime};

/// A project on disk with its own HOME. Built by value (`TestProject::new().enabling(..).file(..)`); every
/// builder call writes through to disk at once, so the directory is always what the value says.
pub struct TestProject {
    dir: tempfile::TempDir,
    home: tempfile::TempDir,
    /// `specforge.json` as it is on disk, while the project owns it; `None`: no file, or one written verbatim
    /// ([`Self::raw_config`]).
    config: Option<Value>,
    /// The `extensions` a test chose ([`Self::enabling`]); `None`: not chosen.
    extensions: Option<Vec<String>>,
    /// What [`Self::serving`] serves; [`Self::compile`] and [`Self::session`] load into it.
    served: Option<Arc<InProcessRuntime>>,
}

impl Default for TestProject {
    fn default() -> Self {
        Self::new()
    }
}

impl TestProject {
    /// `{"name": "t", "version": "0.1.0", "extensions": []}` and no other file.
    pub fn new() -> Self {
        let project = TestProject {
            dir: tempfile::TempDir::new().expect("a temp dir"),
            home: tempfile::TempDir::new().expect("a temp dir"),
            config: Some(json!({"name": "t", "version": "0.1.0", "extensions": []})),
            extensions: None,
            served: None,
        };
        project.write_config();
        project
    }

    /// A copy of the directory tree at `fixture` (`tests/fixtures/...`, an example project, the repository's own
    /// `spec/` + `specforge.json`), leaving out `.specforge/` and `target/`. Its `specforge.json`, if any, is kept
    /// verbatim until a builder call edits it.
    pub fn copy_of(fixture: impl AsRef<Path>) -> Self {
        let project = TestProject {
            dir: tempfile::TempDir::new().expect("a temp dir"),
            home: tempfile::TempDir::new().expect("a temp dir"),
            config: None,
            extensions: None,
            served: None,
        };
        copy_tree(fixture.as_ref(), project.root());
        let config = std::fs::read(project.root().join("specforge.json"))
            .ok()
            .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
            .filter(Value::is_object);
        TestProject { config, ..project }
    }

    /// Copy the directory tree at `from` to `to` under the root (leaving out `.specforge/` and `target/`): a project
    /// assembled from parts of another, such as the repository's corpora beside its own `specforge.json`.
    pub fn copy_in(self, from: impl AsRef<Path>, to: &str) -> Self {
        copy_tree(from.as_ref(), &self.root().join(to));
        self
    }

    /// Enable `extensions`, in load order: builtins by name (`@specforge/software`), or what [`Self::serving`]
    /// serves.
    pub fn enabling(mut self, extensions: &[&str]) -> Self {
        self.extensions = Some(extensions.iter().map(|e| e.to_string()).collect());
        self.refresh();
        self
    }

    /// Apply `edit` to `specforge.json` (`spec_root`, `registries`, `inference`, `name`, …). Panics after
    /// [`Self::raw_config`] or [`Self::without_config`].
    pub fn config(mut self, edit: impl FnOnce(&mut Value)) -> Self {
        let config = self
            .config
            .as_mut()
            .expect("config() after raw_config(), without_config() or a copy with no readable specforge.json");
        edit(config);
        self.refresh();
        self
    }

    /// Write `specforge.json` as these bytes exactly: a malformed or partial config.
    pub fn raw_config(mut self, bytes: impl AsRef<[u8]>) -> Self {
        self.config = None;
        self.write("specforge.json", bytes);
        self
    }

    /// No `specforge.json`: the directory itself, with the default configuration (ADR 0053).
    pub fn without_config(mut self) -> Self {
        self.config = None;
        let _ = std::fs::remove_file(self.root().join("specforge.json"));
        self
    }

    /// Write `contents` at `path` (relative to the root), its directories created.
    pub fn file(self, path: &str, contents: impl AsRef<[u8]>) -> Self {
        self.write(path, contents);
        self
    }

    /// [`Self::file`] for each `(path, text)`.
    pub fn files(self, files: &[(&str, &str)]) -> Self {
        for (path, text) in files {
            self.write(path, text);
        }
        self
    }

    /// Serve the extensions `runtime` serves, in process. Each enabled extension that is not a builtin is
    /// installed (`specforge_installed::testing::install_enabled`), so it loads through the production path.
    pub fn serving(mut self, runtime: InProcessRuntime) -> Self {
        self.served = Some(Arc::new(runtime));
        self.refresh();
        self
    }

    /// Install `names` as [`Self::serving`] would, for a test that passes its own runtime to
    /// [`Self::compile_in`] or [`Self::session_in`].
    pub fn installing(self, names: &[&str]) -> Self {
        specforge_installed::testing::install(self.root(), names);
        self
    }

    /// Install the enabled extensions that are not builtins, as [`Self::serving`] does, for a test that gives its
    /// own runtime to a server or a compile ([`Self::compile_in`], `Served::initialized_in`).
    pub fn installing_enabled(self) -> Self {
        self.install_enabled();
        self
    }

    /// The project directory (where `specforge.json` is).
    pub fn root(&self) -> &Path {
        self.dir.path()
    }

    /// [`ProjectRoot::of`] the root.
    pub fn project_root(&self) -> ProjectRoot {
        ProjectRoot::of(self.root()).expect("the project directory exists")
    }

    /// An empty directory, apart from the root, for the processes a test runs to use as `HOME`.
    pub fn home(&self) -> &Path {
        self.home.path()
    }

    /// The `extensions` [`Self::enabling`] chose, if it was called.
    pub fn enabled(&self) -> Option<&[String]> {
        self.extensions.as_deref()
    }

    /// The in-process runtime [`Self::serving`] serves (recorded calls, limits).
    pub fn served(&self) -> Option<&Arc<InProcessRuntime>> {
        self.served.as_ref()
    }

    /// Write `contents` at `path` now (an edit after the project was built).
    pub fn write(&self, path: &str, contents: impl AsRef<[u8]>) {
        let path = self.root().join(path);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap_or_else(|e| panic!("{}: {e}", parent.display()));
        }
        std::fs::write(&path, contents).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    }

    /// Remove the file at `path`.
    pub fn remove(&self, path: &str) {
        std::fs::remove_file(self.root().join(path))
            .unwrap_or_else(|e| panic!("remove {path}: {e}"));
    }

    /// The text at `path`.
    pub fn read(&self, path: &str) -> String {
        std::fs::read_to_string(self.root().join(path))
            .unwrap_or_else(|e| panic!("read {path}: {e}"))
    }

    /// `specforge.json` as it is on disk now, parsed.
    pub fn config_on_disk(&self) -> Value {
        serde_json::from_str(&self.read("specforge.json"))
            .unwrap_or_else(|e| panic!("specforge.json is not JSON: {e}"))
    }

    /// The project directory, its HOME dropped: for a test that hands a directory to code that owns it.
    pub fn into_dir(self) -> tempfile::TempDir {
        self.dir
    }

    /// The project compiled as every CLI command compiles it: the served runtime, else the component runtime with
    /// the per-user compile cache.
    pub fn compile(&self) -> CompiledProject {
        self.compile_in(self.runtime())
    }

    /// [`Self::compile`], asserting that every [`Self::enabled`] extension is among the declarations the registry
    /// build holds (names the missing ones).
    pub fn loaded(&self) -> CompiledProject {
        let compiled = self.compile();
        let declared: Vec<&str> = compiled
            .environment()
            .registries()
            .declarations()
            .iter()
            .map(|d| d.name())
            .collect();
        let missing: Vec<&str> = self
            .enabled()
            .unwrap_or_default()
            .iter()
            .filter_map(|entry| match ExtensionEntry::parse(entry) {
                ExtensionEntry::Named(name) => Some(name),
                ExtensionEntry::File { name, .. } => name,
            })
            .filter(|name| !declared.contains(name))
            .collect();
        assert!(
            missing.is_empty(),
            "enabled but not loaded: {missing:?} (loaded: {declared:?}); diagnostics: {:?}",
            compiled.diagnostics()
        );
        compiled
    }

    /// The project compiled with `runtime`.
    pub fn compile_in(&self, runtime: SharedRuntime) -> CompiledProject {
        CompiledProject::compile(&self.project_root(), Some(runtime))
    }

    /// The project compiled with no runtime: no extension loads (structure only).
    pub fn compile_bare(&self) -> CompiledProject {
        CompiledProject::compile(&self.project_root(), None)
    }

    /// The project opened as MCP and the LSP open it: the served runtime, else the project's own
    /// ([`RuntimeSource::project`]).
    pub fn session(&self) -> ProjectSession {
        match &self.served {
            Some(served) => ProjectSession::open_with_runtime(
                &self.project_root(),
                Some(Arc::clone(served) as SharedRuntime),
            ),
            None => ProjectSession::open(&self.project_root()),
        }
    }

    /// The project opened with `runtime` for every load.
    pub fn session_in(&self, runtime: SharedRuntime) -> ProjectSession {
        ProjectSession::open_with_runtime(&self.project_root(), Some(runtime))
    }

    /// The project opened with each load's runtime from `source`.
    pub fn session_from(&self, source: RuntimeSource) -> ProjectSession {
        ProjectSession::open_from(&self.project_root(), source)
    }

    /// The project opened with no runtime.
    pub fn session_bare(&self) -> ProjectSession {
        ProjectSession::open_with_runtime(&self.project_root(), None)
    }

    /// The runtime every CLI command compiles with (the component runtime with the per-user compile cache), unless a
    /// test serves its own: for a test that loads the environment itself.
    pub fn runtime(&self) -> SharedRuntime {
        match &self.served {
            Some(served) => Arc::clone(served) as SharedRuntime,
            None => self.component_runtime(),
        }
    }

    /// The component runtime every CLI command compiles with, as its concrete type (for a test that asks it what it
    /// loaded).
    pub fn component_runtime(&self) -> Arc<specforge_component::ComponentRuntime> {
        component_runtime()
    }

    /// Bring the disk up to the value: `specforge.json`, and the installs a served runtime needs.
    fn refresh(&self) {
        self.write_config();
        if self.served.is_some() {
            self.install_enabled();
        }
    }

    /// Install what `specforge.json` enables and is neither a builtin nor a `.wasm` file.
    fn install_enabled(&self) {
        let entries: Vec<String> = match (&self.extensions, &self.config) {
            (Some(chosen), _) => chosen.clone(),
            (None, Some(config)) => config["extensions"]
                .as_array()
                .map(|a| {
                    a.iter()
                        .filter_map(|e| e.as_str().map(str::to_string))
                        .collect()
                })
                .unwrap_or_default(),
            (None, None) => Vec::new(),
        };
        specforge_installed::testing::install_enabled(self.root(), &entries, &crate::builtins());
    }

    /// Write `specforge.json`, its `extensions` merged in. It is what `specforge init` writes: pretty-printed,
    /// with a trailing newline.
    fn write_config(&self) {
        let Some(config) = &self.config else {
            return;
        };
        let mut config = config.clone();
        if let Some(extensions) = &self.extensions {
            config["extensions"] = json!(extensions);
        }
        let mut text = serde_json::to_string_pretty(&config).expect("a config serializes");
        text.push('\n');
        self.write("specforge.json", text);
    }
}

/// The component runtime every CLI command compiles with: the builtins, with the per-user compile cache. For a test
/// that builds its project apart from a [`TestProject`].
pub fn component_runtime() -> Arc<specforge_component::ComponentRuntime> {
    Arc::new(specforge_component::ComponentRuntime::with_user_cache())
}

/// The project at `root`, a directory a test owns (moved, renamed, built apart from a [`TestProject`]), compiled with
/// `runtime`.
pub fn compile_at(root: &Path, runtime: Option<SharedRuntime>) -> CompiledProject {
    CompiledProject::compile(
        &ProjectRoot::of(root).expect("the project directory exists"),
        runtime,
    )
}

/// Copy the tree at `from` to `to`, leaving out `.specforge` and `target`.
fn copy_tree(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap_or_else(|e| panic!("{}: {e}", to.display()));
    for entry in std::fs::read_dir(from).unwrap_or_else(|e| panic!("{}: {e}", from.display())) {
        let entry = entry.expect("a directory entry");
        let name = entry.file_name();
        if name == ".specforge" || name == "target" {
            continue;
        }
        let (source, target) = (entry.path(), to.join(&name));
        if source.is_dir() {
            copy_tree(&source, &target);
        } else {
            std::fs::copy(&source, &target).unwrap_or_else(|e| panic!("{}: {e}", source.display()));
        }
    }
}
