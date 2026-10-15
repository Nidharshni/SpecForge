//! The entity snapshot (ADR 0019): every entity of one built graph as every
//! check after the build reads it, taken once per compile and per session
//! check. It owns the field-text rule, the edge types of its edges (each
//! typed once, by the field that wrote it) and the one obligation rule ([`Standing`]). The registry checks and the rules read
//! its records; the pass input, the custom validators' context and the
//! coverage rule's entities are adapters over it.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};

use specforge_graph::{Graph, Node, ValueShape};
use specforge_parser::{FieldValue, UNION_VARIANTS_FIELD};
use specforge_protocol_types::{
    PassEdge, PassEntity, PassSpan, ValidatorContext, ValidatorEntity, ValidatorField,
    ValidatorMethod, ValidatorParam, ValidatorRef,
};
use specforge_registry::{FieldRegistry, KindRegistry, RegistryBuild};

pub use specforge_registry::entity::{
    Direction, EdgeCounts, EdgeRecord, EntityRecord, Exemption, FieldRecord, MethodRecord,
    ObligationRecord, ParamRecord, RuleInput,
};

/// Every entity of one built graph, read with the registry build it was
/// built with (ADR 0019): taken once per compile and per session check.
#[derive(Debug, Clone, Default)]
pub struct EntitySnapshot {
    /// In id order (the graph's node order).
    records: Vec<EntityRecord>,
    /// Parallel to `records`.
    standings: Vec<Standing>,
    index: HashMap<String, usize>,
    /// In graph order.
    edges: Vec<EdgeRecord>,
    /// In id order.
    declared_types: Vec<String>,
    spec_root: PathBuf,
}

impl EntitySnapshot {
    /// The snapshot of `graph`, read with `registries`; relative file paths
    /// in it resolve against `spec_root`.
    pub fn of(graph: &Graph, registries: &RegistryBuild, spec_root: &Path) -> Self {
        let nodes = graph.nodes();
        let kind_of: HashMap<&str, &str> = nodes
            .iter()
            .map(|n| (n.id.raw.as_str(), n.kind.raw.as_str()))
            .collect();
        let records: Vec<EntityRecord> = nodes
            .iter()
            .map(|node| record(graph, &kind_of, node, registries))
            .collect();
        let standings = records
            .iter()
            .map(|record| Standing::of(record, registries))
            .collect();
        let index = records
            .iter()
            .enumerate()
            .map(|(i, record)| (record.id.clone(), i))
            .collect();
        let declared_types = records
            .iter()
            .filter(|record| {
                registries
                    .kinds()
                    .get(&record.kind)
                    .is_some_and(|kind| kind.declared.declares_types)
            })
            .map(|record| record.id.clone())
            .collect();
        let edges = graph
            .edges()
            .iter()
            .map(|e| EdgeRecord {
                source: e.source.as_str().to_string(),
                target: e.target.as_str().to_string(),
                label: e.label.as_str().to_string(),
                edge_type: edge_type_of(&kind_of, registries, e.source.as_str(), e.label.as_str()),
            })
            .collect();
        EntitySnapshot {
            records,
            standings,
            index,
            edges,
            declared_types,
            spec_root: spec_root.to_path_buf(),
        }
    }

    /// Every record, in id order: what the registry checks read.
    pub fn records(&self) -> &[EntityRecord] {
        &self.records
    }

