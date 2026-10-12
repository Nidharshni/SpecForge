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
    /// Read the extension `path` names — a `.wasm` file, or a crate
    /// directory's built component ([`binary_at`]) — and what it declares,
    /// as [`Installable::read`] checks it. E054 "file not found: …" when
    /// nothing is at `path`; E040 when a directory holds no built
    /// component; "cannot read …: …" (E054) when the file cannot be read.
    /// `path` is the component read, so the lock records `local:<that
    /// component>`.
    pub fn read(runtime: &dyn WasmRuntime, path: &Path) -> Result<LocalFile, OpError> {
        if !path.exists() {
            return Err(OpError::diagnostic(
                codes::E054,
                format!("file not found: {}", path.display()),
            ));
        }
        let path = binary_at(path)?;
        let module = Module::read(&path).map_err(|e| {
            OpError::diagnostic(codes::E054, format!("cannot read {}: {e}", path.display()))
        })?;
        Ok(LocalFile {
            binary: Installable::read(runtime, module)?,
            path,
        })
    }
}

/// The binary `path` names: a `.wasm` component as given, or, for a
/// crate directory, the component its crate builds
/// (`target/wasm32-wasip2/release/<crate>.wasm`, the only `.wasm` there
/// when the crate's name can't be read). E040 when there is none (no file,
/// not built, several components and none the crate's).
pub fn binary_at(path: &Path) -> Result<PathBuf, OpError> {
    if !path.is_dir() {
        if path.is_file() {
            return Ok(path.to_path_buf());
        }
        return Err(OpError::coded(
            OpErrorKind::PreconditionFailed,
            codes::E040,
            format!("no extension binary at {}", path.display()),
        )
        .with_suggestion("name a .wasm component, or the extension's crate directory"));
    }
    let release = path.join("target/wasm32-wasip2/release");
    let not_built = || {
        OpError::coded(
            OpErrorKind::PreconditionFailed,
            codes::E040,
            format!(
                "no built component in {}: build the extension first",
                release.display()
            ),
        )
        .with_suggestion("cargo build --release --target wasm32-wasip2")
    };
    if let Some(name) = crate_name(&path.join("Cargo.toml")) {
        let built = release.join(format!("{}.wasm", name.replace('-', "_")));
        if built.is_file() {
            return Ok(built);
        }
    }
    let mut built: Vec<PathBuf> = std::fs::read_dir(&release)
        .map_err(|_| not_built())?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|e| e == "wasm"))
        .collect();
    built.sort();
    match built.len() {
        0 => Err(not_built()),
        1 => Ok(built.remove(0)),
        _ => Err(OpError::coded(
            OpErrorKind::PreconditionFailed,
            codes::E040,
            format!(
                "{} holds several components ({}): name the one to publish",
                release.display(),
                built
                    .iter()
                    .filter_map(|p| p.file_name()?.to_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        )),
    }
}

/// The `[package]` name of the crate whose manifest is `cargo_toml`.
fn crate_name(cargo_toml: &Path) -> Option<String> {
    let text = std::fs::read_to_string(cargo_toml).ok()?;
    let mut in_package = false;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_package = line == "[package]";
            continue;
        }
        if in_package
            && let Some(value) = line.strip_prefix("name")
            && let Some(value) = value.trim_start().strip_prefix('=')
        {
            return Some(value.trim().trim_matches('"').to_string());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn a_crate_directory_names_its_built_component() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("Cargo.toml"),
            "[package]\nname = \"my-ext\"\nversion = \"0.1.0\"\n",
        )
        .unwrap();
        let error = binary_at(dir.path()).unwrap_err();
        assert!(
            error.message.contains("build the extension first"),
            "{error:?}"
        );
        let release = dir.path().join("target/wasm32-wasip2/release");
        std::fs::create_dir_all(&release).unwrap();
        std::fs::write(release.join("other.wasm"), b"x").unwrap();
        std::fs::write(release.join("my_ext.wasm"), b"x").unwrap();
        assert_eq!(binary_at(dir.path()).unwrap(), release.join("my_ext.wasm"));
        let file = release.join("other.wasm");
        assert_eq!(binary_at(&file).unwrap(), file);
        assert!(binary_at(&dir.path().join("missing.wasm")).is_err());
    }
}
