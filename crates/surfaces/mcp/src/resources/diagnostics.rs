use specforge_ops::view::ProjectView;

use crate::resource::{CoreResource, ReadOutcome, ResourceText};
use crate::subscriptions::Watched;

/// `specforge://diagnostics`: what the server reports for the call's
/// project, as `specforge check --format json` writes it.
pub struct Resource;

impl CoreResource for Resource {
    const URI: &'static str = "specforge://diagnostics";
    const NAME: &'static str = "diagnostics";
    const DESCRIPTION: &'static str = "Current compilation diagnostics";
    const CHANGES_WITH: Watched = Watched::Diagnostics;

    fn read(view: ProjectView<'_>, uri: &str) -> ReadOutcome {
        super::view::no_query(uri)?;
        Ok(ResourceText::json(specforge_common::serialize_diagnostics(
            &view.reported(),
        )))
    }
}
