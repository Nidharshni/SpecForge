//! A declared field type's one reading (ADR 0064, ADR 0093): which written values a
//! field takes, how the graph build stores a written value, the JSON every
//! output writes for a field value, and the JSON Schema of that JSON. The
//! graph build's coercion, E061, the export writer, the graph delta and the
//! published schema read it; nothing else decides a field value's form.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use serde::Serialize;
use serde_json::{Map, Value, json};
use specforge_common::SourceSpan;
use specforge_common::find_close_match;
use specforge_common::shape::{Definitions, Shape};
use specforge_parser::{FieldEntry, FieldMap, FieldValue, ListItem, Written};
use specforge_protocol_types::FieldType;

use crate::Graph;

pub mod spell;

/// What a field's value was written as, after the graph build coerced it:
/// one variant per parsed value form, so a check tells `"1"` from `1`, a
/// one-item list from a scalar and a quoted string from a bare word
/// (ADR 0019, ADR 0031).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValueShape {
    /// `"text"`.
    String,
    /// A bare word: an identifier, or a single reference.
    Identifier,
    Date,
    Integer,
    Boolean,
    /// `[a, "b", 1]`: a list, however its items were written.
    List,
    /// A variant list (`a | b` in a body).
    Variants,
    TypeUnion,
    Block,
    /// An `expr { … }` group.
    Expression,
}

impl ValueShape {
    /// The shape of `value`: one per `FieldValue` variant, no `_` arm, so a
    /// new variant does not compile until it has one.
    pub fn of(value: &FieldValue) -> Self {
        match value {
            FieldValue::String(_) => Self::String,
            FieldValue::Identifier(_) => Self::Identifier,
            FieldValue::Date(_) => Self::Date,
            FieldValue::Integer(_) => Self::Integer,
            FieldValue::Boolean(_) => Self::Boolean,
            FieldValue::List(_) => Self::List,
            FieldValue::VariantList(_) => Self::Variants,
            FieldValue::TypeUnion(_) => Self::TypeUnion,
            FieldValue::Block(_) => Self::Block,
            FieldValue::Expression(_) => Self::Expression,
        }
    }

    /// A list, or a variant list.
    pub fn is_list(self) -> bool {
        matches!(self, Self::List | Self::Variants)
    }
}

/// A field's declared type and, for an enum, its declared values: which
/// stored values the field takes ([`Self::admit`]), how the graph build
/// stores a written value ([`Self::coerce`]), and the JSON Schema of what
/// [`write`] writes for every value it takes ([`Self::json_schema`]).
///
/// Invariant (the module's property test): for every written value `v`,
/// with `stored = coerce(v).unwrap_or(v)`, `admit(ValueShape::of(&stored),
/// text)` is `Ok` only if `write(&stored)` conforms to `json_schema()`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldForm {
    field_type: FieldType,
    /// Empty unless `field_type` is `Enum`.
    enum_values: Vec<String>,
    /// The kind its references must name (the descriptor's `target_kind`), if it declares one.
    target_kind: Option<String>,
}

/// Why a field does not take a value: the remedy E061 suggests, if any.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refusal {
    pub suggestion: Option<String>,
}

impl Refusal {
    fn with(suggestion: &str) -> Self {
        Self {
            suggestion: Some(suggestion.to_string()),
        }
    }

    fn none() -> Self {
        Self { suggestion: None }
    }
}

impl FieldForm {
    /// `enum_values` are an enum's declared values; any other type drops them.
    pub fn new(field_type: FieldType, enum_values: Vec<String>) -> Self {
        let enum_values = match field_type {
            FieldType::Enum => enum_values,
            _ => Vec::new(),
        };
        Self {
            field_type,
            enum_values,
            target_kind: None,
        }
    }

    /// The same form, whose references must name `kind` entities (ADR 0117).
    pub fn targeting(mut self, kind: &str) -> Self {
        self.target_kind = Some(kind.to_string());
        self
    }

    /// The kind its references must name, if its descriptor declares one. Whether a loaded
    /// extension declares that kind is [`FieldForms::target`]'s answer.
    pub fn target_kind(&self) -> Option<&str> {
        self.target_kind.as_deref()
    }

