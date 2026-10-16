//! E016: a path a `file_reference` field names that does not exist, read
//! through the disk in the check's input (ADR 0107).

use std::path::Path;

use specforge_common::{Diagnostic, codes, find_close_match};

use specforge_graph::ValueShape;

use crate::FieldRegistry;
use crate::entity::{EntityRecord, RuleInput};

/// The paths `record` names in the `file_reference` fields its kind
/// declares, in field-name order, each field's last occurrence: the items of
/// a list of strings, or a single non-empty string. A field of the same name
/// on another kind is no file reference here.
fn paths<'a>(record: &'a EntityRecord, fields: &FieldRegistry) -> Vec<&'a str> {
    let mut names: Vec<&str> = fields
        .fields_for_kind(&record.kind)
        .into_iter()
        .filter(|entry| entry.declared().file_reference)
        .map(|entry| entry.name())
        .collect();
    names.sort_unstable();
    let mut paths = Vec::new();
    for name in names {
        let Some(field) = record.fields.iter().rev().find(|field| field.key == name) else {
            continue;
        };
        match field.shape {
            ValueShape::List => paths.extend(field.items.iter().flatten().map(String::as_str)),
            ValueShape::String if !field.text.is_empty() => paths.push(field.text.as_str()),
            _ => {}
        }
    }
    paths
}

/// E016 for every path a `file_reference` field names that does not exist
/// under `input.spec_root`, with a close sibling as the suggestion. Only a
/// missing path lists its directory.
pub(super) fn missing(input: &RuleInput<'_>, fields: &FieldRegistry) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    for record in input.entities {
        for path in paths(record, fields) {
            if input.disk.exists(&input.spec_root.join(path)) {
                continue;
            }
            let mut diagnostic = Diagnostic::new(
                codes::E016,
                format!(
                    "file reference '{}' in entity '{}' does not exist",
                    path, record.id
                ),
            )
            .with_span(record.span.clone());
            if let Some(suggestion) = suggest_similar_file(path, input) {
                diagnostic = diagnostic.with_suggestion(suggestion);
            }
            diagnostics.push(diagnostic);
        }
    }
    diagnostics
}

fn suggest_similar_file(path: &str, input: &RuleInput<'_>) -> Option<String> {
    let full_path = input.spec_root.join(path);
    let parent = full_path.parent()?;
    let siblings = input.disk.list(parent)?;
    let file_name = full_path.file_name()?.to_str()?;

    let match_name = find_close_match(file_name, siblings.iter().map(|s| s.as_str()))?;

    // Reconstruct the relative path with the suggested filename
    let path_obj = Path::new(path);
    let suggested = if let Some(dir) = path_obj.parent().filter(|p| !p.as_os_str().is_empty()) {
        format!("{}/{}", dir.display(), match_name)
    } else {
        match_name.to_string()
    };

    Some(format!("did you mean '{}'?", suggested))
}
