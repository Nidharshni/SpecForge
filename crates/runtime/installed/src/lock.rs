use serde::{Deserialize, Serialize};
use specforge_common::{Diagnostic, codes};
use specforge_package::PackageName;
use specforge_registry::Disk;
use std::path::Path;

use crate::layout::lock_path;

/// The lock file format for extension resolution.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LockFile {
    pub lockfile_version: u32,
    pub entries: Vec<LockFileEntry>,
}

/// A single entry in the lock file.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LockFileEntry {
    /// The package the entry locks: text that is no package name makes the
    /// whole lock unreadable (E033), since no module could be installed
    /// under it (ADR 0036).
    pub name: PackageName,
    pub version: String,
    pub source: LockSource,
    pub wasm_hash: String,
    /// The publisher key id the registry install was signed by: the
    /// project's pin for the name, which a fetch expects when the user has
    /// none of their own (ADR 0059). `None` for a local install, an unsigned
    /// one, or a lock written before signing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key_id: Option<String>,
    /// Peer requirements recorded at install from the extension's manifest
    /// (C8-05: doctor verifies these across the other lock entries). Defaults
    /// on deserialize for lock files written before this existed.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub peer_dependencies: Vec<specforge_protocol_types::PeerDependency>,
}

/// Where a lock entry's binary came from. Serialized as the lock's string:
/// `registry`, `local:<path>`, or anything else, kept as it was written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LockSource {
    /// Installed from a registry.
    Registry,
    /// Installed from a `.wasm` file at this path (as `add` recorded it).
    Local(String),
    /// A source this version of SpecForge does not know, kept verbatim.
    Other(String),
}

const LOCAL_PREFIX: &str = "local:";

impl LockSource {
    /// The path of a local install.
    pub fn local_path(&self) -> Option<&str> {
        match self {
            LockSource::Local(path) => Some(path),
            LockSource::Registry | LockSource::Other(_) => None,
        }
    }

    pub fn is_registry(&self) -> bool {
        matches!(self, LockSource::Registry)
    }

    /// Read the lock's string.
    pub fn parse(text: &str) -> LockSource {
        if text == "registry" {
            LockSource::Registry
        } else if let Some(path) = text.strip_prefix(LOCAL_PREFIX) {
            LockSource::Local(path.to_string())
        } else {
            LockSource::Other(text.to_string())
        }
    }
}

impl std::fmt::Display for LockSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LockSource::Registry => f.write_str("registry"),
            LockSource::Local(path) => write!(f, "{LOCAL_PREFIX}{path}"),
            LockSource::Other(text) => f.write_str(text),
        }
    }
}

impl Serialize for LockSource {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for LockSource {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        String::deserialize(deserializer).map(|text| LockSource::parse(&text))
    }
}

impl Default for LockFile {
    fn default() -> Self {
        Self {
            lockfile_version: 1,
            entries: Vec::new(),
        }
    }
}

impl LockFile {
    pub fn new() -> Self {
        Self::default()
    }

    /// `entry` as the lock's entry for its name: replaces the entry of that name, else appended.
    /// The one rule for "the lock as an install leaves it" (`Change::install`, the lock plan's
    /// preview).
    pub fn put(&mut self, entry: LockFileEntry) {
        match self.entries.iter_mut().find(|e| e.name == entry.name) {
            Some(existing) => *existing = entry,
            None => self.entries.push(entry),
        }
    }

    /// Without `name`'s entry; `false` when it had none.
    pub fn without(&mut self, name: &PackageName) -> bool {
        let held = self.entries.len();
        self.entries.retain(|e| e.name != *name);
        self.entries.len() != held
    }
}

/// Write a lock file to disk as JSON.
pub fn write_lock_file(lock: &LockFile, path: &Path) -> Result<(), Diagnostic> {
    let json = serde_json::to_string_pretty(lock).map_err(|e| {
        Diagnostic::new(codes::E033, format!("failed to serialize lock file: {}", e))
    })?;

    // Replace the lock by its name (ADR 0144): a write that fails part-way (a full disk) leaves the
    // old lock whole, and a link at the lock or its temporary file is never written through.
    specforge_common::fresh::replace_file(path, json.as_bytes()).map_err(|e| {
        Diagnostic::new(
            codes::E033,
            format!("failed to write lock file at '{}': {}", path.display(), e),
        )
    })
}

