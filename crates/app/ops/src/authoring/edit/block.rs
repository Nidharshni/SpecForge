//! Where a typed edit puts text: the lines of one entity's block. A block is formatted before it is read (so every
//! member starts on its own line) and again after it is changed (so it is laid out canonically); no byte outside it
//! changes. `site` holds what the kind's registry answers about the entity; `verbs` the edits that change a block.

use super::EditField;
use crate::{OpError, OpErrorKind, Refusal};
use specforge_common::{SourceSpan, codes};
use specforge_formatter::FormatConfig;
use specforge_graph::field_form::spell;
use specforge_parser::lex::{LexemeKind, lex};
use specforge_parser::{Entity, FieldEntry, FieldValue, SpecFile, parse};

mod site;
mod verbs;

pub use site::{Site, order};
pub use verbs::{link, retitle, set, unset, verify};

/// The entity's block holds a region the parser could not read whole.
pub const BLOCK_UNREADABLE: &str = "block_unreadable";

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

/// `text` with the block of `site` laid out canonically, with its file's configuration.
pub fn canonical(site: &Site<'_>, text: &str) -> Result<String, Refusal<EditField>> {
    let file = parse(text, site.key);
    let span = &file.entities[position(&file, site).ok_or_else(|| unreadable(site.id))?].span;
    let formatted = specforge_formatter::format_range(
        text,
        span.start_line - 1,
        span.end_line - 1,
        &site.config,
        site.view.registries().field_forms(),
    );
    // A region the parser could not read whole (W142) is kept as written: the block cannot be edited.
    if formatted.diagnostics.iter().any(|d| d.is(codes::W142)) {
        Err(unreadable(site.id))
    } else {
        Ok(formatted.formatted)
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
            config: site.config.clone(),
        })
    }

    pub fn text(&self) -> String {
        self.lines.join("\n")
    }

    /// The block laid out again after a change.
    pub fn finish(&self, site: &Site<'_>) -> Result<String, Refusal<EditField>> {
        canonical(site, &self.text())
    }

    /// The text `span` covers.
    pub(super) fn text_of(&self, span: &SourceSpan) -> String {
        let (first, last) = (span.start_line - 1, span.end_line - 1);
        let mut lines: Vec<&str> = self.lines[first..=last]
            .iter()
            .map(String::as_str)
            .collect();
        let end = lines.len() - 1;
        lines[end] = &lines[end][..span.end_col - 1];
        lines[0] = &lines[0][span.start_col - 1..];
        lines.join("\n")
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

    pub(super) fn remove(&mut self, first: usize, last: usize) {
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
    pub(super) fn replace_value(
        &mut self,
        span: &SourceSpan,
        new: &str,
        annotations: Option<&[String]>,
    ) {
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
    pub(super) fn insert_member(
        &mut self,
        site: &Site<'_>,
        name: &str,
        value: &str,
        annotations: &[String],
    ) {
        let order = order(site.view, site.kind);
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

pub(super) fn list_text(items: &Items) -> String {
    format!("[{}]", items.written.join(", "))
}
