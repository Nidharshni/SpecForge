//! The files a run formats, de-duplicated, in discovery order (ADR 0021 D3).

use specforge_common::{Diagnostic, ProjectConfig, ProjectRoot, walk_spec_files};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use super::document::directory_of;
use crate::OpError;

/// The files a run from `root` over `paths` formats, de-duplicated, in
/// discovery order (ADR 0021 D3). A file reached several times is one target,
/// shown as first spelled:
///
/// - no paths: the project's sources ([`ProjectConfig::sources`]), which end
///   at its nested projects;
/// - a named `.spec` file: that file, even if excluded (the user named it);
///   any other named file is skipped;
/// - a named directory that is a project root (it holds `specforge.json`):
///   that project's sources;
/// - any other named directory: its `.spec` files as discovery walks them
///   (`specforge_common::walk_spec_files`), taking the sources of each
///   project the walk reaches (a project's sources end at the projects
///   nested in it), without the files their project's `exclude` entries
///   leave out.
pub(super) fn targets(root: &Path, paths: &[PathBuf]) -> Result<Targets, OpError> {
    let mut projects = Projects::default();
    let found: Vec<PathBuf> = if paths.is_empty() {
        projects.config(root)?.sources(root).files()
    } else {
        let mut found = Vec::new();
        for path in paths {
            if path.is_dir() {
                found.extend(projects.walk(path)?);
            } else if path.extension().is_some_and(|ext| ext == "spec") {
                found.push(path.clone());
            }
        }
        found
    };
    // One target per file, however many arguments reach it: keyed by the
    // canonical path (a file that does not exist keys by its spelling),
    // shown as first spelled.
    let mut seen = std::collections::HashSet::new();
    let files = found
        .into_iter()
        .filter(|path| seen.insert(path.canonicalize().unwrap_or_else(|_| path.clone())))
        .collect();
    Ok(Targets {
        files,
        diagnostics: projects.diagnostics,
    })
}

/// The run's targets and the E069s of the configs that selected them.
pub(super) struct Targets {
    pub(super) files: Vec<PathBuf>,
    pub(super) diagnostics: Vec<Diagnostic>,
}

/// The `specforge.json` of each project a run selects sources from, read
/// once through [`crate::config::rewritable`], and the E069s of the problems
/// those reads went around, in the order the configs were read.
#[derive(Default)]
struct Projects {
    configs: HashMap<PathBuf, ProjectConfig>,
    roots: HashMap<PathBuf, Option<PathBuf>>,
    diagnostics: Vec<Diagnostic>,
}

impl Projects {
    /// The configuration of the project at `root`; refused when it does not
    /// say which files are its sources.
    fn config(&mut self, root: &Path) -> Result<&ProjectConfig, OpError> {
        if !self.configs.contains_key(root) {
            let read = crate::config::rewritable(root)?;
            self.diagnostics.extend(read.diagnostics());
            self.configs.insert(root.to_path_buf(), read.config);
        }
        Ok(&self.configs[root])
    }

    /// The `.spec` files of a named directory (see [`targets`]), sorted.
    fn walk(&mut self, dir: &Path) -> Result<Vec<PathBuf>, OpError> {
        if ProjectRoot::is_root(dir) {
            return Ok(self.config(dir)?.sources(dir).files());
        }
        let walked = walk_spec_files(dir, ProjectRoot::is_root);
        let mut files = Vec::new();
        for file in walked.files {
            if !self.excluded(&file)? {
                files.push(file);
            }
        }
        for project in walked.stopped {
            files.extend(self.config(&project)?.sources(&project).files());
        }
        files.sort();
        Ok(files)
    }

    /// Whether the project `file` belongs to leaves it out (`exclude`, or
    /// a skipped directory under its spec root).
    fn excluded(&mut self, file: &Path) -> Result<bool, OpError> {
        let Ok(file) = file.canonicalize() else {
            return Ok(false);
        };
        let dir = directory_of(&file);
        let root = self
            .roots
            .entry(dir.clone())
            .or_insert_with(|| ProjectRoot::enclosing(&dir).map(|r| r.dir().to_path_buf()))
            .clone();
        match root {
            Some(root) => {
                let sources = self.config(&root)?.sources(&root);
                Ok(file
                    .strip_prefix(sources.dir())
                    .is_ok_and(|key| !sources.holds(&key.to_string_lossy())))
            }
            None => Ok(false),
        }
    }
}