/// Read a lock file from disk.
pub fn read_lock_file(path: &Path) -> Result<LockFile, Diagnostic> {
    let content = std::fs::read_to_string(path).map_err(|e| unreadable(path, &e))?;
    parse_lock_file(path, &content)
}

fn unreadable(path: &Path, error: &std::io::Error) -> Diagnostic {
    Diagnostic::new(
        codes::E033,
        format!(
            "failed to read lock file at '{}': {}",
            path.display(),
            error
        ),
    )
}

fn parse_lock_file(path: &Path, content: &str) -> Result<LockFile, Diagnostic> {
    serde_json::from_str::<LockFile>(content).map_err(|e| {
        Diagnostic::new(
            codes::E033,
            format!("corrupt lock file at '{}': {}", path.display(), e),
        )
        .with_suggestion(
            "delete the lock file and run `specforge install` to regenerate".to_string(),
        )
    })
}

/// What `<root>/specforge.lock` held when it was read: the typed result of
/// the one read an environment takes, so every operation over the project
/// sees the same lock and the same problem with it.
#[derive(Debug, Clone, PartialEq)]
pub enum LockState {
    /// No lock file: nothing is installed from a registry or a path.
    Absent,
    /// The lock file, as written.
    Read(LockFile),
    /// A lock file that could not be read or is corrupt (E033). Nothing
    /// is known to be locked.
    Unreadable(Diagnostic),
}

