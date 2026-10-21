//! Which tokens of a file name an entity, read the way the graph build reads them (ADR 0163): the one finder rename,
//! the editor's find-references and a delete's referrer scan ask, over the compiled parses or the texts a plan holds.

use crate::build::is_define_block;
use crate::defaults::{is_defaults_block, takers};
use crate::derive::derived_names;
use crate::{FieldForms, FileDefaults, GraphInputs, Reading};
use specforge_common::{SourceSpan, Sym};
use specforge_parser::{FieldEntry, MethodDecl, SpecFile};
use std::borrow::Cow;

/// Who writes a token.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Holder {
    /// A block, by its id.
    Block(Sym),
    /// The file's `defaults` block for a kind (ADR 0149): no node, and no typed edit writes it (ADR 0151 D5).
    Defaults(Sym),
}

/// How a token is written: what a typed edit can do about it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Form {
    /// The value of a declared `reference` field: `unset` removes it.
    Single,
    /// An item of a declared list field, or a list written as one value (ADR 0156 D1): `unlink` removes it.
    Item,
    /// A bare word or scheme ref ID in a field its kind does not declare (ADR 0093 D5).
    Undeclared,
    /// A type name a derived field links through (ADR 0127 D3): in a field written as a type, or a method signature.
    Derived,
}

/// One token of a file that names an entity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WrittenToken {
    /// The id it names, as written.
    pub target: Sym,
    pub holder: Holder,
    /// The blocks whose node holds it as a reference, sorted, once each: its block; each block that takes the default
    /// (`Holder::Defaults`), but not one a derived name of the default names (a block never derives an edge to itself);
    /// none for a default no block takes. The graph build links exactly these (the debug assertion of `GraphBuild`).
    pub held_by: Vec<Sym>,
    /// The entry's key, or the derived field a type name links through.
    pub field: Sym,
    pub span: SourceSpan,
    pub form: Form,
}

/// Every token of `file` that names an entity, in written order, read the way the graph build reads it:
/// - each applied entry of a `defaults` block, whether or not a block takes it (an E077 or E078 entry is not applied
///   and names nothing, ADR 0149 D4/D5), as [`Holder::Defaults`];
/// - each block's own entries, as the build stores them (coerced once, ADR 0163): their references
///   ([`Reading::references`], ADR 0093) and the type names a derived field of the holder's kind reads in them, that
///   name an entity its target admits, `kind_of` answering an entity's kind (`None`: no such entity); never the
///   block's own id;
/// - each names the blocks that hold it (`held_by`).
///
/// A `define` block (never a node, W143) is skipped. A reference without a span (a value built without a position)
/// is not a token. Unlike the linker, a declared reference is a token whatever it resolves to (E003, E022): it is
/// written there. A duplicate declaration (E002, W060) is a block like any other: one file cannot tell which wins.
pub fn written_tokens(
    file: &SpecFile,
    inputs: &GraphInputs,
    kind_of: impl Fn(&str) -> Option<Sym>,
) -> Vec<WrittenToken> {
    let forms = &inputs.field_forms;
    let (defaults, _) = FileDefaults::of(file, forms);
    let read = Reader {
        inputs,
        kind_of: &kind_of,
    };
    let mut tokens = Vec::new();
    for (kind, entry) in defaults.applied() {
        let holder = Holder::Defaults(kind);
        let entries = stored(forms, kind.as_str(), std::slice::from_ref(entry));
        for entry in entries.iter() {
            read_entry(forms, kind.as_str(), entry, holder, &mut tokens);
        }
        read.derived(kind.as_str(), &entries, &[], holder, None, &mut tokens);
    }
    for entity in &file.entities {
        if is_defaults_block(entity) || is_define_block(entity) {
            continue;
        }
        let (id, kind) = (entity.id.raw, entity.kind.raw.as_str());
        let holder = Holder::Block(id);
        let entries = stored(forms, kind, entity.fields.entries());
        for entry in entries.iter() {
            read_entry(forms, kind, entry, holder, &mut tokens);
        }
        read.derived(
            kind,
            &entries,
            &entity.methods,
            holder,
            Some(id),
            &mut tokens,
        );
    }
    let at = |t: &WrittenToken| {
        (
            t.span.start_line,
            t.span.start_col,
            t.span.end_line,
            t.span.end_col,
        )
    };
    tokens.sort_by_key(at);
    // A name the build reads twice at one span (a default's derived name) is one token.
    tokens.dedup();
    for token in &mut tokens {
        token.held_by = match token.holder {
            Holder::Block(id) => vec![id],
            Holder::Defaults(kind) => {
                let mut ids: Vec<Sym> = takers(file, kind, token.field)
                    .map(|block| block.id.raw)
                    // A block never derives an edge to itself (`link_derived_references`).
                    .filter(|id| token.form != Form::Derived || *id != token.target)
                    .collect();
                ids.sort_unstable();
                ids.dedup();
                ids
            }
        };
    }
    tokens
}

/// `entries` as the graph build stores them (`FieldForms::stored`): borrowed when no value is coerced.
fn stored<'e>(forms: &FieldForms, kind: &str, entries: &'e [FieldEntry]) -> Cow<'e, [FieldEntry]> {
    let mut owned: Option<Vec<FieldEntry>> = None;
    for (i, entry) in entries.iter().enumerate() {
        if let Cow::Owned(value) = forms.stored(kind, entry) {
            owned.get_or_insert_with(|| entries.to_vec())[i].value = value;
        }
    }
    owned.map_or(Cow::Borrowed(entries), Cow::Owned)
}

/// What a derived name needs from the graph: its inputs and the kind of an entity.
struct Reader<'a> {
    inputs: &'a GraphInputs,
    kind_of: &'a dyn Fn(&str) -> Option<Sym>,
}

impl Reader<'_> {
    /// The type names a derived field of `kind` reads from `entries` and `methods`.
    fn derived(
        &self,
        kind: &str,
        entries: &[FieldEntry],
        methods: &[MethodDecl],
        holder: Holder,
        own: Option<Sym>,
        out: &mut Vec<WrittenToken>,
    ) {
        let forms = &self.inputs.field_forms;
        for reference in &self.inputs.derived_references {
            if reference.source_kind != kind {
                continue;
            }
            for (name, at) in derived_names(reference, forms, entries, methods, own, self.kind_of) {
                let Some(at) = at else { continue };
                out.push(WrittenToken {
                    target: name,
                    holder,
                    held_by: Vec::new(),
                    field: Sym::new(&reference.field),
                    span: at,
                    form: Form::Derived,
                });
            }
        }
    }
}

/// The references `entry` writes; `entry` is already as the graph build stores it (see [`stored`]).
fn read_entry(
    forms: &FieldForms,
    kind: &str,
    entry: &FieldEntry,
    holder: Holder,
    out: &mut Vec<WrittenToken>,
) {
    let reading = forms.reading(kind, entry.key.as_str());
    let form = match reading {
        _ if reading.is_single_reference() => Form::Single,
        Reading::Undeclared => Form::Undeclared,
        _ => Form::Item,
    };
    for reference in reading.references(entry) {
        if let Some(span) = reference.span {
            out.push(WrittenToken {
                target: Sym::new(reference.id),
                holder,
                held_by: Vec::new(),
                field: entry.key,
                span: span.clone(),
                form,
            });
        }
    }
}
