//! Wave 0 pin: the registry build of declarations that drive every
//! diagnostic the build reports, at once. It characterizes the build (no
//! spec link): while the tests move behind `build_registries`, its
//! snapshot must not change; it reads what the build answers, never its
//! fields' layout.

use std::collections::BTreeMap;

use serde_json::{Value, json};
use specforge_common::Diagnostic;
use specforge_extension_sdk::prelude::*;
use specforge_graph::GraphInputs;
use specforge_protocol_types::ExtensionDeclaration;
use specforge_registry::RegistryBuild;

use crate::support::{build, declare, peer};

/// A pass's handler that finds nothing.
fn no_findings(_: &PassInput) -> Vec<PassDiagnostic> {
    Vec::new()
}

/// The command `hello`.
fn hello(c: &mut ContributionsBuilder) {
    c.command("hello", |cmd| {
        cmd.title("Hello")
            .description("Say hello")
            .handler(|_| CommandOutput::ok("hello"));
    });
}

/// `@pin/software`: W017 (`thing`), a lifecycle W021 (`note`), a proof-role
/// W021 and a W019 (`rule`), two declaration W021s (`behavior`'s `owner`
/// and `watched`), an E006 rule (`contract`), a W112 (`W901`) and a W145
/// (passes `a`, `b`).
fn pin_software() -> ExtensionDeclaration {
    let mut software = declare("@pin/software", |c| {
        c.kind("Behavior", |k| {
            k.keyword("behavior")
                .testable(true)
                .supports_verify(true)
                .verify_kinds(&["unit", "contract"]);
            k.field("contract", |f| {
                f.field_type(FieldType::Block).required();
            });
            k.field("owner", |f| {
                f.field_type(FieldType::Reference).target_kind("nowhere");
            });
            k.field("watched", |f| {
                f.field_type(FieldType::ReferenceList).edge("missing_edge");
            });
        });
        c.kind("Thing", |k| {
            k.keyword("thing").testable(true).supports_verify(false);
        });
        c.kind("Note", |k| {
            k.keyword("note").lifecycle_field("status");
        });
        c.kind("Rule", |k| {
            k.keyword("rule");
            k.field("limit", |f| {
                f.field_type(FieldType::String).proof_role("assumed");
            });
            k.field("notes", |f| {
                f.field_type(FieldType::String);
            });
        });
        c.edge("links_to", |e| {
            e.description("Links one entity to another");
        });
        c.rule("W900", |r| {
            r.severity(ValidationSeverity::Warning)
                .message_template("'{id}' has no incoming edge")
                .check(CheckKind::NoIncomingEdges)
                .target_kind("behavior");
        });
        c.rule("W901", |r| {
            r.severity(ValidationSeverity::Warning)
                .message_template("'{id}' is bogus")
                .check(CheckKind::NoIncomingEdges);
        });
        c.pass("a", |p| {
            p.after("b").run(no_findings);
        });
        c.pass("b", |p| {
            p.after("a").run(no_findings);
        });
        hello(c);
    });
    // "text" is no field type the protocol defines (W019).
    software.entities[3].fields[1].field_type = "text".into();
    // "bogus" is no check the protocol defines (W112).
    software.validation_rules[1].check = "bogus".into();
    software
}

/// `@pin/product`: two E027s and an E073 (its peers), E026 (`behavior`),
/// W018 (`links_to`), W023 (`W900`), I004 (an enhancement of an unknown
/// kind it owns), E039 (`hello`) and E055 (a tool whose input schema is
/// not an object). Its enhancement of `module`, owned by an extension that
/// is not loaded, is skipped silently.
fn pin_product() -> ExtensionDeclaration {
    let mut c = ContributionsBuilder::new(ExtensionMeta::new("@pin/product", "1.0.0"));
    c.meta.peer_dependencies = vec![
        peer("@pin/software", "^2"),
        peer("@pin/missing", "^1"),
        peer("@pin/software", "not-a-version"),
    ];
    c.kind("Behavior", |k| {
        k.keyword("behavior");
    });
    c.edge("links_to", |e| {
        e.description("Links again");
    });
    c.rule("W900", |r| {
        r.severity(ValidationSeverity::Warning)
            .message_template("'{id}' again")
            .check(CheckKind::NoIncomingEdges);
    });
    c.enhance("nowhere_kind", "@pin/product", |e| {
        e.field("extra", |f| {
            f.field_type(FieldType::String);
        });
    });
    c.enhance("module", "@pin/absent", |e| {
        e.field("owner_team", |f| {
            f.field_type(FieldType::String);
        });
    });
    hello(&mut c);
    c.mcp_tool("probe", |t| {
        t.description("Probe").handler(|_| Ok(json!({})));
    });
    let mut product = c.declaration();
    product.surfaces.mcp_tools[0].input_schema = json!("string");
    product
}

