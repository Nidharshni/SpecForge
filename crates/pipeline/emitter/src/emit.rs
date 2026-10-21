use std::borrow::Cow;
use std::collections::BTreeMap;

use specforge_coverage::{Status, Verdict};
use specforge_graph::{FieldForms, Graph, Node, Selected};
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
    /// A document a reviewer reads without tooling (ADR 0152).
    Markdown,
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
    /// What the export covers ([`Graph::select`], ADR 0190); `None`: the whole graph. A scoped export (one with a
    /// root) references the schema instead of embedding it, draws its root whatever its kind, and a token budget
    /// never cuts the root.
    pub selected: Option<&'a Selected>,
    /// The schema the graph is exported against. Its version is every JSON
    /// export's `schema_version`; the schema itself is in the export only
    /// when `attach_schema` (embedded, or referenced when `scope` is set).
    pub schema: &'a GraphProtocolSchema,
    /// Carry `schema` (format 2.0). False: graph format 1.0, and context and
    /// brief without a schema; `schema_version` is still `schema`'s.
    pub attach_schema: bool,
    /// Token budget for truncated output (default: None = no limit).
    pub token_budget: Option<usize>,
    /// Kind registry for style lookups (DOT shape/color/fillcolor declared by
    /// extensions; C13-00). Default: None — nodes use built-in defaults.
    pub kind_registry: Option<&'a KindRegistry>,
    /// Field registry, so the context format keeps each entity's normative
    /// and headline fields. Default: None — context carries only title/verify.
    pub field_registry: Option<&'a FieldRegistry>,
    /// Each entity's coverage verdict, by id (`ProjectCoverage::verdicts`).
    /// With it, every node whose entity has a verdict carries its status as
    /// `coverage_status` (graph, context and brief; DOT draws none), and a
    /// token budget counts the key. Default: None — no node carries one.
    pub coverage: Option<&'a BTreeMap<String, Verdict>>,
    /// How each field reads (ADR 0093): the markdown format lists a field's
    /// references from it, linked or not. Default: None, so links come from
    /// the edges alone.
    pub field_forms: Option<&'a FieldForms>,
}

/// Default: graph format 1.0 of the whole graph against the unversioned
/// empty schema (1.0.0), unattached: what tests and benches compare.
impl Default for EmitOptions<'_> {
    fn default() -> Self {
        Self {
            format: EmitFormat::Json,
            selected: None,
            schema: unversioned(),
            attach_schema: false,
            token_budget: None,
            kind_registry: None,
            field_registry: None,
            coverage: None,
            field_forms: None,
        }
    }
}

/// The empty schema at 1.0.0, shared by every default.
fn unversioned() -> &'static GraphProtocolSchema {
    static EMPTY: std::sync::LazyLock<GraphProtocolSchema> =
        std::sync::LazyLock::new(GraphProtocolSchema::empty);
    &EMPTY
}

