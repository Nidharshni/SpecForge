//! W159: a value written in the other form than its field's declared type
//! gives it (ADR 0147).

use specforge_common::{Diagnostic, codes};
use specforge_parser::Written;

use crate::entity::EntityRecord;
use crate::{FieldRegistry, KindRegistry};

/// W159 for every value written in the other form than its field's, one per
/// value, at the value; a field E061 refuses gets none (the error is the one
/// to fix first).
pub(super) fn off_form(
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
            if field.rewrites.is_empty() || declared.form().admit(field.shape, &field.text).is_err()
            {
                continue;
            }
            for rewrite in &field.rewrites {
                let (what, fix, as_written) = match rewrite.to {
                    Written::Bare => ("a word", "write it bare", rewrite.text.clone()),
                    _ => ("text", "quote it", format!("\"{}\"", rewrite.text)),
                };
                let mut diagnostic = Diagnostic::new(
                    codes::W159,
                    format!(
                        "field '{}' of {} '{}' is {what}: {fix}, as {} {as_written}",
                        field.key, record.kind, record.id, field.key
                    ),
                )
                .with_span(rewrite.span.clone());
                diagnostic.suggestion = Some(
                    "run specforge format (or format the document) to write every value in its field's form"
                        .to_string(),
                );
                diagnostics.push(diagnostic);
            }
        }
    }
    diagnostics
}
