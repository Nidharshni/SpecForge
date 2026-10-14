use crate::target::View;
use crate::tool::{CoreTool, Effect, ReplySpec, ToolGroup};
use serde::Serialize;
use serde_json::{Map, Value};
use specforge_common::shape::Shape;
use specforge_ops::read::query::{ListRequest, Listing, list};

use crate::args::Arguments;
use crate::reply::{Answer, Answered};
use specforge_ops::view::ProjectView;

/// `specforge.list`'s arguments. A `where`, `limit` or `offset` of the wrong
/// type is invalid input, an `isError` result (ADR 0004 D4-a), as every
/// argument of a wrong type is: never a silently unfiltered list.
#[derive(Debug, Arguments)]
pub struct Args {
    /// Filter by entity kind (e.g. 'feature', 'behavior')
    kind: Option<String>,
    /// Only entities whose fields hold these values, e.g. {"status": "done"}
    r#where: Option<Map<String, Value>>,
    /// Return at most this many entities
    limit: Option<usize>,
    /// Skip this many entities first
    offset: Option<usize>,
}

/// `specforge.list`'s reply: the entities, sorted by id.
#[derive(Debug, Serialize, Shape)]
pub struct Reply {
    entities: Vec<Listed>,
}

/// One entity of a listing (`McpListedEntity`).
#[derive(Debug, Serialize, Shape)]
pub struct Listed {
    id: String,
    kind: String,
    /// The entity's title; empty when it has none.
    title: String,
}

/// The rows of a listing: the one presenter of it, the tool's and
/// `specforge://entities/{kind}`'s (which answers the bare array).
pub(crate) fn rows(listing: &Listing) -> Vec<Listed> {
    listing
        .entities
        .iter()
        .map(|node| Listed {
            id: node.id.raw.to_string(),
            kind: node.kind.raw.to_string(),
            title: node.title.as_deref().unwrap_or("").to_string(),
        })
        .collect()
}

/// `specforge.list`: the list read view (`specforge_ops::read::query::list`), the
/// entities of `kind` (every entity without one) whose fields hold what
/// `where` asks, sorted by id, then paged by `offset` and `limit`. Domain
/// free: any kind, any field (an extension's own list commands, such as
/// `specforge.product.features`, render their kinds their way). A kind the
/// project does not know lists nothing and is reported (I020).
pub(crate) struct Tool;

impl CoreTool for Tool {
    const NAME: &'static str = "specforge.list";
    const DESCRIPTION: &'static str =
        "List entities sorted by id, optionally filtered by kind and field values, and paged";
    const EFFECT: Effect = Effect::Reads {
        group: ToolGroup::Core,
    };
    const SPEC: ReplySpec = ReplySpec::Type("McpListResult");
    type Args = Args;
    type Input = View;
    type Returns = Answered<Reply>;

    fn call(view: ProjectView<'_>, args: Args) -> Answered<Reply> {
        let request = ListRequest {
            kind: args.kind.as_deref(),
            fields: args.r#where.as_ref(),
            offset: args.offset.unwrap_or(0),
            limit: args.limit,
        };
        let listing = list(&view, &request);
        Ok(Answer::new(Reply {
            entities: rows(&listing),
        })
        .with_diagnostics(listing.notices))
    }
}
