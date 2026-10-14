//! The editor buffers a project session holds (behavior
//! `hold_editor_buffers`, ADR 0046). While the session holds a buffer, the
//! buffer's text is the truth for its file, whatever happens to the file on
//! disk; when the editor releases it, the file is the disk's again.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// An editor buffer: the editor's text of one file. Given to a session with
/// [`crate::SourceChange::Hold`]; held until
/// [`crate::ProjectSession::release`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Buffer {
    /// The file, as the editor names it (absolute). The session keys it as
    /// it keys every source ([`crate::Environment::source_key`]), again
    /// after each environment load, so a reload that moves the spec root or
    /// `exclude` keys it anew.
    pub path: PathBuf,
    /// The editor's text.
    pub text: String,
    /// The editor's version of `text` (an LSP document version), kept with
    /// it so what the session reports from this text is labelled with the
    /// version it was computed from. `None`: unversioned.
    pub version: Option<i32>,
}

impl Buffer {
    /// An unversioned buffer of `path` holding `text`.
    pub fn new(path: impl Into<PathBuf>, text: impl Into<String>) -> Buffer {
        Buffer {
            path: path.into(),
            text: text.into(),
            version: None,
        }
    }

    /// This buffer at the editor's `version`.
    pub fn at_version(mut self, version: Option<i32>) -> Buffer {
        self.version = version;
        self
    }
}

/// The buffers a session holds, by the session's key for each.
#[derive(Debug, Default)]
pub(crate) struct Held {
    by_key: BTreeMap<String, Buffer>,
}

impl Held {
    /// `buffers`, each keyed by `key(&buffer.path)`.
    pub(crate) fn keyed(
        buffers: impl IntoIterator<Item = Buffer>,
        key: impl Fn(&Path) -> String,
    ) -> Held {
        Held {
            by_key: buffers
                .into_iter()
                .map(|buffer| (key(&buffer.path), buffer))
                .collect(),
        }
    }

    /// Hold `buffer` under `key`, replacing what was held there.
    pub(crate) fn hold(&mut self, key: String, buffer: Buffer) {
        self.by_key.insert(key, buffer);
    }

    /// Stop holding `key` (nothing when it was not held).
    pub(crate) fn release(&mut self, key: &str) {
        self.by_key.remove(key);
    }

    pub(crate) fn get(&self, key: &str) -> Option<&Buffer> {
        self.by_key.get(key)
    }

    /// `keys` without the held ones: a held file is not the disk's.
    pub(crate) fn leave_out(&self, keys: &mut Vec<String>) {
        keys.retain(|key| !self.by_key.contains_key(key));
    }

    /// The text of every held buffer whose key `is_source` accepts: what a
    /// cold build reads in place of its file.
    pub(crate) fn sources(&self, is_source: impl Fn(&str) -> bool) -> BTreeMap<String, &str> {
        self.by_key
            .iter()
            .filter(|(key, _)| is_source(key))
            .map(|(key, buffer)| (key.clone(), buffer.text.as_str()))
            .collect()
    }

    pub(crate) fn into_buffers(self) -> Vec<Buffer> {
        self.by_key.into_values().collect()
    }
}
