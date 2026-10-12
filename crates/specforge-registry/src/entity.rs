//! What every check after the graph build reads about one entity (ADR 0019):
//! the records of the entity snapshot. Graph-free, so the registry checks and
//! the rules read them without the graph; `specforge_project::snapshot`
//! builds them, once per compile.

use std::collections::BTreeMap;
use std::path::Path;

use specforge_common::SourceSpan;

/// One entity as the registry checks, the rules and the custom validators
/// read it. Built by the project's entity snapshot; tests build it with the
/// `with_*` methods.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntityRecord {
    pub id: String,
    pub kind: String,
    pub span: SourceSpan,
    /// Every field the entity writes, in source order (a name written twice
    /// appears twice), each with its field text.
    pub fields: Vec<FieldRecord>,
    /// Each reference-list field with the ids it names, in source order
    /// (resolved or not).
    pub references: Vec<(String, Vec<String>)>,
    /// Its `verify` statements, in order: what it declares it will prove.
    pub obligations: Vec<ObligationRecord>,
    /// Edges into it, in total and by edge type.
    pub incoming: EdgeCounts,
    /// Edges out of it, in total and by edge type.
    pub outgoing: EdgeCounts,
    /// What exempts it from obligations of its own whatever its kind, if
    /// anything: decided from its structure and the registries, never from
    /// a field's name.
    pub exemption: Option<Exemption>,
    /// Its declared methods (ports), in order.
    pub methods: Vec<MethodRecord>,
}

/// One written field: its key, its field text (ADR 0019), its value's shape
/// and span (ADR 0031), and the names of the annotations on it, without the
/// `@`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldRecord {
    pub key: String,
    pub text: String,
    pub annotations: Vec<String>,
    /// A list-shaped value's items, unjoined (each item's text), so a host
    /// reader never splits a joined text (lossy when an item contains the
    /// joiner); `None` for a scalar. Host-internal: never on the wire.
    pub items: Option<Vec<String>>,
    /// What the value was written as, after the graph build coerced it to
    /// its field's declared type. Host-internal: never on the wire.
    pub shape: ValueShape,
    /// Where the value is written; `None` when the parser recorded no span
    /// for it (a check then points at the entity). Host-internal: never on
    /// the wire.
    pub value_span: Option<SourceSpan>,
}

/// What a field's value was written as: the structure its field text loses
/// (ADR 0019, "What would reopen this"). One variant per parsed value form,
/// so a check tells `"1"` from `1`, a one-item list from a scalar and a
/// quoted string from a bare word.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValueShape {
    /// `"text"`.
    String,
    /// A bare word: an identifier, or a single reference.
    Identifier,
    Date,
    Integer,
    Boolean,
    /// `["a", "b"]`.
    Strings,
    /// `[a, b]`.
    References,
    /// A list whose items have different shapes.
    Mixed,
    /// A variant list (`a | b` in a body).
    Variants,
    TypeUnion,
    Block,
    /// `verify` statements.
    Verify,
    Expression,
}

impl ValueShape {
    /// A list: of strings, references, mixed items or variants.
    pub fn is_list(self) -> bool {
        matches!(
            self,
            Self::Strings | Self::References | Self::Mixed | Self::Variants
        )
    }
}

/// One `verify` statement: its kind (`""` for a bare `verify "…"`) and text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObligationRecord {
    pub kind: String,
    pub text: String,
}

/// Edge counts in one direction.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EdgeCounts {
    pub total: usize,
    /// By edge type: the edges whose field writes one
    /// ([`crate::EdgeRegistry::written_by`]); an edge of no type counts in
    /// `total` only.
    pub by_type: BTreeMap<String, usize>,
}

/// One graph edge (label = the field that wrote it) and its edge type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EdgeRecord {
    pub source: String,
    pub target: String,
    pub label: String,
    /// The edge type the field `label` writes from the kind `source` is,
    /// when it writes one.
    pub edge_type: Option<String>,
}

/// Why an entity owes no obligations of its own whatever its kind.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Exemption {
    /// A union (`type X = A | B`): no body to hold them.
    Union,
    /// It sets `field`, which its kind's registry entry declares
    /// `exempts_obligations` (`@specforge/formal`'s `abstract true`).
    Flag { field: String },
    /// Its kind accepts no `verify` statements (`supports_verify` unset), so
    /// it has nowhere to declare them. Exempts it from statement obligations
    /// only: a `no_verify_statements` rule whose obligation is another field
    /// ignores it.
    NoVerify,
}

