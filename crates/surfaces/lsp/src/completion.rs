//! What completes at a cursor: the items of a [`CompletionSite`], read from
//! the project view's registries and graph (ADR 0023). Where the cursor is
//! is the document module's to say.

use specforge_common::structural;
use specforge_graph::Reading;
use specforge_ops::read::infer::{KindGuide, guide};
use specforge_ops::read::navigate::{EntityQuery, MatchScope, find_entities};
use specforge_ops::view::ProjectView;
use specforge_registry::{FieldRegistry, FieldType};
use tower_lsp::lsp_types::{
    CompletionItem, CompletionItemKind, CompletionTextEdit, Documentation, InsertReplaceEdit,
    InsertTextFormat, MarkupContent, MarkupKind, TextEdit,
};

use crate::document::{CompletionSite, WordEdit};

/// The completion items of `site`. Each carries an edit over the word
/// under the cursor (`edit`): an insert-and-replace edit when the client
/// supports one (`insert_replace`), else a plain edit over the word's start
/// to the cursor; its `filterText` is its label.
pub fn items(
    site: &CompletionSite,
    edit: &WordEdit,
    insert_replace: bool,
    view: &ProjectView,
) -> Vec<CompletionItem> {
    let registries = view.registries();
    let mut items = match *site {
        CompletionSite::Keywords { prefix } => keywords(prefix, view),
        CompletionSite::Fields { kind, prefix } => {
            let mut fields = registries.fields().fields_for_kind(kind);
            fields.sort_by(|a, b| a.declared().name.cmp(&b.declared().name));
            fields
                .into_iter()
                .filter(|field| starts_with(&field.declared().name, prefix))
                .map(|field| CompletionItem {
                    label: field.declared().name.clone(),
                    kind: Some(CompletionItemKind::FIELD),
                    detail: field.declared().description.clone(),
                    insert_text: Some(field_snippet(field, 1)),
                    insert_text_format: Some(InsertTextFormat::SNIPPET),
                    ..Default::default()
                })
                .collect()
        }
        CompletionSite::VerifyKind { kind, prefix } => registries
            .kinds()
            .get(kind)
            .filter(|entry| entry.supports_verify)
            .map(|entry| {
                entry
                    .allowed_verify_kinds
                    .iter()
                    .filter(|verify| starts_with(verify, prefix))
                    .map(|verify| CompletionItem {
                        label: verify.clone(),
                        kind: Some(CompletionItemKind::ENUM_MEMBER),
                        detail: Some(format!("verify kind of {kind}")),
                        ..Default::default()
                    })
                    .collect()
            })
            .unwrap_or_default(),
        CompletionSite::ListItem {
            kind,
            field,
            prefix,
        } => {
            let entry = registries.fields().get(kind, field);
            match registries.field_forms().reading(kind, field) {
                // A reference list: the ids of its target kind.
                Reading::Declared(form) if form.field_type() == FieldType::ReferenceList => {
                    let target = entry.and_then(|e| e.declared().target_kind.as_deref());
                    entity_ids(view, prefix, target)
                }
                // A string list's items are strings; the only entities it
                // names are the refs a scheme ref ID names.
                Reading::Declared(form) if form.field_type() == FieldType::StringList => {
                    entity_ids(view, prefix, Some(structural::REF))
                }
                Reading::Declared(_) => Vec::new(),
                // An undeclared field, or a member of an open-fields kind
                // (whose lists name types): every id.
                Reading::Member | Reading::Undeclared => entity_ids(view, prefix, None),
            }
        }
        CompletionSite::Value {
            kind,
            field,
            prefix,
        } => value(registries.fields(), kind, field, prefix, view),
        CompletionSite::Nothing => Vec::new(),
    };
    for item in &mut items {
        let new_text = item
            .insert_text
            .clone()
            .unwrap_or_else(|| item.label.clone());
        item.text_edit = Some(if insert_replace {
            CompletionTextEdit::InsertAndReplace(InsertReplaceEdit {
                new_text,
                insert: edit.insert,
                replace: edit.replace,
            })
        } else {
            CompletionTextEdit::Edit(TextEdit {
                range: edit.insert,
                new_text,
            })
        });
        item.filter_text = Some(item.label.clone());
    }
    items
}

/// Whether `label` starts with `prefix`, ignoring case.
fn starts_with(label: &str, prefix: &str) -> bool {
    label.to_lowercase().starts_with(&prefix.to_lowercase())
}

