//! What a project session depends on, and what a changed path is to it
//! (behavior `classify_project_changes`, ADR 0030).
//!
//! A session is built from its sources (the `.spec` files discovery finds
//! under the spec root), its environment inputs (`specforge.json`,
//! `specforge.lock` and the extension modules the environment loads) and
//! its check inputs (files the checks read but no parse does: the build
//! cache check-phase passes read, the files `file_reference` fields and
//! `file_exists` rules name). [`SessionInputs`] is that set as one value;
//! watch, the LSP and MCP all ask the session's inputs what a changed path
//! means, which directories to watch and what to report, so "a change"
//! has one meaning.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use specforge_common::{Exclude, ProjectConfig, discover_spec_files, is_discovered};

use crate::build_cache::BUILD_CACHE_FILE;
use crate::{Environment, builtins};
use specforge_installed::Installed;

/// What a changed path is to a project session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputRole {
    /// A `.spec` file discovery finds under the spec root (key relative to it).
    Source(String),
    /// specforge.json, specforge.lock, or an extension module the environment loaded.
    Environment,
    /// A file the checks read: specforge-cache.json (ADR 0009), or a file a
    /// `file_reference` field names.
    CheckInput,
    /// Anything else (an unloaded `.wasm`, an excluded `.spec`, target/…).
    Unrelated,
}

/// What a batch of changed paths means: the input to
/// [`crate::ProjectSession::apply`].
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Changes {
    /// Changed sources, by key relative to the spec root: sorted, deduplicated.
    pub sources: Vec<String>,
    /// An environment input changed: the environment must load again.
    pub environment: bool,
    /// A check input changed: the checks must run again.
    pub check_inputs: bool,
}

impl Changes {
    /// Nothing the session is built from changed.
    pub fn is_empty(&self) -> bool {
        self.sources.is_empty() && !self.environment && !self.check_inputs
    }

    /// The changes `roles` amount to.
    pub fn from_roles(roles: impl IntoIterator<Item = InputRole>) -> Self {
        let mut changes = Changes::default();
        for role in roles {
            match role {
                InputRole::Source(key) => changes.sources.push(key),
                InputRole::Environment => changes.environment = true,
                InputRole::CheckInput => changes.check_inputs = true,
                InputRole::Unrelated => {}
            }
        }
        changes.sources.sort();
        changes.sources.dedup();
        changes
    }
}

/// What kind of change an [`crate::Update`] applied.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdateKind {
    /// Sources were re-parsed (and every check ran again).
    Sources,
    /// Only the checks ran again: a check input changed.
    Checks,
    /// The environment loaded again and the project was rebuilt from disk.
    Environment,
}

/// Everything a project session depends on besides its sources' text:
/// where its sources are discovered (the spec root and `exclude`), the
/// files its environment was loaded from (`specforge.json`,
/// `specforge.lock`, each extension module it loaded) and the files its
/// last checks read (the build cache its check-phase passes read, each
/// file a `file_reference` field or a `file_exists` rule names, and the
/// directory of each such file that was missing, whose listing the E016
/// suggestion reads). Behavior `classify_project_changes`, ADR 0030.
///
/// One value per environment load, renewed each time the checks run;
/// `Update::inputs_changed` says when it changed. Every answer to "what
/// does this session depend on" is read from it: what a changed path is
/// ([`Self::classify`]), which directories a file watcher watches
/// ([`Self::watch_roots`]), which files an editor asks its client to report
/// ([`Self::watched`]), and, inside the crate, what the session stamps for
/// freshness and where it discovers its sources. A detached session's
/// inputs are [`Self::detached`]: no root, every `.spec` path a buffer
/// source keyed by itself, nothing to watch, nothing to stamp.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionInputs {
    place: Place,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Place {
    Detached,
    Disk(Box<OnDisk>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct OnDisk {
    root: PathBuf,
    spec_root: PathBuf,
    exclude: Exclude,
    config: PathBuf,
    lock: PathBuf,
    /// The module of every enabled extension that is not built in:
    /// `.specforge/extensions/<name>/extension.wasm` for an installed one,
    /// the resolved path of a local `.wasm` entry.
    modules: Vec<PathBuf>,
    /// `<root>/specforge-cache.json` when the environment has check-phase
    /// passes (they read it on every compile, ADR 0009).
    build_cache: Option<PathBuf>,
    /// The files the checks read, as they read them (OS resolution of `..`).
    named: Vec<PathBuf>,
    /// The directories of `named` that were missing when the checks ran.
    listings: Vec<PathBuf>,
    /// Directories on the way to an input's missing directory outside the
    /// root, from their nearest existing ancestor.
    pending: Vec<Pending>,
    canonical: Canonical,
}

/// An input whose directory does not exist yet, outside the root: what
/// creating the next directory on the way to it is.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Pending {
    /// The nearest existing ancestor of the missing directory (canonical).
    ancestor: PathBuf,
    /// The missing directory (canonical, as far as it can be).
    toward: PathBuf,
    role: InputRole,
}

impl OnDisk {
    /// `path` canonical, re-spelled from the root as it was opened when it
    /// lies under it.
    fn respell(&self, path: &Path) -> PathBuf {
        let path = canonical(path);
        match path.strip_prefix(&self.canonical.root) {
            Ok(relative) if relative.as_os_str().is_empty() => self.root.clone(),
            Ok(relative) => self.root.join(relative),
            Err(_) => path,
        }
    }
}

/// The canonical forms a changed path is compared with, taken when the
/// value is made.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Canonical {
    root: PathBuf,
    spec_root: PathBuf,
    environment: BTreeSet<PathBuf>,
    checks: BTreeSet<PathBuf>,
    listings: BTreeSet<PathBuf>,
}