    /// What the rules run over.
    pub fn rule_input(&self) -> RuleInput<'_> {
        RuleInput {
            entities: &self.records,
            edges: &self.edges,
            spec_root: &self.spec_root,
        }
    }

    /// Every entity with its standing, in id order.
    pub fn iter(&self) -> impl Iterator<Item = (&EntityRecord, &Standing)> {
        self.records.iter().zip(&self.standings)
    }

    /// The entity `id`, with its standing.
    pub fn get(&self, id: &str) -> Option<(&EntityRecord, &Standing)> {
        let i = *self.index.get(id)?;
        Some((&self.records[i], &self.standings[i]))
    }

    /// The standing of the entity `id`.
    pub fn standing(&self, id: &str) -> Option<&Standing> {
        self.get(id).map(|(_, standing)| standing)
    }

    /// The kind of the entity `id`.
    pub fn kind_of(&self, id: &str) -> Option<&str> {
        self.get(id).map(|(record, _)| record.kind.as_str())
    }

    /// The graph's edges, in graph order.
    pub fn edges(&self) -> &[EdgeRecord] {
        &self.edges
    }

    /// The ids of the entities whose kind declares types (`declares_types`),
    /// in id order.
    pub fn declared_types(&self) -> &[String] {
        &self.declared_types
    }

    /// What relative paths in it resolve against.
    pub fn spec_root(&self) -> &Path {
        &self.spec_root
    }

    pub fn len(&self) -> usize {
        self.records.len()
    }

    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    // ── Adapters to fixed external shapes (ADR 0019, "Wire") ──

    /// `PassInput::entities`: one [`pass_entity`] per entity, in id order.
    pub fn pass_entities(&self) -> Vec<PassEntity> {
        self.iter()
            .map(|(record, standing)| pass_entity(record, standing))
            .collect()
    }

    /// `PassInput::edges`, in graph order.
    pub fn pass_edges(&self) -> Vec<PassEdge> {
        self.edges
            .iter()
            .map(|e| PassEdge {
                source: e.source.clone(),
                target: e.target.clone(),
                label: e.label.clone(),
            })
            .collect()
    }

    /// What a custom rule's `wasm_function` receives for `id`: its fields
    /// (every written one, in order, `value` its text as a JSON string), its
    /// methods, its references resolved against the snapshot (unique, in
    /// order; a dangling one has no kind), the declared types and the
    /// host's primitives.
    pub fn validator_context(&self, id: &str) -> Option<ValidatorContext> {
        let (record, _) = self.get(id)?;
        let mut seen = HashSet::new();
        let referenced = record
            .references
            .iter()
            .flat_map(|(_, targets)| targets)
            .filter(|target| seen.insert(target.as_str()))
            .map(|target| ValidatorRef {
                id: target.clone(),
                kind: self.kind_of(target).map(str::to_string),
            })
            .collect();
        Some(ValidatorContext {
            entity: ValidatorEntity {
                id: record.id.clone(),
                kind: record.kind.clone(),
                fields: record
                    .fields
                    .iter()
                    .map(|f| ValidatorField {
                        key: f.key.clone(),
                        value: serde_json::Value::String(f.text.clone()),
                        annotations: f.annotations.clone(),
                    })
                    .collect(),
                methods: record
                    .methods
                    .iter()
                    .map(|m| ValidatorMethod {
                        name: m.name.clone(),
                        params: m
                            .params
                            .iter()
                            .map(|p| ValidatorParam {
                                name: p.name.clone(),
                                ty: p.ty.clone(),
                            })
                            .collect(),
                        returns: m.returns.clone(),
                    })
                    .collect(),
            },
            referenced,
            declared_types: self.declared_types.clone(),
            primitives: primitives(),
        })
    }

    /// The context of the load-time probe of a custom rule: an entity of
    /// `kind` that writes nothing.
    pub fn probe_context(kind: &str) -> ValidatorContext {
        ValidatorContext {
            entity: ValidatorEntity {
                id: "__probe__".to_string(),
                kind: kind.to_string(),
                fields: Vec::new(),
                methods: Vec::new(),
            },
            referenced: Vec::new(),
            declared_types: Vec::new(),
            primitives: primitives(),
        }
    }

    /// The coverage rule's entities, in id order. They are the pass
    /// entities, read as the `@specforge/testing:coverage` pass reads them
    /// (`specforge_coverage::Entity::from`), so a per-entity view and the
    /// pass cannot disagree.
    pub fn coverage_entities(&self) -> Vec<specforge_coverage::Entity> {
        self.pass_entities()
            .iter()
            .map(specforge_coverage::Entity::from)
            .collect()
    }
}

