//! The project's peer set (ADR 0041, amendment "the gate judges the project's peer set"): the
//! extensions a project's `specforge.json` entries load, as the peer rule reads them. The registry
//! build judges the declarations a compile loaded; the lock plan judges this set, predicted from the
//! entries and the lock a change leaves, before anything is written. Both resolve entries by
//! `Installed::resolve` and judge by `Peers`, so `add`, `init`, `update` and `check` judge one set.

use std::path::{Path, PathBuf};

use specforge_installed::{Installed, Module, Origin, Resolved};
use specforge_package::peers::{Member, Peers};
use specforge_protocol_types::{ExtensionDeclaration, PeerDependency};
use specforge_wasm::WasmRuntime;

use super::candidate::{builtin_declaration, read_declaration};
use crate::OpError;
use crate::view::ProjectView;

/// One extension the project loads: what the peer rule reads of it, and where it comes from.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct PeerMember {
    pub(crate) name: String,
    /// The version it loads at: the builtin's or the file's declared one, or the lock entry's.
    pub(crate) version: String,
    pub(crate) peers: Vec<PeerDependency>,
    /// Where its version comes from: it decides how a peer it leaves unsatisfied is refused. Never
    /// `Unknown`: an entry that loads nothing is no member.
    pub(crate) origin: Origin,
}

/// The extensions a project loads, in entry order (the registry build's input order).
#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct ProjectPeers {
    members: Vec<PeerMember>,
}

impl ProjectPeers {
    /// The members `entries` load over `installed`, each entry resolved by `Installed::resolve`,
    /// the extension load's one rule:
    /// - a builtin by its embedded declaration;
    /// - an installed extension by its lock entry (version and peers, what its pinned module
    ///   declares);
    /// - a `.wasm` file entry by its file's declaration, when the file loads and the entry admits
    ///   it.
    ///
    /// An entry that loads nothing (the compile's E028/E072) and a locked extension no entry
    /// enables are not members. E028 when a builtin's declaration can't be read (this build's
    /// builtins are broken).
    pub(crate) fn of(
        installed: &Installed,
        entries: &[String],
        read: &mut Declarations<'_>,
    ) -> Result<ProjectPeers, OpError> {
        let builtins = specforge_project::builtins();
        let resolution = installed.resolve(entries, &builtins);
        let mut found: Vec<(usize, PeerMember)> = Vec::new();
        for (i, resolved) in resolution.in_load_order() {
            let member = match resolved {
                Resolved::Builtin { name, .. } => {
                    let Some(declared) = read.builtin(name)? else {
                        continue;
                    };
                    PeerMember {
                        name: (*name).to_string(),
                        version: declared.version().to_string(),
                        peers: declared.peers().to_vec(),
                        origin: resolution.origin(i),
                    }
                }
                Resolved::Locked { entry, .. } => PeerMember {
                    name: entry.name.to_string(),
                    version: entry.version.clone(),
                    peers: entry.peer_dependencies.clone(),
                    origin: resolution.origin(i),
                },
                Resolved::File(file) => {
                    let Some(declared) = read.file(&file.file) else {
                        continue;
                    };
                    let name = declared.name().to_string();
                    let admitted = file.admit(&name, |loaded| {
                        found.iter().any(|(_, member)| member.name == loaded)
                    });
                    if admitted.is_err() {
                        continue;
                    }
                    PeerMember {
                        name,
                        version: declared.version().to_string(),
                        peers: declared.peers().to_vec(),
                        origin: resolution.origin(i),
                    }
                }
                Resolved::Unloadable { .. } | Resolved::Again { .. } => continue,
            };
            found.push((i, member));
        }
        // The order `Installed::load` pushes declarations in: entry order.
        found.sort_by_key(|(i, _)| *i);
        Ok(ProjectPeers {
            members: found.into_iter().map(|(_, member)| member).collect(),
        })
    }

    pub(crate) fn members(&self) -> &[PeerMember] {
        &self.members
    }

    /// The member named `name`.
    pub(crate) fn member(&self, name: &str) -> Option<&PeerMember> {
        self.members.iter().find(|member| member.name == name)
    }

    /// The one rule over the members (`specforge_package::peers`, ADR 0041 D1).
    pub(crate) fn rule(&self) -> Peers<'_> {
        Peers::of(self.members.iter().map(|member| Member {
            name: &member.name,
            version: &member.version,
            peers: &member.peers,
        }))
    }

    /// Every member that declares a peer on `peer`, with that declaration, in entry order: the
    /// requirers a version diamond names (ADR 0001).
    pub(crate) fn requirers(&self, peer: &str) -> Vec<(&str, &PeerDependency)> {
        self.members
            .iter()
            .flat_map(|member| {
                member
                    .peers
                    .iter()
                    .filter(move |declared| declared.name == peer)
                    .map(move |declared| (member.name.as_str(), declared))
            })
            .collect()
    }
}

