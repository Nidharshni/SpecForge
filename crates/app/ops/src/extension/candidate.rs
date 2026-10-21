//! A candidate's extension declaration: what an extension the project does
//! not load yet declares — a builtin `add` or `init` enables, a binary `add`,
//! `update` or `init` installs, a binary `publish` uploads — read once,
//! through the `WasmRuntime` the operation's caller passes (ADR 0028 D7).
//! What a project loads is the environment's extension load
//! (`Installed::load`); this module reads everything else, and never builds
//! a runtime of its own. What a binary declares is the load's answer
//! (`Loaded`); the package it names is derived here once (ADR 0172).

use std::path::{Path, PathBuf};

use specforge_common::codes;
use specforge_installed::{Module, declaration_of};
use specforge_package::PackageName;
use specforge_protocol_types::ExtensionDeclaration;
use specforge_wasm::WasmRuntime;
use specforge_wasm::protocol::Loaded;

use super::{NOT_FOUND, builtin_name};
use crate::{Containment, Held, OpError, OpErrorKind, ReadFailure};

/// What `bytes` declare, loaded as every environment loads an extension (under
/// a candidate name, then unloaded): E028 "not a loadable SpecForge
/// extension: …" when they are no extension. `publish` and `extension
/// validate` read a binary through it.
pub(crate) fn read_declaration(runtime: &dyn WasmRuntime, bytes: &[u8]) -> Result<Loaded, OpError> {
    declaration_of(bytes, runtime).map_err(|why| OpError::of(OpErrorKind::InvalidInput, why))
}

/// What the builtin `name` declares, read from the binary this host embeds
/// (`specforge_project::builtins()`). E028 when it does not load (this
/// build's builtins are broken); `extension_not_found` when `name` is no
/// builtin.
pub fn builtin_declaration(runtime: &dyn WasmRuntime, name: &str) -> Result<Loaded, OpError> {
    let Some(bytes) = specforge_project::builtins().get(name) else {
        return Err(OpError::new(
            OpErrorKind::ExtensionNotFound,
            NOT_FOUND,
            format!("'{name}' is not a builtin extension"),
        ));
    };
    declaration_of(bytes, runtime).map_err(|why| {
        OpError::coded(
            crate::OpErrorKind::Internal,
            codes::E028,
            format!("the builtin '{name}' does not load: {}", why.message),
        )
        .with_suggestion(
            "this specforge build's builtin extensions are broken: reinstall specforge",
        )
    })
}

/// The package name `declaration` declares: E072 when it is none, before
/// anything is written (ADR 0036).
pub(crate) fn package(declaration: &ExtensionDeclaration) -> Result<PackageName, OpError> {
    PackageName::declared_by(declaration).map_err(|why| {
        OpError::of(
            OpErrorKind::InvalidInput,
            specforge_common::package::invalid(&why),
        )
    })
}

/// The one rule an extension's declared name is held to beside the builtins this specforge embeds (ADR 0028 D7): it
/// is none of theirs, since a builtin's name always enables the builtin. `add`, `update` and `init` refuse to install
/// such a binary, `publish` and `extension validate` to read it, `extension init` to scaffold it: `extension_conflict`
/// "the extension declares the name of the builtin '<name>'". Each caller adds what to do instead.
pub fn not_a_builtins_name(name: &str) -> Result<(), OpError> {
    match builtin_name(name) {
        None => Ok(()),
        Some(builtin) => Err(OpError::new(
            OpErrorKind::Conflict,
            "extension_conflict",
            format!("the extension declares the name of the builtin '{builtin}'"),
        )),
    }
}

/// A binary to install (`add`, `update`, `init`): its module and what it
/// declares, checked as an installed extension must be — a loadable
/// extension (E028) that does not claim a builtin's name
/// (`extension_conflict`).
#[derive(Debug, Clone, PartialEq)]
pub struct Installable {
    module: Module,
    declaration: ExtensionDeclaration,
}

