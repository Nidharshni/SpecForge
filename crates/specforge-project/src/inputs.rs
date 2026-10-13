//! What a project session depends on, and what a changed path reaches of it
//! (behavior `classify_project_changes`, ADR 0030, ADR 0073).
//!
//! A session is built from its sources (the `.spec` files discovery finds
//! under the spec root), its environment inputs (`specforge.json`,
//! `specforge.lock` and the extension modules the environment loads) and
//! its check inputs (files the checks read but no parse does: the build
//! cache check-phase passes read, the files `file_reference` fields and
//! `file_exists` rules name). [`SessionInputs`] is that set as one value;
//! the session asks its inputs what a changed path reaches
//! (`ProjectSession::changes`), and watch and the LSP ask them which
//! directories to watch and what to report.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use specforge_common::{ProjectConfig, SourceRoot};

use crate::build_cache::BUILD_CACHE_FILE;
use crate::{Environment, builtins};
use specforge_installed::Installed;

/// What a batch of changed paths, or the disk since the last read, means to
/// a session: the input to [`crate::ProjectSession::apply`]. Produced only
/// by [`crate::ProjectSession::changes`] and [`crate::ProjectSession::stale`].
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Changes {
    /// Changed sources, by key relative to the spec root: sorted, deduplicated.
    pub sources: Vec<String>,
    /// The environment must load again. Each path of the batch that reached
    /// an environment input which changed, as the batch spelled it (for
    /// `stale`, each changed input as the session recorded it): sorted,
    /// deduplicated.
    pub environment: Vec<PathBuf>,
    /// The checks must run again: the paths that reached a changed check
    /// input, named the same way.
    pub check_inputs: Vec<PathBuf>,
}

impl Changes {
    /// Nothing the session is built from changed.
    pub fn is_empty(&self) -> bool {
        self.sources.is_empty() && self.environment.is_empty() && self.check_inputs.is_empty()
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
/// suggestion reads), and the target of each symlinked source (where its text
/// lies). Behavior `classify_project_changes`, ADR 0030.
///
/// One value per environment load, renewed each time the checks run;
/// `Update::inputs_changed` says when it changed. Every answer to "what
/// does this session depend on" is read from it: what a changed path
/// reaches (inside the crate, for [`crate::ProjectSession::changes`]),
/// which directories a file watcher watches
/// ([`Self::watch_roots`]), which files an editor asks its client to report
/// ([`Self::watched`]), and, inside the crate, what the session stamps for
/// freshness and where it discovers its sources. A detached session's
/// inputs are [`Self::detached`]: no root, nothing to reach, nothing to
/// watch, nothing to stamp.
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
    sources: SourceRoot,
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
    /// The symlinked sources, by key (sorted): where each one's text lies.
    links: Vec<Link>,
    /// Directories on the way to an input's missing directory outside the
    /// root, from their nearest existing ancestor.
    pending: Vec<Pending>,
    canonical: Canonical,
}

/// A source that is a symlink to a file: keyed by its own name, read through
/// the link.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Link {
    key: String,
    /// Where the text lies (canonical).
    target: PathBuf,
}

/// An input whose directory does not exist yet, outside the root: what
/// creating the next directory on the way to it is.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Pending {
    /// The nearest existing ancestor of the missing directory (canonical).
    ancestor: PathBuf,
    /// The missing directory (canonical, as far as it can be).
    toward: PathBuf,
    kind: Kind,
}

/// Environment or check input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    Environment,
    Check,
}

/// What a batch of changed paths reaches of what a session read
/// ([`SessionInputs::reach`]).
#[derive(Debug, Default)]
pub(crate) struct Reach {
    /// Keys relative to the spec root whose sources (read, or found now) are
    /// compared; "" is every source. Sorted; none lies under another.
    pub(crate) regions: Vec<String>,
    /// Each batch path that reached a recorded input, with the inputs it
    /// reached, as recorded.
    pub(crate) inputs: Vec<(PathBuf, Vec<(Kind, PathBuf)>)>,
    /// Each batch path that is a directory created on the way to a missing
    /// input's directory.
    pub(crate) pending: Vec<(PathBuf, Kind)>,
    /// Symlinked sources a path reached through their target, by key.
    pub(crate) linked: std::collections::BTreeSet<String>,
}

