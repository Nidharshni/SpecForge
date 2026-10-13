use specforge_ops::view::ProjectView;

use crate::resources::{ReadOutcome, ResourceText};
use crate::tool::{ErrorCode, McpError};

/// `specforge://schema`: the GraphProtocolSchema a full export embeds, the
/// same document `specforge.schema` returns unfiltered.
pub fn read(view: &ProjectView) -> ReadOutcome {
    let schema = view.versioned_schema();
    let schema_json = serde_json::to_string(schema.schema()).map_err(|err| {
        Box::new(McpError::new(
            ErrorCode::InternalError,
            format!("schema serialization failed: {err}"),
        ))
    })?;
    Ok(ResourceText::json(schema_json))
}