    pub fn field_type(&self) -> FieldType {
        self.field_type
    }

    /// How a value of this type is written (ADR 0147): `Bare` for an `enum`,
    /// `identifier`, `reference` or `reference_list` item; `Quoted` for a
    /// `string` or `string_list` item; `None` for the rest.
    pub fn written(&self) -> Option<Written> {
        match self.field_type {
            FieldType::Enum
            | FieldType::Identifier
            | FieldType::Reference
            | FieldType::ReferenceList => Some(Written::Bare),
            FieldType::String | FieldType::StringList => Some(Written::Quoted),
            FieldType::Integer | FieldType::Bool | FieldType::Block => None,
        }
    }

    /// An enum's declared values; empty for every other type.
    pub fn enum_values(&self) -> &[String] {
        &self.enum_values
    }

    /// What the graph build stores for `value` when it is not stored as
    /// written; `None` keeps the parsed value. Never diagnoses (what still
    /// cannot be the type is E061's); idempotent. `span` is the value's own,
    /// which a single reference keeps when it becomes a one-item list.
    ///
    /// - `string_list`, `reference_list`: a list is stored as written; a
    ///   bare word, string, integer, boolean or date becomes the one-item
    ///   list `field [value]` parses to.
    /// - `integer`: a quoted integer becomes the integer.
    /// - `bool`: a quoted or bare `true` / `false` becomes the boolean.
    /// - `string`, `enum`: an integer or boolean becomes its text.
    /// - `reference`, `block`: stored as written.
    pub fn coerce(&self, value: &FieldValue, span: Option<&SourceSpan>) -> Option<FieldValue> {
        match (self.field_type, value) {
            (FieldType::StringList | FieldType::ReferenceList, value) => coerce_list(value, span),
            (FieldType::Integer, FieldValue::String(s)) => {
                s.trim().parse::<i64>().ok().map(FieldValue::Integer)
            }
            (FieldType::Bool, FieldValue::String(s) | FieldValue::Identifier(s)) => {
                match s.as_str() {
                    "true" => Some(FieldValue::Boolean(true)),
                    "false" => Some(FieldValue::Boolean(false)),
                    _ => None,
                }
            }
            (
                FieldType::String | FieldType::Enum | FieldType::Identifier,
                FieldValue::Integer(n),
            ) => Some(FieldValue::String(n.to_string())),
            (
                FieldType::String | FieldType::Enum | FieldType::Identifier,
                FieldValue::Boolean(b),
            ) => Some(FieldValue::String(b.to_string())),
            _ => None,
        }
    }

    /// Whether a stored value of `shape`, whose field text is `text`, is one
    /// this field takes. `Err` carries E061's remedy.
    ///
    /// | declared | takes |
    /// |---|---|
    /// | `string`, `enum` without values | text, a bare word, a date, a type union, an expression group |
    /// | `enum` | a string or bare word that is a declared value |
    /// | `identifier` | one word (letters, digits and `_`, starting with a letter or `_`), bare or quoted |
    /// | `integer` | an integer |
    /// | `bool` | `true` or `false` |
    /// | `reference` | a bare word or a string |
    /// | `string_list`, `reference_list` | a list, however its items were written |
    /// | `block` | a block |
    pub fn admit(&self, shape: ValueShape, text: &str) -> Result<(), Refusal> {
        let single_value = || Refusal::with("give a single value, not a list");
        match self.field_type {
            FieldType::Integer => match shape {
                ValueShape::Integer => Ok(()),
                _ => Err(Refusal::none()),
            },
            FieldType::Bool => match shape {
                ValueShape::Boolean => Ok(()),
                _ => Err(Refusal::with("use true or false")),
            },
            FieldType::Enum if !self.enum_values.is_empty() => {
                let values = &self.enum_values;
                let one_of = || Refusal::with(&format!("use one of: {}", values.join(", ")));
                match shape {
                    ValueShape::String | ValueShape::Identifier
                        if values.iter().any(|v| v == text) =>
                    {
                        Ok(())
                    }
                    ValueShape::String | ValueShape::Identifier => Err(Refusal::with(
                        &match find_close_match(text, values.iter().map(String::as_str)) {
                            Some(close) => format!("did you mean '{close}'?"),
                            None => format!("use one of: {}", values.join(", ")),
                        },
                    )),
                    _ => Err(one_of()),
                }
            }
            FieldType::String | FieldType::Enum => match shape {
                ValueShape::String
                | ValueShape::Identifier
                | ValueShape::Date
                | ValueShape::TypeUnion
                | ValueShape::Expression => Ok(()),
                s if s.is_list() => Err(single_value()),
                _ => Err(Refusal::with("write the value as text")),
            },
            FieldType::Identifier => match shape {
                ValueShape::String | ValueShape::Identifier if is_word(text) => Ok(()),
                s if s.is_list() => Err(single_value()),
                _ => Err(Refusal::with(
                    "write one word: letters, digits and _, starting with a letter or _",
                )),
            },
            FieldType::Reference => match shape {
                ValueShape::Identifier | ValueShape::String => Ok(()),
                s if s.is_list() => Err(single_value()),
                _ => Err(Refusal::with("name one entity by its id")),
            },
            FieldType::StringList | FieldType::ReferenceList => match shape {
                ValueShape::List | ValueShape::Variants => Ok(()),
                _ => Err(Refusal::with("write a list: [a, b]")),
            },
            FieldType::Block => match shape {
                ValueShape::Block => Ok(()),
                _ => Err(Refusal::with("write a block: { name value }")),
            },
        }
    }

