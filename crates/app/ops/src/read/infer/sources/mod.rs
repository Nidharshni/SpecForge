//! The inference sources (ADR 0113): the files the enabled analyzers claim under the manifest's
//! source roots, keyed by one source path rule, and read inside the project root.

use std::collections::BTreeSet;
use std::path::Path;

use sha2::{Digest, Sha256};
use specforge_common::walk_files;
use specforge_protocol_types::ExtensionDeclaration;

use super::manifest::{InferenceManifest, SourceFileEntry};
use crate::{Containment, ReadFailure};

/// Directories inference skips beyond the shared walk's own list.
const INFERENCE_SKIP: &[&str] = &["__pycache__", "vendor"];

/// A path outside the project root, or naming nothing: the source path rule refuses it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct OutsideRoot;

/// A source file's key: root-relative, `/`-separated, no empty or `.` component, never absolute,
/// a drive path or `..`. What the manifest records and what every read opens (joined onto the
/// project root).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct SourceKey(String);

impl SourceKey {
    /// The source path rule: `given` as the root-relative, `/`-separated path the manifest
    /// records. Separators `/` and `\` are both accepted; empty and `.` components are dropped.
    /// A path that is absolute, has a drive prefix, has a `..` component, or names nothing is
    /// outside the root.
    pub(crate) fn of(given: &str) -> Result<SourceKey, OutsideRoot> {
        let parts = components(given)?;
        if parts.is_empty() {
            return Err(OutsideRoot);
        }
        Ok(SourceKey(parts.join("/")))
    }

    /// [`Self::of`] for a source root: the project root itself (`.`) is a root, naming nothing
    /// at all is not.
    pub(crate) fn root(given: &str) -> Result<SourceKey, OutsideRoot> {
        let parts = components(given)?;
        if parts.is_empty() {
            return if given.is_empty() {
                Err(OutsideRoot)
            } else {
                Ok(SourceKey(".".to_string()))
            };
        }
        Ok(SourceKey(parts.join("/")))
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

/// The components of `query` under the source path rule's separators (`/` and `\`), without the
/// empty and `.` ones. A path that is absolute, has a drive prefix or has a `..` component is
/// outside the root.
fn components(query: &str) -> Result<Vec<&str>, OutsideRoot> {
    let bytes = query.as_bytes();
    let absolute = query.starts_with(['/', '\\']);
    let drive = bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':';
    if absolute || drive {
        return Err(OutsideRoot);
    }
    let parts: Vec<&str> = query
        .split(['/', '\\'])
        .filter(|part| !part.is_empty() && *part != ".")
        .collect();
    if parts.contains(&"..") {
        return Err(OutsideRoot);
    }
    Ok(parts)
}

/// A recorded path as the rule spells it, leniently: an entry an older version recorded as the
/// agent spelled it (`./src/a.rs`) compares as the path it names; one the rule refuses compares
/// as spelled.
pub(crate) fn normal_path(recorded: &str) -> String {
    components(recorded).map_or_else(|_| recorded.to_string(), |parts| parts.join("/"))
}

/// One extension an enabled analyzer claims.
#[derive(Debug, Clone)]
pub(crate) struct Claim {
    /// With its dot (`.rs`).
    pub(crate) extension: String,
    /// The extension that declares the analyzer.
    pub(crate) analyzer: String,
    pub(crate) scan_export: String,
}

/// The enabled analyzers' claims, in declaration order, and the directories they never scan.
#[derive(Debug, Default)]
pub(crate) struct Analyzers {
    claims: Vec<Claim>,
    skipped: BTreeSet<String>,
}

impl Analyzers {
    pub(crate) fn of(declarations: &[ExtensionDeclaration]) -> Self {
        let mut analyzers = Analyzers::default();
        for declaration in declarations {
            for analyzer in &declaration.analyzers {
                for extension in &analyzer.file_extensions {
                    let extension = if extension.starts_with('.') {
                        extension.clone()
                    } else {
                        format!(".{extension}")
                    };
                    analyzers.claims.push(Claim {
                        extension,
                        analyzer: declaration.name().to_string(),
                        scan_export: analyzer.scan_export.clone(),
                    });
                }
                analyzers
                    .skipped
                    .extend(analyzer.excluded_dirs.iter().cloned());
            }
        }
        analyzers
    }

    /// The claim whose extension `file_name` ends with: the longest such extension; for one
    /// extension, the first declarer.
    pub(crate) fn claim(&self, file_name: &str) -> Option<&Claim> {
        let mut found: Option<&Claim> = None;
        for claim in &self.claims {
            if file_name.ends_with(claim.extension.as_str())
                && found.is_none_or(|f| claim.extension.len() > f.extension.len())
            {
                found = Some(claim);
            }
        }
        found
    }

    /// Whether the walk enters no directory named `name`: a hidden name, one inference skips,
    /// or one an analyzer never scans (the shared walk's own list applies besides).
    pub(crate) fn skips(&self, name: &str) -> bool {
        name.starts_with('.') || INFERENCE_SKIP.contains(&name) || self.skipped.contains(name)
    }
}

/// What reading a source file found.
#[derive(Debug)]
pub(crate) enum SourceRead {
    Bytes(Vec<u8>),
    Missing,
    Outside,
    Unreadable(std::io::Error),
}

/// The project's files as inference reads them: held inside the project root on every surface
/// (ADR 0113 D4).
#[derive(Debug)]
pub(crate) struct ProjectFiles {
    held: Containment,
}

impl ProjectFiles {
    pub(crate) fn at(root: &Path) -> Self {
        ProjectFiles {
            held: Containment::within(root),
        }
    }

    /// The file at `key`, read once through its held name.
    pub(crate) fn read(&self, key: &SourceKey) -> SourceRead {
        match self.held.read(Path::new(key.as_str())) {
            Ok((_, bytes)) => SourceRead::Bytes(bytes),
            Err(ReadFailure::Outside(_)) => SourceRead::Outside,
            Err(ReadFailure::Io { error, .. }) if error.kind() == std::io::ErrorKind::NotFound => {
                SourceRead::Missing
            }
            Err(ReadFailure::Io { error, .. }) => SourceRead::Unreadable(error),
        }
    }
}

/// Where an index entry's file stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum IndexState {
    /// Its hash is the recorded one.
    Current { lines: Option<usize> },
    /// Its hash differs from the recorded one.
    Changed { lines: Option<usize> },
    /// The path is no key, lands outside the project, or names no file.
    Deleted,
    /// The file exists and cannot be read.
    Unreadable,
}

/// An index entry and where its file stands.
#[derive(Debug)]
pub(crate) struct Indexed<'m> {
    pub(crate) entry: &'m SourceFileEntry,
    pub(crate) state: IndexState,
}

