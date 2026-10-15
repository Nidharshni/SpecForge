use specforge_ops::read::export::Format;
use specforge_ops::view::ProjectView;

use crate::resource::{CoreResource, ReadOutcome};
use crate::subscriptions::Watched;
use crate::uri_template::Matched;

/// `specforge://context`: the context export.
pub struct ContextResource;

impl CoreResource for ContextResource {
    const URI: &'static str = "specforge://context";
    const NAME: &'static str = "context";
    const DESCRIPTION: &'static str = "Context-optimized graph (contract, status, verify fields)";
    const CHANGES_WITH: Watched = Watched::Content;

    fn read(view: ProjectView<'_>, uri: &Matched<'_>) -> ReadOutcome {
        super::view::export_view(view, uri, Format::Context, None)
    }
}
