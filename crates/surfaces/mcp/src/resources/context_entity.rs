use specforge_ops::read::export::Format;
use specforge_ops::view::ProjectView;

use crate::resource::{CoreResource, ReadOutcome};
use crate::subscriptions::Watched;
use crate::uri_template::Matched;

/// `specforge://context/{entity_id}`: the context export rooted at an entity.
pub struct ContextEntityResource;

impl CoreResource for ContextEntityResource {
    const URI: &'static str = "specforge://context/{entity_id}";
    const NAME: &'static str = "context_entity";
    const DESCRIPTION: &'static str = "Context-optimized subgraph rooted at an entity";
    const CHANGES_WITH: Watched = Watched::Content;

    fn read(view: ProjectView<'_>, uri: &Matched<'_>) -> ReadOutcome {
        super::view::export_view(view, uri, Format::Context, Some("entity_id"))
    }
}