impl Installable {
    pub fn read(runtime: &dyn WasmRuntime, module: Module) -> Result<Installable, OpError> {
        let declaration = read_declaration(runtime, module.bytes())?.declaration;
        not_a_builtins_name(declaration.name()).map_err(|e| {
            e.with_suggestion(format!(
                "enable the builtin instead: specforge add {}",
                declaration.name()
            ))
        })?;
        Ok(Installable {
            module,
            declaration,
        })
    }

    pub fn declaration(&self) -> &ExtensionDeclaration {
        &self.declaration
    }

    pub fn into_module(self) -> Module {
        self.module
    }
}

/// The target an extension's crate builds its component for: `specforge extension build` builds it, `extension init`
/// scaffolds it, and the reader looks for it (ADR 0172).
pub const WASM_TARGET: &str = "wasm32-wasip2";

/// Cargo's profile directory for `--release`.
const CARGO_RELEASE: &str = "release"; // zero-entity-core: exempt

/// Where an extension's crate at `krate` builds, whatever CARGO_TARGET_DIR says: its own `target/`.
pub fn target_dir(krate: &Path) -> PathBuf {
    krate.join("target")
}

/// The component a path names, read once through a containment (ADR 0104): a `.wasm` file as named, or the
/// component an extension's crate directory builds (`target_dir(dir)/WASM_TARGET/release/<crate>.wasm`, else the only
/// `.wasm` there). The one reader of an extension a request names by path: add, init, publish, extension validate and
/// the CLI build (ADR 0172).
///
/// Every refusal of the disk is E040, one message per cause, naming the path as the caller spelled it (a crate
/// directory's built component joined on), never where a link lands: nothing there (`file_not_found`), no built
/// component or several (`precondition_failed`), a file that cannot be read. A name that leaves the containment is
/// `OUTSIDE_ROOT`; bounded, a file with more than one name is refused too (ADR 0146).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalComponent {
    /// The component read, named as the caller named it.
    pub path: PathBuf,
    pub bytes: Vec<u8>,
}

impl LocalComponent {
    pub fn read(path: &Path, containment: &Containment) -> Result<LocalComponent, OpError> {
        let held = containment.resolve(path)?;
        if !held.real.exists() {
            return Err(OpError::coded(
                OpErrorKind::FileNotFound,
                codes::E040,
                format!("no extension binary at {}", held.spelled.display()),
            )
            .with_suggestion("name a .wasm component, or the extension's crate directory"));
        }
        let named = if held.real.is_dir() {
            built(&held, containment)?
        } else {
            held.spelled.clone()
        };
        match containment.read(&named) {
            Ok((held, bytes)) => Ok(LocalComponent {
                path: held.spelled,
                bytes,
            }),
            Err(ReadFailure::Outside(outside)) => Err(outside.into()),
            Err(ReadFailure::Io { held, error }) => Err(OpError::coded(
                if error.kind() == std::io::ErrorKind::InvalidInput {
                    OpErrorKind::InvalidInput
                } else {
                    OpErrorKind::of_io(&error)
                },
                codes::E040,
                format!("failed to read {}: {error}", held.spelled.display()),
            )),
        }
    }
}

