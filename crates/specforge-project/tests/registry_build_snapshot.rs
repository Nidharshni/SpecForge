//! The registry build of the builtins, pinned: a canonical digest of what
//! `Environment::load` derives from every builtin together, from each
//! builtin alone and from the SDK greet fixture alone. The digest reads what
//! the environment answers (its registries, its graph configuration, its
//! diagnostics in report order), never a field of the build's layout.

use std::collections::BTreeMap;
use std::path::Path;

use serde_json::{Value, json};
use specforge_common::Diagnostic;
use specforge_component::ComponentRuntime;
use specforge_component::builtins::BUILTIN_EXTENSIONS;
use specforge_project::Environment;
use specforge_protocol_types::{FieldType, SurfaceDescriptor};
use tempfile::TempDir;

fn runtime() -> ComponentRuntime {
    ComponentRuntime::new()
}

/// The SDK greet fixture's component blob.
fn greet() -> Vec<u8> {
    let greet =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/greet-extension/greet.wasm");
    std::fs::read(greet).expect("vendored greet component blob")
}

fn load(runtime: &std::sync::Arc<ComponentRuntime>, extensions: &[&str]) -> Environment {
    let dir = TempDir::new().unwrap();
    std::fs::write(
        dir.path().join("specforge.json"),
        json!({ "name": "p", "version": "0.1.0", "extensions": extensions }).to_string(),
    )
    .unwrap();
    if extensions.contains(&"@sdk/greet") {
        specforge_installed::testing::install_module(dir.path(), "@sdk/greet", &greet());
    }
    Environment::load(dir.path(), Some(runtime.clone()))
}

fn diagnostics<'a>(diagnostics: impl IntoIterator<Item = &'a Diagnostic>) -> Value {
    diagnostics
        .into_iter()
        .map(|d| json!({ "code": d.code, "message": d.message }))
        .collect()
}

fn sorted(mut values: Vec<Value>) -> Value {
    values.sort_by_key(|v| v.to_string());
    Value::Array(values)
}

fn surfaces(s: &SurfaceDescriptor) -> Value {
    json!({
        "commands": s.commands.iter().map(|c| json!({
            "id": c.id,
            "title": c.title,
            "description": c.description,
            "category": c.category,
            "export": c.export,
            "args": c.args.iter().map(|a| json!({
                "name": a.name,
                "arg_type": serde_json::to_value(&a.arg_type).unwrap(),
                "required": a.required,
                "default_value": a.default_value,
                "description": a.description,
            })).collect::<Vec<_>>(),
        })).collect::<Vec<_>>(),
        "mcp_tools": s.mcp_tools.iter().map(|t| json!({
            "name": t.name,
            "description": t.description,
            "category": t.category,
            "export": t.export,
            "input_schema": t.input_schema,
            "output_schema": t.output_schema,
        })).collect::<Vec<_>>(),
        "mcp_resources": s.mcp_resources.iter().map(|r| json!({
            "uri_template": r.uri_template,
            "name": r.name,
            "description": r.description,
            "export": r.export,
            "mime_type": r.mime_type,
        })).collect::<Vec<_>>(),
    })
}

/// What every graph of the project is built with, as the environment answers it
/// (`Environment::graph_config`), without the provider schemes.
fn graph_inputs(env: &Environment) -> Value {
    let config = env.graph_config().inputs;
    let bidirectional_pairs: Vec<Value> = config
        .bidirectional_pairs
        .iter()
        .map(|(a, b)| {
            let (a, b) = if a <= b { (a, b) } else { (b, a) };
            json!([a, b])
        })
        .collect();
    let absent_reference_targets: BTreeMap<String, &String> = config
        .absent_reference_targets
        .iter()
        .map(|((kind, field), target)| (format!("{kind}.{field}"), target))
        .collect();
    let field_coercions: BTreeMap<String, String> = config
        .field_coercions
        .iter()
        .map(|((kind, field), coercion)| (format!("{kind}.{field}"), format!("{coercion:?}")))
        .collect();
    let derived_references: Vec<Value> = config
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
    json!({
        "body_parser_kinds": sorted(config.body_parser_kinds.iter().map(|k| json!(k)).collect()),
        "single_reference_fields": sorted(
            config.single_reference_fields.iter().map(|(k, f)| json!([k, f])).collect()
        ),
        "bidirectional_pairs": sorted(bidirectional_pairs),
        "absent_reference_targets": absent_reference_targets,
        "field_coercions": field_coercions,
        "derived_references": derived_references,
    })
}