/// One entity as a pass receives it:
/// - its field texts by key (a key written twice keeps its last text);
/// - its edge totals and span;
/// - `testable`, and `owes_none` = it owes no obligations of its own.
fn pass_entity(record: &EntityRecord, standing: &Standing) -> PassEntity {
    PassEntity {
        id: record.id.clone(),
        kind: record.kind.clone(),
        fields: record
            .fields
            .iter()
            .map(|f| (f.key.clone(), f.text.clone()))
            .collect(),
        incoming_edge_count: record.incoming.total,
        outgoing_edge_count: record.outgoing.total,
        span: Some(PassSpan {
            file: record.span.file.as_str().to_string(),
            start_line: record.span.start_line,
            start_col: record.span.start_col,
            end_line: record.span.end_line,
            end_col: record.span.end_col,
        }),
        testable: standing.testable,
        owes_none: !standing.owes_obligations(),
        verify_kinds: record.obligations.iter().map(|o| o.kind.clone()).collect(),
        verify_texts: record.obligations.iter().map(|o| o.text.clone()).collect(),
    }
}

/// How the obligation rule sees one entity (ADR 0019): whether its kind is
/// testable, which rule requires its kind to declare obligations, what
/// exempts it whatever its kind, and how many it declares.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Standing {
    /// Its kind is testable (its kind's registry entry says so). Nothing is
    /// testable by default, and accepting `verify` statements does not make
    /// a kind testable.
    pub testable: bool,
    /// The code of the `no_verify_statements` rule that applies to its kind
    /// (the first in code order), if any: its kind must declare obligations.
    pub rule: Option<String>,
    /// What exempts it from obligations of its own, if anything.
    pub exemption: Option<Exemption>,
    /// How many obligations (`verify` statements) it declares.
    pub declared: usize,
}

impl Standing {
    /// The one obligation rule: read from the record and the registry build.
    pub fn of(record: &EntityRecord, registries: &RegistryBuild) -> Standing {
        Standing {
            testable: registries
                .kinds()
                .get(&record.kind)
                .is_some_and(|kind| kind.testable),
            rule: registries
                .rules()
                .verify_rule_for(&record.kind)
                .map(|rule| rule.code().to_string()),
            exemption: record.exemption.clone(),
            declared: record.obligations.len(),
        }
    }

    /// Its kind must declare obligations (a rule applies to its kind).
    pub fn obligated(&self) -> bool {
        self.rule.is_some()
    }

    /// It owes obligations of its own: obligated, and nothing exempts it.
    /// `PassEntity::owes_none` and `specforge_coverage::Entity::owes_none` are its
    /// negation.
    pub fn owes_obligations(&self) -> bool {
        self.rule.is_some() && self.exemption.is_none()
    }

    /// It declares at least one obligation (`verify` statement) of its own,
    /// whatever its kind owes: the plan check's, inspect's and the hover's
    /// "declared".
    pub fn declares_obligations(&self) -> bool {
        self.declared > 0
    }

    /// The facts the coverage rule counts it by.
    pub fn counting(&self) -> specforge_coverage::Counting {
        specforge_coverage::Counting {
            testable: self.testable,
            owes_obligations: self.owes_obligations(),
            declared: self.declared,
        }
    }

    /// It counts toward coverage: the coverage rule's
    /// [`Counting::counts`](specforge_coverage::Counting::counts) over its
    /// facts. Every reader (inspect, the coverage view, stats) asks this.
    pub fn counts(&self) -> bool {
        self.counting().counts()
    }

    /// Testable, but it owes none and declares none, so it does not count
    /// (the coverage view's and inspect's `exempt`).
    pub fn exempt(&self) -> bool {
        self.counting().exempt()
    }

    /// The rule that reports it now: it owes obligations and declares none.
    pub fn reported_by(&self) -> Option<&str> {
        if self.owes_obligations() && self.declared == 0 {
            self.rule.as_deref()
        } else {
            None
        }
    }

    /// A verify stub fixes something here: it declares none, and either
    /// owes obligations (its stub fixes [`Self::reported_by`]) or is of a
    /// testable kind that nothing exempts. An exempt entity (a union, an
    /// `abstract` one, one whose kind accepts no `verify`) is offered none.
    pub fn wants_obligations(&self) -> bool {
        self.declared == 0 && self.exemption.is_none() && (self.rule.is_some() || self.testable)
    }
}