    /// The JSON Schema of what [`write`] writes for every value
    /// [`Self::admit`] takes: `{"type":"string"}` for `string`, `reference`
    /// and a valueless `enum`; `{"type":"string","enum":[…]}` for an `enum`;
    /// `{"type":"integer"}`; `{"type":"boolean"}`;
    /// `{"type":"array","items":{"type":"string"}}` for both lists;
    /// `{"type":"object","additionalProperties":<FieldJson>}` for a `block`
    /// (its entries are undeclared values), `FieldJson` defined in `defs`.
    pub fn json_schema(&self, defs: &mut Definitions) -> Value {
        match self.field_type {
            // An identifier's one-word rule is `admit`'s: the in-repo schema checker reads no `pattern`.
            FieldType::String | FieldType::Reference | FieldType::Identifier => {
                json!({"type": "string"})
            }
            FieldType::Enum if self.enum_values.is_empty() => json!({"type": "string"}),
            FieldType::Enum => json!({"type": "string", "enum": self.enum_values}),
            FieldType::Integer => json!({"type": "integer"}),
            FieldType::Bool => json!({"type": "boolean"}),
            FieldType::StringList | FieldType::ReferenceList => {
                json!({"type": "array", "items": {"type": "string"}})
            }
            FieldType::Block => {
                json!({"type": "object", "additionalProperties": FieldJson::schema(defs)})
            }
        }
    }
}

/// Whether `text` is one word: the grammar's identifier (letters, digits and
/// `_`, starting with a letter or `_`).
pub fn is_word(text: &str) -> bool {
    let mut chars = text.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// A value written in the other form than its field's: where it is (quotes
/// included), its text, and the form it takes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rewrite {
    pub span: SourceSpan,
    pub text: String,
    pub to: Written,
}

/// Every registered field's form, and the kinds that take any field: what
/// the graph build and every reader ask what a field's value is and
/// references (ADR 0093). Built once per registry build.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FieldForms {
    forms: HashMap<(String, String), FieldForm>,
    member_kinds: BTreeSet<String>,
    /// Every kind a loaded extension declares.
    kinds: BTreeSet<String>,
}

impl FieldForms {
    /// `forms` maps (kind, field) to the field's form; `member_kinds` are
    /// the kinds that declare `open_fields`; `kinds` are every kind a loaded
    /// extension declares (what makes a form's target [`Target::Loaded`]).
    pub fn new(
        forms: HashMap<(String, String), FieldForm>,
        member_kinds: BTreeSet<String>,
        kinds: BTreeSet<String>,
    ) -> Self {
        Self {
            forms,
            member_kinds,
            kinds,
        }
    }

