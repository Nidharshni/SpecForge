//! Where a typed edit puts text: the lines of one entity's block. A block is formatted before it is read (so every
//! member starts on its own line) and again after it is changed (so it is laid out canonically); no byte outside it
//! changes.

use super::write::{not_editable, written};
use super::{EditField, Input};
use crate::authoring::format::{self, Lines, Place};
use crate::view::ProjectView;
use crate::{OpError, OpErrorKind, Refusal};
use specforge_common::{SourceSpan, find_close_match};
use specforge_formatter::FormatConfig;
use specforge_graph::FieldMap;
use specforge_graph::field_form::{FieldForm, Reading, spell};
use specforge_parser::lex::{LexemeKind, lex};
use specforge_parser::{Entity, FieldEntry, FieldValue, SpecFile, parse};
use specforge_protocol_types::FieldType;

/// The edit names a field its kind does not declare.
pub const UNKNOWN_FIELD: &str = "unknown_field";
/// The entity's block holds a region the parser could not read whole.
pub const BLOCK_UNREADABLE: &str = "block_unreadable";

/// The entity an edit changes, and where it is.
pub struct Site<'a> {
    pub view: &'a ProjectView<'a>,
    /// The file, relative to the spec root.
    pub key: &'a str,
    pub id: &'a str,
    pub kind: &'a str,
    /// The entity's fields as the graph resolved them (the file defaults' included); `None` for one this plan
    /// created.
    pub resolved: Option<&'a FieldMap>,
}

impl Site<'_> {
    /// How `field` of the entity's kind reads; `unknown_field` for one it does not declare.
    pub fn reading(&self, field: &str) -> Result<Reading<'_>, Refusal<EditField>> {
        match self
            .view
            .registries()
            .field_forms()
            .reading(self.kind, field)
        {
            Reading::Undeclared => Err(self.unknown_field(field)),
            reading => Ok(reading),
        }
    }

    fn unknown_field(&self, field: &str) -> Refusal<EditField> {
        let declared = self.order();
        let error = OpError::new(
            OpErrorKind::InvalidInput,
            UNKNOWN_FIELD,
            format!("kind '{}' has no field '{field}'", self.kind),
        );
        match find_close_match(field, declared.iter().map(String::as_str)) {
            Some(near) => error.with_suggestion(format!("did you mean '{near}'?")),
            None => error,
        }
        .about(EditField::Field)
    }

    /// The kind's declared fields, in declaration order, then the enhancements' by name.
    pub fn order(&self) -> Vec<String> {
        let registries = self.view.registries();
        let mut order: Vec<String> = registries
            .kinds()
            .get(self.kind)
            .map(|k| k.declared.fields.iter().map(|f| f.name.clone()).collect())
            .unwrap_or_default();
        let mut more: Vec<String> = registries
            .fields()
            .fields_for_kind(self.kind)
            .iter()
            .map(|f| f.name().to_string())
            .filter(|name| !order.contains(name))
            .collect();
        more.sort();
        order.extend(more);
        order
    }

    fn path(&self) -> std::path::PathBuf {
        self.view.env().spec_root().join(self.key)
    }
}

fn unreadable(id: &str) -> Refusal<EditField> {
    OpError::new(
        OpErrorKind::Conflict,
        BLOCK_UNREADABLE,
        format!(
            "the block of '{id}' holds text the parser could not read whole; fix it by hand first"
        ),
    )
    .with_entity(id)
    .about(EditField::Entity)
}

fn position(file: &SpecFile, site: &Site<'_>) -> Option<usize> {
    file.entities
        .iter()
        .position(|e| e.id.raw.as_str() == site.id && e.kind.raw.as_str() == site.kind)
}

/// The lines of one entity's block, formatted, and what the parser reads in them.
pub struct Block {
    lines: Vec<String>,
    file: SpecFile,
    index: usize,
    /// The file's format configuration: what a multi-line value is laid out with.
    config: FormatConfig,
}

