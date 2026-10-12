//! A project's installed extensions: where they live
//! (`.specforge/extensions/<name>/extension.wasm`), what pins them
//! (`specforge.lock`), how a project's `extensions` entries load into a
//! runtime, and how installing, updating and removing them changes all of
//! it at once or not at all (ADR 0028).

#![allow(clippy::result_large_err)]

mod change;
mod declare;
mod health;
mod layout;
mod load;
mod lock;
mod module;
#[cfg(feature = "testing")]
pub mod testing;

use std::path::{Path, PathBuf};

use specforge_common::ExtensionEntry;
use specforge_package::PackageName;

pub use change::{Change, Committed, Failed, Pin};
pub use declare::declaration_of;
pub use health::Health;
pub use layout::{LOCK_FILE, lock_path};
pub use load::{Builtins, EnabledExtension, LoadFailure, LoadProblem, LoadSubject, Loaded};
pub use lock::{LockFile, LockFileEntry, LockSource, LockState, read_lock_file, write_lock_file};
pub use module::{Module, hex_sha256};

/// A project's installed extensions: its root and what `specforge.lock`
/// held when it was read, once. Every rule about installed extensions is
/// a method of this value; nothing else derives their paths.
#[derive(Debug, Clone, PartialEq)]
pub struct Installed {
    root: PathBuf,
    lock: LockState,
}

impl Installed {
    /// The installed extensions of the project at `root`: `specforge.lock`
    /// read once (absent, read, or unreadable with its E033).
    pub fn at(root: &Path) -> Installed {
        Installed {
            root: root.to_path_buf(),
            lock: LockState::at(root),
        }
    }

    /// No project (a graph built in memory): no root, no lock.
    pub const fn none() -> Installed {
        Installed {
            root: PathBuf::new(),
            lock: LockState::Absent,
        }
    }

    /// The installed extensions at `root` as `lock` says (a lock read
    /// elsewhere, a test's fixture).
    pub fn with_lock(root: &Path, lock: LockState) -> Installed {
        Installed {
            root: root.to_path_buf(),
            lock,
        }
    }

