//! File defaults (ADR 0149): a `defaults <kind> { … }` block gives each of its fields to every block of that kind in
//! its file that does not write the field. The graph build resolves them here, so a node holds the value it means.

use crate::field_form::{self, FieldForms, Reading};
use specforge_common::{Diagnostic, SourceSpan, Sym, codes, structural};
use specforge_parser::{Entity, FieldEntry, FieldMap, SpecFile};
use std::collections::HashSet;

/// What a block's field holds (ADR 0149 D3, ADR 0182 D2): its own entry (and the default it replaces, if its file
/// states one), else the default it takes, else nothing. Entries are as written, spans included: an inherited one's
/// spans are in the defaults block (ADR 0149 D9). `E` is `&FieldEntry` from [`FileDefaults::held`]; a caller that must
/// mutate the text it was read from holds `Held<FieldEntry>` ([`Held::cloned`]).
#[derive(Debug, Clone, PartialEq)]
pub enum Held<E> {
    Own { own: E, over: Option<E> },
    Inherited(E),
    Absent,
}

impl<E> Held<E> {
    /// The block's own entry.
    pub fn own(&self) -> Option<&E> {
        match self {
            Self::Own { own, .. } => Some(own),
            _ => None,
        }
    }

    /// The entry the field holds: its own, else the default it takes.
    pub fn entry(&self) -> Option<&E> {
        match self {
            Self::Own { own, .. } => Some(own),
            Self::Inherited(default) => Some(default),
            Self::Absent => None,
        }
    }

    /// Whether the file states a default for the field, taken or replaced.
    pub fn has_default(&self) -> bool {
        matches!(self, Self::Own { over: Some(_), .. } | Self::Inherited(_))
    }
}

impl Held<&FieldEntry> {
    pub fn cloned(&self) -> Held<FieldEntry> {
        match self {
            Self::Own { own, over } => Held::Own {
                own: (*own).clone(),
                over: over.map(Clone::clone),
            },
            Self::Inherited(default) => Held::Inherited((*default).clone()),
            Self::Absent => Held::Absent,
        }
    }
}

/// One file's defaults (ADR 0149): the entry each kind's first `defaults` block applies, and whether a block's own
/// field restates it (W161). The graph build resolves blocks through it; a typed edit asks it over the text it is
/// planning (ADR 0154).
pub struct FileDefaults<'f> {
    by_kind: Vec<(Sym, Vec<&'f FieldEntry>)>,
}

/// Whether `entity` is a `defaults` block: never a node.
pub(crate) fn is_defaults_block(entity: &Entity) -> bool {
    entity.kind.raw == structural::DEFAULTS
}

fn at(entry: &FieldEntry, entity: &Entity) -> SourceSpan {
    entry
        .value_span
        .clone()
        .unwrap_or_else(|| entity.span.clone())
}

