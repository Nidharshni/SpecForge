use specforge_ops::read::export::Format;
use specforge_ops::view::ProjectView;

use crate::resource::{CoreResource, ReadOutcome};
use crate::subscriptions::Watched;

/// `specforge://context`: the context export.
pub struct Resource;

impl CoreResource for Resource {
    const URI: &'static str = "specforge://context";
    const NAME: &'static str = "context";
    const DESCRIPTION: &'static str = "Context-optimized graph (contract, status, verify fields)";
    const CHANGES_WITH: Watched = Watched::Content;

    fn read(view: ProjectView<'_>, uri: &str) -> ReadOutcome {
        super::view::export_view(view, uri, Format::Context, None)
    }
}
