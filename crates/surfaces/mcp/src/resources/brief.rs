use specforge_ops::read::export::Format;
use specforge_ops::view::ProjectView;

use crate::resource::{CoreResource, ReadOutcome};
use crate::subscriptions::Watched;
use crate::uri_template::Matched;

/// `specforge://brief`: the brief export.
pub struct Resource;

impl CoreResource for Resource {
    const URI: &'static str = "specforge://brief";
    const NAME: &'static str = "brief";
    const DESCRIPTION: &'static str = "Brief graph (id, kind, title, edges only)";
    const CHANGES_WITH: Watched = Watched::Content;

    fn read(view: ProjectView<'_>, uri: &Matched<'_>) -> ReadOutcome {
        super::view::export_view(view, uri, Format::Brief, None)
    }
}