/// What a builtin or a `.wasm` file entry declares, read once each: through the operation's
/// runtime (`add`, `update`, `init`), or from what the view's compile loaded, falling back to the
/// view's runtime for an entry it did not load (`remove`). What the lock does not record (ADR 0028
/// D7).
pub(crate) struct Declarations<'r> {
    /// With no runtime (a view assembled in a test), an entry that was not loaded is no member.
    runtime: Option<&'r dyn WasmRuntime>,
    builtins: Vec<(String, ExtensionDeclaration)>,
    files: Vec<(PathBuf, Option<ExtensionDeclaration>)>,
}

impl<'r> Declarations<'r> {
    /// Declarations read through `runtime`.
    pub(crate) fn through(runtime: &'r dyn WasmRuntime) -> Self {
        Declarations {
            runtime: Some(runtime),
            builtins: Vec::new(),
            files: Vec::new(),
        }
    }

    /// The declarations `view`'s compile loaded, by the entry that loaded each: a builtin by its
    /// name, a `.wasm` file entry by the file it names; anything else is read through the view's
    /// runtime.
    pub(crate) fn loaded_by(view: &ProjectView<'r>) -> Self {
        let registries = view.registries();
        let (mut builtins, mut files) = (Vec::new(), Vec::new());
        for entry in view.env().enabled().iter().filter(|entry| entry.loaded) {
            let Some(declared) = registries.declaration(&entry.name) else {
                continue;
            };
            match &entry.origin {
                Origin::Builtin => builtins.push((entry.name.clone(), declared.clone())),
                Origin::File { .. } => {
                    let file = view.root().and_then(|root| {
                        specforge_common::ExtensionEntry::parse(&entry.entry).file(root)
                    });
                    files.extend(file.map(|file| (file, Some(declared.clone()))));
                }
                // An installed extension is a member by its lock entry; an unknown one never loads.
                Origin::Installed { .. } | Origin::Unknown => {}
            }
        }
        Declarations {
            runtime: view.runtime().map(|runtime| &**runtime),
            builtins,
            files,
        }
    }

    /// The builtin `name`'s declaration (`builtin_declaration`): E028 "the builtin '<name>' does not
    /// load: …" when it does not; `None` when it was not loaded and there is no runtime to read it.
    pub(crate) fn builtin(&mut self, name: &str) -> Result<Option<&ExtensionDeclaration>, OpError> {
        if let Some(at) = self.builtins.iter().position(|(read, _)| read == name) {
            return Ok(Some(&self.builtins[at].1));
        }
        let Some(runtime) = self.runtime else {
            return Ok(None);
        };
        let declared = builtin_declaration(runtime, name)?.declaration;
        self.builtins.push((name.to_string(), declared));
        Ok(self.builtins.last().map(|(_, declared)| declared))
    }

