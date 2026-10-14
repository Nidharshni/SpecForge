//! Query, list and search: the read views that select entities of the
//! project view (ADR 0015, section "Query"). `specforge query` and
//! `specforge.query` render one query; `specforge.list` and
//! `specforge://entities/{kind}` one listing; `specforge.search` one
//! search. Each request holds its defaults; a kind filter reports the
//! kinds the project does not know (I020) and still drops them.

use serde_json::{Map, Value, json};
use specforge_common::Diagnostic;
use specforge_graph::Node;

use crate::Refusal;
use crate::export::{self, AGENT_FORMAT, ExportField, FORMAT, Format, Schema};
use crate::navigate::{
    self, EntityMatch, EntityQuery, MatchScope, MatchedOn, find_entities, snippet,
};
use crate::view::ProjectView;

/// Hops from the queried entity when a query names none: its direct
/// neighbours. `specforge query --depth` and `specforge.query`'s `depth`
/// default to it.
pub const DEFAULT_DEPTH: usize = 1;
/// Matches a search returns when it names no limit.
pub const DEFAULT_SEARCH_LIMIT: usize = 20;
/// Characters of a matched field's text shown on each side of the match.
pub const SNIPPET_WIDTH: usize = 40;

/// The field of a query request a refusal can be about (ADR 0072).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueryField {
    /// `QueryRequest::entity_id`.
    EntityId,
    /// `QueryRequest::format`.
    Format,
}

/// The subgraph around one entity.
#[derive(Debug, Clone, Default)]
pub struct QueryRequest<'a> {
    pub entity_id: &'a str,
    /// Hops from the entity; `None` is [`DEFAULT_DEPTH`]. 0 is the entity
    /// alone.
    pub depth: Option<usize>,
    /// Keep only entities of these kinds, the queried entity always, and
    /// an edge when both of its entities are kept. A kind the project does
    /// not know matches nothing and is a notice.
    pub kinds: Vec<&'a str>,
    /// `None` is [`AGENT_FORMAT`]'s default (graph). `dot` is refused: it
    /// is not an agent format.
    pub format: Option<Format>,
    /// Give every exported entity its coverage status (`coverage_status`,
    /// a [`crate::coverage::STATUS`] name).
    pub include_coverage: bool,
}

/// What a query answers.
#[derive(Debug, Clone, PartialEq)]
pub struct QueryOutcome {
    /// The export of the entity's subgraph (`export::export` of the scoped
    /// request under the export schema policy: a graph-format query
    /// references the published schema, Graph Protocol 2.0); with
    /// `include_coverage`, each node carrying its `coverage_status`, written
    /// by the export.
    pub document: String,
    /// I020 for each kind of the filter the project does not know.
    pub notices: Vec<Diagnostic>,
}

/// The query `request` over the view. An entity the graph lacks is
/// [`crate::navigate::not_found`] (E003, did-you-mean), refused before the
/// recorded report is read; a recorded report that cannot be read, when
/// coverage is asked for, is its E045 failure.
pub fn query(
    view: &ProjectView,
    request: &QueryRequest,
) -> Result<QueryOutcome, Refusal<QueryField>> {
    let format = request
        .format
        .or(AGENT_FORMAT.default)
        .expect("the agent formats have a default");
    if !AGENT_FORMAT.admits(format) {
        return Err(AGENT_FORMAT
            .parse(FORMAT.name_of(format))
            .expect_err("a format the agent table does not admit is refused")
            .about(QueryField::Format));
    }
    let notices = view.kinds().unknown_in(&request.kinds);
    let graph = view.graph();
    graph
        .node(request.entity_id)
        .ok_or_else(|| navigate::not_found(graph, request.entity_id).about(QueryField::EntityId))?;
    let coverage = request
        .include_coverage
        .then(|| view.coverage())
        .transpose()
        .map_err(Refusal::from)?;
    let document = export::export(
        view,
        &export::Request {
            format: Some(format),
            scope: Some(request.entity_id),
            depth: Some(request.depth.unwrap_or(DEFAULT_DEPTH)),
            kinds: request.kinds.clone(),
            schema: Schema::Default,
            coverage: coverage.as_deref(),
            ..export::Request::default()
        },
    )
    .map_err(|refusal| {
        refusal.map_subject(|field| match field {
            ExportField::Scope => Some(QueryField::EntityId),
            // A query always scopes and sends neither of these.
            ExportField::Depth | ExportField::MaxTokens | ExportField::SchemaVersion => None,
        })
    })?;
    Ok(QueryOutcome { document, notices })
}

