//! Every format x schema x scope x budget combination `emit` renders, pinned
//! as one snapshot (plan 14, P2): the refactor of the emitter's envelope and
//! budget (T5, T6) changes only the cells it names.

use specforge_emitter::{
    EmitFormat, EmitOptions, GraphProtocolSchema, estimate_tokens, generate_schema,
};
use specforge_registry::{
    EdgeRegistry, EdgeRegistryEntry, FieldRegistry, KindRegistry, KindRegistryEntry,
};

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

fn kind(name: &str, ext: &str, testable: bool) -> KindRegistryEntry {
    KindRegistryEntry {
        kind_name: name.to_string(),
        source_extension: ext.to_string(),
        testable,
        supports_verify: testable,
        allowed_verify_kinds: vec![],
        lifecycle_field: None,
        ..Default::default()
    }
}

fn schema(fields: &FieldRegistry) -> GraphProtocolSchema {
    let mut kinds = KindRegistry::new();
    kinds.register(kind("behavior", "@specforge/software", true));
    kinds.register(kind("feature", "@specforge/product", false));
    let mut edges = EdgeRegistry::new();
    edges.register(EdgeRegistryEntry {
        source_extension: "@specforge/product".to_string(),
        declared: specforge_registry::EdgeTypeDescriptor {
            label: "behaviors".to_string(),
            source_kind: Some("feature".to_string()),
            target_kind: Some("behavior".to_string()),
            ..Default::default()
        },
        writers: Vec::new(),
    });
    generate_schema(
        &kinds,
        &edges,
        fields,
        &[
            ("@specforge/software".to_string(), "1.0.0".to_string()),
            ("@specforge/product".to_string(), "1.0.0".to_string()),
        ],
    )
}

#[test]
fn every_format_schema_scope_and_budget_renders_as_today() {
    let (graph, _) = specforge_graph::build_graph(&[specforge_parser::parse(SRC, "main.spec")]);
    let fields = headline_registry(&["behavior"]);
    let schema = schema(&fields);

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
                    schema: with_schema.then_some(&schema),
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