/// Unified graph emission: the one entry point for every export, and the one
/// tests use.
///
/// `selected` chooses the entities ([`Graph::select`]); a scoped
/// export references the schema instead of embedding it. `format` and `schema`
/// choose the shape (graph 1.0, or 2.0 with the schema; context and brief, 2.0
/// when a schema is given); `coverage` gives each node whose entity has a
/// verdict its `coverage_status`. Markdown, like DOT, is rendered from the
/// selection before any shape: not budgeted, no schema. Under `token_budget`
/// every other format is fitted
/// by `budget::fit`: the whole export when it fits; otherwise the least
/// central entities (never the scope's own) are dropped, with their edges, until it fits, and the
/// export says which in its `token_budget` block. An embedded schema counts
/// and is never cut: a schema over the budget is E062, as is a budget below the
/// export with no entities. DOT is not budgeted.
///
/// Err: `BudgetTooSmall` (E062) for a budget too small.
pub fn emit(graph: &Graph, options: &EmitOptions<'_>) -> Result<String, EmitterError> {
    let selected = select(graph, options);
    if options.format == EmitFormat::Dot {
        return Ok(crate::dot::emit_dot(
            &selected,
            &crate::dot::DotOptions {
                kind_registry: options.kind_registry,
                ..Default::default()
            },
        ));
    }

    if options.format == EmitFormat::Markdown {
        return Ok(crate::markdown::render(graph, &selected, options));
    }

    // Scoped exports reference the published schema instead of embedding it
    // (C6-07); full exports embed.
    let attach = if options.selected.and_then(Selected::root).is_some() {
        SchemaAttachment::Referenced
    } else {
        SchemaAttachment::Embedded
    };
    let shape = Shape {
        format: options.format,
        schema: options.schema,
        attach: options.attach_schema.then_some(attach),
        fields: options.field_registry,
        coverage: options.coverage,
    };
    match options.token_budget {
        None => shape.render(&selected, None),
        Some(max_tokens) => {
            let root = options.selected.and_then(Selected::root);
            crate::budget::fit(
                &selected,
                max_tokens,
                shape.schema_cost(),
                root,
                |g, budget| shape.render(g, budget),
            )
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
        EmitFormat::Dot | EmitFormat::Markdown => None,
    }
}

/// The subgraph an export covers: the selection's entities and its root.
fn select<'g>(graph: &'g Graph, options: &EmitOptions<'_>) -> Cow<'g, Graph> {
    match options.selected {
        Some(selected) if !selected.is_whole() => Cow::Owned(graph.induced(selected.drawn())),
        _ => Cow::Borrowed(graph),
    }
}

/// How one export is written: its format, its schema and how it is attached,
/// and the field registry the context format reads.
struct Shape<'a> {
    /// Never DOT: it is rendered before a `Shape` exists.
    format: EmitFormat,
    schema: &'a GraphProtocolSchema,
    /// How the schema is carried; `None`: not at all.
    attach: Option<SchemaAttachment>,
    fields: Option<&'a FieldRegistry>,
    coverage: Option<&'a BTreeMap<String, Verdict>>,
}

impl Shape<'_> {
    /// `graph` in this shape, with `budget` as its `token_budget` block when
    /// given: the one format x schema table.
    fn render(&self, graph: &Graph, budget: Option<&TokenBudget>) -> Result<String, EmitterError> {
        match self.format {
            EmitFormat::Json => self
                .envelope(
                    graph,
                    Some(ExportVersion::V1),
                    graph_nodes(graph, self.coverage),
                    budget,
                )
                .to_json(),
            EmitFormat::Context => self
                .envelope(
                    graph,
                    None,
                    context_nodes(graph, self.fields, self.coverage),
                    budget,
                )
                .to_json(),
            EmitFormat::Brief => self
                .envelope(graph, None, brief_nodes(graph, self.coverage), budget)
                .to_json(),
            EmitFormat::Dot | EmitFormat::Markdown => {
                unreachable!("DOT and markdown are rendered before a Shape exists")
            }
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
        let schema = self.schema;
        let mut export = Export::plain(plain_version, &schema.schema_version, graph, nodes);
        if let Some(attach) = self.attach {
            export.format_version = Some(ExportVersion::V2);
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
        match self.attach {
            Some(SchemaAttachment::Embedded) => serde_json::to_string(self.schema)
                .ok()
                .map(|json| crate::budget::estimate_tokens(&json)),
            _ => None,
        }
    }
}

/// `node`'s coverage status, when `coverage` holds a verdict for it.
fn status_of(node: &Node, coverage: Option<&BTreeMap<String, Verdict>>) -> Option<Status> {
    coverage?.get(node.id.raw.as_str()).map(Verdict::status)
}

fn graph_nodes(
    graph: &Graph,
    coverage: Option<&BTreeMap<String, Verdict>>,
) -> Vec<crate::json::JsonNode> {
    graph
        .nodes()
        .into_iter()
        .map(|n| crate::json::graph_node(n, status_of(n, coverage)))
        .collect()
}

fn context_nodes(
    graph: &Graph,
    fields: Option<&FieldRegistry>,
    coverage: Option<&BTreeMap<String, Verdict>>,
) -> Vec<crate::context::ContextNode> {
    graph
        .nodes()
        .into_iter()
        .map(|n| crate::context::context_node(n, fields, status_of(n, coverage)))
        .collect()
}

fn brief_nodes(
    graph: &Graph,
    coverage: Option<&BTreeMap<String, Verdict>>,
) -> Vec<crate::brief::BriefNode> {
    graph
        .nodes()
        .into_iter()
        .map(|n| crate::brief::brief_node(n, status_of(n, coverage)))
        .collect()
}