fn digest(env: &Environment) -> Value {
    let r = &env.registries;
    let kinds: BTreeMap<&str, Value> = r
        .kinds
        .iter()
        .map(|(name, k)| {
            (
                name.as_str(),
                json!({
                    "kind_name": k.kind_name,
                    "description": k.declared.description,
                    "source_extension": k.source_extension,
                    "testable": k.testable,
                    "singleton": k.declared.singleton,
                    "supports_verify": k.supports_verify,
                    "allowed_verify_kinds": k.allowed_verify_kinds,
                    "has_body_parser": k.declared.has_body_parser,
                    "semantic_token": k.declared.semantic_token,
                    "lsp_icon": k.declared.lsp_icon,
                    "dot_shape": k.declared.dot_shape,
                    "dot_color": k.declared.dot_color,
                    "dot_fillcolor": k.declared.dot_fillcolor,
                    "open_fields": k.declared.open_fields,
                    "contract_target": k.declared.contract_target,
                    "declares_types": k.declared.declares_types,
                    "lifecycle_field": k.lifecycle_field,
                }),
            )
        })
        .collect();
    let fields: BTreeMap<String, Value> = r
        .fields
        .iter()
        .map(|(kind, field, f)| {
            (
                format!("{kind}.{field}"),
                json!({
                    "kind_name": f.kind_name(),
                    "field_name": f.declared().name,
                    "description": f.declared().description,
                    "field_type": match f.field_type() {
                        FieldType::Enum => format!("Enum({:?})", f.enum_values()),
                        t => format!("{t:?}"),
                    },
                    "source_extension": f.source_extension(),
                    "edge": f.declared().edge,
                    "target_kind": f.declared().target_kind,
                    "file_reference": f.declared().file_reference,
                    "required": f.declared().required,
                    "inverse_of": f.declared().inverse_of,
                    "normative": f.declared().normative,
                    "exempts_obligations": f.declared().exempts_obligations,
                    "headline": f.declared().headline,
                    "derived_from": f.declared().derived_from,
                    "proof_role": f.proof_role().map(|p| format!("{p:?}")),
                }),
            )
        })
        .collect();
    let edges: BTreeMap<&str, Value> = r
        .edges
        .iter()
        .map(|(label, e)| {
            (
                label.as_str(),
                json!({
                    "label": e.declared.label,
                    "description": e.declared.description,
                    "source_kind": e.declared.source_kind,
                    "target_kind": e.declared.target_kind,
                    "source_extension": e.source_extension,
                    "edge_style": e.declared.edge_style,
                    "edge_color": e.declared.edge_color,
                    "edge_arrowhead": e.declared.edge_arrowhead,
                }),
            )
        })
        .collect();
    let rules: Vec<Value> = r
        .rules
        .iter()
        .map(|rule| {
            let mut described = rule.describe();
            described["owner"] = json!(rule.origin().name());
            described
        })
        .collect();
    // An extension's routing name: its declared short name, else its name's
    // last segment.
    let ext_short: BTreeMap<&str, String> = r
        .declarations()
        .iter()
        .map(|d| (d.name(), d.short().into_owned()))
        .collect();
    json!({
        "kinds": kinds,
        "fields": fields,
        "edges": edges,
        "rules": rules,
        "graph_inputs": graph_inputs(env),
        "surfaces": r.surfaces.iter().map(|s| json!({
            "surface_type": format!("{:?}", s.surface_type),
            "name": s.contribution_name,
            "extension": s.extension_name,
            "export": s.export_name,
        })).collect::<Vec<_>>(),
        "manifest_surfaces": r.declarations().iter()
            .filter(|d| d.surfaces != SurfaceDescriptor::default())
            .map(|d| json!({ "extension": d.name(), "surfaces": surfaces(&d.surfaces) }))
            .collect::<Vec<_>>(),
        "extension_info": r.extension_info().collect::<Vec<_>>(),
        "check_passes": r.check_passes()
            .map(|p| json!({ "extension": p.extension, "name": p.pass.name }))
            .collect::<Vec<_>>(),
        "ext_short": ext_short,
        // Everything the environment reports, in its order (T3: `env.diagnostics()` alone).
        "diagnostics": diagnostics(env.diagnostics().chain(env.surface_diagnostics())),
    })
}

#[specforge_test_macros::test(
    behavior = "build_registries_from_declarations",
    verify = "the registry build of the builtins matches its pinned snapshot"
)]
fn registry_build_of_the_builtins_matches_its_snapshot() {
    let runtime = std::sync::Arc::new(runtime());
    let all: Vec<&str> = BUILTIN_EXTENSIONS.iter().map(|(name, _)| *name).collect();
    insta::assert_json_snapshot!("all_builtins", digest(&load(&runtime, &all)));
    for name in &all {
        let dir = name.rsplit('/').next().unwrap();
        insta::assert_json_snapshot!(format!("alone_{dir}"), digest(&load(&runtime, &[name])));
    }
    insta::assert_json_snapshot!("alone_greet", digest(&load(&runtime, &["@sdk/greet"])));
}