    /// The declaration of the component at `path`, `None` when it can't be read or does not load:
    /// the compile reports that entry's E028, and it is no member.
    pub(crate) fn file(&mut self, path: &Path) -> Option<&ExtensionDeclaration> {
        let at = match self.files.iter().position(|(read, _)| read == path) {
            Some(at) => at,
            None => {
                let declared = self.runtime.and_then(|runtime| {
                    Module::read(path)
                        .ok()
                        .and_then(|module| read_declaration(runtime, module.bytes()).ok())
                        .map(|loaded| loaded.declaration)
                });
                self.files.push((path.to_path_buf(), declared));
                self.files.len() - 1
            }
        };
        self.files[at].1.as_ref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{self, Viewed, declaring, serving_builtin};
    use specforge_installed::{EnabledExtension, LockFile, LockFileEntry, LockSource, LockState};
    use specforge_package::PackageName;
    use specforge_project::testing::Assembled;
    use specforge_test_macros::test as specforge_test;

    fn locked(name: &str, version: &str, source: &str) -> LockFileEntry {
        LockFileEntry {
            name: PackageName::parse(name).unwrap(),
            version: version.to_string(),
            source: LockSource::parse(source),
            wasm_hash: "hash".to_string(),
            key_id: None,
            peer_dependencies: Vec::new(),
        }
    }

    #[specforge_test(
        invariant = "peer_dependency_satisfaction",
        verify = "add, init, update and remove judge the set check judges: the extensions the project's entries load"
    )]
    fn the_set_is_what_the_entries_load() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("base.wasm"), testing::BASE_2).unwrap();
        std::fs::write(dir.path().join("twin.wasm"), testing::BASE_2).unwrap();
        let installed = Installed::with_lock(
            dir.path(),
            LockState::Read(LockFile {
                lockfile_version: 1,
                entries: vec![
                    locked("@acme/tool", "1.4.0", "registry"),
                    locked("@acme/idle", "1.0.0", "registry"),
                ],
            }),
        );
        let runtime = serving_builtin(
            testing::probes(),
            "@specforge/product",
            declaring("@specforge/product", "1.0.0", &[]),
        );
        let entries: Vec<String> = [
            "@specforge/software",
            "@acme/tool",
            "@acme/tool@1.0.0",
            "base.wasm",
            "twin.wasm",
            "@acme/unlocked",
        ]
        .map(String::from)
        .to_vec();

        let set =
            ProjectPeers::of(&installed, &entries, &mut Declarations::through(&runtime)).unwrap();

        let members: Vec<(&str, &str, &Origin)> = set
            .members()
            .iter()
            .map(|m| (m.name.as_str(), m.version.as_str(), &m.origin))
            .collect();
        assert_eq!(
            members,
            [
                ("@specforge/software", "1.0.0", &Origin::Builtin),
                (
                    "@acme/tool",
                    "1.4.0",
                    &Origin::Installed {
                        source: LockSource::Registry
                    }
                ),
                (
                    "@probe/base",
                    "2.0.0",
                    &Origin::File {
                        path: "base.wasm".to_string()
                    }
                ),
            ],
            "the twin file declares a loaded name, the unlocked name loads nothing, \
             and a locked extension no entry enables is no member"
        );
        assert_eq!(
            set.requirers("@probe/base").len(),
            0,
            "nobody declares a peer on it"
        );
    }

    #[specforge_test(
        invariant = "peer_dependency_satisfaction",
        verify = "the lock plan's peer set of the project as it is names what the extension load loads, in entry order, from the entries it loads them by"
    )]
    fn the_prediction_of_the_project_as_it_is_is_what_the_load_loads() {
        let dir = tempfile::tempdir().unwrap();
        for (file, bytes) in [
            ("base.wasm", testing::BASE_2),
            ("twin.wasm", testing::BASE_2),
            ("base3.wasm", testing::BASE_1),
        ] {
            std::fs::write(dir.path().join(file), bytes).unwrap();
        }
        specforge_installed::testing::install_module(dir.path(), "@probe/dep", testing::DEP_1);
        let installed = Installed::at(dir.path());
        let runtime = testing::probes();
        let entries: Vec<String> = [
            "@specforge/software",
            "@probe/dep",
            "@probe/dep@1.0.0",
            "base.wasm",
            "twin.wasm",
            "named=base3.wasm",
            "missing.wasm",
            "@probe/unlocked",
        ]
        .map(String::from)
        .to_vec();

        let loaded = installed.load(
            &entries,
            &specforge_project::builtins(),
            &runtime,
            &specforge_registry::PlainDisk,
        );
        let set =
            ProjectPeers::of(&installed, &entries, &mut Declarations::through(&runtime)).unwrap();

        let by_the_load: Vec<(&str, &Origin)> = (loaded.enabled.iter())
            .filter(|entry| entry.loaded)
            .map(|entry| (entry.name.as_str(), &entry.origin))
            .collect();
        let by_the_prediction: Vec<(&str, &Origin)> = (set.members().iter())
            .map(|member| (member.name.as_str(), &member.origin))
            .collect();
        assert_eq!(by_the_prediction, by_the_load);
        assert_eq!(by_the_load.len(), 3, "{by_the_load:?}");
        let declared: Vec<&str> = loaded.declarations.iter().map(|d| d.name()).collect();
        let predicted: Vec<&str> = set.members().iter().map(|m| m.name.as_str()).collect();
        assert_eq!(predicted, declared);
    }

    #[specforge_test(
        invariant = "peer_dependency_satisfaction",
        verify = "add, init, update and remove judge the set check judges: the extensions the project's entries load"
    )]
    fn loaded_by_takes_a_builtin_only_from_its_entry() {
        // A `.wasm` file entry loaded under a builtin's name: its declaration is the file's, not
        // the builtin's.
        let fixture = Assembled::new()
            .runtime(std::sync::Arc::new(serving_builtin(
                testing::probes(),
                "@specforge/product",
                declaring("@specforge/product", "1.0.0", &[]),
            )))
            .enabled(vec![EnabledExtension {
                name: "@specforge/product".into(),
                loaded: true,
                ..crate::testing::unloaded("p.wasm")
            }])
            .declarations(vec![Assembled::declaration("@specforge/product", "9.9.9")]);

        let mut loaded = Declarations::loaded_by(&fixture.view());

        let builtin = loaded.builtin("@specforge/product").unwrap().unwrap();
        assert_eq!(builtin.version(), "1.0.0", "the builtin's, not the file's");
    }
}
