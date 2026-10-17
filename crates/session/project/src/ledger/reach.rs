//! What a batch of changed paths reaches of what a session read
//! (behavior `classify_project_changes`, ADR 0073).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use specforge_common::PROJECT_FILE;

use super::inputs::SessionInputs;
use super::key::{canonical, relative_canonical};
use super::recorded::Class;

/// What a batch of changed paths reaches of what a session read
/// ([`SessionInputs::reach`]).
#[derive(Debug, Default)]
pub(crate) struct Reach {
    /// Keys relative to the spec root whose sources (read, or found now) are
    /// compared; "" is every source. Sorted; none lies under another.
    pub(crate) regions: Vec<String>,
    /// Each batch path that reached a recorded input, with the inputs it
    /// reached, as recorded.
    pub(crate) inputs: Vec<(PathBuf, Vec<(Class, PathBuf)>)>,
    /// Each batch path that is a directory created on the way to a missing
    /// input's directory.
    pub(crate) pending: Vec<(PathBuf, Class)>,
}

impl SessionInputs {
    /// What `paths` (absolute, spelled as a watcher or an editor reports
    /// them) reach of what the session read: the sources at or under each
    /// (a region), and the environment and check inputs each is or lies
    /// above. Detached: nothing.
    pub(crate) fn reach<'p>(&self, paths: impl IntoIterator<Item = &'p Path>) -> Reach {
        let Some(disk) = self.disk() else {
            return Reach::default();
        };
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
            if disk.canonical_spec_root.starts_with(&canonical_path) {
                regions.push(String::new());
            } else if let Some(relative) = relative_canonical(&disk.canonical_spec_root, &in_dir) {
                let relative = Path::new(&relative);
                // A nested project's `specforge.json` (below the spec root,
                // not an input of this project) is every source under its
                // directory.
                let marker = path.file_name() == Some(std::ffi::OsStr::new(PROJECT_FILE))
                    && !disk
                        .rows
                        .iter()
                        .any(|row| row.class == Class::Environment && row.key == in_dir);
                match relative.parent() {
                    Some(directory) if marker && !directory.as_os_str().is_empty() => {
                        regions.push(directory.to_string_lossy().into_owned());
                    }
                    _ => regions.push(relative.to_string_lossy().into_owned()),
                }
            }
            let mut reached: Vec<(Class, PathBuf)> = Vec::new();
            for row in &disk.rows {
                if row.class.reached_by(&row.key, &canonical_path, &in_dir) {
                    reached.push((row.class.clone(), row.path.clone()));
                }
                // A directory created on the way to an input's missing directory.
                if row
                    .pending
                    .as_ref()
                    .is_some_and(|pending| pending.reached_by(&canonical_path))
                {
                    reach.pending.push((path.to_path_buf(), row.class.clone()));
                }
            }
            if disk
                .spec_root_pending
                .as_ref()
                .is_some_and(|pending| pending.reached_by(&canonical_path))
            {
                reach.pending.push((path.to_path_buf(), Class::Environment));
            }
            if !reached.is_empty() {
                reach.inputs.push((path.to_path_buf(), reached));
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
}
