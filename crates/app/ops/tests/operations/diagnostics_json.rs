//! The diagnostics JSON every surface prints, for one diagnostic of each way
//! the host builds one today: a struct literal (W112, the registry build;
//! R001, the registry client; I202, inference, whose fields are written in
//! another order) and a level constructor (E003, the graph; A010, the
//! `contracts` pass, whose level its pass sets). Each is built by the
//! production function that reports it, so the pin holds while plan 11
//! moves those functions to typed codes.

use serde_json::{Value, json};
use specforge_common::Diagnostic;
use specforge_extension_sdk::prelude::*;
use specforge_ops::analysis::analyze::{AnalyzeOptions, analyze};
use specforge_registry::build_registries;
use specforge_registry_client::registry_client::RegistryError;

use crate::view_support::Project;

/// W112: the registry build refuses a rule that can never fire.
fn unexecutable_rule() -> Diagnostic {
    let mut c = ContributionsBuilder::new(ExtensionMeta::new("@acme/rules", "1.0.0"));
    c.rule("W950", |r| {
        r.message_template("m")
            .check(CheckKind::MissingRequiredField);
    });
    let build = build_registries(vec![c.declaration()]);
    only(build.diagnostics().cloned().collect(), "W112")
}

/// E003: the graph build reports a reference to no entity.
fn unresolved_reference() -> Diagnostic {
    let source = "behavior a \"A\" { contract \"c\" }\nfeature f \"F\" { behaviors [a, ghost] }\n";
    let (_, diagnostics) =
        specforge_graph::build_graph(&[specforge_parser::parse(source, "main.spec")]);
    only(diagnostics, "E003")
}

/// A010: the `contracts` pass reports an entity of a contract-bearing kind
/// that declares no contract reference.
fn entity_without_contracts() -> Diagnostic {
    let mut c = ContributionsBuilder::new(ExtensionMeta::new("@acme/contracts", "1.0.0"));
    c.kind("Widget", |k| {
        k.keyword("widget");
        k.field("requires", |f| {
            f.field_type(FieldType::ReferenceList)
                .edge("requires")
                .target_kind("rule");
        });
    });
    c.kind("Rule", |k| {
        k.keyword("rule").contract_target();
    });
    let registries = build_registries(vec![c.declaration()]);
    let (graph, _) = specforge_graph::build_graph(&[specforge_parser::parse(
        "widget w \"W\" {\n}\n",
        "main.spec",
    )]);
    let project = Project::of_graph(graph, registries);
    let outcome = analyze(
        &project.view(),
        &AnalyzeOptions {
            pass: "contracts".to_string(),
            ..Default::default()
        },
    )
    .unwrap();
    let findings = outcome
        .passes
        .into_iter()
        .flat_map(|p| p.findings)
        .collect();
    only(findings, "A010")
}

/// R001: the registry client's refused login.
fn registry_unauthorized() -> Diagnostic {
    RegistryError::Unauthorized {
        guidance: "token expired".to_string(),
    }
    .to_diagnostic()
}

/// I202: inference reports a source file that produced too many entities,
/// through the lint the `inferred` profile runs.
fn dense_inference() -> Diagnostic {
    let project = Project::assembled("behavior a \"A\" {\n}\n", |root| {
        specforge_project::Environment::assemble(
            crate::view_support::config_read(
                root,
                json!({"inference": {"density_threshold": 0.5}}),
            ),
            specforge_installed::Installed::unread(root),
            specforge_project::ExtensionLoad::NoRuntime,
        )
    });
    let root = project.dir.path();
    std::fs::write(root.join("lib.rs"), "fn a() {}\n").unwrap();
    std::fs::write(
        root.join("specforge-infer.json"),
        json!({
            "version": 1,
            "source_roots": ["."],
            "source_index": [{
                "path": "lib.rs",
                "content_hash": "",
                "entities_produced": ["a", "b"],
                "analyzed_at": "",
            }],
        })
        .to_string(),
    )
    .unwrap();
    only(specforge_ops::read::infer::lint(&project.view()), "I202")
}

/// The one diagnostic of `diagnostics` whose code is `code`.
fn only(diagnostics: Vec<Diagnostic>, code: &str) -> Diagnostic {
    let mut found: Vec<Diagnostic> = diagnostics.into_iter().filter(|d| d.code == code).collect();
    assert_eq!(found.len(), 1, "one {code}: {found:?}");
    found.remove(0)
}

#[test]
fn diagnostics_json_keeps_code_severity_title() {
    let diagnostics = [
        unexecutable_rule(),
        unresolved_reference(),
        entity_without_contracts(),
        registry_unauthorized(),
        dense_inference(),
    ];

    let printed: Vec<Value> =
        serde_json::to_value(specforge_common::diagnostics_json(&diagnostics))
            .unwrap()
            .as_array()
            .unwrap()
            .iter()
            .map(|entry| json!([entry["code"], entry["title"], entry["severity"]]))
            .collect();

    assert_eq!(
        printed,
        [
            json!(["W112", "Validation rule cannot fire", "Warning"]),
            json!(["E003", "Unresolved reference", "Error"]),
            json!(["A010", "Entity without contract obligations", "Info"]),
            json!(["R001", "Registry authentication failed", "Error"]),
            json!(["I202", "High inference density", "Info"]),
        ]
    );
}