/// The JSON Schema of the document [`query`] answers: the export document of
/// each agent format, derived by the emitter from the types that write it (a
/// node's optional `coverage_status` included). A brief document is also a
/// valid context document, so the formats are alternatives, not exclusive.
pub fn document_schema(defs: &mut specforge_common::shape::Definitions) -> Value {
    let branches: Vec<Value> = [Format::Graph, Format::Context, Format::Brief]
        .into_iter()
        .filter(|format| AGENT_FORMAT.admits(*format))
        .filter_map(|format| specforge_emitter::document_schema(format.emit_format(), defs))
        .collect();
    json!({ "type": "object", "anyOf": branches })
}

/// Entities of one kind (or every kind) whose fields hold given values.
#[derive(Debug, Clone, Default)]
pub struct ListRequest<'a> {
    /// `None` or `""`: every kind. A kind the project does not know lists
    /// nothing and is a notice.
    pub kind: Option<&'a str>,
    /// Field name to the value it holds, compared as the field's JSON
    /// (`specforge_graph::field_form::write`), any kind's fields.
    pub fields: Option<&'a Map<String, Value>>,
    /// Skip this many, after sorting by id.
    pub offset: usize,
    /// At most this many; `None` is every one.
    pub limit: Option<usize>,
}

/// What a listing answers: the entities sorted by id, paged.
#[derive(Debug, Clone)]
pub struct Listing<'v> {
    pub entities: Vec<&'v Node>,
    pub notices: Vec<Diagnostic>,
}

/// The entities `request` selects, sorted by id and paged.
pub fn list<'v>(view: &ProjectView<'v>, request: &ListRequest) -> Listing<'v> {
    let kind = request.kind.filter(|kind| !kind.is_empty());
    let notices = kind
        .map(|kind| view.kinds().unknown_in(&[kind]))
        .unwrap_or_default();
    let entities = view
        .graph()
        .nodes()
        .into_iter()
        .filter(|node| kind.is_none_or(|kind| node.kind.raw.as_str() == kind))
        .filter(|node| request.fields.is_none_or(|wanted| holds(node, wanted)))
        .skip(request.offset)
        .take(request.limit.unwrap_or(usize::MAX))
        .collect();
    Listing { entities, notices }
}

/// Whether every field of `wanted` is one `node` holds, with that value.
fn holds(node: &Node, wanted: &Map<String, Value>) -> bool {
    wanted.iter().all(|(field, value)| {
        node.fields
            .entries()
            .iter()
            .find(|entry| entry.key.as_str() == field)
            .is_some_and(|entry| {
                &Value::from(specforge_graph::field_form::write(&entry.value)) == value
            })
    })
}

/// A field and the text it must contain (ignoring case): both, or neither.
#[derive(Debug, Clone, Copy)]
pub struct FieldHolds<'a> {
    pub field: &'a str,
    pub value: &'a str,
}

/// Entities a text finds, ranked as the LSP ranks completion and
/// workspace symbols ([`find_entities`], over names and string fields),
/// every filter ANDed.
#[derive(Debug, Clone, Default)]
pub struct SearchRequest<'a> {
    /// Empty: every entity that passes the filters.
    pub text: &'a str,
    pub kinds: Vec<&'a str>,
    pub field: Option<FieldHolds<'a>>,
    /// Only entities that reference this entity.
    pub referencing: Option<&'a str>,
    /// `None` is [`DEFAULT_SEARCH_LIMIT`].
    pub limit: Option<usize>,
}

/// One match: how it matched, and for a field-text match the field's text
/// around it ([`SNIPPET_WIDTH`] characters each side).
#[derive(Debug, Clone)]
pub struct Hit<'v> {
    pub found: EntityMatch<'v>,
    pub snippet: Option<String>,
}

/// What a search answers.
#[derive(Debug, Clone)]
pub struct SearchOutcome<'v> {
    /// Best first, at most the limit.
    pub hits: Vec<Hit<'v>>,
    pub notices: Vec<Diagnostic>,
}

/// The entities `request` finds.
pub fn search<'v>(view: &ProjectView<'v>, request: &SearchRequest) -> SearchOutcome<'v> {
    let notices = view.kinds().unknown_in(&request.kinds);
    let query = EntityQuery {
        text: request.text,
        scope: MatchScope::NamesAndText,
        kinds: &request.kinds,
        field_contains: request.field.map(|held| (held.field, held.value)),
        referencing: request.referencing,
        limit: Some(request.limit.unwrap_or(DEFAULT_SEARCH_LIMIT)),
    };
    let hits = find_entities(view.graph(), &query)
        .into_iter()
        .map(|found| {
            let snippet = match found.on {
                MatchedOn::Field(field) => snippet(found.node, field, request.text, SNIPPET_WIDTH),
                _ => None,
            };
            Hit { found, snippet }
        })
        .collect();
    SearchOutcome { hits, notices }
}
