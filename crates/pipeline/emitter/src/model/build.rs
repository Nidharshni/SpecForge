use std::collections::HashMap;

use super::cardinality::infer_cardinality;
use super::{
    Cardinality, ModelEntity, ModelExtension, ModelField, ModelIntermediate, ModelRelationship,
};
use crate::schema::{GraphProtocolSchema, SchemaEdgeType, SchemaEntityKind};
use specforge_registry::FieldType;

impl ModelIntermediate {
    pub(super) fn of(schema: &GraphProtocolSchema) -> Self {
        let entities: Vec<ModelEntity> = schema.entity_kinds.iter().map(build_entity).collect();

        let entity_map: HashMap<&str, &ModelEntity> =
            entities.iter().map(|e| (e.name.as_str(), e)).collect();

        let relationships: Vec<ModelRelationship> = schema
            .edge_types
            .iter()
            .flat_map(|edge| build_relationships(edge, &entity_map))
            .collect();

        let extensions = schema
            .extensions
            .iter()
            .map(|ext| ModelExtension {
                name: ext.name.clone(),
                version: ext.version.clone(),
                entity_count: 0,
                edge_count: 0,
                color: None,
            })
            .collect();

        let edge_type_owners: Vec<(String, String)> = schema
            .edge_types
            .iter()
            .map(|e| (e.label.clone(), e.source_extension.clone()))
            .collect();

        ModelIntermediate {
            schema_version: schema.schema_version.to_string(),
            extensions,
            entities,
            relationships,
            edge_type_owners,
        }
        .recount()
    }

    /// Each extension's counts, recomputed from what the model holds: `entity_count`, the kinds it
    /// declares that the model keeps; `edge_count`, the edge types it declares (`edge_type_owners`)
    /// that name at least one kept relationship. The same rule before and after a selection.
    pub(super) fn recount(mut self) -> Self {
        let mut entity_counts: HashMap<&str, usize> = HashMap::new();
        for entity in &self.entities {
            *entity_counts.entry(entity.extension.as_str()).or_insert(0) += 1;
        }
        let mut edge_counts: HashMap<&str, usize> = HashMap::new();
        for (label, owner) in &self.edge_type_owners {
            if self.relationships.iter().any(|r| r.name == *label) {
                *edge_counts.entry(owner.as_str()).or_insert(0) += 1;
            }
        }
        for extension in &mut self.extensions {
            extension.entity_count = entity_counts
                .get(extension.name.as_str())
                .copied()
                .unwrap_or(0);
            extension.edge_count = edge_counts
                .get(extension.name.as_str())
                .copied()
                .unwrap_or(0);
        }
        self
    }
}

fn build_entity(kind: &SchemaEntityKind) -> ModelEntity {
    let mut fields = vec![ModelField {
        name: "id".to_string(),
        field_type: FieldType::String,
        required: true,
        description: None,
        default_value: None,
        enum_values: None,
        is_primary_key: true,
        references: None,
        edge_label: None,
        contributed_by: None,
        contribution: None,
    }];

    let mut enhancing_extensions: Vec<String> = Vec::new();

    for sf in &kind.fields {
        let field_type = sf.field_type;
        let references = sf.target_kind.clone();

        // Determine contribution info: "EdgeLabel -> target_kind"
        let contribution = match (&sf.edge, &sf.target_kind) {
            (Some(edge), Some(target)) => Some(format!("{} -> {}", edge, target)),
            (None, Some(target)) => Some(format!("-> {}", target)),
            _ => None,
        };

        // Track cross-extension contributions
        let contributed_by = if sf.source_extension != kind.source_extension {
            let ext = sf.source_extension.clone();
            if !enhancing_extensions.contains(&ext) {
                enhancing_extensions.push(ext.clone());
            }
            Some(ext)
        } else {
            None
        };

        fields.push(ModelField {
            name: sf.name.clone(),
            field_type,
            required: sf.required,
            description: sf.description.clone(),
            default_value: sf.default_value.clone(),
            enum_values: sf.enum_values.clone(),
            is_primary_key: false,
            references,
            edge_label: sf.edge.clone(),
            contributed_by,
            contribution,
        });
    }

    enhancing_extensions.sort();

    ModelEntity {
        name: kind.name.clone(),
        extension: kind.source_extension.clone(),
        description: None,
        dot_color: kind.dot_color.clone(),
        fields,
        enhanced_by: enhancing_extensions,
    }
}

fn build_relationships(
    edge: &SchemaEdgeType,
    entity_map: &HashMap<&str, &ModelEntity>,
) -> Vec<ModelRelationship> {
    let source_kinds = match &edge.source_kinds {
        Some(kinds) => kinds,
        None => return Vec::new(),
    };
    let target_kinds = match &edge.target_kinds {
        Some(kinds) => kinds,
        None => return Vec::new(),
    };

    let mut relationships = Vec::new();

    for source_name in source_kinds {
        for target_name in target_kinds {
            // Pairs reference kinds that must exist in the model.
            let (Some(source_entity), Some(target_entity)) = (
                entity_map.get(source_name.as_str()),
                entity_map.get(target_name.as_str()),
            ) else {
                continue;
            };
            // C5-06: the manifest edge_type is the declaration; the fields
            // refine its cardinality (N:1, 1:1, 1:N) instead of every pair
            // defaulting to a blind N:M.
            let (cardinality, source_field) =
                infer_cardinality(&edge.label, source_entity, Some(target_entity))
                    .unwrap_or((Cardinality::ManyToMany, None));
            relationships.push(ModelRelationship {
                name: edge.label.clone(),
                source: source_name.clone(),
                target: target_name.clone(),
                cardinality,
                source_field,
                description: None,
            });
        }
    }

    relationships
}
