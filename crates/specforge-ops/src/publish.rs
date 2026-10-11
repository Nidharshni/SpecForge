//! `specforge publish`: an extension binary and the declaration read from it
//! (ADR 0012), uploaded to the registry that serves its name (ADR 0045).
//! Every check that needs no registry runs before the registry is asked.

use std::path::{Path, PathBuf};

use specforge_common::{Code, Diagnostic, Severity, codes};
use specforge_package::{PackageName, Version};
use specforge_protocol_types::ExtensionDeclaration;
use specforge_wasm::WasmRuntime;

use crate::extension::Candidate;
use crate::registry::{Published, Registry, Upload};
use crate::{OpError, OpErrorKind};

/// The diagnostic for an extension that can't be found or read.
const UNREADABLE: Code = codes::E040;

/// What a publish found and how it ended. Its warnings are reported whatever
/// the result.
#[derive(Debug)]
pub struct PublishReport {
    /// The declaration's load warnings (W153, W138), then what its registry
    /// build alone reports (W021, ...), without E027. Empty when the binary
    /// could not be read or loaded.
    pub warnings: Vec<Diagnostic>,
    pub result: Result<PublishOutcome, OpError>,
}

/// A package published.
#[derive(Debug, Clone, PartialEq)]
pub struct PublishOutcome {
    pub name: PackageName,
    pub version: Version,
    /// The binary's size in bytes.
    pub size_bytes: usize,
    pub published: Published,
}

/// Publish the extension at `extension` (a `.wasm` component, or the crate
/// directory that builds one) to `registry`.
///
/// Refused in this order, each before anything after it is read or asked:
/// 1. the binary: E040 (none at the path, none built, unreadable), E028 (not
///    a loadable extension, read through `runtime`);
/// 2. its declaration's errors, as the registry build of it alone reports
///    them (E030, a refused tool schema, ...), naming every error;
/// 3. its name and version: E072 unless a scoped package name and a full
///    SemVer version (ADR 0036);
/// 4. the registry: [`Registry::publish`]'s refusals.
pub fn publish(
    extension: &Path,
    registry: &dyn Registry,
    runtime: &dyn WasmRuntime,
) -> PublishReport {
    let mut warnings = Vec::new();
    let result = run(extension, registry, runtime, &mut warnings);
    PublishReport { warnings, result }
}

fn run(
    extension: &Path,
    registry: &dyn Registry,
    runtime: &dyn WasmRuntime,
    warnings: &mut Vec<Diagnostic>,
) -> Result<PublishOutcome, OpError> {
    let wasm = read(&binary_at(extension)?)?;
    let (declaration, diagnostics) = declare(runtime, &wasm)?;
    *warnings = diagnostics
        .iter()
        .filter(|d| d.severity != Severity::Error)
        .cloned()
        .collect();
    refuse_errors(&declaration, &diagnostics)?;
    let (name, version) = identity(&declaration)?;
    let published = registry.publish(&Upload {
        name: &name,
        version: &version,
        wasm: &wasm,
        declaration: &declaration,
    })?;
    Ok(PublishOutcome {
        name,
        version,
        size_bytes: wasm.len(),
        published,
    })
}

fn read(binary: &Path) -> Result<Vec<u8>, OpError> {
    std::fs::read(binary).map_err(|error| {
        OpError::coded(
            OpErrorKind::of_io(&error),
            UNREADABLE,
            format!("failed to read {}: {error}", binary.display()),
        )
    })
}

/// What is uploaded is a registry package: a scoped name and a full version
/// (E072).
fn identity(declaration: &ExtensionDeclaration) -> Result<(PackageName, Version), OpError> {
    let name = match PackageName::declared_by(declaration) {
        Ok(name) if name.scope().is_some() => name,
        Ok(name) => {
            return Err(specforge_common::package::invalid(&format_args!(
                "'{name}' is not a registry package name: registry packages are named @scope/name"
            ))
            .into());
        }
        Err(why) => return Err(specforge_common::package::invalid(&why).into()),
    };
    let version = Version::parse(declaration.version()).map_err(|why| {
        OpError::from(specforge_common::package::invalid(&format_args!(
            "'{}' is not a SemVer version: {why}",
            declaration.version()
        )))
    })?;
    Ok((name, version))
}

/// The binary `path` names: a `.wasm` component as given, or, for a
/// directory, the component its crate builds
/// (`target/wasm32-wasip2/release/<crate>.wasm`, the only `.wasm` there
/// when the crate's name can't be read).
pub fn binary_at(path: &Path) -> Result<PathBuf, OpError> {
    if !path.is_dir() {
        if path.is_file() {
            return Ok(path.to_path_buf());
        }
        return Err(OpError::coded(
            OpErrorKind::PreconditionFailed,
            UNREADABLE,
            format!("no extension binary at {}", path.display()),
        )
        .with_suggestion("name a .wasm component, or the extension's crate directory"));
    }
    let release = path.join("target/wasm32-wasip2/release");
    let not_built = || {
        OpError::coded(
            OpErrorKind::PreconditionFailed,
            UNREADABLE,
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
            UNREADABLE,
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

/// Load `wasm` and read its declaration, with what the registry build
/// alone reports of it (its load warnings, W153 and W138, first). A binary
/// that isn't a loadable extension is E028. Missing peers (E027) are left
/// out: they are installed beside the extension, not with it.
pub fn declare(
    runtime: &dyn WasmRuntime,
    wasm: &[u8],
) -> Result<(ExtensionDeclaration, Vec<Diagnostic>), OpError> {
    let (declaration, warnings) = Candidate::read(runtime, wasm)?.into_parts();
    let diagnostics = diagnostics_of(&declaration, warnings);
    Ok((declaration, diagnostics))
}

/// `warnings`, then what the registry build of `declaration` alone reports,
/// in its order, without missing peers (E027).
fn diagnostics_of(
    declaration: &ExtensionDeclaration,
    warnings: Vec<Diagnostic>,
) -> Vec<Diagnostic> {
    let build = specforge_registry::build_registries(vec![declaration.clone()]);
    warnings
        .into_iter()
        .chain(build.diagnostics().filter(|d| !d.is(codes::E027)).cloned())
        .collect()
}

fn refuse_errors(
    declaration: &ExtensionDeclaration,
    diagnostics: &[Diagnostic],
) -> Result<(), OpError> {
    let errors: Vec<&Diagnostic> = diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .collect();
    let Some(first) = errors.first() else {
        return Ok(());
    };
    let detail: Vec<String> = errors
        .iter()
        .map(|d| format!("{}: {}", d.code, d.message))
        .collect();
    Err(OpError::new(
        OpErrorKind::of_diagnostic(&first.code),
        first.code.clone(),
        format!(
            "{}@{} can't be published: its declaration has errors ({})",
            declaration.name(),
            declaration.version(),
            detail.join("; ")
        ),
    )
    .with_suggestion("fix the declaration in the extension's source and rebuild it"))
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
