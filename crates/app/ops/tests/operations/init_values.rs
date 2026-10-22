//! What `init` writes of its values into the starter, and what it does with a starter that does not parse
//! (ADR 0226).

use specforge_extension_sdk::prelude::*;
use specforge_ops::authoring::init::{self, InitField};
use specforge_parser::FieldValue;
use specforge_wasm::testing::InProcessRuntime;
use tempfile::TempDir;

use crate::writes::files_under;

const BROKEN: &[u8] = b"\0asm in-process @x/broken 1.0.0";

#[specforge_test_macros::test(
    behavior = "scaffold_starter_spec_file",
    verify = "init writes the project's version into its starter as one string the starter reads back, whatever the version holds"
)]
fn init_writes_any_version_as_the_one_string_its_starter_reads_back() {
    let scratch = TempDir::new().unwrap();
    let runtime = specforge_project::testing::component_runtime();
    let sets: [Vec<String>; 3] = [
        vec![],
        vec!["@specforge/software".to_string()],
        vec!["@specforge/product".to_string()],
    ];
    let mut n = 0;
    for extensions in &sets {
        for version in [
            "1.0\" oops {",
            "v1\\nbeta\\q",
            "two\nlines",
            "tab\there",
            "{project}",
        ] {
            n += 1;
            let dir = scratch.path().join(format!("p{n}"));
            init::init(
                &init::Request {
                    dir: &dir,
                    name: Some("demo"),
                    version,
                    extensions,
                    caller: specforge_ops::testing::caller(),
                },
                &*runtime,
            )
            .unwrap();
            let starter = std::fs::read_to_string(dir.join("spec/hello.spec")).unwrap();
            let file = specforge_parser::parse(&starter, "hello.spec");
            assert!(
                file.errors.is_empty(),
                "{extensions:?} {version:?}: {:?}\n{starter}",
                file.errors
            );
            let spec = file
                .entities
                .iter()
                .find(|e| e.kind.raw.as_str() == "spec")
                .expect("a spec block");
            assert_eq!(
                spec.fields.get("version"),
                Some(&FieldValue::String(version.to_string())),
                "{extensions:?} {version:?}\n{starter}"
            );
            let config: serde_json::Value =
                serde_json::from_slice(&std::fs::read(dir.join("specforge.json")).unwrap())
                    .unwrap();
            assert_eq!(config["version"], version);
        }
    }
}

#[specforge_test_macros::test(
    behavior = "scaffold_starter_spec_file",
    verify = "init refuses a starter that does not parse, naming the extension whose template it is, before anything is written"
)]
#[ignore = "flipped by T4"]
fn init_refuses_a_starter_template_that_does_not_parse_writing_nothing() {
    let scratch = TempDir::new().unwrap();
    let wasm = scratch.path().join("broken.wasm");
    std::fs::write(&wasm, BROKEN).unwrap();
    let runtime = InProcessRuntime::new().binary(BROKEN, || {
        let mut c = ContributionsBuilder::new(ExtensionMeta::new("@x/broken", "1.0.0"));
        c.kind("widget", |_| {});
        c.starter_template("spec \"{project}\" {\n  version \"{version}\"\n\nwidget w \"W\" {\n");
        c
    });
    let dir = scratch.path().join("demo");
    let refusal = init::init(
        &init::Request {
            dir: &dir,
            name: Some("demo"),
            version: init::DEFAULT_VERSION,
            extensions: &[wasm.display().to_string()],
            caller: specforge_ops::testing::caller(),
        },
        &runtime,
    )
    .unwrap_err();
    assert_eq!(refusal.error.code, "starter_unparsed");
    assert_eq!(refusal.subject, Some(InitField::Extensions));
    assert!(
        refusal.error.message.contains("@x/broken"),
        "{}",
        refusal.error.message
    );
    assert!(files_under(&dir).is_empty(), "nothing was written");
}
