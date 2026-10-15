use specforge_ops::read::export::Format;
use specforge_ops::view::ProjectView;

use crate::resource::{CoreResource, ReadOutcome};
use crate::subscriptions::Watched;
use crate::uri_template::Matched;

/// `specforge://graph/{entity_id}`: the entity and its neighbours, the
/// scoped graph export at depth 1 (a `depth` query widens it). A malformed
/// ID is `invalid_input` (the 400 case); a well-formed one that names no
/// entity is the export's `entity_not_found` with its E003 (the 404 case).
pub struct Resource;

impl CoreResource for Resource {
    const URI: &'static str = "specforge://graph/{entity_id}";
    const NAME: &'static str = "entity";
    const DESCRIPTION: &'static str = "Subgraph rooted at a specific entity";
    const CHANGES_WITH: Watched = Watched::Graph;

    fn read(view: ProjectView<'_>, uri: &Matched<'_>) -> ReadOutcome {
        let mut query = super::view::ViewQuery::parse(uri, Some("entity_id"))?;
        super::view::check_entity_id(query.scope.as_deref().unwrap_or_default())?;
        // The entity and its immediate neighbors, not everything reachable.
        query.depth.get_or_insert(1);
        super::view::exported(view, &query, Format::Graph)
    }
}
