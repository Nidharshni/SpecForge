//! The inference sources (ADR 0113): the files the enabled analyzers claim under the manifest's
//! source roots, keyed by one source path rule, and read inside the project root.

use sha2::{Digest, Sha256};

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

/// The SHA-256 of `bytes`, lowercase hex.
pub(crate) fn content_hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

#[cfg(test)]
mod tests;
