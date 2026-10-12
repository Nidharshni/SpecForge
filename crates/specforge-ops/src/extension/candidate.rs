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