impl LockState {
    /// Read the lock at the project root `root` ([`lock_path`]). A file
    /// that does not exist is [`Self::Absent`], not a problem.
    pub(crate) fn read(root: &Path, disk: &dyn Disk) -> LockState {
        let path = lock_path(root);
        let content = match disk.read(&path) {
            Ok(bytes) => String::from_utf8(bytes).map_err(|_| {
                std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "stream did not contain valid UTF-8",
                )
            }),
            Err(e) => Err(e),
        };
        match content {
            Ok(content) => match parse_lock_file(&path, &content) {
                Ok(lock) => LockState::Read(lock),
                Err(problem) => LockState::Unreadable(problem),
            },
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => LockState::Absent,
            Err(e) => LockState::Unreadable(unreadable(&path, &e)),
        }
    }

    /// The lock, when there is a readable one.
    pub fn file(&self) -> Option<&LockFile> {
        match self {
            LockState::Read(lock) => Some(lock),
            LockState::Absent | LockState::Unreadable(_) => None,
        }
    }

    /// Its entries (none without a readable lock).
    pub fn entries(&self) -> &[LockFileEntry] {
        self.file().map_or(&[], |lock| lock.entries.as_slice())
    }

    /// Why there is a lock file that cannot be used.
    pub fn problem(&self) -> Option<&Diagnostic> {
        match self {
            LockState::Unreadable(problem) => Some(problem),
            LockState::Absent | LockState::Read(_) => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use specforge_registry::PlainDisk;
    use tempfile::TempDir;

    // -- write_lock_file + read_lock_file --

    // B:write_lock_file — verify unit "serializes lock file to JSON"
    #[test]
    fn test_write_lock_file_serializes_to_json() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("specforge.lock");

        let lock = LockFile {
            lockfile_version: 1,
            entries: vec![LockFileEntry {
                name: specforge_package::PackageName::parse("@specforge/software").unwrap(),
                version: "1.0.0".to_string(),
                source: crate::LockSource::parse("registry"),
                wasm_hash: "abc123".to_string(),
                key_id: None,
                peer_dependencies: Vec::new(),
            }],
        };

        write_lock_file(&lock, &path).unwrap();
        assert!(path.exists());

        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("@specforge/software"));
        assert!(content.contains("abc123"));
    }

    #[test]
    fn a_failed_lock_write_leaves_the_old_lock_whole() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("specforge.lock");
        write_lock_file(&LockFile::new(), &path).unwrap();
        let before = std::fs::read(&path).unwrap();
        // The sibling it writes first cannot be created.
        std::fs::create_dir(dir.path().join("specforge.lock.tmp")).unwrap();

        let error = write_lock_file(&LockFile::new(), &path).unwrap_err();

        assert_eq!(error.code, "E033");
        assert_eq!(std::fs::read(&path).unwrap(), before);
    }

    // B:read_lock_file — verify unit "deserializes lock file from JSON"
    #[test]
    fn test_read_lock_file_roundtrip() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("specforge.lock");

        let lock = LockFile {
            lockfile_version: 1,
            entries: vec![
                LockFileEntry {
                    name: specforge_package::PackageName::parse("@specforge/software").unwrap(),
                    version: "1.0.0".to_string(),
                    source: crate::LockSource::parse("registry"),
                    wasm_hash: "abc123".to_string(),
                    key_id: None,
                    peer_dependencies: Vec::new(),
                },
                LockFileEntry {
                    name: specforge_package::PackageName::parse("@specforge/governance").unwrap(),
                    version: "1.0.0".to_string(),
                    source: crate::LockSource::parse("local"),
                    wasm_hash: "def456".to_string(),
                    key_id: None,
                    peer_dependencies: Vec::new(),
                },
            ],
        };

        write_lock_file(&lock, &path).unwrap();
        let read_back = read_lock_file(&path).unwrap();
        assert_eq!(lock, read_back);
    }

    // B:read_lock_file — verify unit "corrupt file produces E033 diagnostic"
    #[test]
    fn test_read_lock_file_corrupt_produces_diagnostic() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("specforge.lock");
        std::fs::write(&path, "not valid json {{{").unwrap();

        let err = read_lock_file(&path).unwrap_err();
        assert_eq!(err.code, "E033");
        assert!(err.message.contains("corrupt lock file"));
        assert!(err.suggestion.is_some());
    }

    // B:read_lock_file — verify unit "missing file produces E033 diagnostic"
    #[test]
    fn test_read_lock_file_missing_produces_diagnostic() {
        let err = read_lock_file(Path::new("/nonexistent/specforge.lock")).unwrap_err();
        assert_eq!(err.code, "E033");
        assert!(err.message.contains("failed to read lock file"));
    }

    // -- LockState --

    #[test]
    fn a_lock_state_tells_absent_read_and_unreadable_apart() {
        let dir = TempDir::new().unwrap();
        assert_eq!(LockState::read(dir.path(), &PlainDisk), LockState::Absent);
        assert!(LockState::read(dir.path(), &PlainDisk).entries().is_empty());
        assert!(LockState::read(dir.path(), &PlainDisk).problem().is_none());

        let lock = LockFile::default();
        write_lock_file(&lock, &lock_path(dir.path())).unwrap();
        let state = LockState::read(dir.path(), &PlainDisk);
        assert_eq!(state, LockState::Read(lock));
        assert!(state.file().is_some() && state.problem().is_none());

        std::fs::write(lock_path(dir.path()), "not valid json {{{").unwrap();
        let state = LockState::read(dir.path(), &PlainDisk);
        let problem = state.problem().expect("a corrupt lock is a problem");
        assert_eq!(problem.code, "E033");
        assert!(problem.message.contains("corrupt lock file"));
        assert!(state.file().is_none() && state.entries().is_empty());

        // A lock path that is a directory cannot be read either.
        std::fs::remove_file(lock_path(dir.path())).unwrap();
        std::fs::create_dir(lock_path(dir.path())).unwrap();
        assert_eq!(
            LockState::read(dir.path(), &PlainDisk)
                .problem()
                .map(|p| p.code.as_str()),
            Some("E033")
        );
    }

    #[specforge_test_macros::test(
        behavior = "run_doctor_check",
        verify = "a lock file that cannot be read is an error finding naming E033"
    )]
    fn a_lock_entry_that_names_no_package_is_unreadable() {
        let dir = TempDir::new().unwrap();
        let entry = |name: &str| {
            format!(
                r#"{{"name": "{name}", "version": "1.0.0", "source": "registry", "wasm_hash": "h"}}"#
            )
        };
        for name in ["../../../outside1", "@acme/..", "Bad Name", ""] {
            let lock = format!(r#"{{"lockfile_version": 1, "entries": [{}]}}"#, entry(name));
            std::fs::write(lock_path(dir.path()), lock).unwrap();

            let state = LockState::read(dir.path(), &PlainDisk);

            let problem = state
                .problem()
                .unwrap_or_else(|| panic!("{name:?}: {state:?}"));
            assert_eq!(problem.code, "E033", "{name:?}");
            assert!(problem.message.contains("corrupt lock file"), "{problem:?}");
            assert!(state.entries().is_empty());
        }
        // A name is read as a name: the entries of a good lock are typed.
        let lock = format!(
            r#"{{"lockfile_version": 1, "entries": [{}]}}"#,
            entry("@acme/tool")
        );
        std::fs::write(lock_path(dir.path()), lock).unwrap();
        assert_eq!(
            LockState::read(dir.path(), &PlainDisk).entries()[0]
                .name
                .as_str(),
            "@acme/tool"
        );
    }

    #[test]
    #[ignore = "flipped by T3"]
    fn a_lock_that_cannot_be_read_names_one_remedy() {
        const ONE: &str = "fix specforge.lock, or delete it and install the extensions again with `specforge add`";
        let dir = TempDir::new().unwrap();
        let bad_name = r#"{"lockfile_version": 1, "entries": [{"name": "Bad Name", "version": "1.0.0", "source": "registry", "wasm_hash": "h"}]}"#;
        for cause in [Some("not valid json {{{"), None, Some(bad_name)] {
            let path = lock_path(dir.path());
            match cause {
                Some(text) => std::fs::write(&path, text).unwrap(),
                None => {
                    std::fs::remove_file(&path).unwrap();
                    std::fs::create_dir(&path).unwrap();
                }
            }
            let problem = LockState::read(dir.path(), &PlainDisk)
                .problem()
                .cloned()
                .unwrap();
            assert_eq!(problem.suggestion.as_deref(), Some(ONE), "{cause:?}");
            let refused = crate::Installed::read(dir.path(), &PlainDisk)
                .writable_lock()
                .unwrap_err();
            assert_eq!(refused, problem, "{cause:?}");
            if cause.is_none() {
                std::fs::remove_dir(&path).unwrap();
                std::fs::write(&path, "x").unwrap();
            }
        }
    }

    #[test]
    fn a_source_reads_and_writes_as_the_lock_spells_it() {
        for (text, source) in [
            ("registry", LockSource::Registry),
            ("local:ext/x.wasm", LockSource::Local("ext/x.wasm".into())),
            ("local:", LockSource::Local(String::new())),
            (
                "git+https://h/r",
                LockSource::Other("git+https://h/r".into()),
            ),
            ("local", LockSource::Other("local".into())),
        ] {
            assert_eq!(LockSource::parse(text), source, "{text}");
            assert_eq!(source.to_string(), text);
            let json = serde_json::to_string(&source).unwrap();
            assert_eq!(json, format!("\"{text}\""));
            assert_eq!(serde_json::from_str::<LockSource>(&json).unwrap(), source);
        }
        assert_eq!(
            LockSource::Local("a.wasm".into()).local_path(),
            Some("a.wasm")
        );
        assert!(LockSource::Registry.is_registry());

        // A lock written before sources were typed reads and writes back
        // byte for byte.
        let text = r#"{
  "lockfile_version": 1,
  "entries": [
    {
      "name": "@acme/tool",
      "version": "1.0.0",
      "source": "local:ext/tool.wasm",
      "wasm_hash": "abc"
    },
    {
      "name": "@acme/other",
      "version": "2.0.0",
      "source": "registry",
      "wasm_hash": "def",
      "key_id": "k"
    }
  ]
}"#;
        let lock: LockFile = serde_json::from_str(text).unwrap();
        assert_eq!(serde_json::to_string_pretty(&lock).unwrap(), text);
    }

    #[test]
    fn the_lock_lives_at_the_project_root() {
        assert_eq!(
            lock_path(Path::new("/p")),
            Path::new("/p").join("specforge.lock")
        );
    }
}