impl<'f> FileDefaults<'f> {
    /// `file`'s defaults and what is wrong with them, in written order: E077 (a field the kind does not declare, a
    /// kind no loaded extension declares, a structural kind), E078 (a title, a verify or method member, a field given
    /// twice, a second block for a kind, a block after the file's first entity block) and W161 (a block field that
    /// restates its default; a default no block takes). Without declared kinds (`forms.kinds()` empty) only E078 and
    /// W161 are reported.
    pub fn of(file: &'f SpecFile, forms: &FieldForms) -> (Self, Vec<Diagnostic>) {
        let checked = !forms.kinds().is_empty();
        let mut diagnostics = Vec::new();
        let mut by_kind: Vec<(Sym, Vec<&FieldEntry>)> = Vec::new();
        let mut firsts: Vec<(Sym, &Entity)> = Vec::new();
        let mut first_block: Option<&Entity> = None;
        for entity in &file.entities {
            if !is_defaults_block(entity) {
                first_block.get_or_insert(entity);
                continue;
            }
            let kind = entity.id.raw;
            let error = |message: String, span: SourceSpan| {
                Diagnostic::new(codes::E078, message).with_span(span)
            };
            if entity.title.is_some() {
                diagnostics.push(error(
                    "a defaults block takes no title".into(),
                    entity.span.clone(),
                ));
            }
            if !entity.methods.is_empty() || !entity.obligations.is_empty() {
                diagnostics.push(error(
                    "a defaults block holds only fields".into(),
                    entity.span.clone(),
                ));
            }
            if let Some((_, first)) = firsts.iter().find(|(k, _)| *k == kind) {
                diagnostics.push(error(
                    format!(
                        "a second defaults block for kind '{kind}' in this file (the first at {}:{}) is not applied",
                        first.span.start_line, first.span.start_col
                    ),
                    entity.span.clone(),
                ));
                continue;
            }
            firsts.push((kind, entity));
            if let Some(block) = first_block {
                diagnostics.push(error(
                    format!(
                        "defaults {kind} comes after the file's first block (line {}); move it above every block",
                        block.span.start_line
                    ),
                    entity.span.clone(),
                ));
            }
            if checked
                && (structural::is_structural(kind.as_str())
                    || !forms.kinds().contains(kind.as_str()))
            {
                diagnostics.push(
                    Diagnostic::new(
                        codes::E077,
                        format!("defaults {kind}: no loaded extension declares kind '{kind}'"),
                    )
                    .with_span(entity.span.clone()),
                );
                continue;
            }
            let mut seen: HashSet<Sym> = HashSet::new();
            let mut applied = Vec::new();
            for entry in entity.fields.entries() {
                if !seen.insert(entry.key) {
                    diagnostics.push(error(
                        format!(
                            "'{}' is given twice in defaults {kind}; the first is applied",
                            entry.key
                        ),
                        at(entry, entity),
                    ));
                } else if checked
                    && !matches!(
                        forms.reading(kind.as_str(), entry.key.as_str()),
                        Reading::Declared(_)
                    )
                {
                    diagnostics.push(
                        Diagnostic::new(
                            codes::E077,
                            format!(
                                "defaults {kind}: '{}' is not a field of kind '{kind}'; the default is not applied",
                                entry.key
                            ),
                        )
                        .with_span(at(entry, entity)),
                    );
                } else {
                    applied.push(entry);
                }
            }
            by_kind.push((kind, applied));
        }
        let defaults = Self { by_kind };
        defaults.dead(file, &mut diagnostics);
        (defaults, diagnostics)
    }

    /// Each applied default (kind, entry), in written order: what the finder reads, whether or not a block takes it.
    pub(crate) fn applied(&self) -> impl Iterator<Item = (Sym, &'f FieldEntry)> + '_ {
        self.by_kind
            .iter()
            .flat_map(|(kind, entries)| entries.iter().map(|entry| (*kind, *entry)))
    }

    /// The entry `kind`'s defaults apply for `field`, as written (its spans are in the defaults block): `None` when the
    /// file states none, or states it where it is not applied (an undeclared field or kind, E077; a second block for
    /// the kind; a key given twice, after its first).
    pub fn entry(&self, kind: &str, field: &str) -> Option<&'f FieldEntry> {
        self.by_kind
            .iter()
            .find(|(k, _)| k.as_str() == kind)?
            .1
            .iter()
            .find(|e| e.key.as_str() == field)
            .copied()
    }

    /// What `block` holds for `field`. `block`'s first entry with the key is its own (as `Block::entry` reads it); the
    /// default is [`Self::entry`] of the block's kind, so E077 and E078 are honoured. A `defaults` block holds only its
    /// own entries (no default applies to the kind `defaults`).
    pub fn held<'a>(&'a self, block: &'a Entity, field: &str) -> Held<&'a FieldEntry> {
        let own = block
            .fields
            .entries()
            .iter()
            .find(|e| e.key.as_str() == field);
        let default = if is_defaults_block(block) {
            None
        } else {
            self.entry(block.kind.raw.as_str(), field)
        };
        match (own, default) {
            (Some(own), over) => Held::Own { own, over },
            (None, Some(default)) => Held::Inherited(default),
            (None, None) => Held::Absent,
        }
    }

