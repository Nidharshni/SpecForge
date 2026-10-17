//! A recorded input: one row of a session's inputs, and the class that decides how a
//! changed path reaches it, which directory is watched for it and what its change is
//! (ADR 0114 D3, ADR 0129 D3, ADR 0133).

use std::path::{Path, PathBuf};

use specforge_common::ProjectConfig;

use super::Reading;
use super::key::canonical;

/// What read an input. Ordered: the rows of one class sit together, in this order.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Class {
    /// A file the environment load read (the config, the lock, a module) or a hop it
    /// followed: its change loads the environment again.
    Environment,
    /// A file the checks read or a hop they followed: its change runs the checks again.
    Check,
    /// A link hop source `key`'s last read followed (its target included): its change is
    /// that source's. Never stamped: its source's stamp follows the chain (ADR 0129).
    Hop(String),
    /// A directory the checks listed (E016's suggestion): its change runs the checks again;
    /// any file directly in it reaches it.
    Listing,
}

impl Class {
    /// Whether a changed path reaches a row of this class keyed `key`: at or above it,
    /// resolved (`canonical_path`) or by its own name in its canonical directory
    /// (`in_dir`); a listing also by any file directly in it.
    pub(super) fn reached_by(&self, key: &Path, canonical_path: &Path, in_dir: &Path) -> bool {
        match self {
            Class::Listing => {
                key.starts_with(canonical_path) || canonical_path.parent() == Some(key)
            }
            // An input that is a symlink is keyed by its own name: pointing it
            // elsewhere is the change of the name, not of its old target.
            _ => key.starts_with(canonical_path) || key.starts_with(in_dir),
        }
    }

    /// Whether the ledger stamps a row of this class (not a hop: its source's stamp
    /// follows the chain, ADR 0129).
    pub(super) fn stamped(&self) -> bool {
        !matches!(self, Class::Hop(_))
    }

    /// The directory a watcher must watch for a row at `path`: its parent; none for a
    /// listing (the missing file it lists for is a check row of its own).
    pub(super) fn directory<'p>(&self, path: &'p Path) -> Option<&'p Path> {
        match self {
            Class::Listing => None,
            _ => path.parent(),
        }
    }
}

/// One recorded input. Its key and pending directory are taken when it is recorded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Recorded {
    /// As the read spelled it.
    pub(super) path: PathBuf,
    pub(super) class: Class,
    /// Its own name in its canonical directory (a listing: the directory, canonical).
    pub(super) key: PathBuf,
    /// Its directory does not exist, outside the root: where creating the next directory
    /// toward it is seen (ADR 0030 D6).
    pub(super) pending: Option<Pending>,
}

/// An input whose directory does not exist yet, outside the root: what creating the
/// next directory on the way to it is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Pending {
    /// The nearest existing ancestor of the missing directory (canonical).
    pub(super) ancestor: PathBuf,
    /// The missing directory (canonical, as far as it can be).
    pub(super) toward: PathBuf,
}

impl Pending {
    /// The missing directory `dir` outside `root` (canonical) and its nearest existing
    /// ancestor; `None` when it exists or lies under the root.
    pub(super) fn toward(dir: &Path, root: &Path) -> Option<Pending> {
        let toward = canonical(dir);
        if toward.is_dir() || toward.starts_with(root) {
            return None;
        }
        let mut ancestor = toward.parent()?;
        while !ancestor.is_dir() {
            ancestor = ancestor.parent()?;
        }
        Some(Pending {
            ancestor: ancestor.to_path_buf(),
            toward,
        })
    }

    /// Whether `canonical_path`, a changed path, is the next directory created toward it.
    pub(super) fn reached_by(&self, canonical_path: &Path) -> bool {
        canonical_path.parent() == Some(self.ancestor.as_path())
            && self.toward.starts_with(canonical_path)
    }
}

/// What one run of reads (or a forgetting) records: which rows and stamps it replaces.
pub(crate) enum Recording<'a> {
    /// The environment load at `root` from `config`: every row and every input stamp.
    Environment {
        root: &'a Path,
        config: &'a ProjectConfig,
    },
    /// One run of the checks: the `Check` and `Listing` rows, a set.
    Checks,
    /// One read of source `key` (its file `path`): its `Hop(key)` rows and its stamp.
    Source { key: &'a str, path: &'a Path },
    /// Discovery starts the record of the sources again: every `Hop` row, every source stamp.
    Sources,
}

impl Recording<'_> {
    /// Whether this recording replaces a row of `class`.
    pub(super) fn covers(&self, class: &Class) -> bool {
        match self {
            Recording::Environment { .. } => true,
            Recording::Checks => matches!(class, Class::Check | Class::Listing),
            Recording::Source { key, .. } => matches!(class, Class::Hop(hop) if hop == key),
            Recording::Sources => matches!(class, Class::Hop(_)),
        }
    }

    /// The class of one read in this recording; `None` for a source's own file, which is
    /// its stamp by key, not a row.
    pub(super) fn class_of(&self, reading: Reading, path: &Path) -> Option<Class> {
        match self {
            // A listing folds into the files: nothing read is dropped.
            Recording::Environment { .. } => Some(Class::Environment),
            Recording::Checks => Some(match reading {
                Reading::File => Class::Check,
                Reading::Listing => Class::Listing,
            }),
            Recording::Source { key, path: file } => {
                (path != *file).then(|| Class::Hop((*key).to_string()))
            }
            Recording::Sources => None,
        }
    }
}
