//! Adapters from the snapshot to fixed external shapes (ADR 0019, "Wire"): the pass input, the
//! custom validators' context and the coverage rule's entities.

use std::collections::HashSet;

use specforge_protocol_types::{
    PassEdge, PassEntity, PassSpan, ValidatorContext, ValidatorEntity, ValidatorField,
    ValidatorMethod, ValidatorParam, ValidatorRef,
};

use super::{EntitySnapshot, Standing};
use crate::entity::EntityRecord;

impl EntitySnapshot {
    /// `PassInput::entities`: one [`pass_entity`] per entity, in id order.
    pub fn pass_entities(&self) -> Vec<PassEntity> {
        self.iter()
            .map(|(record, standing)| pass_entity(record, standing))
            .collect()
    }

    /// `PassInput::edges`, in graph order.
    pub fn pass_edges(&self) -> Vec<PassEdge> {
        self.edges()
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
            declared_types: self.declared_types().to_vec(),
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
pub(super) fn pass_entity(record: &EntityRecord, standing: &Standing) -> PassEntity {
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