impl ProjectFiles {
    /// Each index entry, in index order, its file read once: `Deleted` when the path is no key,
    /// lands outside or is missing; `Unreadable` when it exists and cannot be read; else
    /// `Current` or `Changed` by its hash, with its line count when it is UTF-8.
    pub(crate) fn indexed<'m>(&self, manifest: &'m InferenceManifest) -> Vec<Indexed<'m>> {
        manifest
            .source_index
            .iter()
            .map(|entry| {
                let state = match SourceKey::of(&entry.path).map(|key| self.read(&key)) {
                    Err(OutsideRoot) | Ok(SourceRead::Missing | SourceRead::Outside) => {
                        IndexState::Deleted
                    }
                    Ok(SourceRead::Unreadable(_)) => IndexState::Unreadable,
                    Ok(SourceRead::Bytes(bytes)) => {
                        let lines = std::str::from_utf8(&bytes)
                            .ok()
                            .map(|text| text.lines().count().max(1));
                        if content_hash(&bytes) == entry.content_hash {
                            IndexState::Current { lines }
                        } else {
                            IndexState::Changed { lines }
                        }
                    }
                };
                Indexed { entry, state }
            })
            .collect()
    }
}

/// The inference sources of one project, walked once.
#[derive(Debug)]
pub(crate) struct InferenceSources {
    files: ProjectFiles,
    analyzers: Analyzers,
    keys: Vec<SourceKey>,
}

impl InferenceSources {
    /// The files `declarations`' analyzers claim under `manifest`'s source roots (the project
    /// root when it has none), inside `root`.
    pub(crate) fn of(
        root: &Path,
        declarations: &[ExtensionDeclaration],
        manifest: &InferenceManifest,
    ) -> Self {
        let files = ProjectFiles::at(root);
        let analyzers = Analyzers::of(declarations);
        let keys = walk(&files, &analyzers, &manifest.source_roots);
        InferenceSources {
            files,
            analyzers,
            keys,
        }
    }

    /// Sorted, each once.
    pub(crate) fn keys(&self) -> &[SourceKey] {
        &self.keys
    }

    pub(crate) fn claim(&self, key: &SourceKey) -> Option<&Claim> {
        let name = key.as_str().rsplit('/').next().unwrap_or_default();
        self.analyzers.claim(name)
    }

    pub(crate) fn files(&self) -> &ProjectFiles {
        &self.files
    }

    /// Sources at the given keys, walked by nobody (a scan of a file that is gone).
    #[cfg(test)]
    pub(crate) fn with_keys(
        root: &Path,
        declarations: &[ExtensionDeclaration],
        keys: &[impl AsRef<str>],
    ) -> Self {
        InferenceSources {
            files: ProjectFiles::at(root),
            analyzers: Analyzers::of(declarations),
            keys: keys
                .iter()
                .map(|k| SourceKey::of(k.as_ref()).unwrap())
                .collect(),
        }
    }
}

/// The keys of the files `analyzers` claim under `roots`, held inside the project root.
fn walk(files: &ProjectFiles, analyzers: &Analyzers, roots: &[String]) -> Vec<SourceKey> {
    let Some(project) = files.held.root() else {
        return Vec::new();
    };
    let roots: Vec<&str> = if roots.is_empty() {
        vec!["."]
    } else {
        roots.iter().map(String::as_str).collect()
    };
    let mut keys: Vec<SourceKey> = Vec::new();
    for root in roots {
        let Ok(root) = SourceKey::root(root) else {
            continue;
        };
        let Ok(held) = files.held.resolve(Path::new(root.as_str())) else {
            continue;
        };
        if !held.real.is_dir() {
            continue;
        }
        let walked = walk_files(
            &held.real,
            |name| analyzers.skips(name),
            |path| {
                path.file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| !name.starts_with('.') && analyzers.claim(name).is_some())
            },
            |_| false,
        );
        for path in walked.files {
            let a_link = path.symlink_metadata().is_ok_and(|m| m.is_symlink());
            if a_link && files.held.resolve(&path).is_err() {
                continue;
            }
            let Ok(relative) = path.strip_prefix(project) else {
                continue;
            };
            let parts: Option<Vec<&str>> = relative
                .components()
                .map(|c| c.as_os_str().to_str())
                .collect();
            if let Some(key) = parts.and_then(|p| SourceKey::of(&p.join("/")).ok()) {
                keys.push(key);
            }
        }
    }
    keys.sort();
    keys.dedup();
    keys
}

/// The SHA-256 of `bytes`, lowercase hex.
pub(crate) fn content_hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

#[cfg(test)]
mod tests;
