//! A candidate's extension declaration: what an extension the project does
//! not load yet declares — a builtin `add` or `init` enables, a binary `add`,
//! `update` or `init` installs, a binary `publish` uploads — read once,
//! through the `WasmRuntime` the operation's caller passes (ADR 0028 D7).
//! What a project loads is the environment's extension load
//! (`Installed::load`); this module reads everything else, and never builds
//! a runtime of its own.

use std::path::{Path, PathBuf};

use specforge_common::{Diagnostic, codes};
use specforge_installed::{Module, declaration_of};
use specforge_package::PackageName;
use specforge_package::peers::{Verdict, verdict};
use specforge_protocol_types::{ExtensionDeclaration, PeerDependency};
use specforge_wasm::WasmRuntime;

use super::{NOT_FOUND, builtin_name};
use crate::{OpError, OpErrorKind};

/// What an extension the project does not load yet declares, read from its
/// binary once, as every environment loads an extension (under a candidate
/// name, then unloaded).
#[derive(Debug, Clone, PartialEq)]
pub struct Candidate {
    declaration: ExtensionDeclaration,
    /// What reading it reported (W153, then W138), in the order read.
    warnings: Vec<Diagnostic>,
}

impl Candidate {
    /// What `bytes` declare: E028 "not a loadable SpecForge extension: …"
    /// when they are no extension. `publish` and `extension validate` read a
    /// binary through it.
    pub fn read(runtime: &dyn WasmRuntime, bytes: &[u8]) -> Result<Candidate, OpError> {
        let loaded = declaration_of(bytes, runtime)?;
        Ok(Candidate {
            declaration: loaded.declaration,
            warnings: loaded.warnings,
        })
    }

    /// What the builtin `name` declares, read from the binary this host
    /// embeds (`specforge_project::builtins()`). E028 when it does not load
    /// (this build's builtins are broken); `extension_not_found` when `name`
    /// is no builtin.
    pub fn builtin(runtime: &dyn WasmRuntime, name: &str) -> Result<Candidate, OpError> {
        let Some(bytes) = specforge_project::builtins().get(name) else {
            return Err(OpError::new(
                OpErrorKind::ExtensionNotFound,
                NOT_FOUND,
                format!("'{name}' is not a builtin extension"),
            ));
        };
        let loaded = declaration_of(bytes, runtime).map_err(|why| {
            OpError::diagnostic(
                codes::E028,
                format!("the builtin '{name}' does not load: {}", why.message),
            )
            .with_suggestion(
                "this specforge build's builtin extensions are broken: reinstall specforge",
            )
        })?;
        Ok(Candidate {
            declaration: loaded.declaration,
            warnings: loaded.warnings,
        })
    }

    pub fn declaration(&self) -> &ExtensionDeclaration {
        &self.declaration
    }

    pub fn warnings(&self) -> &[Diagnostic] {
        &self.warnings
    }

    pub fn name(&self) -> &str {
        self.declaration.name()
    }

    pub fn version(&self) -> &str {
        self.declaration.version()
    }

    pub fn peers(&self) -> &[PeerDependency] {
        self.declaration.peers()
    }

    /// The starter template its handshake contributes (`init`).
    pub fn starter_template(&self) -> Option<&str> {
        self.declaration.handshake.starter_template.as_deref()
    }

    /// Its declaration and the warnings reading it reported.
    pub fn into_parts(self) -> (ExtensionDeclaration, Vec<Diagnostic>) {
        (self.declaration, self.warnings)
    }
}

/// A binary to install (`add`, `update`, `init`): its module and what it
/// declares, checked as an installed extension must be — a loadable
/// extension (E028) that does not claim a builtin's name
/// (`extension_conflict`).
#[derive(Debug, Clone, PartialEq)]
pub struct Installable {
    module: Module,
    candidate: Candidate,
}

impl Installable {
    pub fn read(runtime: &dyn WasmRuntime, module: Module) -> Result<Installable, OpError> {
        let candidate = Candidate::read(runtime, module.bytes())?;
        if builtin_name(candidate.name()).is_some() {
            return Err(OpError::new(
                OpErrorKind::Conflict,
                "extension_conflict",
                format!(
                    "the extension declares the name of the builtin '{}'",
                    candidate.name()
                ),
            )
            .with_suggestion(format!(
                "enable the builtin instead: specforge add {}",
                candidate.name()
            )));
        }
        Ok(Installable { module, candidate })
    }

    pub fn module(&self) -> &Module {
        &self.module
    }