/// The crate directory's built component, named under `held.spelled`: the crate's own, else the only `.wasm` in its
/// release directory. E040 when there is none or several.
fn built(held: &Held, containment: &Containment) -> Result<PathBuf, OpError> {
    let shown = target_dir(&held.spelled)
        .join(WASM_TARGET)
        .join(CARGO_RELEASE);
    // Where the build lands, judged by the containment before it is listed.
    let release = containment.resolve(&shown)?.real;
    let manifest = containment
        .read(&held.spelled.join("Cargo.toml"))
        .ok()
        .and_then(|(_, bytes)| String::from_utf8(bytes).ok());
    if let Some(name) = manifest.as_deref().and_then(crate_name) {
        let file = format!("{}.wasm", name.replace('-', "_"));
        if release.join(&file).is_file() {
            return Ok(shown.join(file));
        }
    }
    let mut names: Vec<String> = std::fs::read_dir(&release)
        .into_iter()
        .flatten()
        .filter_map(|entry| entry.ok()?.file_name().into_string().ok())
        .filter(|name| name.ends_with(".wasm"))
        .collect();
    names.sort();
    match names.len() {
        0 => Err(OpError::coded(
            OpErrorKind::PreconditionFailed,
            codes::E040,
            format!(
                "no built component in {}: build the extension first",
                shown.display()
            ),
        )
        .with_suggestion(format!("cargo build --release --target {WASM_TARGET}"))),
        1 => Ok(shown.join(&names[0])),
        _ => Err(OpError::coded(
            OpErrorKind::PreconditionFailed,
            codes::E040,
            format!(
                "{} holds several components ({}): name the component to use",
                shown.display(),
                names.join(", ")
            ),
        )),
    }
}

