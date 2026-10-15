//! The registry build reports its diagnostics in one order, and runs the
//! custom rules' probes itself.

use specforge_extension_sdk::prelude::*;
use specforge_protocol_types::{ExtensionDeclaration, McpToolDescriptor};
use specforge_registry::BuildPart;
use specforge_registry::rules::{CustomCall, NoVerdicts, Verdict, VerdictError};
use specforge_test_macros::test as spec;

use crate::support::{build, codes, declare, peer};

/// `name`, declaring the kind `dup`, the rule `W900` and the command
/// `hello`; `first` also a field targeting a kind nobody declares and a
/// custom rule whose function does not exist.
fn colliding(name: &str, first: bool) -> ExtensionDeclaration {
    declare(name, |c| {
        c.meta.short = Some("dup".to_string());
        c.kind("Dup", |k| {
            k.keyword("dup");
            if first {
                k.field("owner", |f| {
                    f.field_type(FieldType::Reference).target_kind("ghost");
                });
            }
        });
        c.rule("W900", |r| {
            r.severity(ValidationSeverity::Warning)
                .message_template("'{id}' has no outgoing edge")
                .check(CheckKind::NoOutgoingEdges)
                .target_kind("dup");
        });
        if first {
            c.rule("W901", |r| {
                r.severity(ValidationSeverity::Warning)
                    .message_template("'{id}' is wrong")
                    .check(CheckKind::Custom)
                    .wasm_function("gone")
                    .target_kind("dup")
                    .validate(|_| ValidatorVerdict::Pass);
            });
        }
        c.command("hello", |cmd| {
            cmd.title("Hello")
                .description("Say hello")
                .handler(|_| CommandOutput::ok("hello"));
        });
    })
}

/// A verdict source whose every function fails.
fn failing(_: CustomCall<'_>) -> Result<Verdict, VerdictError> {
    Err(VerdictError::Failed("no export".into()))
}

#[spec(
    behavior = "build_registries_from_declarations",
    verify = "the registry build reports its diagnostics in one order: the declarations', the registries', the custom rules' probes, the surfaces'"
)]
fn the_build_reports_its_diagnostics_in_one_order() {
    let mut build = build([
        colliding("@t/a", true),
        colliding("@t/b", false),
        declare("", |_| {}),
    ]);
    build.probe(&failing);

    // The declarations' own (E030, W021), the registries' (E026, W023), the
    // probe's (W112), then the surfaces' (E039).
    assert_eq!(
        codes(&build),
        ["E030", "W021", "E026", "W023", "W112", "E039"]
    );
}

#[spec(
    behavior = "build_registries_from_declarations",
    verify = "probing the build reports each custom rule whose function cannot answer once, as W112 after the registries' diagnostics"
)]
fn a_probe_is_w112_after_the_registries_and_probing_again_reports_it_once() {
    let mut build = build([colliding("@t/a", true), colliding("@t/b", false)]);
    assert_eq!(codes(&build), ["W021", "E026", "W023", "E039"]);

    build.probe(&failing);
    let after = ["W021", "E026", "W023", "W112", "E039"];
    assert_eq!(codes(&build), after);
    let probe = build.diagnostics().find(|d| d.code == "W112").unwrap();
    assert!(
        probe
            .message
            .contains("wasm_function 'gone' could not be resolved"),
        "{}",
        probe.message
    );

    // Probing again replaces the earlier result.
    build.probe(&failing);
    assert_eq!(codes(&build), after);

    // A source that cannot answer reports nothing.
    build.probe(&NoVerdicts);
    assert_eq!(codes(&build), ["W021", "E026", "W023", "E039"]);
}

#[spec(
    behavior = "build_registries_from_declarations",
    verify = "the registry build says what each of its diagnostics is about: a declaration, the peers, the registries, a rule, a surface or a collision"
)]
fn the_build_says_what_each_diagnostic_is_about() {
    let mut build = build([
        colliding("@t/a", true),
        colliding("@t/b", false),
        declare("", |_| {}),
    ]);
    build.probe(&failing);
    let parts: Vec<_> = build
        .reported()
        .map(|(p, d)| (p, d.code.as_str()))
        .collect();
    assert_eq!(
        parts,
        [
            (BuildPart::Declaration, "E030"),
            (BuildPart::Declaration, "W021"),
            (BuildPart::Collision, "E026"),
            (BuildPart::Collision, "W023"),
            (BuildPart::Rules, "W112"),
            (BuildPart::Collision, "E039"),
        ]
    );

    // A tool whose schema is no object is about a surface; a peer nobody
    // loaded is about the peers.
    let mut tools = declare("@t/tools", |_| {});
    tools.surfaces.mcp_tools.push(McpToolDescriptor {
        name: "bad".into(),
        description: "bad".into(),
        category: None,
        export: "mcp__bad".into(),
        input_schema: serde_json::json!("not an object"),
        output_schema: None,
    });
    let needy = declare("@t/needy", |c| {
        c.meta.peer_dependencies = vec![peer("@t/absent", "^1")];
    });
    let build = crate::support::build([tools, needy]);
    let parts: Vec<_> = build
        .reported()
        .map(|(p, d)| (p, d.code.as_str()))
        .collect();
    assert_eq!(
        parts,
        [(BuildPart::Peers, "E027"), (BuildPart::Surfaces, "E055")]
    );
}
