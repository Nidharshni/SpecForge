//! The checks over what a project writes, reached as production reaches them: each `(path, text)`
//! parsed, the graph built with the registry build's own graph inputs
//! (`RegistryBuild::graph_of`, as `Environment::graph_of` does), the entity snapshot taken
//! with that build at the spec root `/spec`, and `RegistryBuild::check` run over it (ADR 0031 D1,
//! ADR 0115).

use std::collections::HashSet;
use std::path::Path;

use specforge_common::Diagnostic;
use specforge_extension_sdk::prelude::*;
use specforge_graph::GraphBuild;
use specforge_protocol_types::ExtensionDeclaration;
use specforge_registry::rules::NoVerdicts;
use specforge_registry::{Disk, EntitySnapshot, RegistryBuild};

use super::{NoDisk, declare};

/// `ticket`: an enum, a bool, an integer, a string list, a string and a reference to another
/// `ticket`.
pub fn tickets() -> ExtensionDeclaration {
    declare("@test/tickets", |c| {
        c.kind("Ticket", |k| {
            k.keyword("ticket");
            k.field("priority", |f| {
                f.vocabulary(&Vocabulary::new(&["low", "medium", "high"]));
            });
            k.field("urgent", |f| {
                f.field_type(FieldType::Bool);
            });
            k.field("points", |f| {
                f.field_type(FieldType::Integer);
            });
            k.field("labels", |f| {
                f.field_type(FieldType::StringList);
            });
            k.field("summary", |f| {
                f.field_type(FieldType::String);
            });
            k.field("blocker", |f| {
                f.field_type(FieldType::Reference).target_kind("ticket");
            });
            k.field("details", |f| {
                f.field_type(FieldType::Block);
            });
            k.field("watchers", |f| {
                f.field_type(FieldType::ReferenceList).target_kind("ticket");
            });
        });
    })
}

/// The snapshot of `files` read with `build`. Panics on a parse error: a test's text is valid
/// `.spec`.
pub fn snapshot_of(build: &RegistryBuild, files: &[(&str, &str)]) -> EntitySnapshot {
    EntitySnapshot::of(&graph_of(build, files), build, Path::new("/spec"))
}

/// The graph build of `files` with `build`'s own graph inputs.
fn graph_of(build: &RegistryBuild, files: &[(&str, &str)]) -> GraphBuild {
    let parsed = files.iter().map(|(path, text)| {
        let file = specforge_parser::parse(text, path);
        assert!(file.errors.is_empty(), "{path}: {:?}", file.errors);
        file
    });
    build.graph_of(parsed, HashSet::new())
}

/// What the graph build reports on `files` (E003, I004, E022, ...), the linker's own: the
/// registry's checks are not included.
pub fn linked(build: &RegistryBuild, files: &[(&str, &str)]) -> Vec<Diagnostic> {
    graph_of(build, files).diagnostics().to_vec()
}

/// What the checks report on `files`, in `RegistryBuild::check`'s order, with nothing on disk
/// (`NoDisk`) and no custom verdicts. The graph build's own diagnostics (E003, I004, I005, W143,
/// ...) are not included.
pub fn checked(build: &RegistryBuild, files: &[(&str, &str)]) -> Vec<Diagnostic> {
    checked_on(build, files, &NoDisk)
}

/// `checked`, reading files through `disk`.
pub fn checked_on(
    build: &RegistryBuild,
    files: &[(&str, &str)],
    disk: &dyn Disk,
) -> Vec<Diagnostic> {
    let snapshot = snapshot_of(build, files);
    build.check(&snapshot.rule_input(disk), &NoVerdicts)
}
