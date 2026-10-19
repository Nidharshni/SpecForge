//! Creating and deleting entities (ADR 0151): a new block goes after the last block of its kind in its file, laid
//! out canonically; a deleted block takes the comment lines above it and one blank line with it.

use super::block::{self, Block, Site, group_of_text, rank};
use super::write::{not_editable, written};
use super::{EditField, Input, Obligation, Planner};
use crate::authoring::rename::{self, validate_id};
use crate::{OpError, OpErrorKind, Refusal};
use specforge_common::structural;
use specforge_graph::field_form::{Reading, spell};
use specforge_parser::{FieldEntry, parse};
use specforge_protocol_types::FieldType;
use std::collections::BTreeSet;

/// `Create`'s file is not a project source.
pub const NOT_A_SOURCE: &str = "not_a_source";

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
        validate_id(id, self.view.registries().kinds()).map_err(|e| e.about(EditField::Id))?;
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
        };
        let order = site.order();
        let config = block::layout(&site);
        let mut members: Vec<((u8, usize), String, String)> = Vec::new();
        for (name, input) in fields {
            let reading = site.reading(name)?;
            let value = written(reading, name, input, &config.indent_str(), &config)?;
            members.push((
                rank(&order, name, group_of_text(&value)),
                name.clone(),
                value,
            ));
        }
        members.sort();
        let mut lines = vec![format!("{kind} {id} {} {{", spell::quoted(title))];
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
            format!("  verify {kind}{}", spell::quoted(&o.text))
        }));
        lines.push("}".to_string());
        let text = place(&existing, &parsed, kind, &lines.join("\n"));
        let mut text = block::canonical(&site, &text)?;
        // A field that restates its file's default is not written (W161), judged on what was written (ADR 0154).
        for name in fields.keys() {
            text = super::defaults::without_repeat(&site, &text, name)?;
        }
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
            for (referrer, field, list) in self.referrers(id)? {
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

    /// Every field of another entity whose value references `id` as the graph build reads it ([`names`]): (entity,
    /// field, whether it is a list), in file order, over the texts the plan holds so far. A reference written where no
    /// typed edit writes refuses the delete, naming each place: an entry of a `defaults` block (ADR 0151 D5) or a
    /// field its holder's kind does not declare.
    fn referrers(&self, id: &str) -> Result<Vec<(String, String, bool)>, Refusal<EditField>> {
        let forms = self.view.registries().field_forms();
        let mut keys: BTreeSet<String> = self.texts.keys().cloned().collect();
        keys.extend(
            self.view
                .project()
                .source_keys()
                .into_iter()
                .map(String::from),
        );
        let (mut found, mut elsewhere) = (Vec::new(), Vec::new());
        for key in keys {
            let Some(text) = self.text(&key) else {
                continue;
            };
            let file = parse(&text, &key);
            for entity in &file.entities {
                let defaults = entity.kind.raw.as_str() == structural::DEFAULTS;
                if !defaults && entity.id.raw.as_str() == id {
                    continue;
                }
                // A defaults block's entries read as the kind it names (ADR 0149).
                let kind = if defaults {
                    entity.id.raw.as_str()
                } else {
                    entity.kind.raw.as_str()
                };
                for entry in entity.fields.entries() {
                    let reading = forms.reading(kind, entry.key.as_str());
                    // A default for an undeclared field is E077 and is not applied: it names nothing.
                    if (defaults && !matches!(reading, Reading::Declared(_)))
                        || !names(&reading, entry, id)
                    {
                        continue;
                    }
                    match reading {
                        Reading::Declared(form) if !defaults => found.push((
                            entity.id.raw.to_string(),
                            entry.key.to_string(),
                            form.field_type() != FieldType::Reference,
                        )),
                        _ => {
                            let line = entry
                                .value_span
                                .as_ref()
                                .map_or(entity.span.start_line, |s| s.start_line);
                            let holder = if defaults {
                                format!("defaults {kind}")
                            } else {
                                entity.id.raw.to_string()
                            };
                            elsewhere.push(format!("{key}:{line} ({holder}, {})", entry.key));
                        }
                    }
                }
            }
        }
        if elsewhere.is_empty() {
            return Ok(found);
        }
        Err(not_editable(
            EditField::Entity,
            format!(
                "cannot unlink '{id}' where no typed edit writes: {}; remove it there first",
                elsewhere.join(", ")
            ),
        ))
    }
}

/// Whether `entry`, read as `reading`, references `id`: ADR 0093's one rule, asked of the value as the graph build
/// stores it (a list written as one value is its one item), as the linker asks it of a node's entries.
fn names(reading: &Reading<'_>, entry: &FieldEntry, id: &str) -> bool {
    let stored = reading
        .coerce(&entry.value, entry.value_span.as_ref())
        .map(|value| FieldEntry {
            value,
            ..entry.clone()
        });
    reading
        .references(stored.as_ref().unwrap_or(entry))
        .iter()
        .any(|r| r.id == id)
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
