//! W159: a value written in the other form than its field's declared type
//! gives it (ADR 0147); a value a defaults block writes once, at the default, naming
//! `defaults <kind>` (ADR 0224).

use std::collections::HashSet;

use specforge_common::{Diagnostic, SourceSpan, codes};
use specforge_graph::field_form::spell;
use specforge_parser::Written;

use crate::entity::EntityRecord;
use crate::{FieldRegistry, KindRegistry};

/// W159 for every value written in the other form than its field's, one per
/// value, at the value; a field E061 refuses gets none (the error is the one
/// to fix first). A value a defaults block writes reaches each block that takes it at the default's own span
/// (ADR 0149 D9): one written value, one W159, naming `defaults <kind>` (ADR 0224).
pub(super) fn off_form(
    entities: &[EntityRecord],
    kinds: &KindRegistry,
    fields: &FieldRegistry,
) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let mut defaults_reported: HashSet<&SourceSpan> = HashSet::new();
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
                let holder = if inherited(&record.span, &rewrite.span) {
                    if !defaults_reported.insert(&rewrite.span) {
                        continue;
                    }
                    format!("defaults {}", record.kind)
                } else {
                    format!("{} '{}'", record.kind, record.id)
                };
                let (what, fix) = match rewrite.to {
                    Written::Bare => ("a word", "write it bare"),
                    _ => ("text", "quote it"),
                };
                let as_written = spell::spelled(&rewrite.text, rewrite.to);
                let mut diagnostic = Diagnostic::new(
                    codes::W159,
                    format!(
                        "field '{}' of {holder} is {what}: {fix}, as {} {as_written}",
                        field.key, field.key
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

/// Whether `value` lies outside `block`: an inherited entry keeps its default's spans (ADR 0149 D9).
fn inherited(block: &SourceSpan, value: &SourceSpan) -> bool {
    value.file != block.file
        || (value.start_line, value.start_col) < (block.start_line, block.start_col)
        || (value.end_line, value.end_col) > (block.end_line, block.end_col)
}
