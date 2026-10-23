use crate::target::View;
use crate::tool::{CoreTool, Effect, ReplySpec, ToolGroup};
use serde::Serialize;
use specforge_common::shape::Shape;
use specforge_ops::read::query::{FieldHolds, Hit, SearchField, SearchRequest, search};

use crate::args::Arguments;
use crate::refusal::ArgumentOf;
use crate::reply::{Answer, Answered};
use crate::tool::McpError;
use specforge_ops::view::ProjectView;

/// `specforge.search`'s arguments.
#[derive(Debug, Arguments)]
pub struct Args {
    /// Search query
    query: String,
    /// Filter by kinds
    kinds: Vec<String>,
    /// Max results
    #[arg(default = specforge_ops::read::query::DEFAULT_SEARCH_LIMIT)]
    limit: usize,
    /// Only entities whose field of this name contains `value`; needs `value`
    field: Option<String>,
    /// With `field`: text the field contains, ignoring case; needs `field`
    value: Option<String>,
    /// Only entities that reference this entity ID (combined with the other filters)
    references: Option<String>,
}

/// `specforge.search`'s reply: the hits, best first.
#[derive(Debug, Serialize, Shape)]
pub struct Reply {
    results: Vec<Found>,
}

/// One hit (`McpSearchResult`).
#[derive(Debug, Serialize, Shape)]
pub struct Found {
    entity_id: String,
    kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    title: Option<String>,
    file_path: String,
    line: usize,
    score: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    match_field: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    match_snippet: Option<String>,
}

impl Found {
    fn of(hit: &Hit) -> Self {
        let found = &hit.found;
        Found {
            entity_id: found.node.id.raw.to_string(),
            kind: found.node.kind.raw.to_string(),
            title: found.node.title.as_ref().map(ToString::to_string),
            file_path: found.node.source_span.file.to_string(),
            line: found.node.source_span.start_line,
            score: found.score,
            match_field: found.on.field_name().map(ToString::to_string),
            match_snippet: hit.snippet.clone(),
        }
    }
}

/// `specforge.search`: the search read view (`specforge_ops::read::query::search`),
/// the entities the query finds, ranked as the LSP's workspace symbols and
/// completion rank them, over names and field text. Every filter is
/// ANDed: kinds, a field's text (`field` with `value`: one without the other
/// is refused), and `references` (the entities that reference that one; one the
/// graph lacks is `entity_not_found` on `references`).
pub(crate) struct Tool;

impl ArgumentOf<SearchField> for Tool {
    fn argument(field: SearchField) -> Option<&'static str> {
        match field {
            SearchField::Referencing => Some("references"),
        }
    }
}

impl CoreTool for Tool {
    const NAME: &'static str = "specforge.search";
    const DESCRIPTION: &'static str = "Find entities by id, title or field text, ranked as the LSP ranks them (exact, prefix, substring, field text, then fuzzy)";
    const EFFECT: Effect = Effect::Reads {
        group: ToolGroup::Core,
    };
    const SPEC: ReplySpec = ReplySpec::Type("McpSearchResults");
    type Args = Args;
    type Input = View;
    type Returns = Answered<Reply>;

    fn call(view: ProjectView<'_>, args: Args) -> Answered<Reply> {
        let field = match (args.field.as_deref(), args.value.as_deref()) {
            (Some(field), Some(value)) => Some(FieldHolds { field, value }),
            (None, None) => None,
            (Some(_), None) => {
                return Err(Box::new(McpError::invalid_input(
                    "value",
                    "'field' needs 'value': the text the field must contain",
                )));
            }
            (None, Some(_)) => {
                return Err(Box::new(McpError::invalid_input(
                    "field",
                    "'value' needs 'field': the field whose text it must be in",
                )));
            }
        };
        let request = SearchRequest {
            text: &args.query,
            kinds: args.kinds.iter().map(String::as_str).collect(),
            field,
            referencing: args.references.as_deref(),
            limit: Some(args.limit),
        };
        Ok(Answer::noticed(
            search(&view, &request)
                .map_err(Self::refused)?
                .map(|hits| Reply {
                    results: hits.iter().map(Found::of).collect(),
                }),
        ))
    }
}
