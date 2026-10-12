//! The project's peer set (ADR 0041, amendment "the gate judges the project's peer set"): the
//! extensions a project's `specforge.json` entries load, as the peer rule reads them. The registry
//! build judges the declarations a compile loaded; the lock plan judges this set, predicted from the
//! entries and the lock a change leaves, before anything is written. Both resolve entries by
//! `Installed::resolve` and judge by `Peers`, so `add`, `init`, `update` and `check` judge one set.

use std::path::{Path, PathBuf};

use specforge_installed::{Installed, LockSource, Module, Resolved};
use specforge_package::peers::{Member, Peers};
use specforge_protocol_types::PeerDependency;
use specforge_wasm::WasmRuntime;

use super::candidate::Candidate;
use crate::OpError;

/// One extension the project loads: what the peer rule reads of it, and where it comes from.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct PeerMember {
    pub(crate) name: String,
    /// The version it loads at: the builtin's or the file's declared one, or the lock entry's.
    pub(crate) version: String,
    pub(crate) peers: Vec<PeerDependency>,
    pub(crate) origin: MemberOrigin,
}

/// Where a member's version comes from: it decides how a peer it leaves unsatisfied is refused.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum MemberOrigin {
    /// The builtin this specforge embeds: no install changes its version.
    Builtin,
    /// An installed extension, by its lock entry as the change leaves the lock.
    Installed(LockSource),
    /// A `.wasm` file entry, by the path it writes.
    File(String),
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
                    let declared = read.builtin(name)?;
                    PeerMember {
                        name: (*name).to_string(),
                        version: declared.version().to_string(),
                        peers: declared.peers().to_vec(),
                        origin: MemberOrigin::Builtin,
                    }
                }
                Resolved::Locked { entry, .. } => PeerMember {
                    name: entry.name.to_string(),
                    version: entry.version.clone(),
                    peers: entry.peer_dependencies.clone(),
                    origin: MemberOrigin::Installed(entry.source.clone()),
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
                        origin: MemberOrigin::File(file.path.to_string()),
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

    #[cfg(test)]
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

/// What a builtin or a `.wasm` file entry declares, read once each through the operation's runtime
/// (ADR 0028 D7): what the peer set needs that the lock does not record.
pub(crate) struct Declarations<'r> {
    runtime: &'r dyn WasmRuntime,
    builtins: Vec<(String, Candidate)>,
    files: Vec<(PathBuf, Option<Candidate>)>,
}

impl<'r> Declarations<'r> {
    pub(crate) fn new(runtime: &'r dyn WasmRuntime) -> Self {
        Declarations {
            runtime,
            builtins: Vec::new(),
            files: Vec::new(),
        }
    }

    /// The builtin `name`'s declaration (`Candidate::builtin`): E028 "the builtin '<name>' does not
    /// load: …" when it does not.
    pub(crate) fn builtin(&mut self, name: &str) -> Result<&Candidate, OpError> {
        if let Some(at) = self.builtins.iter().position(|(read, _)| read == name) {
            return Ok(&self.builtins[at].1);
        }
        let candidate = Candidate::builtin(self.runtime, name)?;
        self.builtins.push((name.to_string(), candidate));
        Ok(&self.builtins.last().expect("just pushed").1)
    }

    /// The declaration of the component at `path`, `None` when it can't be read or does not load:
    /// the compile reports that entry's E028, and it is no member.
    pub(crate) fn file(&mut self, path: &Path) -> Option<&Candidate> {
        let at = match self.files.iter().position(|(read, _)| read == path) {
            Some(at) => at,
            None => {
                let candidate = Module::read(path)
                    .ok()
                    .and_then(|module| Candidate::read(self.runtime, module.bytes()).ok());
                self.files.push((path.to_path_buf(), candidate));
                self.files.len() - 1
            }
        };
        self.files[at].1.as_ref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{self, declaring, serving_builtin};
    use specforge_installed::{LockFile, LockFileEntry, LockState};
    use specforge_package::PackageName;
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
        verify = "add, init and update judge the set check judges: the extensions the project's entries load"
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

        let set = ProjectPeers::of(&installed, &entries, &mut Declarations::new(&runtime)).unwrap();

        let members: Vec<(&str, &str, &MemberOrigin)> = set
            .members()
            .iter()
            .map(|m| (m.name.as_str(), m.version.as_str(), &m.origin))
            .collect();
        assert_eq!(
            members,
            [
                ("@specforge/software", "1.0.0", &MemberOrigin::Builtin),
                (
                    "@acme/tool",
                    "1.4.0",
                    &MemberOrigin::Installed(LockSource::Registry)
                ),
                (
                    "@probe/base",
                    "2.0.0",
                    &MemberOrigin::File("base.wasm".to_string())
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
}
