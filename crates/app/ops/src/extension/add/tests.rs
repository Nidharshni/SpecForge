use super::super::fixtures::{declaration_of, enabled_project, entry, greet, installed, project};
use super::*;
use crate::OpErrorKind;
use crate::extension::names::{Source, parse};
use crate::registry::testing::{MemoryRegistry, Published, declaration};
use specforge_common::codes;
use specforge_installed::LockFileEntry;
use specforge_package::{PackageName, PackageRef};
use specforge_protocol_types::ExtensionDeclaration;
use specforge_registry::PeerDependency;
use specforge_test_macros::test as specforge_test;
use std::path::{Path, PathBuf};

use crate::testing::{self, GREET, declaring, serving_builtin};
use specforge_wasm::testing::InProcessRuntime;

/// What the tests serve in process: `@sdk/greet` and `@test/probe`.
fn runtime() -> InProcessRuntime {
    testing::candidates()
}

/// A project directory whose `specforge.json` holds `config` verbatim.
fn project_with(config: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("specforge.json"), config).unwrap();
    dir
}

const EMPTY_PROJECT: &str = r#"{"name": "p", "version": "0.1.0", "extensions": []}"#;

fn local_request<'a>(root: &'a specforge_common::ProjectRoot, file: &Path) -> AddRequest<'a> {
    AddRequest {
        root,
        extensions: vec![Source::Local(file.to_path_buf()).into()],
        base: None,
        allow_unsigned: false,
        dry_run: false,
        caller: crate::testing::caller(),
    }
}

/// A project locking `lock`, with a `specforge.json` enabling every locked extension.
fn add_project(lock: Vec<LockFileEntry>) -> tempfile::TempDir {
    enabled_project(lock)
}

fn add_greet(root: &Path, registry: &MemoryRegistry) -> Result<AddOutcome, Refusal<AddField>> {
    add(
        &AddRequest {
            root: &specforge_common::ProjectRoot::of(root).unwrap(),
            extensions: vec![
                Source::Registry(PackageRef::parse("@sdk/greet@0.1.0").unwrap()).into(),
            ],
            base: None,
            allow_unsigned: true,
            dry_run: false,
            caller: crate::testing::caller(),
        },
        registry,
        &runtime(),
    )
    .map(|added| added.added[0].outcome.clone())
}

// --- several specifiers: one change ---

/// The outcomes of `added`, in specifier order.
fn outcomes(added: &Added) -> Vec<AddOutcome> {
    added.added.iter().map(|a| a.outcome.clone()).collect()
}

/// An add of `extensions` into `root`.
fn several(
    root: &specforge_common::ProjectRoot,
    extensions: Vec<String>,
    dry_run: bool,
) -> AddRequest<'_> {
    AddRequest {
        root,
        extensions,
        base: None,
        allow_unsigned: true,
        dry_run,
        caller: crate::testing::caller(),
    }
}

/// `runtime()`, also serving the builtin `@specforge/product`.
fn runtime_with_product() -> InProcessRuntime {
    serving_builtin(
        runtime(),
        "@specforge/product",
        declaring("@specforge/product", "1.0.0", &[]),
    )
}

/// `greet.wasm` written in a directory of its own.
fn greet_file() -> (tempfile::TempDir, PathBuf) {
    let files = tempfile::tempdir().unwrap();
    let wasm = files.path().join("greet.wasm");
    std::fs::write(&wasm, GREET).unwrap();
    (files, wasm)
}

mod builtin;
mod document;
mod dry_run;
mod entry_forms;
mod local;
mod registry;
mod several;
mod specifier;
