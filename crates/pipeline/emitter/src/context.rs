use serde::Serialize;
use specforge_common::Sym;
use specforge_common::shape::Shape;
use specforge_coverage::Status;
use specforge_graph::Node;
use specforge_graph::field_form::{FieldJson, write};
use specforge_graph::{AnnotationJson, Members, MethodJson, Obligation};
use specforge_registry::FieldRegistry;
use specforge_registry::snapshot::field_text;
use std::collections::BTreeMap;

#[derive(Serialize, Shape)]
pub(crate) struct ContextNode {
    id: String,
    kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    title: Option<String>,
    /// The fields an extension declares `headline` (a contract, a status),
    /// by name.
    #[serde(flatten)]
    headline: BTreeMap<String, String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    verify: Option<Vec<Obligation>>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    fields: BTreeMap<String, FieldJson>,
    /// The annotations of the fields it keeps (a headline or a normative one), by path (ADR 0222).
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    annotations: BTreeMap<String, Vec<AnnotationJson>>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    methods: Vec<MethodJson>,
    /// The entity's coverage status, when the export was given verdicts
    /// (`EmitOptions::coverage`): what `specforge query --include-coverage`
    /// shows.
    #[serde(skip_serializing_if = "Option::is_none")]
    coverage_status: Option<Status>,
}

/// Keys a context node has of its own, which no headline field may take.
const NODE_KEYS: [&str; 8] = [
    "id",
    "kind",
    "title",
    "verify",
    "fields",
    "annotations",
    "methods",
    "coverage_status",
];

/// The fields of `node` its extension declares `headline`, as their field text
/// (ADR 0019, ADR 0228), for the context node's top level.
pub(crate) fn headline_fields(node: &Node, registry: &FieldRegistry) -> BTreeMap<String, String> {
    node.fields
        .entries()
        .iter()
        .filter(|entry| is_headline(node, entry.key.as_str(), registry))
        .filter(|entry| !NODE_KEYS.contains(&entry.key.as_str()))
        .map(|entry| (entry.key.to_string(), field_text(&entry.value)))
        .collect()
}

/// The statement that states what `node` promises, as an agent reads it first:
/// which field states it, and its field text (ADR 0019, ADR 0228), whatever
/// its written shape.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Headline {
    pub field: Sym,
    pub text: String,
}

impl std::fmt::Display for Headline {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.text)
    }
}

/// The first field of `node` its extension declares both `headline` and
/// `normative` (a behavior's or an event's `contract`; a `status` is headline
/// but not normative), with its field text. `None` when its kind declares none.
pub fn headline_statement(node: &Node, registry: &FieldRegistry) -> Option<Headline> {
    node.fields.entries().iter().find_map(|entry| {
        registry
            .get(node.kind.raw.as_str(), entry.key.as_str())
            .is_some_and(|f| f.declared().headline && f.declared().normative)
            .then(|| Headline {
                field: entry.key,
                text: field_text(&entry.value),
            })
    })
}

fn is_headline(node: &Node, field: &str, registry: &FieldRegistry) -> bool {
    registry
        .get(node.kind.raw.as_str(), field)
        .is_some_and(|f| f.declared().headline)
}

/// The fields of `node` its extension declares normative (the text that
/// states what the entity promises), except those a context node already
/// carries at its top level (its headline fields and obligations).
pub(crate) fn normative_fields(
    node: &Node,
    registry: &FieldRegistry,
) -> BTreeMap<String, FieldJson> {
    node.fields
        .entries()
        .iter()
        .filter(|entry| !is_headline(node, entry.key.as_str(), registry))
        .filter(|entry| {
            registry
                .get(node.kind.raw.as_str(), entry.key.as_str())
                .is_some_and(|field| field.declared().normative)
        })
        .map(|entry| (entry.key.to_string(), write(&entry.value)))
        .collect()
}

/// `n` as the context export writes an entity: its title and obligations, the
/// headline and normative fields `registry` declares.
pub(crate) fn context_node(
    n: &Node,
    registry: &FieldRegistry,
    coverage_status: Option<Status>,
) -> ContextNode {
    let fields = normative_fields(n, registry);
    let headline = headline_fields(n, registry);
    let Members {
        annotations,
        methods,
        ..
    } = Members::of(n);
    let kept = |field: &str| fields.contains_key(field) || headline.contains_key(field);
    ContextNode {
        id: n.id.raw.to_string(),
        kind: n.kind.raw.to_string(),
        title: n.title.clone(),
        annotations: annotations
            .into_iter()
            .filter(|(path, _)| kept(Members::field_of(path)))
            .collect(),
        methods,
        headline,
        verify: {
            let statements = &n.obligations;
            (!statements.is_empty()).then(|| statements.iter().map(Obligation::of).collect())
        },
        fields,
        coverage_status,
    }
}