    /// The layout of the installed extensions at `root`, the lock not read
    /// (absent): what the paths a session depends on are derived from
    /// before anything is read.
    pub fn unread(root: &Path) -> Installed {
        Installed::with_lock(root, LockState::Absent)
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn lock(&self) -> &LockState {
        &self.lock
    }

    /// `<root>/specforge.lock`.
    pub fn lock_path(&self) -> PathBuf {
        layout::lock_path(&self.root)
    }

    /// `<root>/.specforge/extensions/<name>/extension.wasm`: whatever the
    /// name, a package name is a safe relative path (ADR 0036).
    pub fn module_path(&self, name: &PackageName) -> PathBuf {
        self.package_dir(name).join(layout::MODULE_FILE)
    }

    /// `<root>/.specforge/extensions/<name>`: the directory an installed
    /// extension lives in.
    pub fn package_dir(&self, name: &PackageName) -> PathBuf {
        self.extensions_dir().join(name.relative_path())
    }

    /// `<root>/.specforge/extensions`.
    pub(crate) fn extensions_dir(&self) -> PathBuf {
        layout::extensions_dir(&self.root)
    }

    /// The module each of `entries` (specforge.json's `extensions`) loads
    /// from, in entry order: an installed extension's [`Self::module_path`],
    /// a `.wasm` file entry's file (relative to the root); none for a
    /// builtin or for a name that is no package name. The paths whose
    /// change reloads an environment (plan 03).
    pub fn modules(&self, entries: &[String], builtins: &Builtins) -> Vec<PathBuf> {
        entries
            .iter()
            .filter_map(|entry| match ExtensionEntry::parse(entry) {
                file @ ExtensionEntry::File { .. } => file.file(&self.root),
                ExtensionEntry::Named(name) if builtins.contains(name) => None,
                ExtensionEntry::Named(name) => PackageName::parse(name)
                    .ok()
                    .map(|name| self.module_path(&name)),
            })
            .collect()
    }

    /// The command that reinstalls `name` as its lock entry records it:
    /// `specforge add <path>` for a local install, `specforge add
    /// <name>@<version>` for a registry one (the name alone when the
    /// version is not semver).
    pub fn reinstall(&self, name: &str) -> String {
        let entry = self.lock.entries().iter().find(|e| e.name.as_str() == name);
        let specifier = match entry {
            Some(e) => match e.source.local_path() {
                Some(path) => path.to_string(),
                None if semver::Version::parse(&e.version).is_ok() => {
                    format!("{name}@{}", e.version)
                }
                None => name.to_string(),
            },
            None => name.to_string(),
        };
        format!("specforge add {specifier}")
    }

    /// The publisher key id the lock records for `name`'s registry install;
    /// `None` when the lock holds no registry install of it, or holds one
    /// without a key (unsigned, or locked before signing), or can't be read.
    pub fn locked_key(&self, name: &PackageName) -> Option<&str> {
        self.lock
            .entries()
            .iter()
            .find(|e| e.name == *name && e.source.is_registry())
            .and_then(|e| e.key_id.as_deref())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtins_name_themselves_in_order() {
        let builtins = Builtins(&[("@specforge/b", b"\0asm"), ("@specforge/a", b"\0asm")]);

        assert_eq!(
            builtins.names().collect::<Vec<_>>(),
            ["@specforge/b", "@specforge/a"]
        );
        assert_eq!(Builtins::none().names().count(), 0);
    }

    #[test]
    fn one_rule_names_every_installed_path() {
        let installed = Installed::with_lock(Path::new("/p"), LockState::Absent);
        let builtins = Builtins(&[("@specforge/product", b"\0asm")]);
        let tool = PackageName::parse("@acme/tool").unwrap();

        assert_eq!(installed.lock_path(), Path::new("/p/specforge.lock"));
        assert_eq!(
            installed.module_path(&tool),
            Path::new("/p/.specforge/extensions/@acme/tool/extension.wasm")
        );
        let entries: Vec<String> = [
            "@specforge/product",
            "@acme/tool",
            "tools/greet.wasm",
            "../../outside",
        ]
        .map(String::from)
        .to_vec();
        assert_eq!(
            installed.modules(&entries, &builtins),
            [
                PathBuf::from("/p/.specforge/extensions/@acme/tool/extension.wasm"),
                PathBuf::from("/p/tools/greet.wasm"),
            ],
            "a builtin has no module and a name that is no package has none either"
        );
    }

    #[test]
    fn the_lock_answers_the_key_of_a_registry_install_only() {
        let entry = |name: &str, source: &str, key: Option<&str>| LockFileEntry {
            name: PackageName::parse(name).unwrap(),
            version: "1.0.0".to_string(),
            source: LockSource::parse(source),
            wasm_hash: "hash".to_string(),
            key_id: key.map(str::to_string),
            peer_dependencies: Vec::new(),
        };
        let lock = LockFile {
            entries: vec![
                entry("@a/signed", "registry", Some("k1")),
                entry("@a/unsigned", "registry", None),
                entry("@a/local", "local:a.wasm", Some("k2")),
            ],
            ..Default::default()
        };
        let installed = Installed::with_lock(Path::new("/p"), LockState::Read(lock));
        let key = |name: &str| installed.locked_key(&PackageName::parse(name).unwrap());

        assert_eq!(key("@a/signed"), Some("k1"));
        assert_eq!(key("@a/unsigned"), None);
        assert_eq!(key("@a/local"), None, "a local install has no publisher");
        assert_eq!(key("@a/absent"), None);
        let unreadable = Installed::with_lock(
            Path::new("/p"),
            LockState::Unreadable(specforge_common::Diagnostic::new(
                specforge_common::codes::E033,
                "x",
            )),
        );
        assert_eq!(
            unreadable.locked_key(&PackageName::parse("@a/signed").unwrap()),
            None
        );
    }

    #[test]
    fn a_reinstall_command_is_the_one_the_lock_entry_calls_for() {
        let entry = |name: &str, version: &str, source: &str| LockFileEntry {
            name: specforge_package::PackageName::parse(name).unwrap(),
            version: version.to_string(),
            source: LockSource::parse(source),
            wasm_hash: "hash".to_string(),
            key_id: None,
            peer_dependencies: Vec::new(),
        };
        let lock = LockFile {
            entries: vec![
                entry("@a/local", "1.0.0", "local:ext/a.wasm"),
                entry("@a/registry", "2.1.0", "registry"),
                entry("@a/loose", "latest", "registry"),
            ],
            ..Default::default()
        };
        let installed = Installed::with_lock(Path::new("/p"), LockState::Read(lock));

        assert_eq!(installed.reinstall("@a/local"), "specforge add ext/a.wasm");
        assert_eq!(
            installed.reinstall("@a/registry"),
            "specforge add @a/registry@2.1.0"
        );
        assert_eq!(installed.reinstall("@a/loose"), "specforge add @a/loose");
        assert_eq!(
            installed.reinstall("@a/unlocked"),
            "specforge add @a/unlocked"
        );
    }
}
