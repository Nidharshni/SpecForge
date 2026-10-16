//! The paths an entity's field names, and the close sibling of a missing one (ADR 0118): the one
//! reading every `file_exists` rule, E016 among them, shares.

use std::path::Path;

use specforge_common::find_close_match;
use specforge_graph::ValueShape;

use crate::entity::{EntityRecord, RuleInput};

/// One path a field names that does not exist under the spec root.
pub(super) struct Missing<'r> {
    /// As the entity wrote it: the scalar's text, or the list item.
    pub path: &'r str,
    /// `did you mean '<dir>/<name>'?`: a close sibling in the missing path's directory.
    pub suggestion: Option<String>,
}

/// The paths `record` names in `field`, in order, that `input.disk` says do not exist under
/// `input.spec_root`. A relative path is the spec root's; an absolute one is checked as written
/// (`join` keeps it). One `exists` per named path; only a missing one lists its directory.
pub(super) fn missing<'r>(
    record: &'r EntityRecord,
    field: &str,
    input: &RuleInput<'_>,
) -> Vec<Missing<'r>> {
    named(record, field)
        .into_iter()
        .filter(|path| !input.disk.exists(&input.spec_root.join(path)))
        .map(|path| Missing {
            path,
            suggestion: sibling(path, input),
        })
        .collect()
}

/// The paths `record` names in `field`: its last occurrence's text when it is a scalar, or each
/// item when it is a list, never an empty one. A block, an expression group, a variant list and a
/// type union name none; a joined text is never split (ADR 0019).
fn named<'r>(record: &'r EntityRecord, field: &str) -> Vec<&'r str> {
    let Some(written) = record.fields.iter().rev().find(|f| f.key == field) else {
        return Vec::new();
    };
    let names: Vec<&str> = match written.shape {
        ValueShape::String
        | ValueShape::Identifier
        | ValueShape::Date
        | ValueShape::Integer
        | ValueShape::Boolean => vec![written.text.as_str()],
        ValueShape::List => written.items.iter().flatten().map(String::as_str).collect(),
        ValueShape::Block
        | ValueShape::Expression
        | ValueShape::Variants
        | ValueShape::TypeUnion => Vec::new(),
    };
    names.into_iter().filter(|name| !name.is_empty()).collect()
}

/// The close sibling of `path` in its directory, as the help a missing path carries.
fn sibling(path: &str, input: &RuleInput<'_>) -> Option<String> {
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