/// A directory a file watcher watches.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WatchRoot {
    /// Canonical: the spelling a file watcher reports paths in.
    pub dir: PathBuf,
    /// Everything under it (the root, the spec root, an input's directory),
    /// or only its own entries.
    pub recursive: bool,
}

/// One thing an editor client must report changes to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Watched {
    /// Every `.spec` file under this directory, at any depth: the spec root.
    Sources(PathBuf),
    /// This one file: the config, the lock, a module, the build cache, a
    /// file the checks read.
    File(PathBuf),
    /// Any file directly in this directory: the directory of a file the
    /// checks read that is missing.
    Listing(PathBuf),
}

impl SessionInputs {
    /// A detached session's inputs (no project).
    pub fn detached() -> Self {
        SessionInputs {
            place: Place::Detached,
        }
    }

    /// The project root as the session was opened; `None` when detached.
    pub fn root(&self) -> Option<&Path> {
        self.disk().map(|disk| disk.root.as_path())
    }

    /// Where `.spec` files are discovered; `None` when detached.
    pub fn spec_root(&self) -> Option<&Path> {
        self.disk().map(|disk| disk.spec_root.as_path())
    }

    /// What `path` (absolute, or relative to the working directory) is to
    /// the session: a `.spec` file discovery finds under the spec root is a
    /// source keyed relative to it; the config, the lock and each loaded
    /// module are environment inputs; the build cache, each named file and
    /// any file directly in a missing named file's directory are check
    /// inputs; anything else is unrelated. Paths are compared canonically.
    /// Detached: a `.spec` path is a source keyed by itself, anything else
    /// unrelated.
    pub fn classify(&self, path: &Path) -> InputRole {
        let Some(disk) = self.disk() else {
            return if path.extension().is_some_and(|ext| ext == "spec") {
                InputRole::Source(path.to_string_lossy().into_owned())
            } else {
                InputRole::Unrelated
            };
        };
        let path = canonical(path);
        let known = &disk.canonical;
        if known.environment.contains(&path) {
            return InputRole::Environment;
        }
        if let Ok(relative) = path.strip_prefix(&known.spec_root) {
            let key = relative.to_string_lossy().into_owned();
            if !self.excludes(&key) {
                return InputRole::Source(key);
            }
        }
        if known.checks.contains(&path)
            || path
                .parent()
                .is_some_and(|dir| known.listings.contains(dir))
        {
            return InputRole::CheckInput;
        }
        // A directory created on the way to an input's missing directory.
        for pending in &disk.pending {
            if path.parent() == Some(pending.ancestor.as_path())
                && pending.toward.starts_with(&path)
            {
                return pending.role.clone();
            }
        }
        InputRole::Unrelated
    }

