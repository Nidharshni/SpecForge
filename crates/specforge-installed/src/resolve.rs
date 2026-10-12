//! Which extension each `specforge.json` `extensions` entry names, decided once (ADR 0028 D3): the
//! extension load, the module paths a session watches and the lock plan's peer set read it, so they
//! cannot disagree about what an entry enables.

use std::collections::HashMap;
use std::path::PathBuf;

use specforge_common::ExtensionEntry;
use specforge_package::PackageName;

use crate::Installed;
use crate::load::{Builtins, LoadProblem};
use crate::lock::{LockFileEntry, LockState};

/// What one `extensions` entry names, decided as the extension load decides it, nothing loaded.
#[derive(Debug, Clone, PartialEq)]
pub enum Resolved<'a> {
    /// A builtin's name: it loads from the bytes this host embeds.
    Builtin { name: &'a str, bytes: &'a [u8] },
    /// An installed extension's name: it loads from the module `entry` pins.
    Locked {
        name: &'a str,
        entry: &'a LockFileEntry,
    },
    /// A named entry nothing can load: no package name (`NotAPackageName`, E072), no lock entry
    /// (`NotInstalled`, E028) or a lock that can't be read (`LockUnreadable`, E028).
    Unloadable { name: &'a str, problem: LoadProblem },
    /// A `.wasm` file entry: it loads from its file under the name the component declares, after
    /// every named entry.
    File(FileEntry<'a>),
    /// An entry naming what the entry at `first` enables (the same name, or the same `.wasm` file
    /// entry text): loaded once, by `first`.
    Again { first: usize },
}

/// A `.wasm` file entry of `specforge.json`.
#[derive(Debug, Clone, PartialEq)]
pub struct FileEntry<'a> {
    /// The entry as written, trimmed: what its diagnostics name, and the key its component loads
    /// under until its handshake names it.
    pub key: &'a str,
    /// The path as written.
    pub path: &'a str,
    /// The file it names (a relative path is under the project root).
    pub file: PathBuf,
    /// The name written before `=`, if any.
    pub written: Option<&'a str>,
}

impl FileEntry<'_> {
    /// Whether a component declaring `declared` is what this entry enables. It must declare the name
    /// written before `=` when there is one (`NotTheNameWritten`), and an extension an entry tried
    /// before it must not already be loaded under that name (`AlreadyLoaded`; `loaded` says which
    /// names are).
    pub fn admit(&self, declared: &str, loaded: impl Fn(&str) -> bool) -> Result<(), LoadProblem> {
        if let Some(written) = self.written.filter(|written| *written != declared) {
            return Err(LoadProblem::NotTheNameWritten {
                declared: declared.to_string(),
                written: written.to_string(),
            });
        }
        if declared != self.key && loaded(declared) {
            return Err(LoadProblem::AlreadyLoaded {
                declared: declared.to_string(),
            });
        }
        Ok(())
    }
}

/// What a project's entries name: one [`Resolved`] per entry, in entry order.
#[derive(Debug, Clone, PartialEq)]
pub struct Resolution<'a> {
    entries: Vec<Resolved<'a>>,
}

impl<'a> Resolution<'a> {
    /// Each entry's, in entry order.
    pub fn entries(&self) -> &[Resolved<'a>] {
        &self.entries
    }