/// The format configuration of the file `site` is in.
pub fn layout(site: &Site<'_>) -> FormatConfig {
    format::config(Place::File(&site.path()))
}

/// `text` with the block of `site` laid out canonically.
pub fn canonical(site: &Site<'_>, text: &str) -> Result<String, Refusal<EditField>> {
    let file = parse(text, site.key);
    let span = &file.entities[position(&file, site).ok_or_else(|| unreadable(site.id))?].span;
    let lines = Lines {
        first: span.start_line - 1,
        last: span.end_line - 1,
    };
    let forms = site.view.registries().field_forms();
    let formatted = format::document(Place::File(&site.path()), text, Some(lines), None, forms);
    if formatted.complete() {
        Ok(formatted.formatted)
    } else {
        Err(unreadable(site.id))
    }
}

impl Block {
    pub fn open(site: &Site<'_>, text: &str) -> Result<Block, Refusal<EditField>> {
        let text = canonical(site, text)?;
        let file = parse(&text, site.key);
        let index = position(&file, site).ok_or_else(|| unreadable(site.id))?;
        let lines = text.split('\n').map(String::from).collect();
        Ok(Block {
            lines,
            file,
            index,
            config: layout(site),
        })
    }

    pub fn text(&self) -> String {
        self.lines.join("\n")
    }

    /// The block laid out again after a change.
    pub fn finish(&self, site: &Site<'_>) -> Result<String, Refusal<EditField>> {
        canonical(site, &self.text())
    }

    pub fn entity(&self) -> &Entity {
        &self.file.entities[self.index]
    }

    pub fn file(&self) -> &SpecFile {
        &self.file
    }

    /// The 0-based line of the header, and of the closing brace.
    fn header(&self) -> usize {
        self.entity().span.start_line - 1
    }

    fn close(&self) -> usize {
        self.entity().span.end_line - 1
    }

    pub fn entry(&self, name: &str) -> Option<&FieldEntry> {
        self.entity()
            .fields
            .entries()
            .iter()
            .find(|e| e.key.as_str() == name)
    }

    /// The leading whitespace of `line`.
    fn lead(&self, line: usize) -> &str {
        let text = &self.lines[line];
        &text[..text.len() - text.trim_start().len()]
    }

    /// The leading whitespace of the block's members.
    fn member_lead(&self) -> String {
        let one_in = self.config.indent_str();
        match self.entity().fields.entries().first() {
            Some(entry) => entry
                .value_span
                .as_ref()
                .map_or(one_in, |s| self.lead(s.start_line - 1).to_string()),
            None => format!("{}{one_in}", self.lead(self.header())),
        }
    }

    /// The first line of the comment lines directly above `line` (`line` itself when there are none).
    fn with_comments_above(&self, line: usize) -> usize {
        self.comments_above(line, self.header() + 1)
    }

    /// [`Self::with_comments_above`], climbing no higher than line `floor`.
    fn comments_above(&self, mut line: usize, floor: usize) -> usize {
        while line > floor && self.lines[line - 1].trim_start().starts_with("//") {
            line -= 1;
        }
        line
    }

    /// The block removed, with the comment lines directly above it and one blank line beside it.
    pub fn delete(&mut self) {
        let (first, last) = (self.comments_above(self.header(), 0), self.close());
        let before_blank = first > 0 && self.lines[first - 1].trim().is_empty();
        let after_blank = last + 1 < self.lines.len() && self.lines[last + 1].trim().is_empty();
        let (first, last) = match (before_blank, after_blank) {
            (true, _) => (first - 1, last),
            (false, true) => (first, last + 1),
            _ => (first, last),
        };
        self.lines.drain(first..=last);
    }

    fn remove(&mut self, first: usize, last: usize) {
        let first = self.with_comments_above(first);
        self.lines.drain(first..=last);
    }

