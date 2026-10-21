//! What `init` checks and lays out of its starter: the registry build of the extensions it enables
//! decides the starter's spec ID and the forms of its values (ADR 0189).

use std::path::Path;

use specforge_extension_sdk::prelude::*;
use specforge_ops::authoring::init::{self, InitField};
use specforge_ops::{OpErrorKind, Refusal};
use specforge_wasm::testing::InProcessRuntime;
use tempfile::TempDir;

use crate::writes::files_under;

const TPL: &[u8] = b"\0asm in-process @x/tpl 1.0.0";

/// `@x/tpl`: a `widget` kind whose `category` is an identifier, and a starter that writes one.
fn runtime() -> InProcessRuntime {
    InProcessRuntime::new().binary(TPL, || {
        let mut c = ContributionsBuilder::new(ExtensionMeta::new("@x/tpl", "1.0.0"));
        c.kind("widget", |k| {
            k.field("category", |f| {
                f.field_type(FieldType::Identifier);
            });
        });
        c.starter_template(
            "spec \"{project}\" {\n  version \"{version}\"\n}\n\nwidget sample \"Sample\" {\n  category \"auth\"\n}\n",
        );
        c
    })
}

fn init_with(
    dir: &Path,
    name: Option<&str>,
    extensions: &[String],
    runtime: &dyn specforge_wasm::WasmRuntime,
) -> Result<init::Outcome, Refusal<InitField>> {
    init::init(
        &init::Request {
            dir,
            name,
            version: init::DEFAULT_VERSION,
            extensions,
            caller: specforge_ops::testing::caller(),
        },
        runtime,
    )
}

fn refused_about_the_name(result: Result<init::Outcome, Refusal<InitField>>, dir: &Path) {
    let refusal = result.unwrap_err();
    assert_eq!(refusal.error.code, init::INVALID_NAME);
    assert_eq!(refusal.error.kind, OpErrorKind::InvalidInput);
    assert_eq!(refusal.subject, Some(InitField::Name));
    assert!(
        refusal.error.message.contains("reserved"),
        "{}",
        refusal.error.message
    );
    assert!(files_under(dir).is_empty(), "nothing was written");
}

#[specforge_test_macros::test(
    behavior = "non_interactive_init",
    verify = "init refuses a project name whose starter spec ID a kind of an extension it enables reserves, before anything is written"
)]
fn init_refuses_a_name_a_kind_it_enables_reserves() {
    let scratch = TempDir::new().unwrap();
    let runtime = specforge_project::testing::component_runtime();
    let software = vec!["@specforge/software".to_string()];
    let dir = scratch.path().join("a");
    refused_about_the_name(
        init_with(&dir, Some("behavior"), &software, &*runtime),
        &dir,
    );
    let dir = scratch.path().join("release");
    let product = vec!["@specforge/product".to_string()];
    refused_about_the_name(init_with(&dir, None, &product, &*runtime), &dir);
}

#[specforge_test_macros::test(
    behavior = "non_interactive_init",
    verify = "init refuses a structural keyword as the starter's spec ID, whatever it enables"
)]
fn init_refuses_a_structural_keyword_as_the_starter_id_whatever_it_enables() {
    let scratch = TempDir::new().unwrap();
    let runtime = specforge_project::testing::component_runtime();
    for name in ["spec", "use", "defaults"] {
        let dir = scratch.path().join(name);
        refused_about_the_name(init_with(&dir, Some(name), &[], &*runtime), &dir);
    }
}

#[specforge_test_macros::test(
    behavior = "non_interactive_init",
    verify = "init refuses a project name whose starter spec ID a kind of an extension it enables reserves, before anything is written"
)]
fn init_refuses_a_name_a_local_extensions_kind_reserves() {
    let scratch = TempDir::new().unwrap();
    let wasm = scratch.path().join("tpl.wasm");
    std::fs::write(&wasm, TPL).unwrap();
    let dir = scratch.path().join("demo");
    refused_about_the_name(
        init_with(
            &dir,
            Some("widget"),
            &[wasm.display().to_string()],
            &runtime(),
        ),
        &dir,
    );
}

#[specforge_test_macros::test(
    behavior = "scaffold_starter_spec_file",
    verify = "init writes the starter's values in the forms of the fields the extensions it enables declare"
)]
fn init_writes_a_templates_values_in_their_fields_forms() {
    let scratch = TempDir::new().unwrap();
    let wasm = scratch.path().join("tpl.wasm");
    std::fs::write(&wasm, TPL).unwrap();
    let dir = scratch.path().join("demo");

    init_with(
        &dir,
        Some("demo"),
        &[wasm.display().to_string()],
        &runtime(),
    )
    .unwrap();

    let starter = std::fs::read_to_string(dir.join("spec/hello.spec")).unwrap();
    assert!(starter.contains("category auth"), "{starter}");
    assert!(!starter.contains("category \"auth\""), "{starter}");
}