impl Exemption {
    /// It exempts from obligations declared in a field other than `verify`
    /// statements: a union body or an exempting flag, not a kind that only
    /// lacks `verify`.
    pub fn exempts_fields(&self) -> bool {
        !matches!(self, Exemption::NoVerify)
    }
}

/// One declared method (a port's).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MethodRecord {
    pub name: String,
    pub params: Vec<ParamRecord>,
    pub returns: Option<String>,
}

/// One method parameter: its name and its type as written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParamRecord {
    pub name: String,
    pub ty: String,
}

/// Which way an edge runs, seen from an entity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Incoming,
    Outgoing,
}

/// What the rules run over: the snapshot's records in id order, its edges
/// in graph order, and the spec root relative paths resolve against.
#[derive(Debug, Clone, Copy)]
pub struct RuleInput<'a> {
    pub entities: &'a [EntityRecord],
    pub edges: &'a [EdgeRecord],
    pub spec_root: &'a Path,
}

impl EntityRecord {
    /// A record with nothing written (the test builder's start).
    pub fn new(kind: &str, id: &str, span: &SourceSpan) -> Self {
        EntityRecord {
            id: id.to_string(),
            kind: kind.to_string(),
            span: span.clone(),
            fields: Vec::new(),
            references: Vec::new(),
            obligations: Vec::new(),
            incoming: EdgeCounts::default(),
            outgoing: EdgeCounts::default(),
            exemption: None,
            methods: Vec::new(),
        }
    }

    /// Fields with these keys and empty text (detection tests).
    pub fn with_fields(mut self, keys: &[&str]) -> Self {
        for key in keys {
            self = self.with_field(key, "");
        }
        self
    }

    /// One more written field, with `text`, written as a quoted string.
    pub fn with_field(self, key: &str, text: &str) -> Self {
        self.with_value(key, ValueShape::String, text)
    }

    /// One more written list field of strings: its items, and their text
    /// joined by `", "`.
    pub fn with_list(mut self, key: &str, items: &[&str]) -> Self {
        self.fields.push(FieldRecord {
            key: key.to_string(),
            text: items.join(", "),
            annotations: Vec::new(),
            items: Some(items.iter().map(|item| item.to_string()).collect()),
            shape: ValueShape::Strings,
            value_span: None,
        });
        self
    }

    /// One more written field with `text`, written as `shape` (a list shape
    /// holds `text` as its one item). The checks' tests build the values
    /// they read with this.
    pub fn with_value(mut self, key: &str, shape: ValueShape, text: &str) -> Self {
        self.fields.push(FieldRecord {
            key: key.to_string(),
            text: text.to_string(),
            annotations: Vec::new(),
            items: shape.is_list().then(|| vec![text.to_string()]),
            shape,
            value_span: None,
        });
        self
    }

    /// One more reference-list field naming `targets`.
    pub fn with_reference(mut self, field: &str, targets: &[&str]) -> Self {
        self.references.push((
            field.to_string(),
            targets.iter().map(|t| t.to_string()).collect(),
        ));
        self
    }

    /// One more `verify` statement.
    pub fn with_obligation(mut self, kind: &str, text: &str) -> Self {
        self.obligations.push(ObligationRecord {
            kind: kind.to_string(),
            text: text.to_string(),
        });
        self
    }

    /// `count` more edges in `direction`, of `edge_type` when given (an
    /// edge of no type otherwise).
    pub fn with_edges(
        mut self,
        direction: Direction,
        edge_type: Option<&str>,
        count: usize,
    ) -> Self {
        let counts = match direction {
            Direction::Incoming => &mut self.incoming,
            Direction::Outgoing => &mut self.outgoing,
        };
        counts.total += count;
        if let Some(edge_type) = edge_type {
            *counts.by_type.entry(edge_type.to_string()).or_default() += count;
        }
        self
    }

    /// What exempts it from obligations of its own.
    pub fn exempt(mut self, exemption: Exemption) -> Self {
        self.exemption = Some(exemption);
        self
    }

    /// The text of `key`: the last occurrence's, the value the entity gives
    /// it.
    pub fn field(&self, key: &str) -> Option<&str> {
        self.fields
            .iter()
            .rev()
            .find(|f| f.key == key)
            .map(|f| f.text.as_str())
    }