    /// The kind `kind.field`'s references must name, and whether a loaded
    /// extension declares it. `None` for a field with no declared target
    /// kind, a member of an open-fields kind, or an undeclared field.
    pub fn target(&self, kind: &str, field: &str) -> Option<Target<'_>> {
        let Reading::Declared(form) = self.reading(kind, field) else {
            return None;
        };
        let target = form.target_kind()?;
        Some(if self.kinds.contains(target) {
            Target::Loaded(target)
        } else {
            Target::Absent(target)
        })
    }

    /// Every declared (kind, field) with a target, in (kind, field) order.
    pub fn targets(&self) -> Vec<((&str, &str), Target<'_>)> {
        let mut targets: Vec<_> = self
            .forms
            .keys()
            .filter_map(|(kind, field)| {
                Some(((kind.as_str(), field.as_str()), self.target(kind, field)?))
            })
            .collect();
        targets.sort_by(|a, b| a.0.cmp(&b.0));
        targets
    }

    /// How `kind`'s field `field` reads: its declared form, an undeclared
    /// field of a kind that takes any field, or no form at all.
    pub fn reading(&self, kind: &str, field: &str) -> Reading<'_> {
        if let Some(form) = self.forms.get(&(kind.to_string(), field.to_string())) {
            Reading::Declared(form)
        } else if self.member_kinds.contains(kind) {
            Reading::Member
        } else {
            Reading::Undeclared
        }
    }

    /// The form `kind`'s field `field` writes a value in, when `written`/`text`
    /// is not it (ADR 0147); `None` leaves the value as written: the field
    /// has no declared form, the value is already in it or is neither bare
    /// nor quoted, a bare form cannot hold `text` ([`spell::holds_bare`]: not
    /// a word (on a `reference_list` not an entity id), or `true` / `false`, which would read back as a
    /// boolean), or a bare word sits on
    /// a kind that takes members (it may be a member's type).
    pub fn rewrite(
        &self,
        kind: &str,
        field: &str,
        written: Written,
        text: &str,
    ) -> Option<Written> {
        let Reading::Declared(form) = self.reading(kind, field) else {
            return None;
        };
        let to = form.written()?;
        if written == to || !matches!(written, Written::Bare | Written::Quoted) {
            return None;
        }
        let holds = match to {
            Written::Bare => spell::holds_bare(form.field_type(), text),
            _ => !self.member_kinds.contains(kind),
        };
        holds.then_some(to)
    }

    /// Every value of `entry` (a scalar, or each list item) that
    /// [`Self::rewrite`] would write otherwise, in source order. A scalar
    /// without a value span is reported at `fallback`.
    pub fn rewrites(&self, kind: &str, entry: &FieldEntry, fallback: &SourceSpan) -> Vec<Rewrite> {
        let field = entry.key.as_str();
        let one = |written, text: &str, span: &SourceSpan| {
            self.rewrite(kind, field, written, text).map(|to| Rewrite {
                span: span.clone(),
                text: text.to_string(),
                to,
            })
        };
        let span = entry.value_span.as_ref().unwrap_or(fallback);
        match &entry.value {
            FieldValue::String(text) => one(Written::Quoted, text, span).into_iter().collect(),
            FieldValue::Identifier(text) => one(Written::Bare, text, span).into_iter().collect(),
            FieldValue::List(items) => items
                .iter()
                .filter_map(|item| one(item.written, &item.text, &item.span))
                .collect(),
            _ => Vec::new(),
        }
    }

    /// No field is declared.
    pub fn is_empty(&self) -> bool {
        self.forms.is_empty()
    }

    /// Every declared (kind, field) and its form.
    pub fn iter(&self) -> impl Iterator<Item = (&(String, String), &FieldForm)> {
        self.forms.iter()
    }

    /// Every kind a loaded extension declares; empty when none is loaded.
    pub fn kinds(&self) -> &BTreeSet<String> {
        &self.kinds
    }

    /// The kinds that take any field (`open_fields`).
    pub fn member_kinds(&self) -> &BTreeSet<String> {
        &self.member_kinds
    }
}

