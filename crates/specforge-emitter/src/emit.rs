use std::borrow::Cow;

use specforge_graph::Graph;
use specforge_registry::{FieldRegistry, KindRegistry};

use crate::budget::TokenBudget;
use crate::error::EmitterError;
use crate::json::{Export, ExportVersion};
use crate::schema::{GraphProtocolSchema, SchemaAttachment, SchemaRefBlock};

/// Output format for graph emission.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmitFormat {
    /// Full JSON with all fields, file locations, and line numbers.
    Json,
    /// Agent-optimized: title, headline fields, normative fields, verify.
    Context,
    /// Minimal: id, kind, title only.
    Brief,
    /// Graphviz DOT format.
    Dot,
}

/// Options controlling graph emission.
///
/// Use `EmitOptions::default()` for full JSON, then customize:
/// ```ignore
/// let options = EmitOptions { format: EmitFormat::Context, ..Default::default() };
/// let output = emit(&graph, &options)?;
/// ```
#[derive(Debug, Clone)]
pub struct EmitOptions<'a> {
    /// Output format (default: Json).
    pub format: EmitFormat,
    /// Scope to a subgraph rooted at this entity (default: None = full graph).
    pub scope: Option<&'a str>,
    /// Embed schema in output for V2 format (default: None = V1 format).
    pub schema: Option<&'a GraphProtocolSchema>,
    /// Token budget for truncated output (default: None = no limit).
    pub token_budget: Option<usize>,
    /// Maximum traversal depth from the scoped entity (default: None = unlimited).
    /// Only meaningful when `scope` is set.
    pub depth: Option<usize>,
    /// Filter output to only include nodes of these kinds (default: empty = all kinds).
    /// The scoped root entity is always included regardless of this filter.
    pub kind_filter: Vec<&'a str>,
    /// Kind registry for style lookups (DOT shape/color/fillcolor declared by
    /// extensions; C13-00). Default: None — nodes use built-in defaults.
    pub kind_registry: Option<&'a KindRegistry>,
    /// Field registry, so the context format keeps each entity's normative
    /// and headline fields. Default: None — context carries only title/verify.
    pub field_registry: Option<&'a FieldRegistry>,
}

impl Default for EmitOptions<'_> {
    fn default() -> Self {
        Self {
            format: EmitFormat::Json,
            scope: None,
            schema: None,
            token_budget: None,
            depth: None,
            kind_filter: Vec::new(),
            kind_registry: None,
            field_registry: None,
        }
    }
}

/// Unified graph emission: the one entry point for every export, and the one
/// tests use.
///
/// `scope` (with `depth`) and `kind_filter` select the subgraph; a scoped
/// export references the schema instead of embedding it. `format` and `schema`
/// choose the shape (graph 1.0, or 2.0 with the schema; context and brief, 2.0
/// when a schema is given). Under `token_budget` every format but DOT is fitted
/// by `budget::fit`: the whole export when it fits; otherwise the least
/// central entities are dropped, with their edges, until it fits, and the
/// export says which in its `token_budget` block. An embedded schema counts
/// and is never cut: a schema over the budget is E062, as is a budget below the
/// export with no entities. DOT is not budgeted.
///
/// Err: `ScopeNotFound` (E003) for a scope naming no entity; `BudgetTooSmall`
/// (E062) for a budget too small.
pub fn emit(graph: &Graph, options: &EmitOptions<'_>) -> Result<String, EmitterError> {
    let selected = select(graph, options)?;
    if options.format == EmitFormat::Dot {
        return Ok(crate::dot::emit_dot(
            &selected,
            &crate::dot::DotOptions {
                kind_registry: options.kind_registry,
                ..Default::default()
            },
        ));
    }

    // Scoped exports reference the published schema instead of embedding it
    // (C6-07); full exports embed.
    let attach = if options.scope.is_some() {
        SchemaAttachment::Referenced
    } else {
        SchemaAttachment::Embedded
    };
    let shape = Shape {
        format: options.format,
        schema: options.schema.map(|schema| (schema, attach)),
        fields: options.field_registry,
    };
    match options.token_budget {
        None => shape.render(&selected, None),
        Some(max_tokens) => {
            crate::budget::fit(&selected, max_tokens, shape.schema_cost(), |g, budget| {
                shape.render(g, budget)
            })
        }
    }
}

