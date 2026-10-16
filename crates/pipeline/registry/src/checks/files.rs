//! E016: a path a `file_reference` field names that does not exist, read
//! through the disk in the check's input (ADR 0107). The paths a field names
//! are read by the rules' one reader (ADR 0118).

use specforge_common::{Diagnostic, codes};

use crate::FieldRegistry;
use crate::entity::RuleInput;
use crate::rules::files;

/// E016 for every path a `file_reference` field names that does not exist
/// under `input.spec_root`, with a close sibling as the suggestion. Entities
/// in record order, each kind's file-reference fields in name order. Only a
/// missing path lists its directory. A field of the same name on another kind
/// is no file reference here.
pub(super) fn missing(input: &RuleInput<'_>, fields: &FieldRegistry) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    for record in input.entities {
        let mut names: Vec<&str> = fields
            .fields_for_kind(&record.kind)
            .into_iter()
            .filter(|entry| entry.declared().file_reference)
            .map(|entry| entry.name())
            .collect();
        names.sort_unstable();
        for name in names {
            for missing in files::missing(record, name, input) {
                let mut diagnostic = Diagnostic::new(
                    codes::E016,
                    format!(
                        "file reference '{}' in entity '{}' does not exist",
                        missing.path, record.id
                    ),
                )
                .with_span(record.span.clone());
                if let Some(suggestion) = missing.suggestion {
                    diagnostic = diagnostic.with_suggestion(suggestion);
                }
                diagnostics.push(diagnostic);
            }
        }
    }
    diagnostics
}
