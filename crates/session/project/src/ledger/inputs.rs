//! [`SessionInputs`]: what a project session depends on, as one value
//! (behavior `classify_project_changes`, ADR 0030).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use specforge_common::{ProjectConfig, SourceRoot};

use super::key::{canonical, discovered, relative_canonical};

/// Everything a project session depends on besides its sources' text:
/// where its sources are discovered (the spec root and `exclude`), the
/// files its environment load read (`specforge.json`, `specforge.lock`, each
/// extension module the load read, and each hop of a symlink among them,
/// ADR 0114) and the files its
/// last checks read (every path a check asked about, the build cache a
/// check-phase pass read, and every directory E016's suggestion listed,
/// recorded when read, ADR 0107), and the target of each symlinked source (where its text
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
pub(super) struct OnDisk {
    pub(super) root: PathBuf,
    pub(super) sources: SourceRoot,
    /// The files the environment load read, in read order, each once:
    /// `specforge.json`, `specforge.lock`, each module the extension load
    /// read, and each hop of each that is a symlink (ADR 0114).
    pub(super) environment: Vec<PathBuf>,
    /// The files the checks read, as they read them (OS resolution of `..`),
    /// sorted: the paths a check asked `exists` or `read`, and the target of
    /// each that is a symlink (ADR 0107).
    pub(super) check_files: Vec<PathBuf>,
    /// The directories the checks listed (E016's suggestion for a missing file).
    pub(super) listings: Vec<PathBuf>,
    /// The symlinked sources, by key (sorted): where each one's text lies.
    pub(super) links: Vec<Link>,
    /// Directories on the way to an input's missing directory outside the
    /// root, from their nearest existing ancestor.
    pub(super) pending: Vec<Pending>,
    pub(super) canonical: Canonical,
}

/// A source that is a symlink to a file: keyed by its own name, read through
/// the link.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Link {
    pub(super) key: String,
    /// Where the text lies (canonical).
    pub(super) target: PathBuf,
}

/// An input whose directory does not exist yet, outside the root: what
/// creating the next directory on the way to it is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Pending {
    /// The nearest existing ancestor of the missing directory (canonical).
    pub(super) ancestor: PathBuf,
    /// The missing directory (canonical, as far as it can be).
    pub(super) toward: PathBuf,
    pub(super) kind: Kind,
}

/// Environment or check input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    Environment,
    Check,
}

impl OnDisk {
    /// The link `path` (a source under the spec root) is: it is a symlink
    /// that resolves to a file.
    pub(super) fn link_at(&self, path: &Path) -> Option<Link> {
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
        Some(Link {
            key: discovered(self.sources.dir(), path),
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
pub(super) struct Canonical {
    pub(super) root: PathBuf,
    pub(super) spec_root: PathBuf,
    /// Canonical form to the path as recorded.
    pub(super) environment: BTreeMap<PathBuf, PathBuf>,
    pub(super) checks: BTreeMap<PathBuf, PathBuf>,
    pub(super) listings: BTreeMap<PathBuf, PathBuf>,
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
                disk.environment
                    .iter()
                    .chain(&disk.check_files)
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
        let spec_root_apart = !disk.canonical.spec_root.starts_with(&disk.canonical.root);
        std::iter::once(Watched::Sources(spelled(disk.sources.dir())))
            .chain(std::iter::once(Watched::Directories(spelled(&disk.root))))
            .chain(spec_root_apart.then(|| Watched::Directories(spelled(disk.sources.dir()))))
            .chain(
                disk.environment
                    .iter()
                    .chain(&disk.check_files)
                    .map(|path| Watched::File(spelled(path))),
            )
            .chain(
                disk.links
                    .iter()
                    .filter(|link| {
                        relative_canonical(&disk.canonical.spec_root, &link.target)
                            .is_none_or(|relative| !disk.sources.holds(&relative))
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
        Self::on_disk(OnDisk {
            root: root.to_path_buf(),
            sources: config.sources(root),
            environment,
            check_files: Vec::new(),
            listings: Vec::new(),
            links: Vec::new(),
            pending: Vec::new(),
            canonical: Canonical::empty(),
        })
    }

    /// The symlinked sources among `discovered` (every source found) are
    /// now the links: nothing else of the value changes.
    pub(super) fn set_links(&mut self, discovered: &[PathBuf]) {
        let Some(disk) = self.disk_mut() else {
            return;
        };
        disk.links = discovered
            .iter()
            .filter_map(|path| disk.link_at(path))
            .collect();
        disk.links.sort_by(|a, b| a.key.cmp(&b.key));
    }

    /// The links of `keys` read again, the others kept: whether they moved
    /// (a source became or stopped being a symlink, or points elsewhere).
    pub(super) fn relink(&mut self, keys: &[String]) -> bool {
        let Some(disk) = self.disk_mut() else {
            return false;
        };
        let before = disk.links.clone();
        disk.links.retain(|link| !keys.contains(&link.key));
        let again: Vec<Link> = keys
            .iter()
            .filter(|key| disk.sources.holds(key))
            .filter_map(|key| disk.link_at(&disk.sources.dir().join(key)))
            .collect();
        disk.links.extend(again);
        disk.links.sort_by(|a, b| a.key.cmp(&b.key));
        disk.links.dedup();
        disk.links != before
    }

    /// The check inputs are now `files` and the `listings` the checks
    /// listed: their
    /// canonical forms and missing directories are taken again, nothing else
    /// changes.
    pub(super) fn set_checks(&mut self, files: Vec<PathBuf>, listings: Vec<PathBuf>) {
        let Some(disk) = self.disk_mut() else {
            return;
        };
        disk.check_files = files;
        disk.listings = listings;
        disk.renew_checks();
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

    /// The value for `disk`, its canonical forms taken now.
    fn on_disk(mut disk: OnDisk) -> Self {
        disk.canonical = Canonical {
            root: canonical(&disk.root),
            spec_root: canonical(disk.sources.dir()),
            environment: disk
                .environment
                .iter()
                .map(|path| (canonical(path), path.clone()))
                .collect(),
            checks: disk.canonical_checks(),
            listings: disk.canonical_listings(),
        };
        disk.pending = disk.pending_directories();
        SessionInputs {
            place: Place::Disk(Box::new(disk)),
        }
    }
}

impl OnDisk {
    fn canonical_checks(&self) -> BTreeMap<PathBuf, PathBuf> {
        self.check_files
            .iter()
            .map(|path| (canonical(path), path.clone()))
            .collect()
    }

    /// E016's suggestion for a missing file lists its directory: a file
    /// created or deleted there changes it.
    fn canonical_listings(&self) -> BTreeMap<PathBuf, PathBuf> {
        self.listings
            .iter()
            .map(|dir| (canonical(dir), dir.clone()))
            .collect()
    }

    /// The canonical forms of the check inputs and the missing directories
    /// toward them, taken again.
    fn renew_checks(&mut self) {
        self.canonical.checks = self.canonical_checks();
        self.canonical.listings = self.canonical_listings();
        self.pending = self.pending_directories();
    }

    /// The missing directories of the inputs outside the root, each with
    /// its nearest existing ancestor. A root that does not exist has none.
    pub(super) fn pending_directories(&self) -> Vec<Pending> {
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
                self.environment
                    .iter()
                    .filter_map(|file| Some((file.parent()?, Kind::Environment))),
            )
            .chain(
                self.check_files
                    .iter()
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