/// The JSON Schema of the document [`emit`] writes in `format`, derived from
/// the types that write it (every object closed; a context node open to the
/// headline fields its extension declares). `None` for DOT, which is text.
pub fn document_schema(
    format: EmitFormat,
    defs: &mut specforge_common::shape::Definitions,
) -> Option<serde_json::Value> {
    use specforge_common::shape::Shape;
    match format {
        EmitFormat::Json => Some(Export::<crate::json::JsonNode>::schema(defs)),
        EmitFormat::Context => Some(Export::<crate::context::ContextNode>::schema(defs)),
        EmitFormat::Brief => Some(Export::<crate::brief::BriefNode>::schema(defs)),
        EmitFormat::Dot => None,
    }
}

/// The subgraph an export covers: the scope's (to `depth` when given), then
/// only the kinds in `kind_filter`, the scope's own entity always kept.
fn select<'g>(graph: &'g Graph, options: &EmitOptions<'_>) -> Result<Cow<'g, Graph>, EmitterError> {
    let scoped = match options.scope {
        None => Cow::Borrowed(graph),
        Some(scope) => {
            let sub = match options.depth {
                Some(depth) => graph.subgraph_depth(scope, depth),
                None => graph.subgraph(scope),
            };
            Cow::Owned(sub.ok_or_else(|| EmitterError::ScopeNotFound {
                entity_id: scope.to_string(),
            })?)
        }
    };
    if options.kind_filter.is_empty() {
        return Ok(scoped);
    }
    let mut filtered = Graph::new();
    for node in scoped.nodes() {
        if Some(node.id.raw.as_str()) == options.scope
            || options.kind_filter.contains(&node.kind.raw.as_str())
        {
            filtered.add_node(node.clone());
        }
    }
    for edge in scoped.edges() {
        if filtered.node(edge.source.as_str()).is_some()
            && filtered.node(edge.target.as_str()).is_some()
        {
            filtered.add_edge(*edge);
        }
    }
    Ok(Cow::Owned(filtered))
}

/// How one export is written: its format, its schema and how it is attached,
/// and the field registry the context format reads.
struct Shape<'a> {
    /// Never DOT: it is rendered before a `Shape` exists.
    format: EmitFormat,
    schema: Option<(&'a GraphProtocolSchema, SchemaAttachment)>,
    fields: Option<&'a FieldRegistry>,
}

impl Shape<'_> {
    /// `graph` in this shape, with `budget` as its `token_budget` block when
    /// given: the one format x schema table.
    fn render(&self, graph: &Graph, budget: Option<&TokenBudget>) -> Result<String, EmitterError> {
        match self.format {
            EmitFormat::Json => self
                .envelope(graph, Some(ExportVersion::V1), graph_nodes(graph), budget)
                .to_json(),
            EmitFormat::Context => self
                .envelope(graph, None, context_nodes(graph, self.fields), budget)
                .to_json(),
            EmitFormat::Brief => self
                .envelope(graph, None, brief_nodes(graph), budget)
                .to_json(),
            EmitFormat::Dot => unreachable!("DOT is rendered before a Shape exists"),
        }
    }

    /// `nodes` and `graph`'s edges in this shape's envelope: `plain_version`
    /// when there is no schema, "2.0" when there is.
    fn envelope<'e, N: serde::Serialize>(
        &'e self,
        graph: &Graph,
        plain_version: Option<ExportVersion>,
        nodes: Vec<N>,
        budget: Option<&'e TokenBudget>,
    ) -> Export<'e, N> {
        let mut export = Export::plain(plain_version, graph, nodes);
        if let Some((schema, attach)) = self.schema {
            export.format_version = Some(ExportVersion::V2);
            export.schema_version = Cow::Owned(schema.schema_version.to_string());
            match attach {
                SchemaAttachment::Embedded => export.schema = Some(schema),
                SchemaAttachment::Referenced => {
                    export.schema_ref = Some(SchemaRefBlock::for_schema(schema));
                }
            }
        }
        export.token_budget = budget;
        export
    }

    /// What an embedded schema alone costs (`None` when none is embedded).
    fn schema_cost(&self) -> Option<usize> {
        match self.schema {
            Some((schema, SchemaAttachment::Embedded)) => serde_json::to_string(schema)
                .ok()
                .map(|json| crate::budget::estimate_tokens(&json)),
            _ => None,
        }
    }
}

fn graph_nodes(graph: &Graph) -> Vec<crate::json::JsonNode> {
    graph
        .nodes()
        .into_iter()
        .map(crate::json::graph_node)
        .collect()
}

fn context_nodes(
    graph: &Graph,
    fields: Option<&FieldRegistry>,
) -> Vec<crate::context::ContextNode> {
    graph
        .nodes()
        .into_iter()
        .map(|n| crate::context::context_node(n, fields))
        .collect()
}

fn brief_nodes(graph: &Graph) -> Vec<crate::brief::BriefNode> {
    graph
        .nodes()
        .into_iter()
        .map(crate::brief::brief_node)
        .collect()
}
