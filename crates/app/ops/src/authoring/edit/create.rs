//! Creating and deleting entities (ADR 0151): a new block goes after the last block of its kind in its file, laid
//! out canonically; a deleted block takes the comment lines above it and one blank line with it.

use super::block::{self, Block, Site, group_of_text, items_of, rank};
use super::write::{quoted, written};
use super::{EditField, Input, Obligation, Planner};
use crate::authoring::rename::{self, validate_id};
use crate::{OpError, OpErrorKind, Refusal};
use specforge_graph::FieldValue;
use specforge_graph::field_form::Reading;
use specforge_parser::parse;
use specforge_protocol_types::FieldType;
use std::collections::BTreeSet;

/// `Create`'s file is not a project source.
pub const NOT_A_SOURCE: &str = "not_a_source";

/// Whether `input` is the value `default` holds, so a block need not write it.
fn is_default(default: &FieldValue, input: &Input) -> bool {
    match (default, input) {
        (FieldValue::List(items), Input::Many(given)) => {
            items.len() == given.len() && items.iter().zip(given).all(|(i, g)| &i.text == g)
        }
        (FieldValue::List(items), Input::One(given)) => {
            let given: Vec<&str> = given
                .split(',')
                .map(str::trim)
                .filter(|g| !g.is_empty())
                .collect();
            items.len() == given.len() && items.iter().zip(given).all(|(i, g)| i.text == g)
        }
        (FieldValue::String(s) | FieldValue::Identifier(s), Input::One(given)) => s == given,
        _ => false,
    }
}

impl Planner<'_> {
    pub(super) fn create(
        &mut self,
        kind: &str,
        id: &str,
        title: &str,
        file: &str,
        fields: &std::collections::BTreeMap<String, Input>,
        verify: &[Obligation],
    ) -> Result<(), Refusal<EditField>> {
        validate_id(id).map_err(|e| e.about(EditField::Id))?;
        let graph = self.view.graph();
        if (graph.node(id).is_some() && !self.deleted.contains(id)) || self.created.contains_key(id)
        {
            return Err(OpError::new(
                OpErrorKind::Conflict,
                rename::TAKEN,
                format!("cannot create '{id}': an entity with that id exists"),
            )
            .with_entity(id)
            .about(EditField::Id));
        }
        self.view
            .kinds()
            .declared(kind)
            .map_err(|e| e.about(EditField::Kind))?;
        if title.trim().is_empty() {
            return Err(OpError::new(
                OpErrorKind::InvalidInput,
                "invalid_title",
                "a block needs a title",
            )
            .about(EditField::Title));
        }
        if !self.view.env().holds_source(file) {
            return Err(OpError::new(
                OpErrorKind::InvalidInput,
                NOT_A_SOURCE,
                format!("'{file}' is not a source of this project (a .spec file inside the spec root, not excluded)"),
            )
            .about(EditField::File));
        }
        let existing = self.text(file).unwrap_or_default();
        let parsed = parse(&existing, file);
        let site = Site {
            view: self.view,
            key: file,
            id,
            kind,
            resolved: None,
        };
        let order = site.order();
        let mut members: Vec<((u8, usize), String, String)> = Vec::new();
        for (name, input) in fields {
            let reading = site.reading(name)?;
            if let Reading::Declared(_) = reading
                && super::defaults::default_entry(&parsed, kind, name)
                    .is_some_and(|d| is_default(&d.value, input))
            {
                continue;
            }
            let value = written(reading, name, input, 2)?;
            members.push((
                rank(&order, name, group_of_text(&value)),
                name.clone(),
                value,
            ));
        }
        members.sort();
        let mut lines = vec![format!("{kind} {id} {} {{", quoted(title))];
        lines.extend(
            members
                .iter()
                .map(|(_, name, value)| format!("  {name} {value}")),
        );
        lines.extend(verify.iter().map(|o| {
            let kind = o
                .kind
                .as_deref()
                .map(|k| format!("{k} "))
                .unwrap_or_default();
            format!("  verify {kind}{}", quoted(&o.text))
        }));
        lines.push("}".to_string());
        let text = place(&existing, &parsed, kind, &lines.join("\n"));
        let text = block::canonical(&site, &text)?;
        self.texts.insert(file.to_string(), text);
        self.created
            .insert(id.to_string(), (kind.to_string(), file.to_string()));
        self.deleted.remove(id);
        self.entities.insert(id.to_string());
        Ok(())
    }

    pub(super) fn delete(
        &mut self,
        id: &str,
        unlink_references: bool,
    ) -> Result<(), Refusal<EditField>> {
        self.locate(id)?;
        if unlink_references {
            for (referrer, field, list) in self.referrers(id) {
                self.on(&referrer, |site, text| {
                    if list {
                        block::link(site, text, &field, id, false)
                    } else {
                        block::unset(site, text, &field)
                    }
                })?;
            }
        }
        self.on(id, |site, text| {
            let mut block = Block::open(site, text)?;
            block.delete();
            Ok(block.text())
        })?;
        self.deleted.insert(id.to_string());
        self.created.remove(id);
        Ok(())
    }

    /// Every field of another entity that references `id`: (entity, field, whether it is a list), in file order.
    fn referrers(&self, id: &str) -> Vec<(String, String, bool)> {
        let forms = self.view.registries().field_forms();
        let mut keys: BTreeSet<String> = self.texts.keys().cloned().collect();
        keys.extend(
            self.view
                .project()
                .source_keys()
                .into_iter()
                .map(String::from),
        );
        let mut found = Vec::new();
        for key in keys {
            let Some(text) = self.text(&key) else {
                continue;
            };
            let file = parse(&text, &key);
            for entity in file.entities.iter().filter(|e| {
                e.kind.raw.as_str() != specforge_common::structural::DEFAULTS
                    && e.id.raw.as_str() != id
            }) {
                for entry in entity.fields.entries() {
                    let Reading::Declared(form) =
                        forms.reading(entity.kind.raw.as_str(), entry.key.as_str())
                    else {
                        continue;
                    };
                    let refers = match (form.field_type(), &entry.value) {
                        (FieldType::ReferenceList, value @ FieldValue::List(_)) => {
                            items_of(value).is_some_and(|i| i.texts.iter().any(|t| t == id))
                        }
                        (
                            FieldType::Reference,
                            FieldValue::Identifier(t) | FieldValue::String(t),
                        ) => t == id,
                        _ => false,
                    };
                    if refers {
                        found.push((
                            entity.id.raw.to_string(),
                            entry.key.to_string(),
                            form.field_type() == FieldType::ReferenceList,
                        ));
                    }
                }
            }
        }
        found
    }
}

/// `existing` with `block` after the last block of `kind`, else at the end, one blank line between.
fn place(existing: &str, parsed: &specforge_parser::SpecFile, kind: &str, block: &str) -> String {
    if existing.trim().is_empty() {
        return format!("{block}\n");
    }
    let mut lines: Vec<&str> = existing.split('\n').collect();
    let after = parsed
        .entities
        .iter()
        .filter(|e| e.kind.raw.as_str() == kind && e.span.end_line <= lines.len())
        .map(|e| e.span.end_line)
        .max();
    let new: Vec<&str> = std::iter::once("").chain(block.split('\n')).collect();
    match after {
        Some(end) => {
            lines.splice(end..end, new);
        }
        None => {
            if lines.last() == Some(&"") {
                lines.pop();
            }
            lines.extend(new);
            lines.push("");
        }
    }
    lines.join("\n")
}
