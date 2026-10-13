//! E016: a path a `file_reference` field names that does not exist, and the
//! one derivation of those paths the session's check inputs share.

use std::path::Path;

use specforge_common::{Diagnostic, codes, find_close_match};

use specforge_graph::ValueShape;

use crate::FieldRegistry;
use crate::entity::EntityRecord;

/// The paths `record` names in the `file_reference` fields its kind
/// declares, in field-name order, each field's last occurrence: the items of
/// a list of strings, or a single non-empty string. A field of the same name
/// on another kind is no file reference here.
pub(super) fn paths<'a>(record: &'a EntityRecord, fields: &FieldRegistry) -> Vec<&'a str> {
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
            ValueShape::Strings => paths.extend(field.items.iter().flatten().map(String::as_str)),
            ValueShape::String if !field.text.is_empty() => paths.push(field.text.as_str()),
            _ => {}
        }
    }
    paths
}

/// E016 for every path a `file_reference` field names that does not exist
/// under `spec_root`, with a close sibling as the suggestion.
pub(super) fn missing(
    entities: &[EntityRecord],
    fields: &FieldRegistry,
    spec_root: &Path,
) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    for record in entities {
        for path in paths(record, fields) {
            if spec_root.join(path).exists() {
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
            if let Some(suggestion) = suggest_similar_file(path, spec_root) {
                diagnostic = diagnostic.with_suggestion(suggestion);
            }
            diagnostics.push(diagnostic);
        }
    }
    diagnostics
}

fn suggest_similar_file(path: &str, spec_root: &Path) -> Option<String> {
    let full_path = spec_root.join(path);
    let parent = full_path.parent()?;
    if !parent.is_dir() {
        return None;
    }

    let file_name = full_path.file_name()?.to_str()?;
    let siblings: Vec<String> = std::fs::read_dir(parent)
        .ok()?
        .filter_map(|e| e.ok())
        .filter_map(|e| e.file_name().into_string().ok())
        .collect();

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
