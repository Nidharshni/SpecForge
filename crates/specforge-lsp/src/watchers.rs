//! The files the LSP asks its client to watch (`workspace/
//! didChangeWatchedFiles`), derived from the project session's inputs: every
//! file the project is built from (`lsp_extension_reload_consistency`). What
//! a reported change means is the session's to say
//! (`ProjectSession::changes`); this only makes sure the client reports it.

use std::path::Path;

use specforge_project::{SessionInputs, Watched};
use tower_lsp::lsp_types::{FileSystemWatcher, GlobPattern, OneOf, RelativePattern, WatchKind};

/// The id the watchers are registered under, so they can be replaced.
pub const REGISTRATION_ID: &str = "specforge-file-watcher";

/// What the client watches before a project is open, or when it does not
/// accept the project's own watchers: every `.spec`, config and lock file.
pub fn default_watchers() -> Vec<FileSystemWatcher> {
    ["**/*.spec", "**/specforge.json", "**/specforge.lock"]
        .into_iter()
        .map(|glob| watcher(GlobPattern::String(glob.to_string())))
        .collect()
}

/// The watchers that cover every file a session is built from, read from
/// its `inputs`: the `.spec` files under its spec root, its config and
/// lock, the module of each extension it loads, the build cache its check
/// passes read, and each file the checks read. With `relative_patterns`
/// (the client declared `relativePatternSupport`) each is a pattern
/// relative to its directory; otherwise an absolute glob. Detached inputs
/// get [`default_watchers`].
pub fn file_watchers(inputs: &SessionInputs, relative_patterns: bool) -> Vec<FileSystemWatcher> {
    if inputs.root().is_none() {
        return default_watchers();
    }
    let pattern = |base: &Path, glob: &str| -> Option<FileSystemWatcher> {
        let pattern = if relative_patterns {
            GlobPattern::Relative(RelativePattern {
                base_uri: OneOf::Right(crate::uri::directory_uri(base)?),
                pattern: glob.to_string(),
            })
        } else {
            GlobPattern::String(format!("{}/{glob}", base.to_string_lossy()))
        };
        Some(watcher(pattern))
    };
    inputs
        .watched()
        .iter()
        .filter_map(|watched| match watched {
            Watched::Sources(dir) => pattern(dir, "**/*.spec"),
            Watched::File(path) => pattern(path.parent()?, &path.file_name()?.to_string_lossy()),
            Watched::Listing(dir) => pattern(dir, "*"),
        })
        .collect()
}

fn watcher(glob_pattern: GlobPattern) -> FileSystemWatcher {
    FileSystemWatcher {
        glob_pattern,
        kind: Some(WatchKind::all()),
    }
}
