//! File defaults (ADR 0149): a `defaults <kind> { … }` block gives each of its fields to every block of that kind in
//! its file that does not write the field. The graph build resolves them here, so a node holds the value it means.

use crate::field_form::{self, FieldForms, Reading};
use specforge_common::{Diagnostic, SourceSpan, Sym, codes, structural};
use specforge_parser::{Entity, FieldEntry, FieldMap, SpecFile};
use std::collections::HashSet;

/// One file's defaults: the fields each kind's first `defaults` block applies, as written, in written order.
pub(crate) struct FileDefaults<'f> {
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
    pub(crate) fn of(file: &'f SpecFile, forms: &FieldForms) -> (Self, Vec<Diagnostic>) {
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
                let mut reached = false;
                for block in &blocks {
                    match block.fields.entries().iter().find(|e| e.key == default.key) {
                        None => reached = true,
                        Some(own)
                            if field_form::write(&own.value)
                                == field_form::write(&default.value) =>
                        {
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
                        Some(_) => {}
                    }
                }
                if !reached {
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
                if !entity
                    .fields
                    .entries()
                    .iter()
                    .any(|own| own.key == default.key)
                {
                    fields.push_entry((*default).clone());
                }
            }
        }
        fields
    }
}
