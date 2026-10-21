//! The project a new project is created inside (ADR 0197): the nearest directory above the one init creates that
//! holds `specforge.json`. Once init's directory holds `specforge.json`, that project's walk ends there (ADR 0080 D1),
//! and a spec root is read whatever it holds (D2).

use specforge_common::ProjectRoot;
use std::path::{Path, PathBuf};

/// The root of the project `dir` lies inside; `None` when no project encloses it. `dir` is where the directory
/// lands (canonical as far as it exists, `containment::landing`) and is no project itself.
pub(super) fn enclosing(dir: &Path) -> Option<PathBuf> {
    dir.ancestors()
        .skip(1)
        .find(|d| ProjectRoot::is_root(d))
        .map(Path::to_path_buf)
}
