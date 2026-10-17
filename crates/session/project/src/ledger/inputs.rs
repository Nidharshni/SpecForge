//! [`SessionInputs`]: what a project session depends on, as one value
//! (behavior `classify_project_changes`, ADR 0030).

use std::path::{Path, PathBuf};

use specforge_common::{ProjectConfig, SourceRoot};

use super::key::{canonical, discovered, in_canonical_dir, relative_canonical};
use super::recorded::{Class, Pending, Recorded};

/// Everything a project session depends on besides its sources' text:
/// where its sources are discovered (the spec root and `exclude`), the
/// files its environment load read (`specforge.json`, `specforge.lock`, each
/// extension module the load read, and each hop of a symlink among them,
/// ADR 0114) and the files its
/// last checks read (every path a check asked about, the build cache a
/// check-phase pass read, and every directory E016's suggestion listed,
/// recorded when read, ADR 0107), and the link hops each symlinked
/// source's last read followed (its target included, ADR 0129). Behavior `classify_project_changes`, ADR 0030.
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
pub(super) struct OnDisk {
    pub(super) root: PathBuf,
    pub(super) sources: SourceRoot,
    /// Every recorded input (ADR 0133), ordered by class: the environment's
    /// files in read order, the check files sorted, each source's link hops
    /// (by key, in read order), then the listed directories sorted.
    pub(super) rows: Vec<Recorded>,
    /// The spec root's missing directory outside the root (an environment
    /// change, ADR 0030 D6).
    pub(super) spec_root_pending: Option<Pending>,
    /// The root and the spec root, canonical, taken when the value is made.
    pub(super) canonical_root: PathBuf,
    pub(super) canonical_spec_root: PathBuf,
}

impl OnDisk {
    /// `path` canonical, re-spelled from the root as it was opened when it
    /// lies under it.
    fn respell(&self, path: &Path) -> PathBuf {
        self.respelled(canonical(path))
    }

    fn respelled(&self, path: PathBuf) -> PathBuf {
        match path.strip_prefix(&self.canonical_root) {
            Ok(relative) if relative.as_os_str().is_empty() => self.root.clone(),
            Ok(relative) => self.root.join(relative),
            Err(_) => path,
        }
    }

    /// The row for `path` read as `class`: its key and pending directory
    /// taken now.
    fn row(&self, class: Class, path: PathBuf, root_exists: bool) -> Recorded {
        let key = match class {
            Class::Listing => canonical(&path),
            _ => in_canonical_dir(&path),
        };
        let pending = match class {
            // shortcut: a hop's pending directory is T6's fix; this commit
            // changes no behaviour.
            Class::Environment | Class::Check if root_exists => path
                .parent()
                .and_then(|dir| Pending::toward(dir, &self.canonical_root)),
            _ => None,
        };
        Recorded {
            path,
            class,
            key,
            pending,
        }
    }
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
    /// Every `.spec` file and every `specforge.json` under this directory,
    /// at any depth: the spec root.
    Sources(PathBuf),
    /// Every entry created or deleted under this directory, at any depth: the
    /// project root, and the spec root when it lies outside the root. A
    /// directory created, moved or deleted there reaches every source and
    /// input under it (ADR 0073).
    Directories(PathBuf),
    /// This one file: the config, the lock, a module, a file the checks read.
    File(PathBuf),
    /// Any file directly in this directory: a directory the checks listed.
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
                disk.rows
                    .iter()
                    .filter_map(|row| row.class.directory(&row.path).map(canonical)),
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
            let pending = disk
                .spec_root_pending
                .iter()
                .chain(disk.rows.iter().filter_map(|row| row.pending.as_ref()));
            for pending in pending {
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
    /// it lies outside it), then the files the environment load read (the config,
    /// the lock, each module), each file the checks read and each directory they
    /// listed. Each path is canonical
    /// (as the checks read it, `..` resolved), re-spelled from the root as
    /// the session was opened when it lies under it (the spelling the
    /// editor gave), absolute otherwise. Empty when detached.
    pub fn watched(&self) -> Vec<Watched> {
        let Some(disk) = self.disk() else {
            return Vec::new();
        };
        let spelled = |path: &Path| disk.respell(path);
        let spec_root_apart = !disk.canonical_spec_root.starts_with(&disk.canonical_root);
        let mut watched = vec![
            Watched::Sources(spelled(disk.sources.dir())),
            Watched::Directories(spelled(&disk.root)),
        ];
        if spec_root_apart {
            watched.push(Watched::Directories(spelled(disk.sources.dir())));
        }
        for row in &disk.rows {
            let entry = match &row.class {
                Class::Listing => Watched::Listing(disk.respelled(row.key.clone())),
                // A hop is registered unless the sources glob covers it.
                Class::Hop(_)
                    if relative_canonical(&disk.canonical_spec_root, &row.key)
                        .is_some_and(|relative| disk.sources.holds(&relative)) =>
                {
                    continue;
                }
                _ => Watched::File(disk.respelled(row.key.clone())),
            };
            if !watched.contains(&entry) {
                watched.push(entry);
            }
        }
        watched
    }

    pub(super) fn disk(&self) -> Option<&OnDisk> {
        match &self.place {
            Place::Detached => None,
            Place::Disk(disk) => Some(disk),
        }
    }

    fn disk_mut(&mut self) -> Option<&mut OnDisk> {
        match &mut self.place {
            Place::Detached => None,
            Place::Disk(disk) => Some(disk),
        }
    }
}

// Crate-private: how the session builds, renews and reads it.
impl SessionInputs {
    /// The inputs of the environment rooted at `root` that was loaded from
    /// `config`: the spec root and `exclude`, and the `environment` files the
    /// load read, in read order (ADR 0114).
    pub(crate) fn opened(root: &Path, config: &ProjectConfig, environment: Vec<PathBuf>) -> Self {
        let sources = config.sources(root);
        let canonical_root = canonical(root);
        let root_exists = canonical_root.is_dir();
        let mut disk = OnDisk {
            root: root.to_path_buf(),
            canonical_spec_root: canonical(sources.dir()),
            spec_root_pending: root_exists
                .then(|| Pending::toward(sources.dir(), &canonical_root))
                .flatten(),
            sources,
            rows: Vec::new(),
            canonical_root,
        };
        disk.rows = environment
            .into_iter()
            .map(|path| disk.row(Class::Environment, path, root_exists))
            .collect();
        SessionInputs {
            place: Place::Disk(Box::new(disk)),
        }
    }

