use super::{Cardinality, ModelEntity};
use specforge_registry::FieldType;

/// Infer cardinality for an edge by examining the source and target
/// entities' fields carrying `edge_label` (C5-06).
///
/// - Source has a single `Reference` field with the label:
///   **N:1** — many sources point at one target. If the target *also*
///   carries a single `Reference` back with the same label, it is **1:1**.
/// - Source has a `ReferenceList`: **N:M**.
/// - Source declares nothing, but the target has a single `Reference`
///   back with the label: the edge is the inverse of the target's ref —
///   one source owns many targets, **1:N**.
/// - No declaration on either side: `None` (the pair is fabricated).
///
/// Returns (cardinality, source_field_name).
pub fn infer_cardinality(
    edge_label: &str,
    source_entity: &ModelEntity,
    target_entity: Option<&ModelEntity>,
) -> Option<(Cardinality, Option<String>)> {
    let source_field = source_entity
        .fields
        .iter()
        .find(|f| !f.is_primary_key && f.edge_label.as_deref() == Some(edge_label));

    if let Some(field) = source_field {
        let cardinality = match field.field_type {
            FieldType::Reference => {
                let target_has_back_ref = target_entity.is_some_and(|t| {
                    t.fields.iter().any(|f| {
                        !f.is_primary_key
                            && f.edge_label.as_deref() == Some(edge_label)
                            && f.field_type == FieldType::Reference
                    })
                });
                if target_has_back_ref {
                    Cardinality::OneToOne
                } else {
                    Cardinality::ManyToOne
                }
            }
            _ => Cardinality::ManyToMany,
        };
        return Some((cardinality, Some(field.name.clone())));
    }

    // Source declares nothing: honor an inverse single reference on the
    // target — one source, many targets.
    let target_inverse = target_entity.and_then(|t| {
        t.fields
            .iter()
            .find(|f| !f.is_primary_key && f.edge_label.as_deref() == Some(edge_label))
    });
    match target_inverse.map(|f| f.field_type) {
        Some(FieldType::Reference) => Some((Cardinality::OneToMany, None)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ModelField;

    fn entity(fields: Vec<ModelField>) -> ModelEntity {
        ModelEntity {
            name: "e".to_string(),
            extension: "@test".to_string(),
            description: None,
            dot_color: None,
            fields,
            enhanced_by: vec![],
        }
    }

    fn ref_field(name: &str, label: &str, list: bool) -> ModelField {
        ModelField {
            name: name.to_string(),
            field_type: if list {
                FieldType::ReferenceList
            } else {
                FieldType::Reference
            },
            required: false,
            description: None,
            default_value: None,
            enum_values: None,
            is_primary_key: false,
            references: None,
            edge_label: Some(label.to_string()),
            contributed_by: None,
            contribution: None,
        }
    }

    #[test]
    fn single_reference_infers_many_to_one() {
        let src = entity(vec![ref_field("owner", "owns", false)]);
        let Some((card, field)) = infer_cardinality("owns", &src, None) else {
            panic!("single reference must infer");
        };
        assert_eq!(card, Cardinality::ManyToOne);
        assert_eq!(field.as_deref(), Some("owner"));
    }

    #[test]
    fn back_reference_on_both_sides_infers_one_to_one() {
        let src = entity(vec![ref_field("owner", "owns", false)]);
        let tgt = entity(vec![ref_field("owned", "owns", false)]);
        let Some((card, _)) = infer_cardinality("owns", &src, Some(&tgt)) else {
            panic!("mutual single references must infer 1:1");
        };
        assert_eq!(card, Cardinality::OneToOne);
    }

    #[test]
    fn reference_list_infers_many_to_many() {
        let src = entity(vec![ref_field("items", "owns", true)]);
        let Some((card, _)) = infer_cardinality("owns", &src, None) else {
            panic!("reference list must infer");
        };
        assert_eq!(card, Cardinality::ManyToMany);
    }

    #[test]
    fn inverse_single_reference_infers_one_to_many() {
        // Source declares nothing; the target's single back-reference makes
        // the edge the inverse of "many targets point at one source".
        let src = entity(vec![]);
        let tgt = entity(vec![ref_field("owner", "owns", false)]);
        let Some((card, field)) = infer_cardinality("owns", &src, Some(&tgt)) else {
            panic!("inverse single reference must infer");
        };
        assert_eq!(card, Cardinality::OneToMany);
        assert!(field.is_none());
    }

    #[test]
    fn no_declaration_on_either_side_returns_none() {
        let src = entity(vec![]);
        let tgt = entity(vec![]);
        assert!(infer_cardinality("owns", &src, Some(&tgt)).is_none());
    }
}
