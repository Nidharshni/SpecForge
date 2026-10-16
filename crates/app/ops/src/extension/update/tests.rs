use super::super::fixtures::{declaration_of, enabled_project, entry, greet, installed};
use super::*;
use crate::registry::testing::{Asked, MemoryRegistry, Published, declaration};
use specforge_installed::{LockSource, hex_sha256, lock_path};
use specforge_test_macros::test as specforge_test;

/// What the tests serve in process: `@sdk/greet` and `@test/probe`.
fn runtime() -> specforge_wasm::testing::InProcessRuntime {
    crate::testing::candidates()
}

fn name(text: &str) -> PackageName {
    PackageName::parse(text).unwrap()
}

/// `@sdk/greet` 0.1.0, a real extension binary, published with the declaration it declares.
fn greet_published() -> Published {
    Published::new(greet(), declaration_of(&greet()))
}

/// `name@version` published with bytes that are no extension.
fn old(name: &str, version: &str) -> Published {
    Published::new(b"\0asm old".to_vec(), declaration(name, version, &[]))
}

static UNBOUNDED: crate::Containment = crate::Containment::unbounded();

fn request(root: &specforge_common::ProjectRoot, reach: Reach) -> UpdateRequest<'_> {
    UpdateRequest {
        root,
        name: None,
        reach,
        allow_unsigned: true,
        asking: Asking::Nobody,
        containment: &UNBOUNDED,
    }
}

fn status_of<'a>(outcome: &'a UpdateOutcome, name: &str) -> &'a UpdateStatus {
    &outcome
        .extensions
        .iter()
        .find(|e| e.name == name)
        .unwrap()
        .status
}

use crate::testing::declaring;
use specforge_wasm::testing::InProcessRuntime;

/// Whether `registry` was asked to fetch anything.
fn fetched(registry: &MemoryRegistry) -> bool {
    registry
        .asked()
        .iter()
        .any(|asked| matches!(asked, Asked::Fetch(..)))
}

/// `wasm` published with the declaration it declares in `runtime`.
fn served(runtime: &InProcessRuntime, wasm: &[u8]) -> Published {
    Published::new(
        wasm.to_vec(),
        crate::extension::declare(runtime, wasm).unwrap().0,
    )
}

mod apply;
mod document;
mod refusals;
mod versions;