/// Type names accepted by E004 without a declared `type` entity. Sent to
/// the guest as `context.primitives`; the guest may also carry its own
/// embedded copy. `number`, `integer`, `boolean` and `timestamp` are the
/// portable primitives docs/entities/type.md documents; `never` marks an
/// impossible error channel (docs/entities/port.md).
const PRIMITIVE_TYPES: &[&str] = &[
    "string",
    "void",
    "bool",
    "i8",
    "i16",
    "i32",
    "i64",
    "u8",
    "u16",
    "u32",
    "u64",
    "f32",
    "f64",
    "usize",
    "isize",
    "any",
    "number",
    "integer",
    "boolean",
    "timestamp",
    "never",
    // stdlib containers: their type arguments are checked recursively
    "Result",
    "Option",
    "Vec",
    "Box",
    "Arc",
    "Rc",
    "HashMap",
    "HashSet",
    "BTreeMap",
    "BTreeSet",
    "String",
];

fn primitives() -> Vec<String> {
    PRIMITIVE_TYPES.iter().map(|s| s.to_string()).collect()
}

/// The edge type of the edge the field `label` writes from the entity
/// `source`, when that field writes one (`EdgeRegistry::written_by`, on the
/// kind `source` is). The one place a graph edge is typed.
fn edge_type_of(
    kind_of: &HashMap<&str, &str>,
    registries: &RegistryBuild,
    source: &str,
    label: &str,
) -> Option<String> {
    let kind = kind_of.get(source)?;
    let entry = registries.edges().written_by(kind, label)?;
    Some(entry.label().to_string())
}

/// One node as its record: what it writes (as field text), its references,
/// obligations, edge counts by edge type, methods, and what exempts it.
fn record(
    graph: &Graph,
    kind_of: &HashMap<&str, &str>,
    node: &Node,
    registries: &RegistryBuild,
) -> EntityRecord {
    let id = node.id.raw.as_str();
    // Edges in one direction, in total and by edge type.
    let counts = |types: Vec<Option<String>>| {
        let mut by_type = BTreeMap::new();
        for edge_type in types.iter().flatten() {
            *by_type.entry(edge_type.clone()).or_insert(0) += 1;
        }
        EdgeCounts {
            total: types.len(),
            by_type,
        }
    };
    let incoming = counts(
        graph
            .edges_to(id)
            .iter()
            .map(|e| edge_type_of(kind_of, registries, e.source.as_str(), e.label.as_str()))
            .collect(),
    );
    let outgoing = counts(
        graph
            .edges_from(id)
            .iter()
            .map(|e| edge_type_of(kind_of, registries, id, e.label.as_str()))
            .collect(),
    );
    let entries = node.fields.entries();
    EntityRecord {
        id: id.to_string(),
        kind: node.kind.raw.to_string(),
        span: node.source_span.clone(),
        fields: entries
            .iter()
            .map(|entry| FieldRecord {
                key: entry.key.to_string(),
                text: field_text(&entry.value),
                annotations: entry
                    .annotations
                    .iter()
                    .map(|a| a.name.to_string())
                    .collect(),
                items: field_items(&entry.value),
                shape: ValueShape::of(&entry.value),
                value_span: entry.value_span.clone(),
            })
            .collect(),
        references: entries
            .iter()
            .filter_map(|entry| match &entry.value {
                FieldValue::ReferenceList(refs) => Some((
                    entry.key.to_string(),
                    refs.iter().map(|r| r.id.clone()).collect(),
                )),
                _ => None,
            })
            .collect(),
        obligations: node
            .obligations
            .iter()
            .map(|statement| ObligationRecord {
                kind: statement.kind.clone(),
                text: statement.description.clone(),
            })
            .collect(),
        incoming,
        outgoing,
        exemption: exemption(node, registries.kinds(), registries.fields()),
        methods: node
            .methods
            .iter()
            .map(|m| MethodRecord {
                name: m.name.clone(),
                params: m
                    .params
                    .iter()
                    .map(|p| ParamRecord {
                        name: p.name.clone(),
                        ty: p.ty.clone(),
                    })
                    .collect(),
                returns: m.returns.clone(),
            })
            .collect(),
    }
}

