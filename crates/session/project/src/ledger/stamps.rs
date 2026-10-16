//! A session brings itself up to date with disk without a watcher
//! (behavior `bring_session_up_to_date`).
//!
//! Every build records what it read: each source's size and modification
//! time, the same for each environment input, and for each check input
//! before the checks run. Each environment input is stamped in the read
//! that reads it (the config, the lock, each module, ADR 0114).
//! [`ProjectSession::stale`] compares that record
//! with disk. Stamps are taken *before* the read they describe, so a write
//! that races the read is seen next time rather than lost.
//!
//! Racy-clean (git's rule): a file modified within the timestamp
//! granularity of when it was stamped could change again without its
//! stamp changing (same second, same length). Its content hash is recorded
//! too, and compared whenever its stamp is unchanged, so such a rewrite is
//! still seen, and an unchanged file is not reported.
//!
//! A batch of changed paths narrows the same comparison
//! ([`ProjectSession::changes`]): only what the paths reach is compared.
//!
//! [`ProjectSession::stale`]: crate::ProjectSession::stale
//! [`ProjectSession::changes`]: crate::ProjectSession::changes

use std::collections::BTreeMap;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use super::Changes;
use super::inputs::{Kind, SessionInputs};
use super::reach::Reach;

/// Coarse filesystems record modification times to the second (some to
/// two); an entry modified this close to when it was stamped is racy.
const GRANULARITY: Duration = Duration::from_secs(2);

/// What a path looked like when it was stamped.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Entry {
    len: u64,
    modified: Option<SystemTime>,
    /// The content's hash, for a racy entry only.
    racy_hash: Option<u64>,
}

impl Entry {
    /// Stamp `path` now: `None` when it does not exist.
    pub(crate) fn stamp(path: &Path) -> Option<Entry> {
        let metadata = std::fs::metadata(path).ok()?;
        let modified = metadata.modified().ok();
        let now = SystemTime::now();
        // No modification time, or one within the granularity of now (a
        // time in the future included): the stamp alone cannot be trusted.
        let racy = modified.is_none_or(|m| m + GRANULARITY >= now);
        Some(Entry {
            len: metadata.len(),
            modified,
            racy_hash: if racy { Some(content_hash(path)) } else { None },
        })
    }

    /// Whether `path` still holds what this entry recorded.
    fn holds(&self, path: &Path) -> bool {
        let Ok(metadata) = std::fs::metadata(path) else {
            return false;
        };
        if metadata.len() != self.len || metadata.modified().ok() != self.modified {
            return false;
        }
        self.racy_hash.is_none_or(|hash| content_hash(path) == hash)
    }
}

/// Whether `path` is as `recorded` says: absent when it recorded nothing.
fn unchanged(path: &Path, recorded: Option<&Entry>) -> bool {
    match recorded {
        Some(entry) => entry.holds(path),
        None => !path.exists(),
    }
}

/// A file's bytes, or a directory's sorted entry names, hashed.
fn content_hash(path: &Path) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    if path.is_dir() {
        let mut names: Vec<_> = std::fs::read_dir(path)
            .map(|entries| {
                entries
                    .filter_map(|e| e.ok().map(|e| e.file_name()))
                    .collect()
            })
            .unwrap_or_default();
        names.sort();
        names.hash(&mut hasher);
    } else {
        std::fs::read(path).ok().hash(&mut hasher);
    }
    hasher.finish()
}

/// What a session last built from, as it was when read.
#[derive(Debug, Default, Clone)]
pub(super) struct Stamps {
    /// Every source, by key relative to the spec root.
    sources: BTreeMap<String, Entry>,
    /// Every environment input (absent ones recorded as `None`).
    environment: BTreeMap<PathBuf, Option<Entry>>,
    /// Every check input, as the last run of the checks read it.
    checks: BTreeMap<PathBuf, Option<Entry>>,
}

impl Stamps {
    /// The environment inputs are exactly `environment`, each stamped before
    /// the environment load read it (ADR 0114).
    pub(super) fn set_environment(&mut self, environment: BTreeMap<PathBuf, Option<Entry>>) {
        self.environment = environment;
    }

    /// No source is stamped: discovery starts the record of the sources again,
    /// each stamped as it is read.
    pub(super) fn forget_sources(&mut self) {
        self.sources.clear();
    }

