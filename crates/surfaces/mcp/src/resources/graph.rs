use specforge_ops::read::export::Format;
use specforge_ops::view::ProjectView;

use crate::resource::{CoreResource, ReadOutcome};
use crate::subscriptions::Watched;
use crate::uri_template::Matched;

/// `specforge://graph`: the whole graph export.
pub struct Resource;

impl CoreResource for Resource {
    const URI: &'static str = "specforge://graph";
    const NAME: &'static str = "graph";
    const DESCRIPTION: &'static str = "Full spec graph in JSON format";
    const CHANGES_WITH: Watched = Watched::Graph;

    fn read(view: ProjectView<'_>, uri: &Matched<'_>) -> ReadOutcome {
        super::view::export_view(view, uri, Format::Graph, None)
    }
}