    /// What a batch of changed paths amounts to.
    pub fn changes<'p>(&self, paths: impl IntoIterator<Item = &'p Path>) -> Changes {
        Changes::from_roles(paths.into_iter().map(|path| self.classify(path)))
    }

    /// The directories a file watcher must watch to see a change to any
    /// input: the root, the spec root when it is outside the root, and the
    /// directory of every input outside both, recursively (canonical, none
    /// inside another); for an input directory outside the root that does
    /// not exist, its nearest existing ancestor, for its own entries only.
    /// Empty when detached or when the root does not exist.
    pub fn watch_roots(&self) -> Vec<WatchRoot> {
        let Some(disk) = self.disk() else {
            return Vec::new();
        };
        let candidates = [canonical(&disk.root), canonical(&disk.spec_root)]
            .into_iter()
            .chain(
                disk.modules
                    .iter()
                    .chain(&disk.build_cache)
                    .chain(&disk.named)
                    .filter_map(|path| path.parent().map(canonical)),
            )
            .filter(|dir| dir.is_dir());
        let mut roots: Vec<PathBuf> = Vec::new();
        for dir in candidates {
            if roots.iter().any(|root| dir.starts_with(root)) {
                continue;
            }
            roots.retain(|root| !root.starts_with(&dir));
            roots.push(dir);
        }
        let mut watch_roots: Vec<WatchRoot> = roots
            .iter()
            .map(|dir| WatchRoot {
                dir: dir.clone(),
                recursive: true,
            })
            .collect();
        // An input's missing directory outside the root is watched from its
        // nearest existing ancestor, for that ancestor's own entries only.
        if !roots.is_empty() {
            for pending in &disk.pending {
                let covered = roots.iter().any(|root| pending.ancestor.starts_with(root));
                let watch = WatchRoot {
                    dir: pending.ancestor.clone(),
                    recursive: false,
                };
                if !covered && !watch_roots.contains(&watch) {
                    watch_roots.push(watch);
                }
            }
        }
        watch_roots
    }

    /// What an editor client must report: the spec root's `.spec` files,
    /// then the config, the lock, each module, the build cache, each named
    /// file and each missing named file's directory. Each path is canonical
    /// (as the checks read it, `..` resolved), re-spelled from the root as
    /// the session was opened when it lies under it (the spelling the
    /// editor gave), absolute otherwise. Empty when detached.
    pub fn watched(&self) -> Vec<Watched> {
        let Some(disk) = self.disk() else {
            return Vec::new();
        };
        let spelled = |path: &Path| disk.respell(path);
        std::iter::once(Watched::Sources(spelled(&disk.spec_root)))
            .chain(
                [&disk.config, &disk.lock]
                    .into_iter()
                    .chain(&disk.modules)
                    .chain(&disk.build_cache)
                    .chain(&disk.named)
                    .map(|path| Watched::File(spelled(path))),
            )
            .chain(
                disk.listings
                    .iter()
                    .map(|dir| Watched::Listing(spelled(dir))),
            )
            .collect()
    }

    fn disk(&self) -> Option<&OnDisk> {
        match &self.place {
            Place::Detached => None,
            Place::Disk(disk) => Some(disk),
        }
    }
}

// Crate-private: how the session builds, renews and reads it.
impl SessionInputs {
    /// The inputs an environment rooted at `root` with `config` is loaded
    /// from, before it is: config, lock, the module of every enabled
    /// extension that is not built in, the spec root and `exclude`.
    pub(crate) fn opened(root: &Path, config: &ProjectConfig) -> Self {
        // The paths, before anything is read: `Installed::at` reads the lock
        // once the session has stamped it.
        let installed = Installed::unread(root);
        let modules = installed.modules(&config.extensions, &builtins());
        Self::on_disk(OnDisk {
            root: root.to_path_buf(),
            spec_root: config.spec_root_in(root),
            exclude: config.exclude.clone(),
            config: root.join("specforge.json"),
            lock: installed.lock_path(),
            modules,
            build_cache: None,
            named: Vec::new(),
            listings: Vec::new(),
            pending: Vec::new(),
            canonical: Canonical::empty(),
        })
    }

    /// With the build cache, when the loaded environment declares
    /// check-phase passes.
    pub(crate) fn with_check_passes(self, check_passes: bool) -> Self {
        match self.place {
            Place::Detached => self,
            Place::Disk(mut disk) => {
                disk.build_cache = check_passes.then(|| disk.root.join(BUILD_CACHE_FILE));
                Self::on_disk(*disk)
            }
        }
    }