    /// The blocks of `kind` in `file` that take its default for `field`: each whose [`Self::held`] is `Inherited`, in
    /// file order (a duplicate declaration's block included: one file cannot tell which wins). What the finder holds a
    /// default's tokens by and what W161's "reaches no block" asks.
    pub(crate) fn takers<'a>(
        &'a self,
        file: &'a SpecFile,
        kind: Sym,
        field: &'a str,
    ) -> impl Iterator<Item = &'a Entity> + 'a {
        file.entities.iter().filter(move |block| {
            block.kind.raw == kind && matches!(self.held(block, field), Held::Inherited(_))
        })
    }

    /// Whether `own`, a field a block of `kind` writes, restates its default: both are written alike
    /// (`field_form::write`), so deleting the line changes nothing in the graph's JSON. W161 asks this.
    pub fn restates(&self, kind: &str, own: &FieldEntry) -> bool {
        self.entry(kind, own.key.as_str())
            .is_some_and(|d| field_form::write(&own.value) == field_form::write(&d.value))
    }

    /// Whether `own` holds what its default holds, as the graph build stores both ([`FieldForms::stored`]): deleting
    /// the line leaves the node's value as it is. What a typed edit removes (ADR 0182 D4). Implied by
    /// [`Self::restates`].
    pub fn repeats(&self, kind: &str, own: &FieldEntry, forms: &FieldForms) -> bool {
        self.entry(kind, own.key.as_str()).is_some_and(|default| {
            field_form::write(&forms.stored(kind, own))
                == field_form::write(&forms.stored(kind, default))
        })
    }

    /// W161: a block field equal to its default, and a default no block of its kind takes.
    fn dead(&self, file: &SpecFile, diagnostics: &mut Vec<Diagnostic>) {
        for (kind, applied) in &self.by_kind {
            let blocks: Vec<&Entity> = file
                .entities
                .iter()
                .filter(|e| e.kind.raw == *kind && !is_defaults_block(e))
                .collect();
            let Some(defaults) = file
                .entities
                .iter()
                .find(|e| is_defaults_block(e) && e.id.raw == *kind)
            else {
                continue;
            };
            for default in applied {
                for block in &blocks {
                    match self.held(block, default.key.as_str()) {
                        Held::Own { own, .. } if self.restates(kind.as_str(), own) => {
                            diagnostics.push(
                                Diagnostic::new(
                                    codes::W161,
                                    format!(
                                        "field '{}' of {kind} '{}' restates its file's default; delete the line",
                                        own.key, block.id.raw
                                    ),
                                )
                                .with_span(at(own, block)),
                            );
                        }
                        _ => {}
                    }
                }
                if self
                    .takers(file, *kind, default.key.as_str())
                    .next()
                    .is_none()
                {
                    diagnostics.push(
                        Diagnostic::new(
                            codes::W161,
                            format!(
                                "the default '{}' of {kind} reaches no block in this file",
                                default.key
                            ),
                        )
                        .with_span(at(default, defaults)),
                    );
                }
            }
        }
    }

    /// `entity`'s fields: its own entries in written order, then each applied default of its kind whose key it does
    /// not write, in the default's order, as written (spans in the defaults block).
    pub(crate) fn resolve(&self, entity: &Entity) -> FieldMap {
        let mut fields = entity.fields.clone();
        if let Some((_, applied)) = self
            .by_kind
            .iter()
            .find(|(kind, _)| *kind == entity.kind.raw)
        {
            for default in applied {
                if let Held::Inherited(default) = self.held(entity, default.key.as_str()) {
                    fields.push_entry(default.clone());
                }
            }
        }
        fields
    }
}
