//! The entity snapshot (ADR 0019): every entity of one built graph as every
//! check after the build reads it, taken once per compile and per session
//! check. It owns the field-text rule, the edge counts by peer kind and the
//! one obligation rule ([`Standing`]). The registry checks and the rules read
//! its records; the pass input, the custom validators' context and the
//! coverage rule's entities are adapters over it.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};

use specforge_graph::{Graph, Node};
use specforge_parser::{FieldValue, UNION_VARIANTS_FIELD};
use specforge_protocol_types::{
    PassEdge, PassEntity, PassSpan, ValidatorContext, ValidatorEntity, ValidatorField,
    ValidatorMethod, ValidatorParam, ValidatorRef,
};
use specforge_registry::{FieldRegistry, KindRegistry, RegistryBuild};

pub use specforge_registry::entity::{
    Direction, EdgeCounts, EdgeRecord, EntityRecord, Exemption, FieldRecord, MethodRecord,
    ObligationRecord, ParamRecord, RuleInput, ValueShape,
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
                    .kinds
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

    /// `PassInput::entities`: each entity's field texts by key (a key
    /// written twice keeps its last text), edge totals, span, `testable`,
    /// and `exempt` = it owes no obligations of its own.
    pub fn pass_entities(&self) -> Vec<PassEntity> {
        self.iter()
            .map(|(record, standing)| PassEntity {
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
                exempt: !standing.owes_obligations(),
                verify_kinds: record.obligations.iter().map(|o| o.kind.clone()).collect(),
                verify_texts: record.obligations.iter().map(|o| o.text.clone()).collect(),
            })
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

    /// The coverage rule's entities (`specforge_coverage::Entity`), in id
    /// order: the same facts the `@specforge/testing:coverage` pass
    /// receives, so a per-entity view and the pass cannot disagree.
    pub fn coverage_entities(&self) -> Vec<specforge_coverage::Entity> {
        self.iter()
            .map(|(record, standing)| coverage_entity(record, standing))
            .collect()
    }
}

/// An entity as the coverage rule sees it.
fn coverage_entity(record: &EntityRecord, standing: &Standing) -> specforge_coverage::Entity {
    specforge_coverage::Entity {
        id: record.id.clone(),
        kind: record.kind.clone(),
        testable: standing.testable,
        exempt: !standing.owes_obligations(),
        verify_kinds: record.obligations.iter().map(|o| o.kind.clone()).collect(),
        verify_texts: record.obligations.iter().map(|o| o.text.clone()).collect(),
        // The host grades no kind by risk (ADR 0009, B): the testing pass
        // reads risk for the kind it grades.
        risk: None,
        referenced: record.edges(Direction::Incoming, None) > 0,
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
                .kinds
                .get(&record.kind)
                .is_some_and(|kind| kind.testable),
            rule: registries
                .rules
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
    /// `PassEntity::exempt` and `specforge_coverage::Entity::exempt` are its
    /// negation.
    pub fn owes_obligations(&self) -> bool {
        self.rule.is_some() && self.exemption.is_none()
    }

    /// It counts toward coverage: testable, and it owes obligations or
    /// declares some. The one place the host states the rule;
    /// `specforge_coverage::Entity::counts_toward_coverage` is its twin for
    /// the pass's entity (the `coverage` extension's input), pinned equal by
    /// `standing_counts_as_the_coverage_rule_does`. Every other reader
    /// (inspect, the coverage view, stats) asks the snapshot's standing.
    pub fn counts(&self) -> bool {
        self.testable && (self.owes_obligations() || self.declared > 0)
    }

    /// Testable, but it owes none and declares none, so it does not count
    /// (the coverage view's and inspect's `exempt`).
    pub fn exempt(&self) -> bool {
        self.testable && !self.counts()
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

/// One node as its record: what it writes (as field text), its references,
/// obligations, edge counts by peer kind, methods, and what exempts it.
fn record(
    graph: &Graph,
    kind_of: &HashMap<&str, &str>,
    node: &Node,
    registries: &RegistryBuild,
) -> EntityRecord {
    let id = node.id.raw.as_str();
    // Edges in one direction, by the kind of the entity at the far end.
    let counts = |peers: Vec<&str>| {
        let mut by_peer_kind = BTreeMap::new();
        for kind in peers.iter().filter_map(|peer| kind_of.get(peer)) {
            *by_peer_kind.entry(kind.to_string()).or_insert(0) += 1;
        }
        EdgeCounts {
            total: peers.len(),
            by_peer_kind,
        }
    };
    let incoming = counts(
        graph
            .edges_to(id)
            .iter()
            .map(|e| e.source.as_str())
            .collect(),
    );
    let outgoing = counts(
        graph
            .edges_from(id)
            .iter()
            .map(|e| e.target.as_str())
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
                shape: field_shape(&entry.value),
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
        obligations: specforge_graph::obligations(node)
            .iter()
            .map(|statement| ObligationRecord {
                kind: statement.kind.clone(),
                text: statement.description.clone(),
            })
            .collect(),
        incoming,
        outgoing,
        exemption: exemption(node, &registries.kinds, &registries.fields),
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
        FieldValue::VerifyList(_) => joined("; "),
    }
}

/// What `value` was written as: one shape per `FieldValue` variant, with no
/// `_` arm, so a new variant does not compile until it has one.
pub fn field_shape(value: &FieldValue) -> ValueShape {
    match value {
        FieldValue::String(_) => ValueShape::String,
        FieldValue::Identifier(_) => ValueShape::Identifier,
        FieldValue::Date(_) => ValueShape::Date,
        FieldValue::Integer(_) => ValueShape::Integer,
        FieldValue::Boolean(_) => ValueShape::Boolean,
        FieldValue::StringList(_) => ValueShape::Strings,
        FieldValue::ReferenceList(_) => ValueShape::References,
        FieldValue::MixedList(_) => ValueShape::Mixed,
        FieldValue::VariantList(_) => ValueShape::Variants,
        FieldValue::TypeUnion(_) => ValueShape::TypeUnion,
        FieldValue::Block(_) => ValueShape::Block,
        FieldValue::VerifyList(_) => ValueShape::Verify,
        FieldValue::Expression(_) => ValueShape::Expression,
    }
}

/// A list-shaped value's items, unjoined, each as its text: the list's
/// strings or ids, a variant list's or type union's members, a mixed
/// list's items' texts, each expression's display form, each verify
/// statement's text, a block's keys. `None` for a scalar. [`field_text`]
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
        FieldValue::VerifyList(statements) => {
            Some(statements.iter().map(|s| s.description.clone()).collect())
        }
        FieldValue::Block(block) => {
            Some(block.entries().iter().map(|e| e.key.to_string()).collect())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use specforge_test_macros::test as specforge_test;

    /// Every field of `source`'s first entity: key, variant name, text.
    fn texts(source: &str) -> Vec<(String, &'static str, String)> {
        let parsed = specforge_parser::parse(source, "t.spec");
        assert!(parsed.errors.is_empty(), "{:?}", parsed.errors);
        parsed.entities[0]
            .fields
            .entries()
            .iter()
            .map(|e| {
                let variant = match &e.value {
                    FieldValue::String(_) => "String",
                    FieldValue::Identifier(_) => "Identifier",
                    FieldValue::Date(_) => "Date",
                    FieldValue::Integer(_) => "Integer",
                    FieldValue::Boolean(_) => "Boolean",
                    FieldValue::StringList(_) => "StringList",
                    FieldValue::ReferenceList(_) => "ReferenceList",
                    FieldValue::VariantList(_) => "VariantList",
                    FieldValue::TypeUnion(_) => "TypeUnion",
                    FieldValue::MixedList(_) => "MixedList",
                    FieldValue::Expression(_) => "Expression",
                    FieldValue::VerifyList(_) => "VerifyList",
                    FieldValue::Block(_) => "Block",
                };
                // The text is the items joined, for every list-shaped value.
                if let Some(items) = field_items(&e.value) {
                    let joiner = match variant {
                        "VariantList" | "TypeUnion" => " | ",
                        "VerifyList" => "; ",
                        _ => ", ",
                    };
                    assert_eq!(items.join(joiner), field_text(&e.value), "{}", e.key);
                }
                (e.key.to_string(), variant, field_text(&e.value))
            })
            .collect()
    }

    #[specforge_test(
        behavior = "snapshot_entities_once",
        verify = "every written field keeps its value shape and value span beside its text"
    )]
    fn field_shape_of_every_variant() {
        let source = r#"item x "X" {
  title "a title"
  owner alice
  due 2026-10-06
  count 42
  active true
  labels ["a, b", "c"]
  needs [y, z]
  values [low, high]
  shape string | string[]
  mix [1, true, two]
  metric expr { latency < 10ms, load > 5 }
  ensures {
    done "it is done"
  }
  verify unit "x works"
}
"#;
        let (graph, _) = specforge_graph::build_graph(&[specforge_parser::parse(source, "t.spec")]);
        let snapshot = EntitySnapshot::of(&graph, &RegistryBuild::default(), Path::new("."));
        let record = snapshot.records().iter().find(|r| r.id == "x").unwrap();
        let shapes: Vec<(&str, ValueShape)> = record
            .fields
            .iter()
            .map(|f| (f.key.as_str(), f.shape))
            .collect();
        assert_eq!(
            shapes,
            [
                ("title", ValueShape::String),
                ("owner", ValueShape::Identifier),
                ("due", ValueShape::Date),
                ("count", ValueShape::Integer),
                ("active", ValueShape::Boolean),
                ("labels", ValueShape::Strings),
                ("needs", ValueShape::References),
                ("values", ValueShape::Variants),
                ("shape", ValueShape::TypeUnion),
                ("mix", ValueShape::Mixed),
                ("metric", ValueShape::Expression),
                ("ensures", ValueShape::Block),
                ("verify", ValueShape::Verify),
            ]
        );
        for shape in [
            ValueShape::Strings,
            ValueShape::References,
            ValueShape::Mixed,
        ] {
            assert!(shape.is_list());
        }
        assert!(!ValueShape::Block.is_list() && !ValueShape::Verify.is_list());

        // Each value carries the span it is written at: its own line.
        let lines: Vec<(&str, Option<usize>)> = record
            .fields
            .iter()
            .map(|f| (f.key.as_str(), f.value_span.as_ref().map(|s| s.start_line)))
            .collect();
        for (key, line) in [("title", 2), ("count", 5), ("labels", 7), ("needs", 8)] {
            assert_eq!(
                lines.iter().find(|(k, _)| *k == key).map(|(_, l)| *l),
                Some(Some(line)),
                "{key}: {lines:?}"
            );
        }
    }

    #[specforge_test(
        behavior = "snapshot_entities_once",
        verify = "a variant list or type union is its members joined by ' | ', a mixed list or expression group its items joined by ', '"
    )]
    fn field_text_of_every_variant() {
        let source = r#"item x "X" {
  title "a title"
  owner alice
  due 2026-10-06
  count 42
  active true
  labels ["a, b", "c"]
  needs [y, z]
  values [low, high]
  shape string | string[]
  mix [1, true, two]
  metric expr { latency < 10ms, load > 5 }
  ensures {
    done "it is done"
    kept "it is kept"
  }
  verify unit "x works"
  verify "x holds"
}
"#;
        let expected = [
            ("title", "String", "a title"),
            ("owner", "Identifier", "alice"),
            ("due", "Date", "2026-10-06"),
            ("count", "Integer", "42"),
            ("active", "Boolean", "true"),
            ("labels", "StringList", "a, b, c"),
            ("needs", "ReferenceList", "y, z"),
            ("values", "VariantList", "low | high"),
            ("shape", "TypeUnion", "string | string[]"),
            ("mix", "MixedList", "1, true, two"),
            ("metric", "Expression", "latency < 10ms, load > 5"),
            ("ensures", "Block", "done, kept"),
            ("verify", "VerifyList", "x works; x holds"),
        ];
        let actual = texts(source);
        assert_eq!(
            actual,
            expected
                .iter()
                .map(|(k, v, t)| (k.to_string(), *v, t.to_string()))
                .collect::<Vec<_>>()
        );

        // A list keeps its items unjoined: the joined text of `labels`
        // cannot tell "a, b" from "a" and "b"; its items can.
        let parsed = specforge_parser::parse(source, "t.spec");
        let items: Vec<(String, Option<Vec<String>>)> = parsed.entities[0]
            .fields
            .entries()
            .iter()
            .map(|e| (e.key.to_string(), field_items(&e.value)))
            .collect();
        let of = |key: &str| {
            items
                .iter()
                .find(|(k, _)| k == key)
                .and_then(|(_, items)| items.clone())
        };
        let owned = |list: &[&str]| Some(list.iter().map(|s| s.to_string()).collect::<Vec<_>>());
        assert_eq!(of("labels"), owned(&["a, b", "c"]));
        assert_eq!(of("values"), owned(&["low", "high"]));
        assert_eq!(of("shape"), owned(&["string", "string[]"]));
        assert_eq!(of("mix"), owned(&["1", "true", "two"]));
        assert_eq!(of("metric"), owned(&["latency < 10ms", "load > 5"]));
        assert_eq!(of("ensures"), owned(&["done", "kept"]));
        assert_eq!(of("verify"), owned(&["x works", "x holds"]));
        for scalar in ["title", "owner", "due", "count", "active"] {
            assert_eq!(of(scalar), None, "{scalar}");
        }
    }

    #[specforge_test(
        behavior = "snapshot_entities_once",
        verify = "an empty list or block is written, with empty text, never left out or null"
    )]
    fn an_empty_value_has_empty_text() {
        let actual = texts("item x \"X\" {\n  values []\n  tags []\n  requires {\n  }\n}\n");
        assert_eq!(
            actual,
            [
                ("values".to_string(), "VariantList", String::new()),
                ("tags".to_string(), "ReferenceList", String::new()),
                ("requires".to_string(), "Block", String::new()),
            ]
        );
    }

    // ── The obligation rule (moved from coverage.rs; links kept) ──

    use specforge_protocol_types::{
        EntityKindDescriptor, ExtensionDeclaration, FieldDescriptor, ValidationRuleDescriptor,
        ValidationSeverity,
    };
    use specforge_registry::rules::NoVerdicts;
    use specforge_registry::{RegistryBuild, build_registries};

    fn graph_of(source: &str) -> Graph {
        let (graph, _) =
            specforge_graph::build_graph(&[specforge_parser::parse(source, "test.spec")]);
        graph
    }

    fn w004(kind: &str) -> ValidationRuleDescriptor {
        ValidationRuleDescriptor {
            code: "W004".into(),
            severity: ValidationSeverity::Warning,
            message_template: "{kind} '{id}' is testable but declares no verify obligations".into(),
            check: "no_verify_statements".into(),
            target_kind: Some(kind.into()),
            field: Some("verify".into()),
            ..Default::default()
        }
    }

    /// The registry build of one extension declaring `kinds` as
    /// `(name, testable, supports_verify)`, the `abstract` flag on
    /// `behavior` (as @specforge/formal does) when `abstract_behaviors`, and
    /// `rules`.
    fn declared(
        kinds: &[(&str, bool, bool)],
        abstract_behaviors: bool,
        rules: Vec<ValidationRuleDescriptor>,
    ) -> RegistryBuild {
        let mut declaration = ExtensionDeclaration {
            validation_rules: rules,
            entities: kinds
                .iter()
                .map(|(name, testable, supports_verify)| EntityKindDescriptor {
                    name: name.to_string(),
                    keyword: Some(name.to_string()),
                    testable: *testable,
                    supports_verify: *supports_verify,
                    ..Default::default()
                })
                .collect(),
            ..Default::default()
        };
        declaration.handshake.name = "@specforge/testing".into();
        declaration.handshake.version = "1.0.0".into();
        if abstract_behaviors {
            declaration
                .entities
                .iter_mut()
                .find(|kind| kind.name == "behavior")
                .expect("a behavior kind")
                .fields
                .push(FieldDescriptor {
                    name: "abstract".into(),
                    field_type: "bool".into(),
                    exempts_obligations: true,
                    ..Default::default()
                });
        }
        let build = build_registries(vec![declaration]);
        assert!(
            build.diagnostics().next().is_none(),
            "{:?}",
            build.diagnostics().collect::<Vec<_>>()
        );
        build
    }

    fn snapshot(source: &str, registries: &RegistryBuild) -> EntitySnapshot {
        EntitySnapshot::of(&graph_of(source), registries, Path::new(""))
    }

    /// The ids W004 reports on `source` (rules on `behavior` and `type`).
    fn w004_ids(source: &str, abstract_behaviors: bool) -> Vec<String> {
        // A rule for a kind nobody declares is inert (ADR 0020 D5): declare
        // the two the rules target.
        let registries = declared(
            &[("behavior", true, true), ("type", true, true)],
            abstract_behaviors,
            vec![w004("behavior"), w004("type")],
        );
        let snapshot = snapshot(source, &registries);
        let mut ids: Vec<String> = registries
            .rules
            .check(&snapshot.rule_input(), &NoVerdicts)
            .into_iter()
            .map(|d| d.message.split('\'').nth(1).unwrap().to_string())
            .collect();
        ids.sort();
        ids
    }

    #[specforge_test(
        behavior = "te_validate_unverified_testable",
        verify = "a union type never produces W004"
    )]
    fn a_union_type_owes_no_obligations() {
        let ids = w004_ids(
            "type Status = active | inactive\n\ntype Plain \"Plain\" {\n  id string\n}\n",
            false,
        );
        assert_eq!(ids, ["Plain"]);
    }

    #[test]
    fn an_enum_values_list_is_not_a_union() {
        let ids = w004_ids(
            "type Priority \"Priority\" {\n  values [high, low]\n}\n",
            false,
        );
        assert_eq!(ids, ["Priority"]);
    }

    #[specforge_test(
        behavior = "te_validate_unverified_testable",
        verify = "an abstract entity never produces W004"
    )]
    fn an_abstract_entity_owes_no_obligations_when_its_kind_declares_the_flag() {
        let source = "behavior base \"Base\" {\n  contract \"The system MUST work\"\n  abstract true\n}\n\n\
                      behavior concrete \"Concrete\" {\n  contract \"The system MUST work\"\n  abstract false\n}\n";
        assert_eq!(w004_ids(source, true), ["concrete"]);
        // Without a registry entry declaring it, `abstract` is just a name.
        assert_eq!(w004_ids(source, false), ["base", "concrete"]);
    }

    /// The ids of `source`'s entities whose kind is testable, under `kinds`
    /// as `(name, testable, supports_verify)`.
    fn testable_ids(source: &str, kinds: &[(&str, bool, bool)]) -> Vec<String> {
        let snapshot = snapshot(source, &declared(kinds, false, Vec::new()));
        snapshot
            .iter()
            .filter(|(_, standing)| standing.testable)
            .map(|(record, _)| record.id.clone())
            .collect()
    }

    #[specforge_test(
        invariant = "testable_entity_classification",
        verify = "no default testability assumed by core"
    )]
    fn no_kind_is_testable_unless_an_extension_says_so() {
        let source = "behavior login \"Login\" {\n}\n";
        assert!(testable_ids(source, &[]).is_empty());
        assert!(testable_ids(source, &[("behavior", false, false)]).is_empty());
    }

    #[specforge_test(
        invariant = "testable_entity_classification",
        verify = "testable=false entity excluded from coverage"
    )]
    fn only_kinds_declared_testable_count() {
        let kinds = [
            ("behavior", true, true),
            ("type", true, true),
            // Accepts verify statements but does not count toward coverage.
            ("property", false, true),
            ("feature", false, false),
        ];
        let source = "behavior b \"B\" {\n}\n\ntype t \"T\" {\n}\n\n\
                      property p \"P\" {\n  verify unit \"holds\"\n}\n\nfeature f \"F\" {\n}\n";
        assert_eq!(testable_ids(source, &kinds), ["b", "t"]);
    }

    #[specforge_test(
        behavior = "snapshot_entities_once",
        verify = "an entity owes obligations when a no_verify_statements rule applies to its kind and neither a union body nor an exempting flag exempts it"
    )]
    fn an_entity_owes_obligations_when_a_rule_applies_and_nothing_exempts_it() {
        let kinds = [
            ("behavior", true, true),
            ("type", true, true),
            ("memo", false, false),
        ];
        let source = "behavior open \"Open\" {\n}\n\nbehavior base \"Base\" {\n  abstract true\n}\n\n\
                      type Status = active | inactive\n\ntype Plain \"Plain\" {\n  id string\n}\n\n\
                      memo note \"Note\" {\n}\n";
        let owes = |rules: Vec<ValidationRuleDescriptor>| -> Vec<String> {
            let registries = declared(&kinds, true, rules);
            snapshot(source, &registries)
                .iter()
                .filter(|(_, standing)| standing.owes_obligations())
                .map(|(record, _)| record.id.clone())
                .collect()
        };
        // No rule: nobody owes anything.
        assert!(owes(Vec::new()).is_empty());
        // A rule on `behavior`: its entities owe, unless a flag exempts.
        assert_eq!(owes(vec![w004("behavior")]), ["open"]);
        // A rule without a target kind applies to every kind; a union body,
        // a flag and a kind without `verify` still exempt.
        let mut untargeted = w004("behavior");
        untargeted.target_kind = None;
        assert_eq!(owes(vec![untargeted]), ["Plain", "open"]);
        // What exempts each, decided once.
        let registries = declared(&kinds, true, Vec::new());
        let exemptions: Vec<(String, Option<Exemption>)> = snapshot(source, &registries)
            .iter()
            .map(|(record, standing)| {
                assert_eq!(record.exemption, standing.exemption);
                (record.id.clone(), standing.exemption.clone())
            })
            .collect();
        assert_eq!(
            exemptions,
            [
                ("Plain".to_string(), None),
                ("Status".to_string(), Some(Exemption::Union)),
                (
                    "base".to_string(),
                    Some(Exemption::Flag {
                        field: "abstract".to_string()
                    })
                ),
                ("note".to_string(), Some(Exemption::NoVerify)),
                ("open".to_string(), None),
            ]
        );
    }

    #[specforge_test(
        behavior = "read_views_over_the_project_view",
        verify = "an entity is unverified when it counts toward coverage and is not proven"
    )]
    fn standing_counts_as_the_coverage_rule_does() {
        let span = specforge_common::SourceSpan {
            file: specforge_common::Sym::new("t.spec"),
            start_line: 1,
            start_col: 1,
            end_line: 1,
            end_col: 1,
        };
        let exemptions = [
            None,
            Some(Exemption::Union),
            Some(Exemption::Flag {
                field: "abstract".into(),
            }),
            Some(Exemption::NoVerify),
        ];
        let mut combinations = 0;
        for testable in [false, true] {
            for rule in [None, Some("W004".to_string())] {
                for exemption in &exemptions {
                    for declared in [0, 1] {
                        let mut record = EntityRecord::new("item", "a", &span);
                        record.exemption = exemption.clone();
                        if declared == 1 {
                            record = record.with_obligation("unit", "works");
                        }
                        let standing = Standing {
                            testable,
                            rule: rule.clone(),
                            exemption: exemption.clone(),
                            declared,
                        };
                        let entity = coverage_entity(&record, &standing);
                        assert_eq!(
                            standing.counts(),
                            entity.counts_toward_coverage(),
                            "{standing:?}"
                        );
                        assert_eq!(entity.exempt, !standing.owes_obligations());
                        assert_eq!(standing.exempt(), testable && !standing.counts());
                        combinations += 1;
                    }
                }
            }
        }
        assert_eq!(combinations, 32);
    }

    #[specforge_test(
        behavior = "snapshot_entities_once",
        verify = "the coverage rule reads each entity as the pass input carries it"
    )]
    fn coverage_entities_are_the_pass_entities_as_the_rule_reads_them() {
        let registries = declared(
            &[
                ("behavior", true, true),
                ("type", true, true),
                ("memo", false, false),
            ],
            true,
            vec![w004("behavior"), w004("type")],
        );
        let source = "behavior open \"Open\" {\n  features [base]\n  verify unit \"works\"\n}\n\n\
                      behavior other \"Other\" {\n  features [base]\n}\n\n\
                      behavior base \"Base\" {\n  abstract true\n}\n\n\
                      type Status = active | inactive\n\ntype Plain \"Plain\" {\n  id string\n}\n\n\
                      memo note \"Note\" {\n}\n";
        let snapshot = snapshot(source, &registries);
        let passed = snapshot.pass_entities();
        let covered = snapshot.coverage_entities();
        assert_eq!(passed.len(), 6);
        assert_eq!(covered.len(), passed.len());
        // The reading the rule makes of a pass entity, written out.
        for (entity, pass) in covered.iter().zip(&passed) {
            assert_eq!(entity.id, pass.id);
            assert_eq!(entity.kind, pass.kind);
            assert_eq!(entity.testable, pass.testable, "{}", pass.id);
            assert_eq!(entity.exempt, pass.exempt, "{}", pass.id);
            assert_eq!(entity.verify_kinds, pass.verify_kinds, "{}", pass.id);
            assert_eq!(entity.verify_texts, pass.verify_texts, "{}", pass.id);
            assert_eq!(entity.risk, None, "{}", pass.id);
            assert_eq!(
                entity.referenced,
                pass.incoming_edge_count > 0,
                "{}",
                pass.id
            );
        }
        // The cases are all there: a testable entity that owes, one that is
        // exempt, one nothing refers to and one that something does.
        let by_id = |id: &str| covered.iter().find(|e| e.id == id).unwrap();
        assert!(by_id("open").testable && !by_id("open").exempt);
        assert!(by_id("base").exempt);
        assert!(by_id("Status").exempt);
        assert!(!by_id("note").testable && by_id("note").exempt);
        assert!(by_id("base").referenced);
        assert!(!by_id("open").referenced);
    }
}