    /// With `named`, the files the checks about to run read
    /// ([`specforge_registry::RegistryBuild::files`]); the missing ones' directories are taken now.
    pub(crate) fn with_named(&self, named: Vec<PathBuf>) -> Self {
        match &self.place {
            Place::Detached => self.clone(),
            Place::Disk(disk) => {
                let mut disk = (**disk).clone();
                disk.listings = named
                    .iter()
                    .filter(|path| !path.exists())
                    .filter_map(|path| path.parent().map(Path::to_path_buf))
                    .collect();
                disk.listings.sort();
                disk.listings.dedup();
                disk.named = named;
                Self::on_disk(disk)
            }
        }
    }

    /// The lock and the modules: stamped before the runtime and the
    /// environment read them (the config is stamped before its one read).
    pub(crate) fn environment_files(&self) -> impl Iterator<Item = &Path> {
        self.disk()
            .into_iter()
            .flat_map(|disk| std::iter::once(&disk.lock).chain(&disk.modules))
            .map(PathBuf::as_path)
    }

    /// Build cache, named files, missing files' directories: stamped
    /// before the checks read them.
    pub(crate) fn check_files(&self) -> impl Iterator<Item = &Path> {
        self.disk()
            .into_iter()
            .flat_map(|disk| {
                disk.build_cache
                    .iter()
                    .chain(&disk.named)
                    .chain(&disk.listings)
            })
            .map(PathBuf::as_path)
    }

    /// The `.spec` files discovery finds now (none when detached).
    pub(crate) fn discover(&self) -> Vec<PathBuf> {
        self.disk()
            .map(|disk| discover_spec_files(&disk.spec_root, &disk.exclude))
            .unwrap_or_default()
    }

    /// A changed key the session ignores (never, detached).
    pub(crate) fn excludes(&self, key: &str) -> bool {
        self.disk()
            .is_some_and(|disk| !is_discovered(key, &disk.exclude))
    }

    /// The value for `disk`, its canonical forms taken now.
    fn on_disk(mut disk: OnDisk) -> Self {
        disk.canonical = Canonical {
            root: canonical(&disk.root),
            spec_root: canonical(&disk.spec_root),
            environment: [&disk.config, &disk.lock]
                .into_iter()
                .chain(&disk.modules)
                .map(|path| canonical(path))
                .collect(),
            checks: disk
                .build_cache
                .iter()
                .chain(&disk.named)
                .map(|path| canonical(path))
                .collect(),
            // A missing referenced file's suggestion names a similar file
            // in its directory: a file created or deleted there changes it.
            listings: disk.listings.iter().map(|dir| canonical(dir)).collect(),
        };
        disk.pending = disk.pending_directories();
        SessionInputs {
            place: Place::Disk(Box::new(disk)),
        }
    }
}

impl OnDisk {
    /// The missing directories of the inputs outside the root, each with
    /// its nearest existing ancestor. A root that does not exist has none.
    fn pending_directories(&self) -> Vec<Pending> {
        if !self.canonical.root.is_dir() {
            return Vec::new();
        }
        let toward = |dir: &Path, role: &InputRole| -> Option<Pending> {
            let toward = canonical(dir);
            if toward.is_dir() || toward.starts_with(&self.canonical.root) {
                return None;
            }
            let mut ancestor = toward.parent()?;
            while !ancestor.is_dir() {
                ancestor = ancestor.parent()?;
            }
            Some(Pending {
                ancestor: ancestor.to_path_buf(),
                toward,
                role: role.clone(),
            })
        };
        let environment = InputRole::Environment;
        let check = InputRole::CheckInput;
        let mut pending: Vec<Pending> = Vec::new();
        let candidates = std::iter::once((self.spec_root.as_path(), &environment))
            .chain(
                self.modules
                    .iter()
                    .filter_map(|module| Some((module.parent()?, &environment))),
            )
            .chain(
                self.build_cache
                    .iter()
                    .chain(&self.named)
                    .filter_map(|file| Some((file.parent()?, &check))),
            );
        for (dir, role) in candidates {
            if let Some(next) = toward(dir, role)
                && !pending.contains(&next)
            {
                pending.push(next);
            }
        }
        pending
    }
}

impl Canonical {
    fn empty() -> Self {
        Canonical {
            root: PathBuf::new(),
            spec_root: PathBuf::new(),
            environment: BTreeSet::new(),
            checks: BTreeSet::new(),
            listings: BTreeSet::new(),
        }
    }
}

