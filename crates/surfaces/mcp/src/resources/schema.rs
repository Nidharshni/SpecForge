use specforge_ops::view::ProjectView;

use crate::resource::{CoreResource, ReadOutcome, ResourceText};
use crate::subscriptions::Watched;
use crate::tool::{ErrorCode, McpError};
use crate::uri_template::Matched;

/// `specforge://schema`: the GraphProtocolSchema a full export embeds, the
/// same document `specforge.schema` returns unfiltered.
pub struct SchemaResource;

impl CoreResource for SchemaResource {
    const URI: &'static str = "specforge://schema";
    const NAME: &'static str = "schema";
    const DESCRIPTION: &'static str = "Graph schema definition";
    const CHANGES_WITH: Watched = Watched::Schema;

    fn read(view: ProjectView<'_>, uri: &Matched<'_>) -> ReadOutcome {
        super::view::no_query(uri)?;
        let schema = view.versioned_schema();
        let schema_json = serde_json::to_string(schema.schema()).map_err(|err| {
            Box::new(McpError::new(
                ErrorCode::InternalError,
                format!("schema serialization failed: {err}"),
            ))
        })?;
        Ok(ResourceText::json(schema_json))
    }
}