    fn insert(&mut self, at: usize, text: &str) {
        let new: Vec<String> = text.split('\n').map(String::from).collect();
        self.lines.splice(at..at, new);
    }

    /// Each `verify` statement's first and last 0-based line, in written order.
    fn verify_lines(&self) -> Vec<(usize, usize)> {
        let body = self.lines[self.header()..=self.close()].join("\n");
        let line_of = |at: usize| self.header() + body[..at].matches('\n').count();
        let lexemes = lex(&body);
        let mut found = Vec::new();
        for (i, lexeme) in lexemes.iter().enumerate() {
            let line_start = body[..lexeme.start].rfind('\n').map_or(0, |n| n + 1);
            if lexeme.kind != LexemeKind::Ident
                || lexeme.text(&body) != "verify"
                || !body[line_start..lexeme.start].trim().is_empty()
            {
                continue;
            }
            let Some(last) = lexemes[i + 1..].iter().take(2).find(|l| l.kind.is_str()) else {
                continue;
            };
            found.push((line_of(lexeme.start), line_of(last.end.saturating_sub(1))));
        }
        found
    }
}

/// Where a field goes: one-line values, then block fields, then triple-quoted text, each in the kind's declared
/// order (ADR 0151).
fn group_of(entry: &FieldEntry) -> u8 {
    match (&entry.value, &entry.value_span) {
        (FieldValue::Block(_), _) => 2,
        (FieldValue::String(_), Some(s)) if s.end_line > s.start_line => 3,
        _ => 1,
    }
}

/// The group of a value about to be written.
pub fn group_of_text(written: &str) -> u8 {
    if written.starts_with('{') {
        2
    } else if written.starts_with("\"\"\"") {
        3
    } else {
        1
    }
}

/// The rank of field `name` in `group`: where `order` puts it, undeclared members last.
pub fn rank(order: &[String], name: &str, group: u8) -> (u8, usize) {
    (
        group,
        order.iter().position(|n| n == name).unwrap_or(usize::MAX),
    )
}

/// ` @name` / ` @name "value"` for each of `annotations` (`name` or `name=value`).
fn annotation_text(annotations: &[String]) -> String {
    annotations
        .iter()
        .map(|a| match a.split_once('=') {
            Some((name, value)) => format!(" @{name} {}", spell::quoted(value)),
            None => format!(" @{a}"),
        })
        .collect()
}

/// A comment ending the text after a value, with the space before it.
fn trailing_comment(suffix: &str) -> &str {
    lex(suffix)
        .iter()
        .find(|l| l.kind == LexemeKind::Comment)
        .map_or("", |l| {
            suffix[..l.start]
                .rfind(|c: char| c != ' ')
                .map_or(&suffix[l.start..], |n| &suffix[n + 1..])
        })
}

impl Block {
    /// `span` (a value) replaced by `new`; with `annotations`, the text after it becomes them (a trailing comment
    /// is kept).
    fn replace_value(&mut self, span: &SourceSpan, new: &str, annotations: Option<&[String]>) {
        let (first, last) = (span.start_line - 1, span.end_line - 1);
        let prefix = &self.lines[first][..span.start_col - 1];
        let suffix = &self.lines[last][span.end_col - 1..];
        let suffix = match annotations {
            Some(a) => format!("{}{}", annotation_text(a), trailing_comment(suffix)),
            None => suffix.to_string(),
        };
        let replacement = format!("{prefix}{new}{suffix}");
        self.lines
            .splice(first..=last, replacement.split('\n').map(String::from));
    }

