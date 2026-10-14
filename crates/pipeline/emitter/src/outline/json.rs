use super::{OutlineDependency, OutlineDetail, OutlineIntermediate, OutlineOptions};

pub(super) fn render_json(outline: &OutlineIntermediate, options: &OutlineOptions) -> String {
    let dependencies = super::filter_dependencies(&outline.dependencies, options.deps);
    match options.detail {
        OutlineDetail::None => render_summary(outline, &dependencies),
        OutlineDetail::Keys => render_keys(outline, &dependencies),
        OutlineDetail::All => render_all(outline, &dependencies),
    }
}

fn build_metadata(outline: &OutlineIntermediate) -> serde_json::Value {
    let total_entities: usize = outline
        .extensions
        .iter()
        .map(|e| e.entity_kinds.len())
        .sum();
    let total_edges: usize = outline.extensions.iter().map(|e| e.edge_types.len()).sum();
    let total_rules: usize = outline
        .extensions
        .iter()
        .map(|e| e.validation_rules.len())
        .sum();

    serde_json::json!({
        "total_extensions": outline.extensions.len(),
        "total_entity_kinds": total_entities,
        "total_edge_types": total_edges,
        "total_validation_rules": total_rules,
        "total_cross_edges": outline.cross_edges.len(),
        "total_enhancements": outline.enhancements.len(),
    })
}

fn render_summary(outline: &OutlineIntermediate, dependencies: &[&OutlineDependency]) -> String {
    let summary: serde_json::Value = serde_json::json!({
        "metadata": build_metadata(outline),
        "extensions": outline.extensions.iter().map(|ext| {
            serde_json::json!({
                "name": ext.name,
                "version": ext.version,
                "entity_count": ext.entity_kinds.len(),
                "edge_count": ext.edge_types.len(),
                "rule_count": ext.validation_rules.len(),
                "cross_edge_count": outline.cross_edges.iter()
                    .filter(|ce| ce.owner_extension == ext.name)
                    .count(),
            })
        }).collect::<Vec<_>>(),
        "dependencies": dependencies,
        "enhancements": outline.enhancements,
        "cross_edges": outline.cross_edges,
    });
    serde_json::to_string_pretty(&summary).unwrap_or_default()
}

fn render_keys(outline: &OutlineIntermediate, dependencies: &[&OutlineDependency]) -> String {
    let keys: serde_json::Value = serde_json::json!({
        "metadata": build_metadata(outline),
        "extensions": outline.extensions.iter().map(|ext| {
            serde_json::json!({
                "name": ext.name,
                "version": ext.version,
                "entity_kinds": ext.entity_kinds.iter().map(|k| {
                    serde_json::json!({
                        "name": k.name,
                        "keyword": k.keyword,
                        "testable": k.testable,
                        "field_count": k.field_count,
                    })
                }).collect::<Vec<_>>(),
                "edge_types": ext.edge_types.iter().map(|e| {
                    let mut obj = serde_json::json!({
                        "label": e.label,
                    });
                    if let Some(ref desc) = e.description {
                        obj["description"] = serde_json::json!(desc);
                    }
                    if let Some(ref sk) = e.source_kind {
                        obj["source_kind"] = serde_json::json!(sk);
                    }
                    if let Some(ref tk) = e.target_kind {
                        obj["target_kind"] = serde_json::json!(tk);
                    }
                    obj
                }).collect::<Vec<_>>(),
                "validation_rules": ext.validation_rules.iter().map(|r| {
                    serde_json::json!({
                        "code": r.code,
                        "severity": r.severity,
                        "check": r.check,
                    })
                }).collect::<Vec<_>>(),
                "contributes": ext.contributes,
                "verify_kinds": ext.verify_kinds,
                "surface_counts": ext.surface_counts,
                "shared_fields": ext.shared_fields,
                "collector_count": ext.collector_count,
            })
        }).collect::<Vec<_>>(),
        "dependencies": dependencies,
        "enhancements": outline.enhancements,
        "cross_edges": outline.cross_edges,
    });
    serde_json::to_string_pretty(&keys).unwrap_or_default()
}

fn render_all(outline: &OutlineIntermediate, dependencies: &[&OutlineDependency]) -> String {
    let all: serde_json::Value = serde_json::json!({
        "metadata": build_metadata(outline),
        "extensions": outline.extensions,
        "dependencies": dependencies,
        "enhancements": outline.enhancements,
        "cross_edges": outline.cross_edges,
    });
    serde_json::to_string_pretty(&all).unwrap_or_default()
}