impl OnDisk {
    /// The link `path` (a source under the spec root) is: it is a symlink
    /// that resolves to a file.
    fn link_at(&self, path: &Path) -> Option<Link> {
        if !std::fs::symlink_metadata(path)
            .ok()?
            .file_type()
            .is_symlink()
        {
            return None;
        }
        let target = std::fs::canonicalize(path).ok()?;
        if !target.is_file() {
            return None;
        }
        let key = path.strip_prefix(self.sources.dir()).ok()?;
        Some(Link {
            key: key.to_string_lossy().into_owned(),
            target,
        })
    }

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
    /// Canonical form to the path as recorded.
    environment: BTreeMap<PathBuf, PathBuf>,
    checks: BTreeMap<PathBuf, PathBuf>,
    listings: BTreeMap<PathBuf, PathBuf>,
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
    /// Every entry created or deleted under this directory, at any depth: the
    /// project root, and the spec root when it lies outside the root. A
    /// directory created, moved or deleted there reaches every source and
    /// input under it (ADR 0073).
    Directories(PathBuf),
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
        self.disk().map(|disk| disk.sources.dir())
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
        let candidates = [canonical(&disk.root), canonical(disk.sources.dir())]
            .into_iter()
            .chain(
                disk.modules
                    .iter()
                    .chain(&disk.build_cache)
                    .chain(&disk.named)
                    .filter_map(|path| path.parent().map(canonical))
                    .chain(
                        disk.links
                            .iter()
                            .filter_map(|link| link.target.parent().map(Path::to_path_buf)),
                    ),
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

    /// What an editor client must report: the spec root's `.spec` files, every
    /// entry created or deleted under the root (and under the spec root when
    /// it lies outside it), then the config, the lock, each module, the build cache, each named
    /// file and each missing named file's directory. Each path is canonical
    /// (as the checks read it, `..` resolved), re-spelled from the root as
    /// the session was opened when it lies under it (the spelling the
    /// editor gave), absolute otherwise. Empty when detached.
    pub fn watched(&self) -> Vec<Watched> {
        let Some(disk) = self.disk() else {
            return Vec::new();
        };
        let spelled = |path: &Path| disk.respell(path);
        let spec_root_apart = !disk.canonical.spec_root.starts_with(&disk.canonical.root);
        std::iter::once(Watched::Sources(spelled(disk.sources.dir())))
            .chain(std::iter::once(Watched::Directories(spelled(&disk.root))))
            .chain(spec_root_apart.then(|| Watched::Directories(spelled(disk.sources.dir()))))
            .chain(
                [&disk.config, &disk.lock]
                    .into_iter()
                    .chain(&disk.modules)
                    .chain(&disk.build_cache)
                    .chain(&disk.named)
                    .map(|path| Watched::File(spelled(path))),
            )
            .chain(
                disk.links
                    .iter()
                    .filter(|link| {
                        link.target
                            .strip_prefix(&disk.canonical.spec_root)
                            .map_or(true, |relative| {
                                !disk.sources.holds(&relative.to_string_lossy())
                            })
                    })
                    .map(|link| Watched::File(spelled(&link.target))),
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
            sources: config.sources(root),
            config: root.join("specforge.json"),
            lock: installed.lock_path(),
            modules,
            build_cache: None,
            named: Vec::new(),
            listings: Vec::new(),
            links: Vec::new(),
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
            .map(|disk| disk.sources.files())
            .unwrap_or_default()
    }

    /// With the symlinked sources among `discovered` (every source found).
    pub(crate) fn linked(&self, discovered: &[PathBuf]) -> Self {
        match &self.place {
            Place::Detached => self.clone(),
            Place::Disk(disk) => {
                let mut disk = (**disk).clone();
                disk.links = discovered
                    .iter()
                    .filter_map(|path| disk.link_at(path))
                    .collect();
                disk.links.sort_by(|a, b| a.key.cmp(&b.key));
                Self::on_disk(disk)
            }
        }
    }

    /// With the links of `keys` read again, the others kept.
    pub(crate) fn relinked(&self, keys: &[String]) -> Self {
        match &self.place {
            Place::Detached => self.clone(),
            Place::Disk(disk) => {
                let mut disk = (**disk).clone();
                disk.links.retain(|link| !keys.contains(&link.key));
                let again: Vec<Link> = keys
                    .iter()
                    .filter_map(|key| disk.link_at(&disk.sources.dir().join(key)))
                    .collect();
                disk.links.extend(again);
                disk.links.sort_by(|a, b| a.key.cmp(&b.key));
                disk.links.dedup();
                Self::on_disk(disk)
            }
        }
    }

    /// What `paths` (absolute, spelled as a watcher or an editor reports
    /// them) reach of what the session read: the sources at or under each
    /// (a region), and the environment and check inputs each is or lies
    /// above. Detached: nothing.
    pub(crate) fn reach<'p>(&self, paths: impl IntoIterator<Item = &'p Path>) -> Reach {
        let Some(disk) = self.disk() else {
            return Reach::default();
        };
        let known = &disk.canonical;
        let mut parents: BTreeMap<PathBuf, PathBuf> = BTreeMap::new();
        let mut regions: Vec<String> = Vec::new();
        let mut reach = Reach::default();
        for path in paths {
            let canonical_path = canonical(path);
            let in_dir = match (path.parent(), path.file_name()) {
                (Some(parent), Some(name)) if !parent.as_os_str().is_empty() => parents
                    .entry(parent.to_path_buf())
                    .or_insert_with(|| canonical(parent))
                    .join(name),
                _ => path.to_path_buf(),
            };
            if known.spec_root.starts_with(&canonical_path) {
                regions.push(String::new());
            } else if let Ok(relative) = in_dir.strip_prefix(&known.spec_root) {
                regions.push(relative.to_string_lossy().into_owned());
            }
            let mut reached: Vec<(Kind, PathBuf)> = Vec::new();
            for (known, recorded) in &known.environment {
                if known.starts_with(&canonical_path) {
                    reached.push((Kind::Environment, recorded.clone()));
                }
            }
            for (known, recorded) in &known.checks {
                if known.starts_with(&canonical_path) {
                    reached.push((Kind::Check, recorded.clone()));
                }
            }
            for (known, recorded) in &known.listings {
                if known.starts_with(&canonical_path)
                    || canonical_path.parent() == Some(known.as_path())
                {
                    reached.push((Kind::Check, recorded.clone()));
                }
            }
            if !reached.is_empty() {
                reach.inputs.push((path.to_path_buf(), reached));
            }
            // A symlinked source whose target is this path, or under it.
            for link in &disk.links {
                if link.target.starts_with(&canonical_path) {
                    reach.linked.insert(link.key.clone());
                }
            }
            // A directory created on the way to an input's missing directory.
            for pending in &disk.pending {
                if canonical_path.parent() == Some(pending.ancestor.as_path())
                    && pending.toward.starts_with(&canonical_path)
                {
                    reach.pending.push((path.to_path_buf(), pending.kind));
                }
            }
        }
        regions.sort();
        regions.dedup();
        let mut kept: Vec<String> = Vec::new();
        for region in regions {
            if !kept
                .iter()
                .any(|outer| outer.is_empty() || Path::new(&region).starts_with(outer))
            {
                kept.push(region);
            }
        }
        reach.regions = kept;
        reach
    }

    /// The sources discovery finds at `key` now (a directory: all under it; a
    /// source file: itself; else none), by key. "" is every source.
    pub(crate) fn discover_at(&self, key: &str) -> Vec<String> {
        let Some(disk) = self.disk() else {
            return Vec::new();
        };
        disk.sources
            .files_at(&disk.sources.dir().join(key))
            .into_iter()
            .map(|path| {
                path.strip_prefix(disk.sources.dir())
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .into_owned()
            })
            .collect()
    }

    /// A changed key the session ignores (never, detached).
    pub(crate) fn excludes(&self, key: &str) -> bool {
        self.disk().is_some_and(|disk| !disk.sources.holds(key))
    }

    /// The value for `disk`, its canonical forms taken now.
    fn on_disk(mut disk: OnDisk) -> Self {
        disk.canonical = Canonical {
            root: canonical(&disk.root),
            spec_root: canonical(disk.sources.dir()),
            environment: [&disk.config, &disk.lock]
                .into_iter()
                .chain(&disk.modules)
                .map(|path| (canonical(path), path.clone()))
                .collect(),
            checks: disk
                .build_cache
                .iter()
                .chain(&disk.named)
                .map(|path| (canonical(path), path.clone()))
                .collect(),
            // A missing referenced file's suggestion names a similar file
            // in its directory: a file created or deleted there changes it.
            listings: disk
                .listings
                .iter()
                .map(|dir| (canonical(dir), dir.clone()))
                .collect(),
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
        let toward = |dir: &Path, kind: Kind| -> Option<Pending> {
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
                kind,
            })
        };
        let mut pending: Vec<Pending> = Vec::new();
        let candidates = std::iter::once((self.sources.dir(), Kind::Environment))
            .chain(
                self.modules
                    .iter()
                    .filter_map(|module| Some((module.parent()?, Kind::Environment))),
            )
            .chain(
                self.build_cache
                    .iter()
                    .chain(&self.named)
                    .filter_map(|file| Some((file.parent()?, Kind::Check))),
            );
        for (dir, kind) in candidates {
            if let Some(next) = toward(dir, kind)
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
            environment: BTreeMap::new(),
            checks: BTreeMap::new(),
            listings: BTreeMap::new(),
        }
    }
}