    pub fn candidate(&self) -> &Candidate {
        &self.candidate
    }

    pub fn into_module(self) -> Module {
        self.module
    }

    /// Its declared name as the package name it installs under: E072 when it
    /// is none, before anything is written (ADR 0036).
    pub fn package(&self) -> Result<PackageName, OpError> {
        PackageName::declared_by(&self.candidate.declaration)
            .map_err(|why| OpError::from(specforge_common::package::invalid(&why)))
    }
}

/// A `.wasm` file to install, read once: the path it was named by and the
/// binary it holds. `init`'s plan keeps it, so `apply` installs it without
/// reading it again.
#[derive(Debug, Clone, PartialEq)]
pub struct LocalFile {
    pub path: PathBuf,
    pub binary: Installable,
}

impl LocalFile {
    /// Read the file at `path` — E054 "file not found: …" when it is not
    /// there, "cannot read …: …" when it cannot be read — and what it
    /// declares, as [`Installable::read`] checks it.
    pub fn read(runtime: &dyn WasmRuntime, path: &Path) -> Result<LocalFile, OpError> {
        let module = Module::read(path).map_err(|e| {
            let message = if path.exists() {
                format!("cannot read {}: {e}", path.display())
            } else {
                format!("file not found: {}", path.display())
            };
            OpError::diagnostic(codes::E054, message)
        })?;
        Ok(LocalFile {
            path: path.to_path_buf(),
            binary: Installable::read(runtime, module)?,
        })
    }
}

/// The builtins to enable before the builtin `name`, dependencies first:
/// its non-optional peers that are builtins, and theirs, each read once from
/// its embedded binary. E028 when one does not load; E027 when a required
/// peer is not satisfied by the builtin this specforge embeds, judged by the
/// one peer rule (ADR 0041; E073 for a range that can't be read). A cycle
/// among required builtins stops the walk; the registry build reports it.
pub(crate) fn required_builtins(
    runtime: &dyn WasmRuntime,
    name: &'static str,
) -> Result<Vec<&'static str>, OpError> {
    let mut walk = Walk {
        runtime,
        read: Vec::new(),
        visiting: Vec::new(),
        order: Vec::new(),
    };
    walk.visit(name)?;
    walk.order.retain(|builtin| *builtin != name);
    Ok(walk.order)
}

struct Walk<'r> {
    runtime: &'r dyn WasmRuntime,
    /// Each builtin read so far, once.
    read: Vec<(&'static str, Candidate)>,
    visiting: Vec<&'static str>,
    /// Dependencies first.
    order: Vec<&'static str>,
}

impl Walk<'_> {
    fn candidate(&mut self, name: &'static str) -> Result<&Candidate, OpError> {
        if let Some(i) = self.read.iter().position(|(read, _)| *read == name) {
            return Ok(&self.read[i].1);
        }
        let candidate = Candidate::builtin(self.runtime, name)?;
        self.read.push((name, candidate));
        Ok(&self.read.last().expect("just pushed").1)
    }

    fn visit(&mut self, name: &'static str) -> Result<(), OpError> {
        // A cycle among required builtins stops here; the registry build
        // reports it.
        if self.order.contains(&name) || self.visiting.contains(&name) {
            return Ok(());
        }
        self.visiting.push(name);
        let peers: Vec<(&'static str, PeerDependency)> = self
            .candidate(name)?
            .peers()
            .iter()
            .filter(|peer| !peer.optional)
            .filter_map(|peer| Some((builtin_name(&peer.name)?, peer.clone())))
            .collect();
        for (peer, declared) in peers {
            // The builtin this specforge embeds is the one that gets
            // enabled: it must satisfy the requirement (ADR 0041).
            let embedded = self.candidate(peer)?.version().to_string();
            match verdict(&declared, Some(&embedded)) {
                Verdict::Satisfied | Verdict::Missing => {}
                Verdict::Unreadable(why) => {
                    return Err(OpError::from(specforge_common::peers::unreadable(
                        name, &declared, &why,
                    )));
                }
                Verdict::OutOfRange { .. } | Verdict::NotSemver { .. } => {
                    return Err(OpError::diagnostic(
                        codes::E027,
                        format!(
                            "the builtin '{name}' requires '{peer}' {}, but this specforge embeds {peer} {embedded}",
                            declared.version
                        ),
                    )
                    .with_suggestion(
                        "this specforge build's builtin extensions disagree: reinstall specforge",
                    ));
                }
            }
            self.visit(peer)?;
        }
        self.visiting.pop();
        self.order.push(name);
        Ok(())
    }
}