    /// A new member `name value` at its rank (`(group, declared index)`).
    fn insert_member(&mut self, site: &Site<'_>, name: &str, value: &str, annotations: &[String]) {
        let order = site.order();
        let new_rank = rank(&order, name, group_of_text(value));
        let text = format!(
            "{}{name} {value}{}",
            self.member_lead(),
            annotation_text(annotations)
        );
        let entries = self.entity().fields.entries();
        let after = entries
            .iter()
            .filter(|e| rank(&order, e.key.as_str(), group_of(e)) <= new_rank)
            .filter_map(|e| e.value_span.as_ref())
            .map(|s| s.end_line)
            .max();
        let at = match (after, entries.first().and_then(|e| e.value_span.as_ref())) {
            (Some(end_line), _) => end_line,
            (None, Some(first)) => self.with_comments_above(first.start_line - 1),
            (None, None) => self
                .verify_lines()
                .first()
                .map_or(self.close(), |(first, _)| self.with_comments_above(*first)),
        };
        self.insert(at, &text);
    }
}

fn block_form() -> FieldForm {
    FieldForm::new(FieldType::String, Vec::new())
}

/// The dotted `field.entry` split, and the entry looked up in its block field.
fn nested<'b>(block: &'b Block, field: &str) -> Option<(&'b FieldEntry, Option<&'b FieldEntry>)> {
    let (outer, inner) = field.split_once('.')?;
    let outer = block.entry(outer)?;
    let FieldValue::Block(map) = &outer.value else {
        return Some((outer, None));
    };
    Some((
        outer,
        map.entries().iter().find(|e| e.key.as_str() == inner),
    ))
}

fn finish_edit(
    site: &Site<'_>,
    original: &str,
    before: &str,
    block: &Block,
) -> Result<String, Refusal<EditField>> {
    let after = block.finish(site)?;
    Ok(if after == before {
        original.to_string()
    } else {
        after
    })
}

/// `field` (or `field.entry`) set to `input`. The text unchanged when it already holds that.
pub fn set(
    site: &Site<'_>,
    text: &str,
    field: &str,
    input: &Input,
    annotations: Option<&[String]>,
) -> Result<String, Refusal<EditField>> {
    let mut block = Block::open(site, text)?;
    let before = block.text();
    if let Some((outer, inner)) = field.split_once('.') {
        match site.reading(outer)? {
            Reading::Declared(form) if form.field_type() == FieldType::Block => {}
            _ => {
                return Err(not_editable(
                    EditField::Field,
                    format!("'{outer}' is not a block field, so it has no entry '{inner}'"),
                ));
            }
        }
        let Input::One(_) = input else {
            return Err(not_editable(
                EditField::Value,
                format!("'{field}' takes one text"),
            ));
        };
        match nested(&block, field) {
            Some((_, Some(entry))) => {
                let span = entry
                    .value_span
                    .clone()
                    .ok_or_else(|| unreadable(site.id))?;
                let lead = block.lead(span.start_line - 1).to_string();
                let new = written(
                    Reading::Declared(&block_form()),
                    inner,
                    input,
                    &lead,
                    &block.config,
                )?;
                block.replace_value(&span, &new, annotations);
            }
            Some((outer_entry, None)) => {
                let span = outer_entry
                    .value_span
                    .clone()
                    .ok_or_else(|| unreadable(site.id))?;
                let lead = format!(
                    "{}{}",
                    block.lead(span.start_line - 1),
                    block.config.indent_str()
                );
                let new = written(
                    Reading::Declared(&block_form()),
                    inner,
                    input,
                    &lead,
                    &block.config,
                )?;
                let line = format!("{lead}{inner} {new}");
                block.insert(span.end_line - 1, &line);
            }
            None => {
                let Input::One(text) = input else {
                    unreachable!("checked above")
                };
                let entries = Input::Entries([(inner.to_string(), text.clone())].into());
                let value = written(
                    site.reading(outer)?,
                    outer,
                    &entries,
                    &block.member_lead(),
                    &block.config,
                )?;
                block.insert_member(site, outer, &value, &[]);
            }
        }
        return finish_edit(site, text, &before, &block);
    }
    let reading = site.reading(field)?;
    match block.entry(field).and_then(|e| e.value_span.clone()) {
        Some(span) => {
            let lead = block.lead(span.start_line - 1).to_string();
            let new = written(reading, field, input, &lead, &block.config)?;
            block.replace_value(&span, &new, annotations);
        }
        None => {
            let new = written(reading, field, input, &block.member_lead(), &block.config)?;
            block.insert_member(site, field, &new, annotations.unwrap_or_default());
        }
    }
    let after = finish_edit(site, text, &before, &block)?;
    super::defaults::without_repeat(site, &after, field)
}