impl Environment {
    /// A `.spec` path's key: relative to the spec root when the file is
    /// under it (as `specforge check` names it), else the path itself.
    /// With no spec root (no project), every key is the path itself.
    pub fn source_key(&self, path: &Path) -> String {
        source_key(&self.spec_root, path)
    }
}

/// `path`'s key under `spec_root` (see [`Environment::source_key`]). The
/// editor and the workspace may spell one directory differently (a
/// symlinked temp dir), so canonical forms are compared when the plain
/// ones differ; a deleted file is compared through its directory.
pub fn source_key(spec_root: &Path, path: &Path) -> String {
    if spec_root.as_os_str().is_empty() {
        return path.to_string_lossy().into_owned();
    }
    if let Ok(relative) = path.strip_prefix(spec_root) {
        return relative.to_string_lossy().into_owned();
    }
    if let Ok(canonical_root) = std::fs::canonicalize(spec_root)
        && let Ok(relative) = canonical(path).strip_prefix(&canonical_root)
    {
        return relative.to_string_lossy().into_owned();
    }
    path.to_string_lossy().into_owned()
}

/// `path`, canonical when it can be: the path itself when it exists, else
/// its directory's canonical form joined with its name (a deleted file),
/// else as given.
pub(crate) fn canonical(path: &Path) -> PathBuf {
    if let Ok(canonical) = std::fs::canonicalize(path) {
        return canonical;
    }
    match (path.parent(), path.file_name()) {
        (Some(parent), Some(name)) if !parent.as_os_str().is_empty() => {
            canonical(parent).join(name)
        }
        _ => path.to_path_buf(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(extensions: &[&str]) -> ProjectConfig {
        ProjectConfig {
            extensions: extensions.iter().map(|e| e.to_string()).collect(),
            ..ProjectConfig::default()
        }
    }

    #[test]
    fn a_value_made_twice_from_the_same_config_is_equal() {
        let dir = tempfile::TempDir::new().unwrap();
        let config = config(&["@acme/local=ext/local.wasm"]);
        assert_eq!(
            SessionInputs::opened(dir.path(), &config),
            SessionInputs::opened(dir.path(), &config)
        );
        assert_ne!(
            SessionInputs::opened(dir.path(), &config),
            SessionInputs::opened(dir.path(), &self::config(&[]))
        );
    }

    #[test]
    fn named_files_change_the_value() {
        let dir = tempfile::TempDir::new().unwrap();
        let opened = SessionInputs::opened(dir.path(), &config(&[]));
        let named = vec![dir.path().join("docs/guide.md")];
        let with = opened.with_named(named.clone());
        assert_ne!(opened, with);
        assert_eq!(with, opened.with_named(named));
        assert_eq!(with.with_named(Vec::new()), opened);
        // The missing file's directory is stamped with it.
        let files: Vec<&Path> = with.check_files().collect();
        assert!(
            files.contains(&dir.path().join("docs").as_path()),
            "{files:?}"
        );
        assert!(files.contains(&dir.path().join("docs/guide.md").as_path()));
    }

    #[test]
    fn classify_compares_canonical_forms() {
        let dir = tempfile::TempDir::new().unwrap();
        let real = dir.path().join("real");
        std::fs::create_dir_all(real.join("spec")).unwrap();
        let link = dir.path().join("link");
        #[cfg(unix)]
        std::os::unix::fs::symlink(&real, &link).unwrap();
        #[cfg(not(unix))]
        let link = real.clone();
        let inputs = SessionInputs::opened(&link, &config(&[]));
        std::fs::write(real.join("specforge.json"), "{}").unwrap();

        // The same file, however the directory is spelled.
        assert_eq!(
            inputs.classify(&real.join("specforge.json")),
            InputRole::Environment
        );
        assert_eq!(
            inputs.classify(&link.join("specforge.json")),
            InputRole::Environment
        );
        assert_eq!(
            inputs.classify(&real.join("a.spec")),
            InputRole::Source("a.spec".to_string())
        );
    }

    #[test]
    fn detached_inputs_answer_nothing_but_spec_buffers() {
        let inputs = SessionInputs::detached();
        assert_eq!(inputs.root(), None);
        assert_eq!(inputs.spec_root(), None);
        assert!(inputs.watch_roots().is_empty());
        assert!(inputs.watched().is_empty());
        assert!(inputs.discover().is_empty());
        assert!(!inputs.excludes("target/x.spec"));
        assert_eq!(inputs.environment_files().count(), 0);
        assert_eq!(inputs.check_files().count(), 0);
        assert_eq!(
            inputs.classify(Path::new("/x/a.spec")),
            InputRole::Source("/x/a.spec".to_string())
        );
        assert_eq!(
            inputs.classify(Path::new("/x/specforge.json")),
            InputRole::Unrelated
        );
        assert_eq!(inputs.with_named(vec!["/x/y".into()]), inputs);
    }

    #[test]
    fn watched_paths_are_spelled_from_the_root_as_opened() {
        let dir = tempfile::TempDir::new().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join("spec")).unwrap();
        std::fs::create_dir_all(root.join("docs")).unwrap();
        let config = ProjectConfig {
            spec_root: Some("spec".to_string()),
            extensions: vec!["@acme/local=ext/local.wasm".to_string()],
            ..ProjectConfig::default()
        };
        // Named as the checks join them: through `spec/..`.
        let inputs = SessionInputs::opened(root, &config)
            .with_named(vec![root.join("spec/../docs/guide.md")]);

        assert_eq!(
            inputs.watched(),
            vec![
                Watched::Sources(root.join("spec")),
                Watched::File(root.join("specforge.json")),
                Watched::File(root.join("specforge.lock")),
                Watched::File(root.join("ext/local.wasm")),
                Watched::File(root.join("docs/guide.md")),
                Watched::Listing(root.join("docs")),
            ]
        );
    }

    #[test]
    fn a_watched_path_outside_the_root_is_canonical() {
        let dir = tempfile::TempDir::new().unwrap();
        let root = dir.path().join("proj");
        std::fs::create_dir_all(&root).unwrap();
        let outside = dir.path().join("outside");
        std::fs::create_dir_all(&outside).unwrap();
        let inputs = SessionInputs::opened(&root, &ProjectConfig::default())
            .with_named(vec![root.join("../outside/guide.md")]);

        let canonical_outside = std::fs::canonicalize(&outside).unwrap();
        let watched = inputs.watched();
        assert!(
            watched.contains(&Watched::File(canonical_outside.join("guide.md"))),
            "{watched:?}"
        );
        assert!(
            watched.contains(&Watched::Listing(canonical_outside.clone())),
            "{watched:?}"
        );
        // The root itself is spelled as opened.
        assert_eq!(watched[0], Watched::Sources(root));
    }

    #[test]
    fn a_module_in_a_missing_directory_is_pending_as_an_environment_input() {
        let dir = tempfile::TempDir::new().unwrap();
        let root = dir.path().join("proj");
        std::fs::create_dir_all(&root).unwrap();
        let far = dir.path().join("far");
        std::fs::create_dir_all(&far).unwrap();
        let module = far.join("deep/mods/ext.wasm");
        let inputs = SessionInputs::opened(
            &root,
            &config(&[&format!("@acme/far={}", module.display())]),
        );
        let far = std::fs::canonicalize(&far).unwrap();

        // The nearest existing ancestor is watched for its own entries.
        assert!(
            inputs.watch_roots().contains(&WatchRoot {
                dir: far.clone(),
                recursive: false
            }),
            "{:?}",
            inputs.watch_roots()
        );
        // A directory created on the way is the module's change; one
        // beside it is nothing.
        assert_eq!(inputs.classify(&far.join("deep")), InputRole::Environment);
        assert_eq!(inputs.classify(&far.join("other")), InputRole::Unrelated);
        // Below the first directory on the way is not this ancestor's.
        assert_eq!(
            inputs.classify(&far.join("deep/mods")),
            InputRole::Unrelated
        );
        // Once the directory exists, it is watched whole.
        std::fs::create_dir_all(far.join("deep/mods")).unwrap();
        let inputs = SessionInputs::opened(
            &root,
            &config(&[&format!("@acme/far={}", module.display())]),
        );
        assert!(
            inputs.watch_roots().contains(&WatchRoot {
                dir: far.join("deep/mods"),
                recursive: true
            }),
            "{:?}",
            inputs.watch_roots()
        );
    }

    #[test]
    fn a_missing_root_has_no_watch_roots() {
        let dir = tempfile::TempDir::new().unwrap();
        let inputs = SessionInputs::opened(&dir.path().join("nope"), &config(&[]));
        assert!(inputs.watch_roots().is_empty());
        assert_eq!(inputs.classify(dir.path()), InputRole::Unrelated);
    }
}