/// What exempts an entity from obligations of its own, whatever it declares
/// (ADR 0004, D2-b; ADR 0019), in this order: a union (`type X = A | B`,
/// which has no body to hold them); a set field its kind's registry entry
/// declares `exempts_obligations` (as `@specforge/formal` declares
/// `abstract true`); a kind that accepts no `verify` statements
/// (`supports_verify` unset), which has nowhere to declare them. Decided
/// from the entity's structure and the registries, never from field names:
/// a struct member that only happens to be named `abstract` exempts
/// nothing. A kind no extension declares is not known to refuse `verify`,
/// so it is not exempt for that.
fn exemption(node: &Node, kinds: &KindRegistry, fields: &FieldRegistry) -> Option<Exemption> {
    let kind = node.kind.raw.as_str();
    let entries = node.fields.entries();
    // The union syntax is structural: its body is the variant list, under
    // the parser's own key (a user's `values [a, b]` is a variant list too,
    // and exempts nothing).
    let union = entries.iter().any(|entry| {
        matches!(&entry.value, FieldValue::VariantList(variants)
            if entry.key.as_str() == UNION_VARIANTS_FIELD && !variants.is_empty())
    });
    if union {
        return Some(Exemption::Union);
    }
    let flag = entries.iter().find(|entry| {
        is_set(&entry.value)
            && fields
                .get(kind, entry.key.as_str())
                .is_some_and(|f| f.declared().exempts_obligations)
    });
    if let Some(entry) = flag {
        return Some(Exemption::Flag {
            field: entry.key.to_string(),
        });
    }
    kinds
        .get(kind)
        .is_some_and(|entry| !entry.supports_verify)
        .then_some(Exemption::NoVerify)
}

/// A field value that turns an exempting flag on: `true`, or any value
/// that is not empty.
fn is_set(value: &FieldValue) -> bool {
    match value {
        FieldValue::Boolean(b) => *b,
        FieldValue::String(s) | FieldValue::Identifier(s) => !s.is_empty(),
        FieldValue::StringList(list) => !list.is_empty(),
        FieldValue::ReferenceList(refs) => !refs.is_empty(),
        _ => false,
    }
}

/// A field value's text: the one rule every reader after the graph build
/// shares (ADR 0019). Scalars as written; lists of strings or references
/// and mixed lists joined by `", "`; variant lists and type unions by
/// `" | "`; expressions by `", "` in their display form; verify statements'
/// texts by `"; "`; a block's keys by `", "`. Empty values are `""`.
///
/// A joined list cannot be split back when an item itself contains the
/// joiner (`["a, b", "c"]` is `"a, b, c"`); [`field_items`] keeps them.
pub fn field_text(value: &FieldValue) -> String {
    let joined = |joiner: &str| field_items(value).unwrap_or_default().join(joiner);
    // No `_` arm: a new variant does not compile until it has a text.
    match value {
        FieldValue::String(s) | FieldValue::Identifier(s) | FieldValue::Date(s) => s.clone(),
        FieldValue::Integer(n) => n.to_string(),
        FieldValue::Boolean(b) => b.to_string(),
        FieldValue::StringList(_)
        | FieldValue::ReferenceList(_)
        | FieldValue::MixedList(_)
        | FieldValue::Expression(_)
        | FieldValue::Block(_) => joined(", "),
        FieldValue::VariantList(_) | FieldValue::TypeUnion(_) => joined(" | "),
    }
}

/// A list-shaped value's items, unjoined, each as its text: the list's
/// strings or ids, a variant list's or type union's members, a mixed
/// list's items' texts, each expression's display form, a block's keys. `None` for a scalar. [`field_text`]
/// is these joined.
pub fn field_items(value: &FieldValue) -> Option<Vec<String>> {
    match value {
        FieldValue::String(_)
        | FieldValue::Identifier(_)
        | FieldValue::Date(_)
        | FieldValue::Integer(_)
        | FieldValue::Boolean(_) => None,
        FieldValue::StringList(items)
        | FieldValue::VariantList(items)
        | FieldValue::TypeUnion(items) => Some(items.clone()),
        FieldValue::ReferenceList(refs) => Some(refs.iter().map(|r| r.id.clone()).collect()),
        FieldValue::MixedList(items) => Some(items.iter().map(field_text).collect()),
        FieldValue::Expression(exprs) => Some(exprs.iter().map(ToString::to_string).collect()),
        FieldValue::Block(block) => {
            Some(block.entries().iter().map(|e| e.key.to_string()).collect())
        }
    }
}

#[cfg(test)]
mod tests;