    /// The entity writes `key` (whatever its text, even empty).
    pub fn writes(&self, key: &str) -> bool {
        self.fields.iter().any(|f| f.key == key)
    }

    /// Field keys in source order.
    pub fn field_keys(&self) -> impl Iterator<Item = &str> {
        self.fields.iter().map(|f| f.key.as_str())
    }

    /// Edges in `direction`, only those of `edge_type` when set.
    pub fn edges(&self, direction: Direction, edge_type: Option<&str>) -> usize {
        let counts = match direction {
            Direction::Incoming => &self.incoming,
            Direction::Outgoing => &self.outgoing,
        };
        match edge_type {
            Some(edge_type) => counts.by_type.get(edge_type).copied().unwrap_or(0),
            None => counts.total,
        }
    }

    /// It owes no `verify` statements of its own: anything exempts it.
    pub fn exempts_statements(&self) -> bool {
        self.exemption.is_some()
    }

    /// It owes no obligations declared in another field: a union body or an
    /// exempting flag exempts it ([`Exemption::exempts_fields`]).
    pub fn exempts_fields(&self) -> bool {
        self.exemption
            .as_ref()
            .is_some_and(Exemption::exempts_fields)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use specforge_common::Sym;
    use specforge_test_macros::test as specforge_test;

    fn span() -> SourceSpan {
        SourceSpan {
            file: Sym::new("t.spec"),
            start_line: 1,
            start_col: 1,
            end_line: 1,
            end_col: 1,
        }
    }

    #[specforge_test(
        behavior = "snapshot_entities_once",
        verify = "every field an entity writes has one text, the same for declarative rules, custom validators and compiler passes"
    )]
    fn entity_record_lookups_read_the_last_occurrence() {
        let record = EntityRecord::new("item", "a", &span())
            .with_field("status", "draft")
            .with_field("empty", "")
            .with_list("tags", &["x, y", "z"])
            .with_field("status", "final")
            .with_edges(Direction::Outgoing, Some("Implements"), 2)
            .with_edges(Direction::Outgoing, Some("Emits"), 1)
            .with_edges(Direction::Outgoing, None, 1)
            .with_edges(Direction::Incoming, Some("Contains"), 1);

        // A name written twice reads as its last text; both stay listed.
        assert_eq!(record.field("status"), Some("final"));
        assert_eq!(
            record.field_keys().collect::<Vec<_>>(),
            ["status", "empty", "tags", "status"]
        );
        // Written but empty is written.
        assert!(record.writes("empty"));
        assert_eq!(record.field("empty"), Some(""));
        assert!(!record.writes("missing"));
        assert_eq!(record.field("missing"), None);
        // A list keeps its items unjoined beside the (lossy) joined text.
        let tags = record.fields.iter().find(|f| f.key == "tags").unwrap();
        assert_eq!(tags.text, "x, y, z");
        assert_eq!(tags.shape, ValueShape::Strings);
        assert_eq!(
            tags.items.as_deref(),
            Some(&["x, y".to_string(), "z".to_string()][..])
        );
        // Edge counts, in total and by edge type; an edge of no type counts
        // in the total only.
        assert_eq!(record.edges(Direction::Outgoing, None), 4);
        assert_eq!(record.edges(Direction::Outgoing, Some("Implements")), 2);
        assert_eq!(record.edges(Direction::Outgoing, Some("Emits")), 1);
        assert_eq!(record.edges(Direction::Outgoing, Some("Other")), 0);
        assert_eq!(record.edges(Direction::Incoming, None), 1);
        assert_eq!(record.edges(Direction::Incoming, Some("Contains")), 1);
    }

    #[test]
    fn an_exemption_names_what_it_exempts_from() {
        let record = EntityRecord::new("memo", "m", &span());
        assert!(!record.exempts_statements() && !record.exempts_fields());
        let no_verify = record.clone().exempt(Exemption::NoVerify);
        assert!(no_verify.exempts_statements() && !no_verify.exempts_fields());
        for exemption in [
            Exemption::Union,
            Exemption::Flag {
                field: "abstract".into(),
            },
        ] {
            let exempt = record.clone().exempt(exemption);
            assert!(exempt.exempts_statements() && exempt.exempts_fields());
        }
    }
}
