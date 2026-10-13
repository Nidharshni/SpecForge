//! E061: a field value that cannot be the type its extension declared.

use specforge_common::{Diagnostic, codes, find_close_match};

use specforge_graph::ValueShape;

use crate::entity::{EntityRecord, FieldRecord};
use crate::{FieldRegistry, FieldRegistryEntry, FieldType, KindRegistry};

/// E061 for every registered field whose (coerced) value can't be its
/// declared type: not an integer, not true/false, not a declared enum
/// value, or a list where a single value is declared. Unregistered kinds
/// and fields are left to E024 / W020. Each is reported at the value's own
/// span, else the entity's.
pub(super) fn mistyped(
    entities: &[EntityRecord],
    kinds: &KindRegistry,
    fields: &FieldRegistry,
) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    for record in entities {
        if !kinds.contains(&record.kind) {
            continue;
        }
        for field in &record.fields {
            let Some(declared) = fields.get(&record.kind, &field.key) else {
                continue;
            };
            let Some(mismatch) = mismatch(declared, field) else {
                continue;
            };
            let mut diagnostic = Diagnostic::new(
                codes::E061,
                format!(
                    "field '{}' of {} '{}' is declared {}, but was given {}",
                    field.key,
                    record.kind,
                    record.id,
                    declared.type_label(),
                    mismatch.given
                ),
            )
            .with_span(
                field
                    .value_span
                    .clone()
                    .unwrap_or_else(|| record.span.clone()),
            );
            diagnostic.suggestion = mismatch.suggestion;
            diagnostics.push(diagnostic);
        }
    }
    diagnostics
}

struct Mismatch {
    given: String,
    suggestion: Option<String>,
}

fn mismatch(declared: &FieldRegistryEntry, field: &FieldRecord) -> Option<Mismatch> {
    let wrong = |suggestion: Option<String>| {
        Some(Mismatch {
            given: describe(field),
            suggestion,
        })
    };
    let shape = field.shape;
    match declared.field_type() {
        FieldType::Integer => match shape {
            ValueShape::Integer => None,
            _ => wrong(None),
        },
        FieldType::Bool => match shape {
            ValueShape::Boolean => None,
            _ => wrong(Some("use true or false".to_string())),
        },
        FieldType::Enum if !declared.enum_values().is_empty() => {
            let values = declared.enum_values();
            match shape {
                ValueShape::String | ValueShape::Identifier if values.contains(&field.text) => None,
                ValueShape::String | ValueShape::Identifier => wrong(Some(match find_close_match(
                    &field.text,
                    values.iter().map(String::as_str),
                ) {
                    Some(close) => format!("did you mean '{close}'?"),
                    None => format!("use one of: {}", values.join(", ")),
                })),
                _ => wrong(Some(format!("use one of: {}", values.join(", ")))),
            }
        }
        FieldType::String | FieldType::Enum | FieldType::Reference if shape.is_list() => {
            wrong(Some("give a single value, not a list".to_string()))
        }
        _ => None,
    }
}

/// The value as the message quotes it.
fn describe(field: &FieldRecord) -> String {
    match field.shape {
        ValueShape::String => format!("\"{}\"", field.text),
        ValueShape::Identifier
        | ValueShape::Date
        | ValueShape::Integer
        | ValueShape::Boolean
        | ValueShape::TypeUnion => field.text.clone(),
        ValueShape::Strings | ValueShape::References | ValueShape::Mixed | ValueShape::Variants => {
            "a list".to_string()
        }
        ValueShape::Block => "a block".to_string(),
        ValueShape::Verify => "verify statements".to_string(),
        ValueShape::Expression => "an expression".to_string(),
    }
}