    /// No source hop is recorded: discovery starts the record of the sources
    /// again, each read recording its own (ADR 0129).
    pub(super) fn forget_hops(&mut self) {
        self.replace_rows(|class| matches!(class, Class::Hop(_)), Vec::new());
    }

    /// Source `key`'s hops are now `hops` (empty: none).
    pub(super) fn set_hops(&mut self, key: &str, hops: Vec<PathBuf>) {
        let class = Class::Hop(key.to_string());
        let rows = hops.into_iter().map(|hop| (class.clone(), hop)).collect();
        self.replace_rows(|covered| covered == &class, rows);
    }

    /// The check inputs are now `files` and the `listings` the checks
    /// listed: their keys and missing directories are taken again, nothing
    /// else changes.
    pub(super) fn set_checks(&mut self, files: Vec<PathBuf>, listings: Vec<PathBuf>) {
        let rows = files
            .into_iter()
            .map(|file| (Class::Check, file))
            .chain(listings.into_iter().map(|dir| (Class::Listing, dir)))
            .collect();
        self.replace_rows(|class| matches!(class, Class::Check | Class::Listing), rows);
    }

    /// The rows of the classes `covers` selects are now `rows`, each keyed
    /// and its pending directory taken now. Nothing covered and nothing new:
    /// nothing is touched.
    fn replace_rows(&mut self, covers: impl Fn(&Class) -> bool, rows: Vec<(Class, PathBuf)>) {
        let Some(disk) = self.disk_mut() else {
            return;
        };
        if rows.is_empty() && !disk.rows.iter().any(|row| covers(&row.class)) {
            return;
        }
        let root_exists = disk.canonical_root.is_dir();
        disk.rows.retain(|row| !covers(&row.class));
        let new: Vec<Recorded> = rows
            .into_iter()
            .map(|(class, path)| disk.row(class, path, root_exists))
            .collect();
        disk.rows.extend(new);
        // Stable: the read order inside a class is kept.
        disk.rows.sort_by(|a, b| a.class.cmp(&b.class));
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
            .map(|path| discovered(disk.sources.dir(), &path))
            .collect()
    }

    /// A changed key the session ignores (never, detached).
    pub(crate) fn excludes(&self, key: &str) -> bool {
        self.disk().is_some_and(|disk| !disk.sources.holds(key))
    }
}
