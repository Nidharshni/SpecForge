//! Creating and deleting entities (ADR 0151): a new block goes after the last block of its kind in its file, laid
//! out canonically; a deleted block takes its doc comment and one blank line with it.

use super::block::{self, Block, Site};
use super::path::{self, FieldPath};
use super::write::not_editable;
use super::{EditField, Input, Obligation, Planner};
use crate::authoring::entity_id::validate_id;
use crate::authoring::rename;
use crate::{OpError, OpErrorKind, Refusal};
use specforge_common::Sym;
use specforge_graph::field_form::spell;
use specforge_graph::{Form, Holder, written_tokens};
use specforge_parser::parse;
use std::collections::BTreeSet;

/// The header line of a new `kind` block, its braces on the lines it opens (ADR 0175 D7).
pub(super) fn header(kind: &str, id: &str, title: &str) -> String {
    format!("{kind} {id} {} {{\n}}", spell::quoted(title))
}

/// `text` with `fields` set on `site`'s block and `verify` added, as a create writes them: one `set` per field, one
/// add-verify per obligation (ADR 0175 D7). A `field.entry` path is refused (`path::whole_field`).
pub(super) fn fill(
    site: &Site<'_>,
    mut text: String,
    fields: &std::collections::BTreeMap<String, Input>,
    verify: &[Obligation],
) -> Result<String, Refusal<EditField>> {
    for (name, input) in fields {
        if let FieldPath::Entry { field, entry } = path::resolve(site, name, None)? {
            return Err(path::whole_field("create", field, entry));
        }
        text = block::set(site, &text, name, input, None)?;
    }
    for o in verify {
        text = block::verify(site, &text, o.kind.as_deref(), &o.text, true)?;
    }
    Ok(text)
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
        let file = self
            .view
            .source_place(file)
            .map_err(|e| e.about(EditField::File))?;
        let file = file.as_str();
        let existing = self.text(file).unwrap_or_default();
        let parsed = parse(&existing, file);
        let site = Site {
            view: self.view,
            key: file,
            id,
            kind,
            config: self.config_of(file),
        };
        let header = header(kind, id, title);
        let text = fill(
            &site,
            block::canonical(&site, &place(&existing, &parsed, kind, &header))?,
            fields,
            verify,
        )?;
        self.texts.insert(file.to_string(), text);
        self.created
            .insert(id.to_string(), (kind.to_string(), file.to_string()));
        self.deleted.remove(id);
        Ok(())
    }

    pub(super) fn delete(
        &mut self,
        id: &str,
        unlink_references: bool,
    ) -> Result<(), Refusal<EditField>> {
        let located = self.locate(id)?;
        if unlink_references {
            for (referrer, field, list) in self.referrers(id, &located.kind)? {
                self.declared.field(&referrer, &field);
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

    /// Every field of another block that names `id` (of kind `kind`), as the graph's finder reads the texts the plan
    /// holds so far (ADR 0163): (entity, field, whether it is a list), in file order. A token no typed edit writes
    /// refuses the delete, naming each place: an entry of a `defaults` block (ADR 0151 D5), a field its holder's kind
    /// does not declare, or a type name a derived field reads.
    fn referrers(
        &self,
        id: &str,
        kind: &str,
    ) -> Result<Vec<(String, String, bool)>, Refusal<EditField>> {
        let inputs = self.view.registries().graph_inputs();
        let kind = Sym::new(kind);
        let kind_of = |name: &str| (name == id).then_some(kind);
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
            for token in written_tokens(&parse(&text, &key), inputs, kind_of) {
                if token.target.as_str() != id {
                    continue;
                }
                let place = match (token.holder, token.form) {
                    // The block being deleted takes its own tokens with it.
                    (Holder::Block(holder), _) if holder.as_str() == id => continue,
                    (Holder::Block(holder), Form::Single | Form::Item) => {
                        let referrer = (
                            holder.to_string(),
                            token.field.to_string(),
                            token.form == Form::Item,
                        );
                        if !found.contains(&referrer) {
                            found.push(referrer);
                        }
                        continue;
                    }
                    (Holder::Block(holder), _) => holder.to_string(),
                    (Holder::Defaults(of), _) => format!("defaults {of}"),
                };
                let at = format!("{key}:{} ({place}, {})", token.span.start_line, token.field);
                if !elsewhere.contains(&at) {
                    elsewhere.push(at);
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