impl Environment {
    /// A `.spec` path's key: relative to the spec root when the file is
    /// under it (as `specforge check` names it), else the path itself.
    /// With no spec root (no project), every key is the path itself.
    pub fn source_key(&self, path: &Path) -> String {
        source_key(self.sources.dir(), path)
    }
}

/// `path`'s key under `spec_root` (see [`Environment::source_key`]). The
/// editor and the workspace may spell one directory differently (a
/// symlinked temp dir), so when the plain forms differ the path is compared
/// by its directory's canonical form and its own name: a symlinked file is
/// keyed by its own name, never its target's, and a deleted file is keyed
/// the same way.
pub fn source_key(spec_root: &Path, path: &Path) -> String {
    if spec_root.as_os_str().is_empty() {
        return path.to_string_lossy().into_owned();
    }
    if let Ok(relative) = path.strip_prefix(spec_root) {
        return relative.to_string_lossy().into_owned();
    }
    if let Ok(canonical_root) = std::fs::canonicalize(spec_root)
        && let Ok(relative) = in_canonical_dir(path).strip_prefix(&canonical_root)
    {
        return relative.to_string_lossy().into_owned();
    }
    path.to_string_lossy().into_owned()
}

/// `path` with its directory canonical and its own name kept: a symlink stays
/// itself. `path` as given when it has no directory or no name.
fn in_canonical_dir(path: &Path) -> PathBuf {
    match (path.parent(), path.file_name()) {
        (Some(parent), Some(name)) if !parent.as_os_str().is_empty() => {
            canonical(parent).join(name)
        }
        _ => path.to_path_buf(),
    }
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
        for config in [real.join("specforge.json"), link.join("specforge.json")] {
            let reach = inputs.reach([config.as_path()]);
            assert_eq!(reach.inputs.len(), 1, "{reach:?}");
            assert_eq!(reach.inputs[0].1[0].0, Kind::Environment);
        }
        assert_eq!(
            inputs.reach([real.join("a.spec").as_path()]).regions,
            vec!["a.spec".to_string()]
        );
    }

    #[test]
    fn detached_inputs_reach_nothing() {
        let inputs = SessionInputs::detached();
        assert_eq!(inputs.root(), None);
        assert_eq!(inputs.spec_root(), None);
        assert!(inputs.watch_roots().is_empty());
        assert!(inputs.watched().is_empty());
        assert!(inputs.discover().is_empty());
        assert!(!inputs.excludes("target/x.spec"));
        assert_eq!(inputs.environment_files().count(), 0);
        assert_eq!(inputs.check_files().count(), 0);
        let reach = inputs.reach([Path::new("/x/a.spec"), Path::new("/x/specforge.json")]);
        assert!(reach.regions.is_empty() && reach.inputs.is_empty() && reach.pending.is_empty());
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
                Watched::Directories(root.to_path_buf()),
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
        assert_eq!(
            inputs.reach([far.join("deep").as_path()]).pending,
            vec![(far.join("deep"), Kind::Environment)]
        );
        assert!(
            inputs
                .reach([far.join("other").as_path()])
                .pending
                .is_empty()
        );
        // Below the first directory on the way is not this ancestor's.
        assert!(
            inputs
                .reach([far.join("deep/mods").as_path()])
                .pending
                .is_empty()
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

    #[cfg(unix)]
    #[test]
    fn a_source_key_keeps_a_symlinks_own_name() {
        let dir = tempfile::TempDir::new().unwrap();
        let dir = std::fs::canonicalize(dir.path()).unwrap();
        let real = dir.join("real");
        std::fs::create_dir_all(real.join("shared")).unwrap();
        std::fs::write(real.join("shared/r.spec"), "").unwrap();
        // The root spelled two ways: through `link` and as `real`.
        std::os::unix::fs::symlink(&real, dir.join("link")).unwrap();
        std::os::unix::fs::symlink("shared/r.spec", real.join("alias.spec")).unwrap();

        assert_eq!(
            source_key(&dir.join("link"), &real.join("alias.spec")),
            "alias.spec"
        );
    }

    #[test]
    fn a_missing_root_has_no_watch_roots() {
        let dir = tempfile::TempDir::new().unwrap();
        let inputs = SessionInputs::opened(&dir.path().join("nope"), &config(&[]));
        assert!(inputs.watch_roots().is_empty());
        assert!(inputs.discover_at("").is_empty());
    }
}
