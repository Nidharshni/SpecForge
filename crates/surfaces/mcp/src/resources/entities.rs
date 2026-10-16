use specforge_ops::view::ProjectView;

use crate::resource::{CoreResource, ReadOutcome, ResourceText};
use crate::subscriptions::Watched;
use crate::tool::{ErrorCode, McpError};
use crate::uri_template::Matched;

/// `specforge://entities/{kind}`: what `specforge.list {kind}` lists, the
/// same rows from the same read view and presenter
/// ([`specforge_ops::read::query::list`], [`crate::tools::list::rows`]), as a bare
/// array (a resource is not a tool result).
pub struct EntitiesResource;

impl CoreResource for EntitiesResource {
    const URI: &'static str = "specforge://entities/{kind}";
    const NAME: &'static str = "entities_by_kind";
    const DESCRIPTION: &'static str = "All entities of a specific kind (e.g. feature, behavior)";
    const CHANGES_WITH: Watched = Watched::Content;

    fn read(view: ProjectView<'_>, uri: &Matched<'_>) -> ReadOutcome {
        super::view::no_query(uri)?;
        let kind = uri.value("kind").unwrap_or_default();
        let listing = specforge_ops::read::query::list(
            &view,
            &specforge_ops::read::query::ListRequest {
                kind: Some(kind),
                ..Default::default()
            },
        );
        let text = listing
            .try_map(|entities| serde_json::to_string(&crate::tools::list::rows(&entities)))
            .map_err(|error| {
                Box::new(McpError::new(ErrorCode::InternalError, error.to_string()))
            })?;
        Ok(ResourceText::noticed(text))
    }
}