    /// The entries in the order the extension load tries them, with their index into the entries:
    /// every named entry, then every `.wasm` file entry, each in entry order (a file that declares
    /// a name a named entry loads is refused, never swapped in). `Again` entries are left out.
    pub fn in_load_order(&self) -> impl Iterator<Item = (usize, &Resolved<'a>)> + '_ {
        let is_file = |resolved: &Resolved| matches!(resolved, Resolved::File(_));
        let indexed = || {
            self.entries
                .iter()
                .enumerate()
                .filter(|(_, resolved)| !matches!(resolved, Resolved::Again { .. }))
        };
        indexed()
            .filter(move |(_, resolved)| !is_file(resolved))
            .chain(indexed().filter(move |(_, resolved)| is_file(resolved)))
    }
}

impl Installed {
    /// What `entries` name over this project's lock and `builtins`, nothing read from disk.
    /// - A builtin's name is the builtin, before any lock entry of that name.
    /// - Any other named entry is its lock entry, or `Unloadable`.
    /// - A `.wasm` file entry is a `File`.
    ///
    /// An extension two entries enable is `Again` for the second.
    pub fn resolve<'a>(&'a self, entries: &'a [String], builtins: &Builtins<'a>) -> Resolution<'a> {
        let mut named: HashMap<&str, usize> = HashMap::new();
        let mut files: HashMap<&str, usize> = HashMap::new();
        let resolved = entries
            .iter()
            .enumerate()
            .map(|(i, entry)| match ExtensionEntry::parse(entry) {
                ExtensionEntry::Named(name) => match named.get(name) {
                    Some(&first) => Resolved::Again { first },
                    None => {
                        named.insert(name, i);
                        self.resolve_named(name, builtins)
                    }
                },
                parsed @ ExtensionEntry::File {
                    name: written,
                    path,
                } => {
                    let key = entry.trim();
                    match files.get(key) {
                        Some(&first) => Resolved::Again { first },
                        None => {
                            files.insert(key, i);
                            Resolved::File(FileEntry {
                                key,
                                path,
                                file: parsed.file(&self.root).expect("a file entry names a file"),
                                written,
                            })
                        }
                    }
                }
            })
            .collect();
        Resolution { entries: resolved }
    }

    /// The extension a named entry names: a builtin from its bytes, an installed one by its lock
    /// entry.
    fn resolve_named<'a>(&'a self, name: &'a str, builtins: &Builtins<'a>) -> Resolved<'a> {
        if let Some(bytes) = builtins.get(name) {
            return Resolved::Builtin { name, bytes };
        }
        // What names a directory under `.specforge/extensions` is a package name, checked before
        // the lock is asked (ADR 0036).
        let package = match PackageName::parse(name) {
            Ok(package) => package,
            Err(why) => {
                return Resolved::Unloadable {
                    name,
                    problem: LoadProblem::NotAPackageName {
                        reason: why.to_string(),
                    },
                };
            }
        };
        match &self.lock {
            LockState::Read(lock) => match lock.entries.iter().find(|e| e.name == package) {
                Some(entry) => Resolved::Locked { name, entry },
                None => Resolved::Unloadable {
                    name,
                    problem: LoadProblem::NotInstalled,
                },
            },
            LockState::Absent => Resolved::Unloadable {
                name,
                problem: LoadProblem::NotInstalled,
            },
            LockState::Unreadable(_) => Resolved::Unloadable {
                name,
                problem: LoadProblem::LockUnreadable,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lock::{LockFile, LockSource};
    use specforge_test_macros::test as specforge_test;
    use std::path::Path;

    fn locked(name: &str) -> LockFileEntry {
        LockFileEntry {
            name: PackageName::parse(name).unwrap(),
            version: "1.0.0".to_string(),
            source: LockSource::Registry,
            wasm_hash: "hash".to_string(),
            key_id: None,
            peer_dependencies: Vec::new(),
        }
    }

    fn lock_of(names: &[&str]) -> LockState {
        LockState::Read(LockFile {
            lockfile_version: 1,
            entries: names.iter().map(|name| locked(name)).collect(),
        })
    }

    fn strings(entries: &[&str]) -> Vec<String> {
        entries.iter().map(|e| e.to_string()).collect()
    }

    #[specforge_test(
        behavior = "load_extension_manifests",
        verify = "the extension load and the lock plan resolve an entry by one rule"
    )]
    fn an_entry_resolves_as_the_load_tries_it() {
        let installed = Installed::with_lock(Path::new("/p"), lock_of(&["@acme/tool"]));
        let builtins = Builtins(&[("@specforge/product", b"\0asm")]);
        let entries = strings(&[
            "@specforge/product",
            "@acme/tool",
            "@acme/tool@1.0.0",
            "Bad Name",
            "@acme/none",
            "x.wasm",
            "g=y.wasm",
            "x.wasm",
        ]);

        let resolution = installed.resolve(&entries, &builtins);
        let resolved = resolution.entries();

        assert!(matches!(
            resolved[0],
            Resolved::Builtin {
                name: "@specforge/product",
                ..
            }
        ));
        assert!(
            matches!(&resolved[1], Resolved::Locked { name: "@acme/tool", entry }
            if entry.name.as_str() == "@acme/tool")
        );
        assert_eq!(resolved[2], Resolved::Again { first: 1 });
        assert!(matches!(
            &resolved[3],
            Resolved::Unloadable {
                problem: LoadProblem::NotAPackageName { .. },
                ..
            }
        ));
        assert_eq!(
            resolved[4],
            Resolved::Unloadable {
                name: "@acme/none",
                problem: LoadProblem::NotInstalled
            }
        );
        assert_eq!(
            resolved[5],
            Resolved::File(FileEntry {
                key: "x.wasm",
                path: "x.wasm",
                file: PathBuf::from("/p/x.wasm"),
                written: None,
            })
        );
        assert!(matches!(&resolved[6], Resolved::File(f) if f.written == Some("g")));
        assert_eq!(resolved[7], Resolved::Again { first: 5 });
        let order: Vec<usize> = resolution.in_load_order().map(|(i, _)| i).collect();
        assert_eq!(order, [0, 1, 3, 4, 5, 6]);

        // A lock that can't be read: an installed name is unloadable for that reason.
        let unreadable = Installed::with_lock(
            Path::new("/p"),
            LockState::Unreadable(specforge_common::Diagnostic::new(
                specforge_common::codes::E033,
                "x",
            )),
        );
        let entries = strings(&["@acme/tool"]);
        assert_eq!(
            unreadable.resolve(&entries, &builtins).entries()[0],
            Resolved::Unloadable {
                name: "@acme/tool",
                problem: LoadProblem::LockUnreadable
            }
        );

        // A lock entry named like a builtin never wins over the builtin.
        let shadowed = Installed::with_lock(Path::new("/p"), lock_of(&["@specforge/product"]));
        let entries = strings(&["@specforge/product"]);
        assert!(matches!(
            shadowed.resolve(&entries, &builtins).entries()[0],
            Resolved::Builtin { .. }
        ));
    }

    #[test]
    fn admit_refuses_another_name_and_an_already_loaded_one() {
        let file = |written: Option<&'static str>| FileEntry {
            key: "g.wasm",
            path: "g.wasm",
            file: PathBuf::from("/p/g.wasm"),
            written,
        };

        assert_eq!(file(None).admit("@acme/g", |_| false), Ok(()));
        assert_eq!(file(Some("@acme/g")).admit("@acme/g", |_| false), Ok(()));
        assert_eq!(
            file(Some("@acme/x")).admit("@acme/g", |_| false),
            Err(LoadProblem::NotTheNameWritten {
                declared: "@acme/g".to_string(),
                written: "@acme/x".to_string()
            })
        );
        assert_eq!(
            file(None).admit("@acme/g", |name| name == "@acme/g"),
            Err(LoadProblem::AlreadyLoaded {
                declared: "@acme/g".to_string()
            })
        );
        // The entry's own key is not "another entry".
        assert_eq!(file(None).admit("g.wasm", |_| true), Ok(()));
    }
}
