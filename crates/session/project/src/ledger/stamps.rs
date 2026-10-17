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
//! A stamp also records the link chain it followed (each hop's path): a
//! source swapped for a link, or a link retargeted, to a file with the same
//! length and modification time is still a change (ADR 0129).
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
use super::disk::link_hops;
use super::inputs::SessionInputs;
use super::reach::Reach;
use super::recorded::{Class, Recording};

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
    /// The link hops the path followed: empty for a plain file.
    links: Vec<PathBuf>,
}

impl Entry {
    /// Stamp `path` now: `None` when it does not exist.
    pub(crate) fn stamp(path: &Path) -> Option<Entry> {
        let (metadata, links) = followed(path)?;
        let modified = metadata.modified().ok();
        let now = SystemTime::now();
        // No modification time, or one within the granularity of now (a
        // time in the future included): the stamp alone cannot be trusted.
        let racy = modified.is_none_or(|m| m + GRANULARITY >= now);
        Some(Entry {
            len: metadata.len(),
            modified,
            racy_hash: if racy { Some(content_hash(path)) } else { None },
            links,
        })
    }

    /// Whether `path` still holds what this entry recorded.
    fn holds(&self, path: &Path) -> bool {
        let Some((metadata, links)) = followed(path) else {
            return false;
        };
        if links != self.links
            || metadata.len() != self.len
            || metadata.modified().ok() != self.modified
        {
            return false;
        }
        self.racy_hash.is_none_or(|hash| content_hash(path) == hash)
    }
}

/// What `path` means now: the metadata of the file a read reaches and the
/// hops to it. A plain file costs one `lstat`, as one `stat` did.
fn followed(path: &Path) -> Option<(std::fs::Metadata, Vec<PathBuf>)> {
    let own = std::fs::symlink_metadata(path).ok()?;
    if own.file_type().is_symlink() {
        Some((std::fs::metadata(path).ok()?, link_hops(path)))
    } else {
        Some((own, Vec::new()))
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
    /// Every source read, by key relative to the spec root: `None` when the
    /// read found nothing there (a dangling link, ADR 0141). A key absent was
    /// never read.
    sources: BTreeMap<String, Option<Entry>>,
    /// Every stamped row, by its class and path as recorded (absent ones
    /// recorded as `None`; no `Hop` row: ADR 0129).
    inputs: BTreeMap<(Class, PathBuf), Option<Entry>>,
}

impl Stamps {
    /// The stamps `recording` covers are now these: the environment's
    /// replace every input stamp, the checks' theirs, a source's its own
    /// (`source`: `Some(stamp)` its read took before reading, `Some(None)` it
    /// read and nothing was there; `None`: it was not read), and discovery's
    /// forget every source's.
    pub(super) fn replace(
        &mut self,
        recording: &Recording<'_>,
        inputs: BTreeMap<(Class, PathBuf), Option<Entry>>,
        source: Option<Option<Entry>>,
    ) {
        match recording {
            Recording::Environment { .. } => self.inputs = inputs,
            Recording::Checks => {
                self.inputs.retain(|(class, _), _| !recording.covers(class));
                self.inputs.extend(inputs);
            }
            Recording::Source { key, .. } => match source {
                Some(entry) => {
                    self.sources.insert((*key).to_string(), entry);
                }
                None => {
                    self.sources.remove(*key);
                }
            },
            Recording::Sources => self.sources.clear(),
        }
    }

    /// Whether the row (`class`, `recorded`) differs from its stamp: a hop
    /// compares its source's stamp, by key, as the read that recorded it does.
    fn moved(&self, class: &Class, recorded: &Path, spec_root: &Path) -> bool {
        match class {
            Class::Hop(key) => !unchanged(
                &spec_root.join(key),
                self.sources.get(key).and_then(Option::as_ref),
            ),
            _ => {
                let entry = self
                    .inputs
                    .get(&(class.clone(), recorded.to_path_buf()))
                    .and_then(Option::as_ref);
                !unchanged(recorded, entry)
            }
        }
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
                // A key never read is changed, whether or not its path exists now.
                let moved = self
                    .sources
                    .get(key)
                    .is_none_or(|entry| !unchanged(&spec_root.join(key), entry.as_ref()));
                if seen.insert(key.clone()) && moved {
                    changes.sources.push(key.clone());
                }
            }
            // Recorded, and no longer found: deleted (or excluded since).
            for key in self
                .sources
                .iter()
                .filter_map(|(key, entry)| entry.as_ref().map(|_| key))
            {
                let inside = region.is_empty() || Path::new(key).starts_with(region);
                if inside && !discovered.contains(key) && seen.insert(key.clone()) {
                    changes.sources.push(key.clone());
                }
            }
        }
        match scope {
            Scope::Everything => {
                for ((class, path), entry) in &self.inputs {
                    if !unchanged(path, entry.as_ref()) {
                        note(&mut changes, class, path);
                    }
                }
            }
            Scope::Reached(reach) => {
                let mut memo: BTreeMap<(&Class, &Path), bool> = BTreeMap::new();
                for (path, reached) in &reach.inputs {
                    for (class, recorded) in reached {
                        let moved = *memo
                            .entry((class, recorded.as_path()))
                            .or_insert_with(|| self.moved(class, recorded, spec_root));
                        if moved {
                            note(&mut changes, class, path);
                        }
                    }
                }
                for (path, class) in &reach.pending {
                    note(&mut changes, class, path);
                }
            }
        }
        changes.sources.sort();
        changes.sources.dedup();
        for list in [&mut changes.environment, &mut changes.check_inputs] {
            list.sort();
            list.dedup();
        }
        changes
    }
}

/// Note a change to a row of `class`, reached by `path`: what a change to it
/// is. A hop's is its source's, whatever the stamps say for a pending one.
fn note(changes: &mut Changes, class: &Class, path: &Path) {
    match class {
        Class::Environment => changes.environment.push(path.to_path_buf()),
        Class::Check | Class::Listing => changes.check_inputs.push(path.to_path_buf()),
        Class::Hop(key) => changes.sources.push(key.clone()),
    }
}

/// How much of what a session read a comparison covers.
pub(super) enum Scope<'r> {
    /// Every source, every environment and check input.
    Everything,
    /// What a batch of changed paths reaches.
    Reached(&'r Reach),
}