/// The kind a reference field's ids must name (ADR 0117).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target<'f> {
    /// A loaded extension declares the kind.
    Loaded(&'f str),
    /// No loaded extension declares the kind.
    Absent(&'f str),
}

impl<'f> Target<'f> {
    pub fn kind(&self) -> &'f str {
        match self {
            Self::Loaded(kind) | Self::Absent(kind) => kind,
        }
    }

    /// Whether an entity of `kind` is one this target takes: the target's own kind, loaded or not.
    pub fn admits(&self, kind: &str) -> bool {
        self.kind() == kind
    }
}

/// One id a field's value references and where it is written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Reference<'v> {
    pub id: &'v str,
    pub span: Option<&'v SourceSpan>,
}

/// How a (kind, field) reads: see [`FieldForms::reading`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reading<'f> {
    /// The field is declared; its form decides.
    Declared(&'f FieldForm),
    /// An undeclared field of a kind that takes any field.
    Member,
    /// No form: a closed kind's unknown field, an unregistered kind, a
    /// project without extensions.
    Undeclared,
}

impl Reading<'_> {
    /// What the graph build stores for `value`: the form's coercion when
    /// declared, else `None` (as written).
    pub fn coerce(&self, value: &FieldValue, span: Option<&SourceSpan>) -> Option<FieldValue> {
        match self {
            Self::Declared(form) => form.coerce(value, span),
            Self::Member | Self::Undeclared => None,
        }
    }

    /// What `entry`'s value references, in written order, each with its
    /// span (ADR 0093). The one rule every reader asks:
    ///
    /// | reading | a single value | a list item |
    /// |---|---|---|
    /// | declared `reference` | the value, bare or quoted | none |
    /// | declared `reference_list` | none (the build stores it as a list) | every item |
    /// | declared `string_list` | none | a scheme ref ID |
    /// | other declared types, a member | none | none |
    /// | undeclared | none | a bare word or a scheme ref ID |
    pub fn references<'v>(&self, entry: &'v FieldEntry) -> Vec<Reference<'v>> {
        let item = |item: &'v ListItem| Reference {
            id: item.text.as_str(),
            span: Some(&item.span),
        };
        match (self, &entry.value) {
            (Self::Declared(form), FieldValue::Identifier(id) | FieldValue::String(id))
                if form.field_type() == FieldType::Reference =>
            {
                vec![Reference {
                    id: id.as_str(),
                    span: entry.value_span.as_ref(),
                }]
            }
            (Self::Declared(form), FieldValue::List(items)) => match form.field_type() {
                FieldType::ReferenceList => items.iter().map(item).collect(),
                FieldType::StringList => items
                    .iter()
                    .filter(|i| i.written == Written::SchemeRef)
                    .map(item)
                    .collect(),
                _ => Vec::new(),
            },
            (Self::Undeclared, FieldValue::List(items)) => items
                .iter()
                .filter(|i| matches!(i.written, Written::Bare | Written::SchemeRef))
                .map(item)
                .collect(),
            _ => Vec::new(),
        }
    }

    /// Whether a name written as `written`, as a value of the field or as
    /// an item of its list, may name an entity: the editor's
    /// reference positions. Any name on a declared `reference` or
    /// `reference_list`; a scheme ref ID on a `string_list`; nothing on
    /// another declared type; a bare word or scheme ref ID on a member or
    /// an undeclared field (what its word names, as the graph reads it).
    pub fn may_name(&self, written: Written) -> bool {
        match self {
            Self::Declared(form) => match form.field_type() {
                FieldType::Reference | FieldType::ReferenceList => true,
                FieldType::StringList => written == Written::SchemeRef,
                _ => false,
            },
            Self::Member | Self::Undeclared => {
                matches!(written, Written::Bare | Written::SchemeRef)
            }
        }
    }

    /// Whether the field is a declared `reference`: its edges link after
    /// reference cycles are detected.
    pub fn is_single_reference(&self) -> bool {
        matches!(self, Self::Declared(form) if form.field_type() == FieldType::Reference)
    }
}

/// The one-item list a single written value is stored as on a
/// `string_list` / `reference_list` field; a list is stored as written.
fn coerce_list(value: &FieldValue, span: Option<&SourceSpan>) -> Option<FieldValue> {
    let (text, written) = match value {
        FieldValue::String(s) => (s.clone(), Written::Quoted),
        FieldValue::Identifier(id) => (id.clone(), Written::Bare),
        FieldValue::Date(s) => (s.clone(), Written::Literal),
        FieldValue::Integer(n) => (n.to_string(), Written::Literal),
        FieldValue::Boolean(b) => (b.to_string(), Written::Literal),
        _ => return None,
    };
    Some(FieldValue::List(vec![ListItem {
        text,
        span: span?.clone(),
        written,
    }]))
}

