//! A bounded migration compiles the project through the caller's bound (ADR 0166): a source the bound
//! refuses is read by no step of the run, the compiles before and after included.

use std::sync::Arc;

use specforge_extension_sdk::prelude::*;
use specforge_ops::Containment;
use specforge_ops::authoring::migrate::{Failure, Request};
use specforge_test_macros::test as specforge_test;
use specforge_wasm::testing::InProcessRuntime;

use crate::migrate::request;

/// The project's own source and the outside file, both at the old format; each declares one `item`.
const OWN: &str = "// specforge-format: 0.9\nitem alpha_item \"Alpha\" {\n}\n";
const PROBE: &str = "// specforge-format: 0.9\nitem secretword_id \"Secret\" {\n}\n";

/// `@acme/x` declaring `item` and a custom rule on it: the runtime records the entity each call is given.
fn echoing() -> InProcessRuntime {
    InProcessRuntime::new().with(|| {
        let mut c = ContributionsBuilder::new(ExtensionMeta::new("@acme/x", "1.0.0"));
        c.kind("item", |k| {
            k.description("probe").open_fields(true);
        });
        c.rule("P100", |r| {
            r.check(CheckKind::Custom)
                .severity(ValidationSeverity::Info)
                .target_kind("item")
                .wasm_function("validate__echo")
                .message_template("{id}")
                .validate(|_| ValidatorVerdict::Pass);
        });
        c
    })
}

#[specforge_test(
    invariant = "mcp_served_project_consistency",
    verify = "a source of the served project is read only from inside the project"
)]
fn a_bounded_migration_compiles_no_source_the_bound_refuses() {
    let scene = tempfile::TempDir::new().unwrap();
    let (dir, outside) = (scene.path().join("proj"), scene.path().join("outside"));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::create_dir_all(&outside).unwrap();
    std::fs::write(
        dir.join("specforge.json"),
        r#"{"name": "p", "version": "0.1.0", "extensions": ["@acme/x"]}"#,
    )
    .unwrap();
    specforge_installed::testing::install(&dir, &["@acme/x"]);
    std::fs::write(dir.join("a.spec"), OWN).unwrap();
    std::fs::write(outside.join("probe.spec"), PROBE).unwrap();
    std::fs::hard_link(outside.join("probe.spec"), dir.join("leak.spec")).unwrap();
    let root = specforge_common::ProjectRoot::of(&dir).unwrap();
    let containment = Containment::within(&dir);
    let runtime = Arc::new(echoing());

    let outcome = specforge_ops::authoring::migrate::run(
        &Request {
            caller: specforge_ops::testing::bounded(&containment),
            ..request(&root)
        },
        Some(runtime.clone()),
    )
    .unwrap();

    // The refused source fails its file and the run writes and compiles nothing (ADR 0171) ...
    let Some(Failure::FilesFailed { files }) = outcome.failure() else {
        panic!("{outcome:?}");
    };
    assert!(
        files
            .iter()
            .any(|(path, error)| path.ends_with("leak.spec") && error.contains("2 names")),
        "{files:?}"
    );
    // ... and no compile of the run read it: the rule saw the project's own entity, never the outside one.
    let inputs: Vec<String> = runtime
        .calls()
        .iter()
        .map(|call| call.input.to_string())
        .collect();
    assert!(runtime.calls().is_empty(), "no compile ran: {inputs:?}");
    assert!(
        !inputs.iter().any(|input| input.contains("secretword")),
        "{inputs:?}"
    );
}
