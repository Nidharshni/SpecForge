//! E061: a field value that cannot be the type its extension declared.

use specforge_common::{Diagnostic, codes};

use specforge_graph::ValueShape;

use crate::entity::{EntityRecord, FieldRecord};
use crate::{FieldRegistry, FieldRegistryEntry, KindRegistry};

/// E061 for every registered field whose (coerced) value its field form
/// refuses: a scalar that is not an integer, true/false or a declared enum
/// value; a list on a single-value field; a block or an
/// expression where the type never takes one (a reference or list field);
/// a scalar on a block field. Unregistered kinds and fields are left to
/// E024 / W020. Each is reported at the value's own
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
    let refusal = declared.form().admit(field.shape, &field.text).err()?;
    Some(Mismatch {
        given: describe(field),
        suggestion: refusal.suggestion,
    })
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
        ValueShape::List | ValueShape::Variants => "a list".to_string(),
        ValueShape::Block => "a block".to_string(),
        ValueShape::Expression => "an expression".to_string(),
    }
}
