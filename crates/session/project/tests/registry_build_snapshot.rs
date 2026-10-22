//! The registry build of the builtins, pinned: a canonical digest of what
//! `Environment::load` derives from every builtin together, from each
//! builtin alone and from the SDK greet fixture alone. The digest reads what
//! the environment answers (its registries, its graph configuration, its
//! diagnostics in report order), never a field of the build's layout.

use std::collections::BTreeMap;

use serde_json::{Value, json};
use specforge_common::Diagnostic;
use specforge_component::ComponentRuntime;
use specforge_component::builtins::BUILTIN_EXTENSIONS;
use specforge_graph::Target;
use specforge_project::Environment;
use specforge_protocol_types::{FieldType, SurfaceDescriptor};
use tempfile::TempDir;

fn runtime() -> ComponentRuntime {
    ComponentRuntime::new()
}

/// The SDK greet fixture's component blob.
fn greet() -> Vec<u8> {
    let greet = specforge_component::committed::GREET.path();
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
        specforge_installed::testing::install_module(
            dir.path(),
            runtime.as_ref(),
            "@sdk/greet",
            &greet(),
        );
    }
    Environment::load(
        &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
        Some(runtime.clone()),
    )
}

fn diagnostics<'a>(diagnostics: impl IntoIterator<Item = &'a Diagnostic>) -> Value {
    diagnostics
        .into_iter()
        .map(|d| json!({ "code": d.code, "message": d.message }))
        .collect()
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

/// What every graph of the project is built with, as the Registry build holds it
/// (`RegistryBuild::graph_inputs`), without the provider schemes.
fn graph_inputs(env: &Environment) -> Value {
    let config = env.registries().graph_inputs();
    let absent_targets: BTreeMap<String, String> = config
        .field_forms
        .targets()
        .into_iter()
        .filter_map(|((kind, field), target)| match target {
            Target::Absent(target) => Some((
                format!("{kind}.{field}"),
                match config.target_owners.get(target) {
                    Some(owner) => format!("{target} ({owner})"),
                    None => target.to_string(),
                },
            )),
            Target::Loaded(_) => None,
        })
        .collect();
    let field_forms: BTreeMap<String, String> = config
        .field_forms
        .iter()
        .map(|((kind, field), form)| {
            let label = match form.enum_values() {
                [] => form.field_type().as_str().to_string(),
                values => format!("{} ({})", form.field_type().as_str(), values.join(", ")),
            };
            (format!("{kind}.{field}"), label)
        })
        .collect();
    let derived_references: Vec<Value> = config
        .derived_references
        .iter()
        .map(|d| {
            json!({
                "source_kind": d.source_kind,
                "field": d.field,
                "target_kind": config.field_forms.target(&d.source_kind, &d.field).map(|t| t.kind()),
                "from": format!("{:?}", d.from),
            })
        })
        .collect();
    json!({
        "member_kinds": config.field_forms.member_kinds(),
        "absent_targets": absent_targets,
        "field_forms": field_forms,
        "derived_references": derived_references,
    })
}

fn digest(env: &Environment) -> Value {
    let r = env.registries();
    let kinds: BTreeMap<&str, Value> = r
        .kinds()
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
        .fields()
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
        .edges()
        .iter()
        .map(|e| {
            let declared = e.declared().cloned().unwrap_or_default();
            (
                e.label(),
                json!({
                    "label": e.label(),
                    "description": declared.description,
                    "source_kind": declared.source_kind,
                    "target_kind": declared.target_kind,
                    "source_extension": e.source_extension(),
                    "sources": e.sources(),
                    "targets": e.targets(),
                    "writers": e.writers().iter().map(|w| match &w.target_kind {
                        Some(target) => format!("{} {}.{} -> {target}", w.extension, w.kind, w.field),
                        None => format!("{} {}.{}", w.extension, w.kind, w.field),
                    }).collect::<Vec<_>>(),
                    "edge_style": declared.edge_style,
                    "edge_color": declared.edge_color,
                    "edge_arrowhead": declared.edge_arrowhead,
                }),
            )
        })
        .collect();
    let rules: Vec<Value> = r
        .rules()
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
        "commands": r.commands().routed().iter().map(|c| json!({
            "extension": c.extension(),
            "route": format!("{} {}", c.short(), c.cli_name()),
            "export": c.declaration().export,
        })).collect::<Vec<_>>(),
        "shadowed": r.commands().shadowed().len(),
        "manifest_surfaces": r.declarations().iter()
            .filter(|d| d.surfaces != SurfaceDescriptor::default())
            .map(|d| json!({ "extension": d.name(), "surfaces": surfaces(&d.surfaces) }))
            .collect::<Vec<_>>(),
        "extension_info": r.extension_info().collect::<Vec<_>>(),
        "check_passes": r.check_passes()
            .map(|p| json!({ "extension": p.extension, "name": p.pass.name }))
            .collect::<Vec<_>>(),
        "ext_short": ext_short,
        // Everything the environment reports, in its order.
        "diagnostics": diagnostics(env.diagnostics()),
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