    /// Stamp source `key` (its file is `path`), before it is read.
    pub(super) fn stamp_source(&mut self, key: &str, path: &Path) {
        match Entry::stamp(path) {
            Some(entry) => self.sources.insert(key.to_string(), entry),
            None => self.sources.remove(key),
        };
    }

    /// Stamp the sources `keys` names, before they are read again: the
    /// others keep what they recorded, so a change to one of them that
    /// this rebuild does not read is still seen next time.
    pub(super) fn stamp_sources(&mut self, inputs: &SessionInputs, keys: &[String]) {
        let Some(spec_root) = inputs.spec_root() else {
            return;
        };
        for key in keys {
            if inputs.excludes(key) {
                // No source (any more): a stamp kept would hide its return.
                self.sources.remove(key);
                continue;
            }
            match Entry::stamp(&spec_root.join(key)) {
                Some(entry) => self.sources.insert(key.clone(), entry),
                None => self.sources.remove(key),
            };
        }
    }

    /// The check inputs are exactly `checks`, each stamped before the run
    /// that read it read it.
    pub(super) fn set_checks(&mut self, checks: BTreeMap<PathBuf, Option<Entry>>) {
        self.checks = checks;
    }

    /// What differs from what was read, among `scope`: the sources `inputs`
    /// discovers now (and those recorded) in each region, and the recorded
    /// inputs.
    pub(super) fn changes(&self, inputs: &SessionInputs, scope: Scope<'_>) -> Changes {
        let Some(spec_root) = inputs.spec_root() else {
            return Changes::default();
        };
        let everything = [String::new()];
        let regions: &[String] = match scope {
            Scope::Everything => &everything,
            Scope::Reached(reach) => &reach.regions,
        };
        let mut changes = Changes::default();
        let mut seen = std::collections::BTreeSet::new();
        for region in regions {
            let discovered = inputs.discover_at(region);
            for key in &discovered {
                if seen.insert(key.clone())
                    && !unchanged(&spec_root.join(key), self.sources.get(key))
                {
                    changes.sources.push(key.clone());
                }
            }
            // Recorded, and no longer found: deleted (or excluded since).
            for key in self.sources.keys() {
                let inside = region.is_empty() || Path::new(key).starts_with(region);
                if inside && !discovered.contains(key) && seen.insert(key.clone()) {
                    changes.sources.push(key.clone());
                }
            }
        }
        // A symlinked source a path reached through its target.
        if let Scope::Reached(reach) = scope {
            for key in &reach.linked {
                if seen.insert(key.clone())
                    && !unchanged(&spec_root.join(key), self.sources.get(key))
                {
                    changes.sources.push(key.clone());
                }
            }
        }
        changes.sources.sort();
        changes.sources.dedup();
        match scope {
            Scope::Everything => {
                changes.environment = changed_paths(&self.environment);
                changes.check_inputs = changed_paths(&self.checks);
            }
            Scope::Reached(reach) => {
                let mut memo: BTreeMap<&Path, bool> = BTreeMap::new();
                for (path, reached) in &reach.inputs {
                    for (kind, recorded) in reached {
                        let table = match kind {
                            Kind::Environment => &self.environment,
                            Kind::Check => &self.checks,
                        };
                        let moved = *memo.entry(recorded.as_path()).or_insert_with(|| {
                            !unchanged(recorded, table.get(recorded).and_then(Option::as_ref))
                        });
                        if moved {
                            match kind {
                                Kind::Environment => changes.environment.push(path.clone()),
                                Kind::Check => changes.check_inputs.push(path.clone()),
                            }
                        }
                    }
                }
                for (path, kind) in &reach.pending {
                    match kind {
                        Kind::Environment => changes.environment.push(path.clone()),
                        Kind::Check => changes.check_inputs.push(path.clone()),
                    }
                }
                for list in [&mut changes.environment, &mut changes.check_inputs] {
                    list.sort();
                    list.dedup();
                }
            }
        }
        changes
    }
}

/// How much of what a session read a comparison covers.
pub(super) enum Scope<'r> {
    /// Every source, every environment and check input.
    Everything,
    /// What a batch of changed paths reaches.
    Reached(&'r Reach),
}

/// The recorded paths of `table` whose file is not as recorded, sorted.
fn changed_paths(table: &BTreeMap<PathBuf, Option<Entry>>) -> Vec<PathBuf> {
    table
        .iter()
        .filter(|(path, entry)| !unchanged(path, entry.as_ref()))
        .map(|(path, _)| path.clone())
        .collect()
}
