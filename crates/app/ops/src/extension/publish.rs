//! `specforge publish`: an extension binary and the declaration read from it
//! (ADR 0012), uploaded to the registry that serves its name (ADR 0045).
//! Every check that needs no registry runs before the registry is asked.

use std::path::{Path, PathBuf};

use serde::Serialize;
use specforge_common::shape::Shape;
use specforge_common::{Code, Diagnostic, DiagnosticList, Severity, codes};
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

impl PublishOutcome {
    /// The document `specforge publish` prints.
    pub fn document(&self) -> PublishDocument {
        PublishDocument {
            action: PublishAction::Published,
            extension: self.name.to_string(),
            version: self.version.to_string(),
            registry: self.published.registry.clone(),
            url: self.published.url.clone(),
            size_bytes: self.size_bytes,
            key_id: self.published.key_id.clone(),
            key_created: self.published.key_created,
        }
    }
}

/// What `specforge publish --format json` prints (CLI-only, ADR 0052 D5).
#[derive(Debug, Clone, PartialEq, Serialize, Shape)]
pub struct PublishDocument {
    pub action: PublishAction,
    pub extension: String,
    pub version: String,
    pub registry: String,
    pub url: String,
    pub size_bytes: usize,
    pub key_id: String,
    pub key_created: bool,
}

/// What a publish did.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Shape)]
#[serde(rename_all = "snake_case")]
pub enum PublishAction {
    Published,
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
    let Validated {
        wasm,
        declaration,
        diagnostics,
        ..
    } = validate(extension, runtime)?;
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

/// The extension at `extension` (a `.wasm` component, or the crate directory that builds one), read and declared as
/// publish reads it. Refused, in this order: E040 (no binary at the path, none built, unreadable), E028 (not a
/// loadable extension, read through `runtime`). Its diagnostics are [`declare`]'s.
pub fn validate(extension: &Path, runtime: &dyn WasmRuntime) -> Result<Validated, OpError> {
    let binary = crate::extension::binary_at(extension)?;
    let wasm = read(&binary)?;
    let (declaration, diagnostics) = declare(runtime, &wasm)?;
    Ok(Validated {
        binary,
        wasm,
        declaration,
        diagnostics,
    })
}

/// An extension binary read and declared: what `specforge extension validate` reports and publish's first two steps.
#[derive(Debug)]
pub struct Validated {
    pub binary: PathBuf,
    pub wasm: Vec<u8>,
    pub declaration: ExtensionDeclaration,
    /// [`declare`]'s: the load warnings, then what the registry build of the declaration alone reports.
    pub diagnostics: Vec<Diagnostic>,
}

impl Validated {
    /// No error-level diagnostic: `extension validate` exits 0, and publish's step 2 lets it through.
    pub fn valid(&self) -> bool {
        !self.diagnostics.iter().any(is_error)
    }

    /// What `specforge extension validate --format json` prints.
    pub fn document(&self) -> ValidateDocument {
        ValidateDocument {
            valid: self.valid(),
            extension: self.declaration.name().to_string(),
            version: self.declaration.version().to_string(),
            short: self.declaration.short().to_string(),
            component: self.binary.display().to_string(),
            diagnostics: DiagnosticList(self.diagnostics.clone()),
            declaration: self.declaration.clone(),
        }
    }
}

/// `specforge extension validate --format json` (CLI-only, ADR 0052 D5).
#[derive(Debug, Serialize, Shape)]
pub struct ValidateDocument {
    pub valid: bool,
    pub extension: String,
    pub version: String,
    pub short: String,
    /// The component read, as given or as the crate built it.
    pub component: String,
    pub diagnostics: DiagnosticList,
    /// `ExtensionDeclaration` is a leaf type without a `Shape`.
    #[shape(as = serde_json::Value)]
    pub declaration: ExtensionDeclaration,
}

fn is_error(d: &Diagnostic) -> bool {
    d.severity == Severity::Error
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
            return Err(OpError::of(
                OpErrorKind::InvalidInput,
                specforge_common::package::invalid(&format_args!(
                    "'{name}' is not a registry package name: registry packages are named @scope/name"
                )),
            ));
        }
        Err(why) => {
            return Err(OpError::of(
                OpErrorKind::InvalidInput,
                specforge_common::package::invalid(&why),
            ));
        }
    };
    let version = Version::parse(declaration.version()).map_err(|why| {
        OpError::of(
            OpErrorKind::InvalidInput,
            specforge_common::package::invalid(&format_args!(
                "'{}' is not a SemVer version: {why}",
                declaration.version()
            )),
        )
    })?;
    Ok((name, version))
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
    let errors: Vec<&Diagnostic> = diagnostics.iter().filter(|d| is_error(d)).collect();
    let Some(first) = errors.first() else {
        return Ok(());
    };
    let detail: Vec<String> = errors
        .iter()
        .map(|d| format!("{}: {}", d.code, d.message))
        .collect();
    Err(OpError::new(
        OpErrorKind::InvalidInput,
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
