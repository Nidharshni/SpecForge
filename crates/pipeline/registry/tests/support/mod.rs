//! Declarations the registry's tests build from, with the SDK's builders,
//! and the one way they reach the registries: `build`, which is
//! `build_registries`. No test names a step.

pub mod written;

use std::path::Path;

use specforge_common::{Diagnostic, SourceSpan, Sym};
use specforge_extension_sdk::PeerDependency;
use specforge_extension_sdk::prelude::*;
use specforge_protocol_types::ExtensionDeclaration;
use specforge_registry::entity::{EntityRecord, RuleInput};
use specforge_registry::rules::NoVerdicts;
use specforge_registry::{Disk, RegistryBuild, build_registries};

/// A builder for the extension `name`, version 1.0.0.
pub fn extension(name: &str) -> ContributionsBuilder {
    ContributionsBuilder::new(ExtensionMeta::new(name, "1.0.0"))
}

/// The declaration of the extension `name` (1.0.0) that `f` fills.
pub fn declare(name: &str, f: impl FnOnce(&mut ContributionsBuilder)) -> ExtensionDeclaration {
    let mut c = extension(name);
    f(&mut c);
    c.declaration()
}

/// The declaration of `name` at `version`, with `peers` and nothing else.
pub fn versioned(name: &str, version: &str, peers: Vec<PeerDependency>) -> ExtensionDeclaration {
    let mut c = ContributionsBuilder::new(ExtensionMeta::new(name, version));
    c.meta.peer_dependencies = peers;
    c.declaration()
}

/// A required peer dependency on `name` in `range`.
pub fn peer(name: &str, range: &str) -> PeerDependency {
    PeerDependency {
        name: name.to_string(),
        version: range.to_string(),
        optional: false,
    }
}

/// An optional peer dependency on `name` in `range`.
pub fn optional_peer(name: &str, range: &str) -> PeerDependency {
    PeerDependency {
        optional: true,
        ..peer(name, range)
    }
}

/// `@specforge/software`-like: the testable `behavior` (fields `contract`, a
/// block, and `invariants`, enforcing `invariant`s over the `enforces`
/// edge) and `invariant` kinds; the edge `enforces` (dashed).
pub fn software() -> ExtensionDeclaration {
    declare("@specforge/software", |c| {
        c.kind("Behavior", |k| {
            k.keyword("behavior")
                .testable(true)
                .singleton(false)
                .supports_verify(true)
                .verify_kinds(&["unit", "contract", "integration"])
                .semantic_token("function")
                .lsp_icon("Method")
                .dot_shape("ellipse");
            k.field("contract", |f| {
                f.field_type(FieldType::Block);
            });
            k.field("invariants", |f| {
                f.field_type(FieldType::ReferenceList)
                    .edge("enforces")
                    .target_kind("invariant");
            });
        });
        c.kind("Invariant", |k| {
            k.keyword("invariant")
                .testable(true)
                .supports_verify(true)
                .dot_shape("diamond");
        });
        c.edge("enforces", |e| {
            e.source_kind("behavior")
                .target_kind("invariant")
                .edge_style("dashed");
        });
    })
}

/// `@specforge/product`-like, a peer of software (`>=1.0.0`): the
/// untestable `feature` kind, composing `behavior`s over the `composes`
/// edge.
pub fn product() -> ExtensionDeclaration {
    let mut c = extension("@specforge/product");
    c.meta
        .peer_dependencies
        .push(peer("@specforge/software", ">=1.0.0"));
    c.kind("Feature", |k| {
        k.keyword("feature").testable(false).dot_shape("box");
        k.field("behaviors", |f| {
            f.field_type(FieldType::ReferenceList)
                .edge("composes")
                .target_kind("behavior");
        });
    });
    c.edge("composes", |e| {
        e.source_kind("feature").target_kind("behavior");
    });
    c.declaration()
}

/// The registry build of `declarations`, in this load order.
pub fn build(declarations: impl IntoIterator<Item = ExtensionDeclaration>) -> RegistryBuild {
    build_registries(declarations.into_iter().collect())
}

/// A file system with nothing on it: every file the checks ask for is missing.
pub struct NoDisk;

impl Disk for NoDisk {
    fn exists(&self, _: &Path) -> bool {
        false
    }

    fn list(&self, _: &Path) -> Option<Vec<String>> {
        None
    }

    fn read(&self, _: &Path) -> std::io::Result<Vec<u8>> {
        Err(std::io::ErrorKind::NotFound.into())
    }
}

/// Every check of `build` over `records` (no edges, spec root `/spec`, no
/// custom verdicts): what `RegistryBuild::check` reports, in its order.
pub fn check(build: &RegistryBuild, records: &[EntityRecord]) -> Vec<Diagnostic> {
    build.check(
        &RuleInput {
            entities: records,
            edges: &[],
            spec_root: Path::new("/spec"),
            disk: &NoDisk,
        },
        &NoVerdicts,
    )
}

/// The diagnostics in `diagnostics` with `code`, in that order.
pub fn coded_in<'a>(diagnostics: &'a [Diagnostic], code: &str) -> Vec<&'a Diagnostic> {
    diagnostics.iter().filter(|d| d.code == code).collect()
}

/// Every diagnostic the build reports, in its order.
pub fn diagnostics(build: &RegistryBuild) -> Vec<&Diagnostic> {
    build.diagnostics().collect()
}

/// The build's diagnostics with `code`, in that order.
pub fn coded<'a>(build: &'a RegistryBuild, code: &str) -> Vec<&'a Diagnostic> {
    diagnostics(build)
        .into_iter()
        .filter(|d| d.code == code)
        .collect()
}

/// The codes of `diagnostics(build)`.
pub fn codes(build: &RegistryBuild) -> Vec<&str> {
    diagnostics(build)
        .into_iter()
        .map(|d| d.code.as_str())
        .collect()
}

/// The build's rules as (code, origin), in order (the host's origin is
/// `""`).
pub fn rule_codes(build: &RegistryBuild) -> Vec<(&str, &str)> {
    build
        .rules()
        .iter()
        .map(|rule| (rule.code(), rule.origin().name()))
        .collect()
}

/// A span at the start of `file`, leaked so entity views can borrow it for
/// the whole test.
pub fn span(file: &str) -> &'static SourceSpan {
    Box::leak(Box::new(SourceSpan {
        file: Sym::new(file),
        start_line: 1,
        start_col: 0,
        end_line: 1,
        end_col: 0,
    }))
}