/// `""`: E030 (its name is empty).
fn pin_unnamed() -> ExtensionDeclaration {
    declare("", |_| {})
}

fn rendered<'a>(diagnostics: impl IntoIterator<Item = &'a Diagnostic>) -> Value {
    diagnostics
        .into_iter()
        .map(|d| json!([d.code, format!("{:?}", d.severity), d.message]))
        .collect()
}

/// What the graph build reads of the build's declarations.
fn graph_inputs(inputs: &GraphInputs) -> Value {
    let bidirectional_pairs: Vec<Value> = inputs
        .bidirectional_pairs
        .iter()
        .map(|(a, b)| json!([a, b]))
        .collect();
    let absent_reference_targets: BTreeMap<String, &String> = inputs
        .absent_reference_targets
        .iter()
        .map(|((kind, field), target)| (format!("{kind}.{field}"), target))
        .collect();
    let field_coercions: BTreeMap<String, String> = inputs
        .field_coercions
        .iter()
        .map(|((kind, field), coercion)| (format!("{kind}.{field}"), format!("{coercion:?}")))
        .collect();
    let derived_references: Vec<Value> = inputs
        .derived_references
        .iter()
        .map(|d| {
            json!({
                "source_kind": d.source_kind,
                "field": d.field,
                "target_kind": d.target_kind,
                "from": format!("{:?}", d.from),
            })
        })
        .collect();
    let mut body_parser_kinds: Vec<&String> = inputs.body_parser_kinds.iter().collect();
    body_parser_kinds.sort();
    let mut single_reference_fields: Vec<String> = inputs
        .single_reference_fields
        .iter()
        .map(|(kind, field)| format!("{kind}.{field}"))
        .collect();
    single_reference_fields.sort();
    json!({
        "body_parser_kinds": body_parser_kinds,
        "single_reference_fields": single_reference_fields,
        "bidirectional_pairs": bidirectional_pairs,
        "absent_reference_targets": absent_reference_targets,
        "field_coercions": field_coercions,
        "derived_references": derived_references,
    })
}

/// What the snapshot reads of the build.
fn digest(build: &RegistryBuild) -> Value {
    let mut kinds: Vec<Value> = build
        .kinds
        .iter()
        .map(|(keyword, entry)| {
            json!({
                "keyword": keyword,
                "source_extension": entry.source_extension,
                "testable": entry.testable,
                "allowed_verify_kinds": entry.allowed_verify_kinds,
            })
        })
        .collect();
    kinds.sort_by_key(|k| k["keyword"].as_str().unwrap_or_default().to_string());
    let mut fields: Vec<String> = build
        .fields
        .iter()
        .map(|(kind, field, entry)| format!("{kind}.{field}: {:?}", entry.field_type()))
        .collect();
    fields.sort();
    let mut edges: Vec<String> = build
        .edges
        .iter()
        .map(|(label, entry)| format!("{label}: {}", entry.source_extension))
        .collect();
    edges.sort();
    json!({
        // Every diagnostic the build reports, in its order (T3: `build.diagnostics()`).
        "diagnostics": rendered(build.diagnostics()),
        "rules": build
            .rules
            .iter()
            .map(|rule| {
                json!([
                    rule.code(),
                    rule.origin().name(),
                    rule.target_kind(),
                    rule.describe()["field"]
                ])
            })
            .collect::<Vec<_>>(),
        "kinds": kinds,
        "fields": fields,
        "edges": edges,
        "graph_inputs": graph_inputs(&build.graph_inputs()),
        "passes": build.passes.iter().map(|p| p.full_name()).collect::<Vec<_>>(),
    })
}

#[test]
fn the_build_of_every_diagnosed_declaration() {
    let build = build([pin_software(), pin_product(), pin_unnamed()]);
    insta::assert_json_snapshot!(digest(&build));
}
