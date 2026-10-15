use specforge_ops::view::ProjectView;

use crate::resource::{CoreResource, ReadOutcome, ResourceText};
use crate::subscriptions::Watched;
use crate::tool::{ErrorCode, McpError};

/// `specforge://entities/{kind}`: what `specforge.list {kind}` lists, the
/// same rows from the same read view and presenter
/// ([`specforge_ops::read::query::list`], [`crate::tools::list::rows`]), as a bare
/// array (a resource is not a tool result).
pub struct Resource;

impl CoreResource for Resource {
    const URI: &'static str = "specforge://entities/{kind}";
    const NAME: &'static str = "entities_by_kind";
    const DESCRIPTION: &'static str = "All entities of a specific kind (e.g. feature, behavior)";
    const CHANGES_WITH: Watched = Watched::Content;

    fn read(view: ProjectView<'_>, uri: &str) -> ReadOutcome {
        let (path, _) = uri.split_once('?').unwrap_or((uri, ""));
        super::view::no_query(uri)?;
        let kind = super::view::decode(
            "kind",
            path.strip_prefix(super::view::prefix(Self::URI))
                .unwrap_or_default(),
        )?;
        // A resource has no `_meta`: the listing's notices (an unknown kind)
        // are not carried.
        let listing = specforge_ops::read::query::list(
            &view,
            &specforge_ops::read::query::ListRequest {
                kind: Some(&kind),
                ..Default::default()
            },
        );
        let text = serde_json::to_string(&crate::tools::list::rows(&listing)).map_err(|error| {
            Box::new(McpError::new(ErrorCode::InternalError, error.to_string()))
        })?;
        Ok(ResourceText::json(text))
    }
}
