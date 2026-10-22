//! Which extension each `specforge.json` `extensions` entry names, decided once (ADR 0028 D3): the
//! extension load and the lock plan's peer set read it, so they cannot disagree about what an
//! entry enables.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use specforge_common::ExtensionEntry;
use specforge_package::PackageName;

use crate::Installed;
use crate::load::{Builtins, EnabledExtension, LoadProblem};
use crate::lock::{LockFileEntry, LockSource, LockState};

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
    /// under until the name it declares is admitted.
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
    /// written before `=` when there is one (`NotTheNameWritten`), never a builtin's name
    /// (`BuiltinsName`: only the builtin loads under it, ADR 0214), and an extension an entry tried
    /// before it must not already be loaded under that name (`AlreadyLoaded`; `loaded` says which
    /// names are).
    pub(crate) fn admit(
        &self,
        declared: &str,
        builtins: &Builtins,
        loaded: impl Fn(&str) -> bool,
    ) -> Result<(), LoadProblem> {
        if let Some(written) = self.written.filter(|written| *written != declared) {
            return Err(LoadProblem::NotTheNameWritten {
                declared: declared.to_string(),
                written: written.to_string(),
            });
        }
        if builtins.contains(declared) {
            return Err(LoadProblem::BuiltinsName {
                declared: declared.to_string(),
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
    /// The builtins the entries were resolved over: a `.wasm` file entry is never admitted under
    /// one's name.
    builtins: Builtins<'a>,
}

impl<'a> Resolution<'a> {
    /// Each entry's, in entry order.
    pub fn entries(&self) -> &[Resolved<'a>] {
        &self.entries
    }

    /// The entries in the order the extension load tries them, with their index into the entries:
    /// every named entry, then every `.wasm` file entry, each in entry order (a file that declares
    /// a name a named entry loads is refused, never swapped in). `Again` entries are left out.
    pub(crate) fn in_load_order(&self) -> impl Iterator<Item = (usize, &Resolved<'a>)> + '_ {
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

impl<'a> Resolution<'a> {
    /// The extension load's walk (ADR 0181 D1): every entry in load order ([`Self::in_load_order`];
    /// `Again` entries are left out, they load nothing of their own). `read` says what a resolved
    /// entry declares: the name it declares and what it brings, or why it brings nothing. The
    /// declared name is admitted against the names the entries tried before it were admitted under
    /// ([`Resolved::admit`]); `settle` turns what `read` brought and that verdict (the name the
    /// entry loads under, or the problem refusing it) into the entry's outcome. Only an admitted
    /// entry's name counts as loaded: an entry `read` refused leaves its name free, and a `settle`
    /// that errs for an admitted entry does not free it. `read` is called once per such entry, in
    /// load order, with the entry's index; `settle` right after, only when `read` returned `Ok`.
    /// The outcomes come back in entry order, each with its index.
    pub fn walk<T, U, E>(
        &self,
        mut read: impl FnMut(usize, &Resolved<'a>) -> Result<(String, T), E>,
        mut settle: impl FnMut(&Resolved<'a>, T, Result<&str, LoadProblem>) -> Result<U, E>,
    ) -> Vec<(usize, Result<U, E>)> {
        let mut admitted: HashSet<String> = HashSet::new();
        let mut outcomes = Vec::new();
        for (i, resolved) in self.in_load_order() {
            let outcome = read(i, resolved).and_then(|(declared, brings)| {
                match resolved.admit(&declared, &self.builtins, |name| admitted.contains(name)) {
                    Ok(()) => {
                        let name = match resolved {
                            Resolved::File(_) => declared,
                            Resolved::Builtin { name, .. }
                            | Resolved::Locked { name, .. }
                            | Resolved::Unloadable { name, .. } => (*name).to_string(),
                            Resolved::Again { .. } => unreachable!("left out of the load order"),
                        };
                        let outcome = settle(resolved, brings, Ok(&name));
                        admitted.insert(name);
                        outcome
                    }
                    Err(problem) => settle(resolved, brings, Err(problem)),
                }
            });
            outcomes.push((i, outcome));
        }
        outcomes.sort_by_key(|(i, _)| *i);
        outcomes
    }
}

/// Where an extension comes from, as the extension load takes it (ADR 0028 D3, ADR 0180).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Origin {
    /// Embedded in the binary; enabling it is only a config entry.
    Builtin,
    /// Installed under `.specforge/extensions/`, pinned in `specforge.lock`
    /// with this source.
    Installed { source: LockSource },
    /// Loaded from the `.wasm` file a `specforge.json` entry names; `path`
    /// as the entry writes it.
    File { path: String },
    /// Named by an entry, neither a builtin nor locked: it does not load
    /// (E028).
    Unknown,
}

impl Origin {
    /// What the listings call it: `builtin`, the lock entry's source, or
    /// `file:<path>`.
    pub fn source(&self) -> String {
        match self {
            Origin::Builtin => "builtin".to_string(),
            Origin::Installed { source } => source.to_string(),
            Origin::File { path } => format!("file:{path}"),
            Origin::Unknown => "unknown".to_string(),
        }
    }
}

impl Resolution<'_> {
    /// Where the entry at `at` loads its extension from, by the load's one rule: a builtin's name
    /// is `Builtin`; a locked name `Installed { source }`; a `.wasm` file entry `File { path }`
    /// (as written); a name nothing loads `Unknown`; an entry naming what an earlier one enables,
    /// that entry's origin. Panics on an index the resolution did not make.
    pub fn origin(&self, at: usize) -> Origin {
        match &self.entries[at] {
            Resolved::Builtin { .. } => Origin::Builtin,
            Resolved::Locked { entry, .. } => Origin::Installed {
                source: entry.source.clone(),
            },
            Resolved::File(file) => Origin::File {
                path: file.path.to_string(),
            },
            Resolved::Unloadable { .. } => Origin::Unknown,
            Resolved::Again { first } => self.origin(*first),
        }
    }
}

impl Resolution<'_> {
    /// Each of `entries` as it enables what it names before anything loads: its origin, `loaded:
    /// false`. `entries` are the texts this resolution was made from.
    pub(crate) fn unloaded(&self, entries: &[String]) -> Vec<EnabledExtension> {
        debug_assert_eq!(entries.len(), self.entries.len());
        entries
            .iter()
            .enumerate()
            .map(|(i, entry)| EnabledExtension::unloaded(entry, self.origin(i)))
            .collect()
    }
}

impl Resolved<'_> {
    /// Whether a component declaring `declared` is what this entry enables: the one place the
    /// admit rules live. A builtin admits any name; a locked entry must declare its own name
    /// (`NotItsLockEntry`); a `.wasm` file entry follows [`FileEntry::admit`]: the name written,
    /// never a builtin's, not one already loaded. `Unloadable` and
    /// `Again` never reach a component, so they admit nothing to refuse.
    pub(crate) fn admit(
        &self,
        declared: &str,
        builtins: &Builtins,
        loaded: impl Fn(&str) -> bool,
    ) -> Result<(), LoadProblem> {
        match self {
            Resolved::Locked { name, .. } if *name != declared => {
                Err(LoadProblem::NotItsLockEntry {
                    declared: declared.to_string(),
                })
            }
            Resolved::File(file) => file.admit(declared, builtins, loaded),
            _ => Ok(()),
        }
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
        Resolution {
            entries: resolved,
            builtins: *builtins,
        }
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
    fn the_walk_admits_against_the_names_entries_before_it_were_admitted_under() {
        let installed = Installed::with_lock(Path::new("/p"), lock_of(&["@acme/tool"]));
        let entries = strings(&[
            "@acme/tool",
            "a.wasm",
            "b.wasm",
            "c.wasm",
            "@acme/tool@1.0.0",
        ]);
        let walk = |tool_loads: bool| {
            let resolution = installed.resolve(&entries, &Builtins::none());
            resolution.walk(
                |_, resolved| match resolved {
                    Resolved::Locked { name, .. } if tool_loads => Ok(((*name).to_string(), ())),
                    Resolved::Locked { .. } => Err(LoadProblem::NotInstalled),
                    Resolved::File(file) if file.key == "a.wasm" => {
                        Ok(("@acme/tool".to_string(), ()))
                    }
                    _ => Ok(("@acme/x".to_string(), ())),
                },
                |_, (), verdict| verdict.map(str::to_string),
            )
        };
        let declared = |name: &str| LoadProblem::AlreadyLoaded {
            declared: name.to_string(),
        };

        assert_eq!(
            walk(false),
            [
                (0, Err(LoadProblem::NotInstalled)),
                (1, Ok("@acme/tool".to_string())),
                (2, Ok("@acme/x".to_string())),
                (3, Err(declared("@acme/x"))),
            ],
            "an entry that did not load leaves its name free"
        );
        assert_eq!(
            walk(true),
            [
                (0, Ok("@acme/tool".to_string())),
                (1, Err(declared("@acme/tool"))),
                (2, Ok("@acme/x".to_string())),
                (3, Err(declared("@acme/x"))),
            ]
        );
    }

    #[test]
    fn each_entry_has_the_origin_the_load_takes_it_from() {
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
        let file = |path: &str| Origin::File { path: path.into() };
        let registry = Origin::Installed {
            source: LockSource::Registry,
        };

        let resolution = installed.resolve(&entries, &builtins);

        let origins: Vec<Origin> = (0..entries.len()).map(|i| resolution.origin(i)).collect();
        assert_eq!(
            origins,
            [
                Origin::Builtin,
                registry.clone(),
                registry,
                Origin::Unknown,
                Origin::Unknown,
                file("x.wasm"),
                file("y.wasm"),
                file("x.wasm"),
            ]
        );
        let stray = Installed::with_lock(Path::new("/p"), lock_of(&["@specforge/product"]));
        let entries = strings(&["@specforge/product"]);
        assert_eq!(
            stray.resolve(&entries, &builtins).origin(0),
            Origin::Builtin
        );
        let unreadable = Installed::with_lock(
            Path::new("/p"),
            LockState::Unreadable(specforge_common::Diagnostic::new(
                specforge_common::codes::E033,
                "x",
            )),
        );
        let entries = strings(&["@acme/tool"]);
        assert_eq!(
            unreadable.resolve(&entries, &builtins).origin(0),
            Origin::Unknown
        );
    }

    #[test]
    fn a_named_entry_is_admitted_only_as_its_lock_entry() {
        let entry = locked("@acme/a");
        let builtin = Resolved::Builtin {
            name: "@acme/x",
            bytes: b"",
        };
        let installed = Resolved::Locked {
            name: "@acme/a",
            entry: &entry,
        };
        assert_eq!(
            builtin.admit("@acme/anything", &Builtins::none(), |_| true),
            Ok(())
        );
        assert_eq!(
            installed.admit("@acme/a", &Builtins::none(), |_| true),
            Ok(())
        );
        assert_eq!(
            installed.admit("@acme/b", &Builtins::none(), |_| false),
            Err(LoadProblem::NotItsLockEntry {
                declared: "@acme/b".to_string()
            })
        );
        let file = FileEntry {
            key: "g.wasm",
            path: "g.wasm",
            file: PathBuf::from("/p/g.wasm"),
            written: Some("@acme/x"),
        };
        assert_eq!(
            Resolved::File(file.clone()).admit("@acme/g", &Builtins::none(), |_| false),
            file.admit("@acme/g", &Builtins::none(), |_| false)
        );
    }

    #[test]
    fn admit_refuses_another_name_and_an_already_loaded_one() {
        let file = |written: Option<&'static str>| FileEntry {
            key: "g.wasm",
            path: "g.wasm",
            file: PathBuf::from("/p/g.wasm"),
            written,
        };

        assert_eq!(
            file(None).admit("@acme/g", &Builtins::none(), |_| false),
            Ok(())
        );
        assert_eq!(
            file(Some("@acme/g")).admit("@acme/g", &Builtins::none(), |_| false),
            Ok(())
        );
        assert_eq!(
            file(Some("@acme/x")).admit("@acme/g", &Builtins::none(), |_| false),
            Err(LoadProblem::NotTheNameWritten {
                declared: "@acme/g".to_string(),
                written: "@acme/x".to_string()
            })
        );
        assert_eq!(
            file(None).admit("@acme/g", &Builtins::none(), |name| name == "@acme/g"),
            Err(LoadProblem::AlreadyLoaded {
                declared: "@acme/g".to_string()
            })
        );
        // The entry's own key is not "another entry".
        assert_eq!(
            file(None).admit("g.wasm", &Builtins::none(), |_| true),
            Ok(())
        );
    }

    #[test]
    fn a_file_entry_is_never_admitted_under_a_builtins_name() {
        let builtins = Builtins(&[("@acme/core", b"\0asm")]);
        let file = FileEntry {
            key: "g.wasm",
            path: "g.wasm",
            file: PathBuf::from("/p/g.wasm"),
            written: None,
        };
        let named = FileEntry {
            written: Some("@acme/core"),
            ..file.clone()
        };
        for entry in [file, named] {
            assert_eq!(
                entry.admit("@acme/core", &builtins, |_| false),
                Err(LoadProblem::BuiltinsName {
                    declared: "@acme/core".into()
                })
            );
            assert_eq!(
                entry.admit("@acme/g", &builtins, |_| false).is_ok(),
                entry.written.is_none()
            );
        }
        // A builtin entry admits its own name.
        let builtin = Resolved::Builtin {
            name: "@acme/core",
            bytes: b"",
        };
        assert_eq!(builtin.admit("@acme/core", &builtins, |_| false), Ok(()));
    }
}