/// `field` (or `field.entry`) removed, with the comment lines directly above it. The text unchanged when it is not
/// there. An inherited list becomes `[]`.
pub fn unset(site: &Site<'_>, text: &str, field: &str) -> Result<String, Refusal<EditField>> {
    let mut block = Block::open(site, text)?;
    let before = block.text();
    if field.contains('.') {
        let Some((outer, Some(entry))) = nested(&block, field) else {
            return Ok(text.to_string());
        };
        let alone = matches!(&outer.value, FieldValue::Block(m) if m.entries().len() == 1);
        let span = if alone { outer } else { entry }
            .value_span
            .clone()
            .ok_or_else(|| unreadable(site.id))?;
        let (first, last) = (span.start_line - 1, span.end_line - 1);
        block.remove(first, last);
        return finish_edit(site, text, &before, &block);
    }
    site.reading(field)?;
    let Some(span) = block.entry(field).and_then(|e| e.value_span.clone()) else {
        return super::defaults::unset_inherited(site, text, field);
    };
    block.remove(span.start_line - 1, span.end_line - 1);
    let after = finish_edit(site, text, &before, &block)?;
    Ok(after)
}

/// The header title replaced, or added.
pub fn retitle(site: &Site<'_>, text: &str, title: &str) -> Result<String, Refusal<EditField>> {
    let mut block = Block::open(site, text)?;
    let before = block.text();
    let at = block.header();
    let line = block.lines[at].clone();
    let lexemes = lex(&line);
    let quoted_title = spell::quoted(title);
    let new = match lexemes.iter().find(|l| l.kind.is_str()) {
        Some(old) => format!("{}{}{}", &line[..old.start], quoted_title, &line[old.end..]),
        None => {
            let id = lexemes
                .iter()
                .filter(|l| l.is_name())
                .nth(1)
                .ok_or_else(|| unreadable(site.id))?;
            format!("{} {}{}", &line[..id.end], quoted_title, &line[id.end..])
        }
    };
    block.lines[at] = new;
    finish_edit(site, text, &before, &block)
}

/// `verify [kind] "text"` lines: added after the last, or removed. The text unchanged when there is nothing to do.
pub fn verify(
    site: &Site<'_>,
    text: &str,
    kind: Option<&str>,
    description: &str,
    add: bool,
) -> Result<String, Refusal<EditField>> {
    let mut block = Block::open(site, text)?;
    let before = block.text();
    let kind = kind.unwrap_or("");
    let obligations = &block.entity().obligations;
    let found = obligations
        .iter()
        .position(|o| o.kind == kind && o.description == description);
    let lines = block.verify_lines();
    if add {
        if found.is_some() {
            return Ok(text.to_string());
        }
        let kind_text = if kind.is_empty() {
            String::new()
        } else {
            format!("{kind} ")
        };
        let line = format!(
            "{}verify {kind_text}{}",
            block.member_lead(),
            spell::quoted(description)
        );
        let at = lines.last().map_or(block.close(), |(_, last)| last + 1);
        block.insert(at, &line);
    } else {
        let Some((first, last)) = found.and_then(|i| lines.get(i).copied()) else {
            return Ok(text.to_string());
        };
        block.remove(first, last);
    }
    finish_edit(site, text, &before, &block)
}

/// A list field's items, as `link` and `unlink` rewrite them.
pub struct Items {
    /// Each item as it is written.
    pub written: Vec<String>,
    /// Each item's text.
    pub texts: Vec<String>,
}