/// The `[package]` name in the manifest `text`.
fn crate_name(text: &str) -> Option<String> {
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

/// A `.wasm` file to install, read once: the path it was named by and the
/// binary it holds. `init`'s plan keeps it, so `apply` installs it without
/// reading it again.
#[derive(Debug, Clone, PartialEq)]
pub struct LocalFile {
    pub path: PathBuf,
    pub binary: Installable,
}

impl LocalFile {
    /// Read the extension `path` names ([`LocalComponent::read`]: a `.wasm`
    /// file, or a crate directory's built component) and what it declares,
    /// as [`Installable::read`] checks it. E040 when nothing is at `path`
    /// (the hint is the specifier forms: an add argument that is no builtin's
    /// name, `git+` URL or package reference is read as a path, so a
    /// mistyped name lands here), when a directory holds no built component
    /// or several, or when the file cannot be read. `path` is the component
    /// read, so the lock records `local:<that component>`.
    ///
    /// `path` is read through `containment` (ADR 0104): what is opened is what
    /// landed, so a name that leaves the containment is `OUTSIDE_ROOT`
    /// whichever way it is spelled. Messages and the recorded path name the
    /// joined path.
    pub fn read(
        runtime: &dyn WasmRuntime,
        path: &Path,
        containment: &Containment,
    ) -> Result<LocalFile, OpError> {
        let component =
            LocalComponent::read(path, containment).map_err(|error| match error.kind {
                OpErrorKind::FileNotFound => error.with_suggestion(super::names::SPECIFIER_FORMS),
                _ => error,
            })?;
        Ok(LocalFile {
            binary: Installable::read(runtime, Module::new(component.bytes))?,
            path: component.path,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn a_crate_directory_names_its_built_component() {
        let unbounded = Containment::unbounded();
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("Cargo.toml"),
            "[package]\nname = \"my-ext\"\nversion = \"0.1.0\"\n",
        )
        .unwrap();
        let error = LocalComponent::read(dir.path(), &unbounded).unwrap_err();
        assert!(
            error.message.contains("build the extension first"),
            "{error:?}"
        );
        let release = target_dir(dir.path()).join(WASM_TARGET).join(CARGO_RELEASE);
        std::fs::create_dir_all(&release).unwrap();
        std::fs::write(release.join("other.wasm"), b"x").unwrap();
        std::fs::write(release.join("my_ext.wasm"), b"y").unwrap();
        let read = LocalComponent::read(dir.path(), &unbounded).unwrap();
        assert_eq!(read.path, release.join("my_ext.wasm"));
        assert_eq!(read.bytes, b"y");
        let file = release.join("other.wasm");
        assert_eq!(LocalComponent::read(&file, &unbounded).unwrap().path, file);
        let error = LocalComponent::read(&dir.path().join("missing.wasm"), &unbounded).unwrap_err();
        assert_eq!(error.code, "E040");
        assert_eq!(error.kind, OpErrorKind::FileNotFound);
    }
}

#[cfg(test)]
mod containment_tests {
    use super::*;
    use crate::OUTSIDE_ROOT;
    use crate::testing;

    #[test]
    fn a_local_file_is_read_only_inside_its_containment() {
        let dir = tempfile::tempdir().unwrap();
        let base = std::fs::canonicalize(dir.path()).unwrap();
        std::fs::create_dir_all(base.join("proj")).unwrap();
        std::fs::write(base.join("outside.wasm"), testing::GREET).unwrap();
        std::fs::write(base.join("proj/inside.wasm"), testing::GREET).unwrap();
        let runtime = testing::candidates();
        let bound = Containment::within(&base.join("proj"));

        let error = LocalFile::read(&runtime, &base.join("outside.wasm"), &bound).unwrap_err();
        assert_eq!(error.code, OUTSIDE_ROOT, "{error:?}");
        let error = LocalFile::read(&runtime, Path::new("../outside.wasm"), &bound).unwrap_err();
        assert_eq!(error.code, OUTSIDE_ROOT, "{error:?}");
        let inside = LocalFile::read(&runtime, Path::new("inside.wasm"), &bound).unwrap();
        assert_eq!(inside.path, base.join("proj/inside.wasm"));
        // Unbounded: the same outside file is read.
        let unbounded = Containment::unbounded();
        assert!(LocalFile::read(&runtime, &base.join("outside.wasm"), &unbounded).is_ok());
    }

    #[cfg(unix)]
    #[test]
    fn a_component_with_another_name_is_not_read_through_a_bound() {
        let dir = tempfile::tempdir().unwrap();
        let base = std::fs::canonicalize(dir.path()).unwrap();
        std::fs::write(base.join("outside.wasm"), testing::GREET).unwrap();
        std::fs::create_dir_all(base.join("proj")).unwrap();
        std::fs::hard_link(base.join("outside.wasm"), base.join("proj/linked.wasm")).unwrap();
        let runtime = testing::candidates();

        let bound = Containment::within(&base.join("proj"));
        let error = LocalFile::read(&runtime, Path::new("linked.wasm"), &bound).unwrap_err();
        assert_eq!(error.code, "E040", "{error:?}");
        assert!(
            error.message.contains("it has 2 names"),
            "{}",
            error.message
        );
        // The CLI still reads it (ADR 0146 D5).
        assert!(
            LocalFile::read(
                &runtime,
                &base.join("proj/linked.wasm"),
                &Containment::unbounded()
            )
            .is_ok()
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_crate_directory_whose_build_leaves_the_containment_is_outside() {
        let dir = tempfile::tempdir().unwrap();
        let base = std::fs::canonicalize(dir.path()).unwrap();
        let built = base.join("outside/target/wasm32-wasip2/release");
        std::fs::create_dir_all(&built).unwrap();
        std::fs::write(built.join("greet.wasm"), testing::GREET).unwrap();
        let krate = base.join("proj/ext");
        std::fs::create_dir_all(&krate).unwrap();
        std::fs::write(krate.join("Cargo.toml"), "[package]\nname = \"greet\"\n").unwrap();
        std::os::unix::fs::symlink(base.join("outside/target"), krate.join("target")).unwrap();
        let runtime = testing::candidates();

        // The binary is found through the link when nothing is bounded.
        assert!(LocalComponent::read(&krate, &Containment::unbounded()).is_ok());
        let bound = Containment::within(&base.join("proj"));
        let error = LocalFile::read(&runtime, &krate, &bound).unwrap_err();
        assert_eq!(error.code, OUTSIDE_ROOT, "{error:?}");
        assert!(LocalFile::read(&runtime, &krate, &Containment::unbounded()).is_ok());
    }
}