impl Graph {
    /// Store every node's field values as their forms coerce them. Idempotent,
    /// so rebuilds may re-run it on already coerced nodes.
    pub(crate) fn coerce_field_values(&mut self, forms: &FieldForms) {
        if forms.is_empty() {
            return;
        }
        for node in self.nodes_mut() {
            let kind = node.kind.raw.as_str().to_string();
            for entry in node.fields.entries_mut() {
                let reading = forms.reading(&kind, entry.key.as_str());
                if let Some(value) = reading.coerce(&entry.value, entry.value_span.as_ref()) {
                    entry.value = value;
                }
            }
        }
    }
}

/// A field value as every output writes it: the Graph Protocol's field
/// value. Made only by [`write`]. Its shape (named `FieldJson`) is the schema
/// of every value `write` can produce, declared or not: a string, an
/// integer, a boolean, an array of strings, integers and booleans, or an object whose values are `FieldJson`.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(transparent)]
pub struct FieldJson(Value);

impl From<FieldJson> for Value {
    fn from(written: FieldJson) -> Self {
        written.0
    }
}

impl Shape for FieldJson {
    const NAME: Option<&'static str> = Some("FieldJson");

    fn body(defs: &mut Definitions) -> Value {
        let this = defs.reference::<FieldJson>("FieldJson");
        json!({"anyOf": [
            {"type": "string"},
            {"type": "integer"},
            {"type": "boolean"},
            {"type": "array", "items": {"anyOf": [
                {"type": "string"},
                {"type": "integer"},
                {"type": "boolean"},
            ]}},
            {"type": "object", "additionalProperties": this},
        ]})
    }
}

/// `value` as every output writes it, whatever its field declares: text (a
/// string, an identifier, a date) as a string; a type union as its members
/// joined by `" | "`; an expression group as its conjuncts, each in its
/// display form, joined by `"\n"` (the string form the prove pass reads, one
/// comparison per line, ADR 0009); an integer and a boolean as JSON scalars;
/// a list or variant list as an array of its items' text; a block
/// as an object of its entries, each written by this function. Never the
/// parser's tree, never a position. The field text of the same value
/// (`specforge_registry::snapshot::field_text`) joins an expression group with `", "`.
pub fn write(value: &FieldValue) -> FieldJson {
    FieldJson(match value {
        FieldValue::String(s) | FieldValue::Identifier(s) | FieldValue::Date(s) => {
            Value::String(s.clone())
        }
        FieldValue::TypeUnion(types) => Value::String(
            types
                .iter()
                .map(|m| m.text.as_str())
                .collect::<Vec<_>>()
                .join(" | "),
        ),
        FieldValue::Expression(conjuncts) => Value::String(
            conjuncts
                .iter()
                .map(|e| e.to_string())
                .collect::<Vec<_>>()
                .join("\n"),
        ),
        FieldValue::Integer(n) => Value::Number((*n).into()),
        FieldValue::Boolean(b) => Value::Bool(*b),
        FieldValue::List(items) => Value::Array(
            items
                .iter()
                .map(|item| Value::String(item.text.clone()))
                .collect(),
        ),
        FieldValue::VariantList(items) => Value::Array(
            items
                .iter()
                .map(|m| Value::String(m.text.clone()))
                .collect(),
        ),
        FieldValue::Block(inner) => Value::Object(
            write_fields(inner)
                .into_iter()
                .map(|(key, written)| (key, written.0))
                .collect::<Map<_, _>>(),
        ),
    })
}

/// Every field of `fields` written by [`write`], keyed by name (a name
/// written twice keeps its last value).
pub fn write_fields(fields: &FieldMap) -> BTreeMap<String, FieldJson> {
    fields
        .entries()
        .iter()
        .map(|entry| (entry.key.to_string(), write(&entry.value)))
        .collect()
}
