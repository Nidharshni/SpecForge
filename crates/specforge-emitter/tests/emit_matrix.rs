//! Every format x schema x scope x budget combination `emit` renders, pinned
//! as one snapshot (plan 14, P2): the refactor of the emitter's envelope and
//! budget (T5, T6) changes only the cells it names.

use specforge_emitter::{
    EmitFormat, EmitOptions, GraphProtocolSchema, estimate_tokens, generate_schema,
};
use specforge_protocol_types::{
    EdgeTypeDescriptor, EntityKindDescriptor, ExtensionDeclaration, FieldDescriptor,
};
use specforge_registry::build_registries;

use crate::support::headline_registry;

const SRC: &str = r#"
behavior a "Alpha" {
  contract "The system MUST alpha"
}
behavior b "Beta" {
  contract "The system MUST beta the data"
}
behavior c "Gamma" {
  contract "The system MUST gamma the request"
}
behavior d "Delta" {
  contract "The system MUST delta once and only once"
}
behavior e "Epsilon" {
  contract "The system MUST epsilon"
}
feature hub "Hub" {
  behaviors [a, b, c, d]
}
"#;

/// The kind `name` (its keyword), declared by `ext`.
fn kind(name: &str, testable: bool) -> EntityKindDescriptor {
    EntityKindDescriptor {
        name: name.to_string(),
        keyword: Some(name.to_string()),
        testable,
        supports_verify: testable,
        ..Default::default()
    }
}

fn declaration(name: &str) -> ExtensionDeclaration {
    let mut declaration = ExtensionDeclaration::default();
    declaration.handshake.name = name.to_string();
    declaration.handshake.version = "1.0.0".to_string();
    declaration
}

/// The schema of the software and product extensions: `behavior` declares
/// `contract` and `status` as headline fields (`contract` normative), as
/// `@specforge/software` does.
fn schema() -> GraphProtocolSchema {
    let mut software = declaration("@specforge/software");
    let mut behavior = kind("behavior", true);
    behavior.fields = ["contract", "status"]
        .map(|field| FieldDescriptor {
            name: field.to_string(),
            field_type: "string".to_string(),
            normative: field == "contract",
            headline: true,
            ..Default::default()
        })
        .to_vec();
    software.entities = vec![behavior];
    let mut product = declaration("@specforge/product");
    product.entities = vec![kind("feature", false)];
    product.edges = vec![EdgeTypeDescriptor {
        label: "behaviors".to_string(),
        source_kind: Some("feature".to_string()),
        target_kind: Some("behavior".to_string()),
        ..Default::default()
    }];
    generate_schema(&build_registries(vec![software, product]))
}

#[test]
fn every_format_schema_scope_and_budget_renders_as_today() {
    let (graph, _) = specforge_graph::build_graph(&[specforge_parser::parse(SRC, "main.spec")]);
    let fields = headline_registry(&["behavior"]);
    let schema = schema();

    let formats = [
        ("json", EmitFormat::Json),
        ("context", EmitFormat::Context),
        ("brief", EmitFormat::Brief),
    ];
    let mut table = String::new();
    for (name, format) in formats {
        for with_schema in [false, true] {
            for scope in [None, Some("hub")] {
                // The unbudgeted cost of this cell, to build a budget one under.
                let unbudgeted = EmitOptions {
                    format,
                    scope,
                    schema: &schema,
                    attach_schema: with_schema,
                    field_registry: Some(&fields),
                    ..Default::default()
                };
                let whole = specforge_emitter::emit(&graph, &unbudgeted).unwrap();
                let budgets = [None, Some(estimate_tokens(&whole) - 1), Some(1)];
                for budget in budgets {
                    let options = EmitOptions {
                        token_budget: budget,
                        ..unbudgeted.clone()
                    };
                    let cell = format!(
                        "{name} schema={with_schema} scope={scope:?} budget={}",
                        match budget {
                            None => "none".to_string(),
                            Some(1) => "1".to_string(),
                            Some(_) => "whole-1".to_string(),
                        }
                    );
                    let output = match specforge_emitter::emit(&graph, &options) {
                        Ok(output) => output,
                        Err(error) => format!("Err({error:?})"),
                    };
                    table.push_str(&format!("{cell}: {output}\n"));
                }
            }
        }
    }
    insta::assert_snapshot!(table);
}