/// The items of the list `entry` holds; `None` for a value that is not a list.
pub fn items_of(value: &FieldValue) -> Option<Items> {
    let FieldValue::List(items) = value else {
        return None;
    };
    Some(Items {
        written: items
            .iter()
            .map(|i| spell::spelled(&i.text, i.written))
            .collect(),
        texts: items.iter().map(|i| i.text.clone()).collect(),
    })
}

/// `target` added to, or removed from, the list `field`; an inherited list is copied into an own field first.
pub fn link(
    site: &Site<'_>,
    text: &str,
    field: &str,
    target: &str,
    add: bool,
) -> Result<String, Refusal<EditField>> {
    let Reading::Declared(form) = site.reading(field)? else {
        return Err(not_editable(
            EditField::Field,
            format!("'{field}' takes any text; use set"),
        ));
    };
    let item = match form.field_type() {
        FieldType::ReferenceList | FieldType::StringList => spell::value(form, target),
        FieldType::Reference if !add => String::new(),
        _ => {
            return Err(not_editable(
                EditField::Field,
                format!("'{field}' is not a list; use set"),
            ));
        }
    };
    let block = Block::open(site, text)?;
    let before = block.text();
    let own = block.entry(field);
    let inherited = own.is_none().then(|| site.resolved?.get(field)).flatten();
    let current = match (own, inherited) {
        (Some(entry), _) => match &entry.value {
            FieldValue::List(_) => items_of(&entry.value),
            _ if matches!(form.field_type(), FieldType::Reference) => None,
            _ => {
                return Err(not_editable(
                    EditField::Field,
                    format!("'{field}' is not a list"),
                ));
            }
        },
        (None, Some(value)) => items_of(value),
        (None, None) => Some(Items {
            written: Vec::new(),
            texts: Vec::new(),
        }),
    };
    let Some(mut items) = current else {
        // A single `reference` field: unlink removes it when it names the target.
        return match block.entry(field).map(|e| &e.value) {
            Some(FieldValue::Identifier(t) | FieldValue::String(t)) if t == target => {
                unset(site, text, field)
            }
            _ => Ok(text.to_string()),
        };
    };
    if let Some(entry) = own
        && let Some(span) = &entry.value_span
        && lex(&block.lines[span.start_line - 1..span.end_line].join("\n"))
            .iter()
            .any(|l| l.kind == LexemeKind::Comment)
    {
        return Err(not_editable(
            EditField::Field,
            format!("'{field}' holds a comment; edit it by hand"),
        ));
    }
    let present = items.texts.iter().any(|t| t == target);
    if add == present {
        return Ok(text.to_string());
    }
    if add {
        items.texts.push(target.to_string());
        items.written.push(item);
    } else {
        let mut keep = items
            .texts
            .iter()
            .map(|t| t != target)
            .collect::<Vec<_>>()
            .into_iter();
        items.written.retain(|_| keep.next().unwrap_or(true));
        items.texts.retain(|t| t != target);
    }
    super::defaults::write_list(site, text, &before, field, &items)
}

/// The own field `field` replaced by (or added with) the list `items`, or removed when `items` is `None`.
pub fn put_list(
    site: &Site<'_>,
    text: &str,
    before: &str,
    field: &str,
    items: Option<&Items>,
) -> Result<String, Refusal<EditField>> {
    let mut block = Block::open(site, text)?;
    let span = block.entry(field).and_then(|e| e.value_span.clone());
    match (span, items) {
        (Some(span), None) => block.remove(span.start_line - 1, span.end_line - 1),
        (Some(span), Some(items)) => block.replace_value(&span, &list_text(items), None),
        (None, Some(items)) => block.insert_member(site, field, &list_text(items), &[]),
        (None, None) => {}
    }
    finish_edit(site, text, before, &block)
}

fn list_text(items: &Items) -> String {
    format!("[{}]", items.written.join(", "))
}
