//! `specforge publish`: an extension binary and the declaration read from it
//! (ADR 0012), uploaded to the registry that serves its name (ADR 0045).
//! Every check that needs no registry runs before the registry is asked.

use std::path::{Path, PathBuf};

use serde::Serialize;
use specforge_common::shape::Shape;
use specforge_common::{Diagnostic, DiagnosticList, Severity, codes};
use specforge_package::{DeclaredPackage, PackageName, Version};
use specforge_protocol_types::ExtensionDeclaration;
use specforge_wasm::WasmRuntime;
use specforge_wasm::protocol::Loaded;

use super::candidate::LocalComponent;
use crate::registry::{Published, Registry};
use crate::{Containment, OpError, OpErrorKind};

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
/// 1. the binary: E040 (nothing at the path, none built, several, unreadable: the
///    candidate's reader), E028 (not
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
    let package = DeclaredPackage::of(&declaration).map_err(|why| {
        OpError::of(
            OpErrorKind::InvalidInput,
            specforge_common::package::invalid(&why),
        )
    })?;
    let published = registry.publish(&package, &wasm)?;
    Ok(PublishOutcome {
        name: package.name().clone(),
        version: package.version().clone(),
        size_bytes: wasm.len(),
        published,
    })
}

/// The extension at `extension` (a `.wasm` component, or the crate directory that builds one), read and declared as
/// publish reads it. Refused, in this order: E040 (nothing at the path, none built, several, unreadable: the candidate's reader), E028 (not a
/// loadable extension, read through `runtime`). Its diagnostics are [`declare`]'s.
pub fn validate(extension: &Path, runtime: &dyn WasmRuntime) -> Result<Validated, OpError> {
    // publish and extension validate are the CLI's only (ADR 0045 D6): unbounded, as the CLI is (ADR 0092 D6).
    let LocalComponent { path, bytes } =
        LocalComponent::read(extension, &Containment::unbounded())?;
    let (declaration, diagnostics) = declare(runtime, &bytes)?;
    Ok(Validated {
        binary: path,
        wasm: bytes,
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

/// Load `wasm` and read its declaration, with what the registry build
/// alone reports of it (its load warnings, W153 and W138, first). A binary
/// that isn't a loadable extension is E028. Missing peers (E027) are left
/// out: they are installed beside the extension, not with it.
pub fn declare(
    runtime: &dyn WasmRuntime,
    wasm: &[u8],
) -> Result<(ExtensionDeclaration, Vec<Diagnostic>), OpError> {
    let Loaded {
        declaration,
        warnings,
    } = super::candidate::read_declaration(runtime, wasm)?;
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
