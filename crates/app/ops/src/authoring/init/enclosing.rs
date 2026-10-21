//! The project a new project is created inside (ADR 0197): the nearest directory above the one init creates that
//! holds `specforge.json`, judged before anything is written. Once init's directory holds `specforge.json`, that
//! project's walk ends there (ADR 0080 D1), and a spec root is read whatever it holds (D2).

use super::{INSIDE_PROJECT_SOURCES, STARTER_FILE};
use crate::{OpError, OpErrorKind};
use specforge_common::{ProjectRoot, SourceRoot, read_project_config};
use std::path::{Path, PathBuf};

/// How many of the sources a refusal names; the count says how many more.
const NAMED: usize = 5;

/// The root of the project `dir` lies inside, when that project reads the same sources once `dir` holds
/// `specforge.json`; `None` when no project encloses `dir`. `dir` is where the directory lands (canonical as far as
/// it exists, `containment::landing`) and is no project itself (init refused that first).
///
/// Refused, `inside_project_sources` (conflict), when that project would read fewer sources (`dir` lies strictly
/// below its spec root and holds some of them) or one more (`dir`, or `dir/spec`, is its spec root and the starter
/// would be one of its sources). It reads that project's `specforge.json` as its compile reads it (an unusable one is
/// its default configuration, as there) and lists names under `dir` only.
pub(super) fn judged(dir: &Path) -> Result<Option<PathBuf>, OpError> {
    let Some(root) = dir.ancestors().skip(1).find(|d| ProjectRoot::is_root(d)) else {
        return Ok(None);
    };
    let config = read_project_config(root).config;
    // `files_at` compares by prefix: the spec root spelled as `dir` is, canonically.
    let configured = config.sources(root);
    let spec_root =
        std::fs::canonicalize(configured.dir()).unwrap_or_else(|_| configured.dir().to_path_buf());
    let sources = SourceRoot::new(spec_root.clone(), config.exclude);
    if dir != spec_root && dir.starts_with(&spec_root) {
        let lost = sources.files_at(dir);
        if !lost.is_empty() {
            return Err(loses(dir, root, &spec_root, &lost));
        }
    } else if let Ok(key) = dir.join(STARTER_FILE).strip_prefix(&spec_root)
        && sources.holds(&key.to_string_lossy())
    {
        return Err(reads_the_starter(dir, root));
    }
    Ok(Some(root.to_path_buf()))
}

fn loses(dir: &Path, root: &Path, spec_root: &Path, lost: &[PathBuf]) -> OpError {
    let keys: Vec<String> = lost
        .iter()
        .map(|file| {
            file.strip_prefix(spec_root)
                .unwrap_or(file)
                .display()
                .to_string()
        })
        .collect();
    let more = match keys.len().saturating_sub(NAMED) {
        0 => String::new(),
        n => format!(", and {n} more"),
    };
    OpError::new(
        OpErrorKind::Conflict,
        INSIDE_PROJECT_SOURCES,
        format!(
            "{} holds {} of the sources of the project at {}; as a project of its own it would take them from that project: {}{more}",
            dir.display(),
            keys.len(),
            root.display(),
            keys[..keys.len().min(NAMED)].join(", ")
        ),
    )
    .with_suggestion(format!(
        "init in a directory that holds none of its sources, move them out of {}, or leave {} out of that project with an \"exclude\" entry in {}/specforge.json",
        dir.display(),
        dir.display(),
        root.display()
    ))
}

fn reads_the_starter(dir: &Path, root: &Path) -> OpError {
    OpError::new(
        OpErrorKind::Conflict,
        INSIDE_PROJECT_SOURCES,
        format!(
            "{} is where the project at {} reads its sources from; it would read the new project's {STARTER_FILE} as one of its own",
            dir.display(),
            root.display()
        ),
    )
    .with_suggestion("init in a directory below or beside that project's spec root")
}