/// The top level's keywords: `use` and every registered kind, each kind
/// scaffolding its required fields and documented from its inference guide.
/// `define` is a reserved word whose blocks register nothing (W143, ADR
/// 0005): never suggested.
fn keywords(prefix: &str, view: &ProjectView) -> Vec<CompletionItem> {
    let kinds = view.registries().kinds();
    let guide = guide(view);
    let mut keywords: Vec<String> = kinds.keywords().cloned().collect();
    keywords.push(structural::USE.into());
    keywords.push(structural::DEFAULTS.into());
    keywords.sort();
    keywords.dedup();
    keywords
        .into_iter()
        .filter(|keyword| starts_with(keyword, prefix))
        .map(|keyword| {
            let (detail, snippet) = match kinds.get(&keyword) {
                Some(entry) => (
                    Some(entry.source_extension.clone()),
                    Some(keyword_snippet(&keyword, view.registries().fields())),
                ),
                None => (None, None),
            };
            CompletionItem {
                kind: Some(CompletionItemKind::KEYWORD),
                detail,
                documentation: guide.kind(&keyword).and_then(kind_documentation),
                insert_text_format: snippet.as_ref().map(|_| InsertTextFormat::SNIPPET),
                insert_text: snippet,
                label: keyword,
                ..Default::default()
            }
        })
        .collect()
}

/// A kind's markdown documentation: its description, then its inference
/// guide under "**Inferring it from code**"; none when it has neither.
fn kind_documentation(kind: &KindGuide) -> Option<Documentation> {
    let mut sections: Vec<String> = Vec::new();
    if let Some(description) = kind.description.filter(|d| !d.is_empty()) {
        sections.push(description.to_string());
    }
    if !kind.guide.is_empty() {
        sections.push(format!("**Inferring it from code**\n\n{}", kind.guide));
    }
    if sections.is_empty() {
        return None;
    }
    Some(Documentation::MarkupContent(MarkupContent {
        kind: MarkupKind::Markdown,
        value: sections.join("\n\n"),
    }))
}

/// A field's single value, completed from its declared type: a reference's
/// entity IDs (of its target kind), an enum's declared values, `true` and
/// `false` for a boolean; entity IDs for a field the registry does not
/// type (it may hold a reference); nothing for a string, an integer, a
/// list or a block.
fn value(
    fields: &FieldRegistry,
    kind: &str,
    field: &str,
    prefix: &str,
    view: &ProjectView,
) -> Vec<CompletionItem> {
    let Some(entry) = fields.get(kind, field) else {
        return entity_ids(view, prefix, None);
    };
    let constant = |label: &str, item_kind| CompletionItem {
        label: label.to_string(),
        kind: Some(item_kind),
        detail: entry.declared().description.clone(),
        ..Default::default()
    };
    match entry.field_type() {
        FieldType::Reference => entity_ids(view, prefix, entry.declared().target_kind.as_deref()),
        FieldType::Enum => entry
            .enum_values()
            .iter()
            .filter(|value| starts_with(value, prefix))
            .map(|value| constant(value, CompletionItemKind::ENUM_MEMBER))
            .collect(),
        FieldType::Bool => ["true", "false"]
            .into_iter()
            .filter(|value| starts_with(value, prefix))
            .map(|value| constant(value, CompletionItemKind::KEYWORD))
            .collect(),
        _ => Vec::new(),
    }
}

/// The entity IDs matching `prefix`, of `kind` when one is given, ranked
/// as completion, workspace symbols and MCP search rank them.
fn entity_ids(view: &ProjectView, prefix: &str, kind: Option<&str>) -> Vec<CompletionItem> {
    let kinds: Vec<&str> = kind.into_iter().collect();
    let query = EntityQuery {
        kinds: &kinds,
        ..EntityQuery::new(prefix, MatchScope::Names)
    };
    find_entities(view.graph(), &query)
        .into_iter()
        .enumerate()
        .map(|(rank, found)| {
            let node = found.node;
            let kind = node.kind.raw.as_str();
            let detail = node
                .title
                .as_ref()
                .map(|t| format!("{kind} — {t}"))
                .unwrap_or_else(|| kind.to_string());
            CompletionItem {
                label: node.id.raw.to_string(),
                kind: Some(CompletionItemKind::REFERENCE),
                detail: Some(detail),
                // C4-06: preserve the server's ranking in the editor.
                sort_text: Some(format!("{rank:04}")),
                ..Default::default()
            }
        })
        .collect()
}

/// Insert text for `field` as snippet placeholder `n`: a reference list
/// scaffolds its brackets, a string its quotes.
pub fn field_snippet(field: &specforge_registry::FieldRegistryEntry, n: usize) -> String {
    let name = &field.declared().name;
    match field.field_type() {
        t if t.is_list() => format!("{name} [${n}]"),
        FieldType::String => format!("{name} \"${n}\""),
        FieldType::Block => format!("{name} {{\n    ${n}\n  }}"),
        _ => format!("{name} ${n}"),
    }
}

/// Snippet that scaffolds a `kind` block with its required fields.
pub fn keyword_snippet(kind: &str, field_registry: &FieldRegistry) -> String {
    let mut required: Vec<_> = field_registry
        .fields_for_kind(kind)
        .into_iter()
        .filter(|f| f.declared().required)
        .collect();
    required.sort_by(|a, b| a.declared().name.cmp(&b.declared().name));
    let mut snippet = format!("{kind} ${{1:id}} \"${{2:Title}}\" {{\n");
    for (i, field) in required.iter().enumerate() {
        snippet.push_str(&format!("  {}\n", field_snippet(field, i + 3)));
    }
    snippet.push_str("  $0\n}");
    snippet
}
