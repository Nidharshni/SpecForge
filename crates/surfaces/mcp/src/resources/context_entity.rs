use specforge_ops::read::export::Format;
use specforge_ops::view::ProjectView;

use crate::resource::{CoreResource, ReadOutcome};
use crate::subscriptions::Watched;
use crate::uri_template::Matched;

/// `specforge://context/{entity_id}`: the context export scoped to an entity, as `context?scope=` is:
/// everything it reaches.
pub struct ContextEntityResource;

impl CoreResource for ContextEntityResource {
    const URI: &'static str = "specforge://context/{entity_id}";
    const NAME: &'static str = "context_entity";
    const DESCRIPTION: &'static str =
        "Context-optimized subgraph of everything an entity reaches (a depth query narrows it)";
    const CHANGES_WITH: Watched = Watched::Content;

    fn read(view: ProjectView<'_>, uri: &Matched<'_>) -> ReadOutcome {
        super::view::export_view(view, uri, Format::Context, Some("entity_id"))
    }
}
