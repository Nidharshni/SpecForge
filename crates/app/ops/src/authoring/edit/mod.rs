//! Typed edits (ADR 0151): create, change and delete entities in a project's `.spec` files. Each value is written in
//! the form its field's declaration decides, each touched block is laid out by the formatter, and nothing is
//! written unless the edited project reports no error it did not report before (E080). The edits are written as
//! one project change (ADR 0128).

mod block;
mod create;
mod defaults;
mod outcome;
mod path;
pub(crate) mod validate;
pub mod write;

pub use outcome::{EditDocument, Edited, run};

use crate::authoring::format::{Configs, Place};
use crate::authoring::planned::PlannedChange;
use crate::view::ProjectView;
use crate::{OpError, OpErrorKind, Refusal};
use serde::{Deserialize, Serialize};
use specforge_common::DiagnosticList;
use specforge_common::shape::{Definitions, Shape};
use specforge_formatter::FormatConfig;
use std::collections::{BTreeMap, BTreeSet};

/// One typed edit. The MCP `specforge.edit` tool reads a list of them (`op` names the variant); each CLI verb
/// builds one.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Shape)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Edit {
    /// A new entity in `file` (relative to the spec root; created when missing).
    Create {
        kind: String,
        id: String,
        title: String,
        file: String,
        #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
        fields: BTreeMap<String, Input>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        verify: Vec<Obligation>,
    },
    /// `field` (or `field.entry` inside a block field) holds `value`; `annotations` (`readonly`, `since=1.2`)
    /// replace the field's when given.
    Set {
        entity_id: String,
        field: String,
        value: Input,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        annotations: Option<Vec<String>>,
    },
    /// `field` (or `field.entry`) holds none afterwards: removed, or, over a file default, written `[]` (a list) or
    /// `{}` (a block field); refused for a field of one value with a default. A second unset changes nothing.
    Unset { entity_id: String, field: String },
    /// The header's title becomes `title`.
    Retitle { entity_id: String, title: String },
    /// `target` is added to the list `field`. No change when it is already there (own or inherited).
    Link {
        entity_id: String,
        field: String,
        target: String,
    },
    /// `target` is removed from the list `field`. No change when it is absent.
    Unlink {
        entity_id: String,
        field: String,
        target: String,
    },
    /// A `verify [kind] "text"` line is added (after the last one). No change when an equal one is there.
    AddVerify {
        entity_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        kind: Option<String>,
        text: String,
    },
    /// The `verify` line with this kind and text is removed. No change when there is none.
    RemoveVerify {
        entity_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        kind: Option<String>,
        text: String,
    },
    /// The entity's block (and the comment lines directly above it) is removed. With `unlink_references`, every
    /// reference to it is removed in the same change (an unlink, or an unset for a single `reference` field).
    Delete {
        entity_id: String,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        unlink_references: bool,
    },
}

/// A value as a caller gives it; the field's declaration decides how it is written.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Input {
    One(String),
    Many(Vec<String>),
    Entries(BTreeMap<String, String>),
}

impl Shape for Input {
    fn body(defs: &mut Definitions) -> serde_json::Value {
        serde_json::json!({"oneOf": [
            String::schema(defs),
            Vec::<String>::schema(defs),
            BTreeMap::<String, String>::schema(defs),
        ]})
    }
}

/// A `verify` obligation given to `Create`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Shape)]
pub struct Obligation {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    pub text: String,
}

impl Obligation {
    /// `kind:text` as an obligation: the kind is a word (the grammar's identifier) before the first colon, the
    /// text what follows it with its leading spaces trimmed; with no such word, the whole text and no kind.
    pub fn parse(text: &str) -> Obligation {
        match text.split_once(':') {
            Some((kind, rest)) if specforge_graph::field_form::is_word(kind) => Obligation {
                kind: Some(kind.to_string()),
                text: rest.trim_start().to_string(),
            },
            _ => Obligation {
                kind: None,
                text: text.to_string(),
            },
        }
    }
}

/// The part of a request a refusal is about (ADR 0072). MCP names every one `edits`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditField {
    Entity,
    Kind,
    Id,
    Title,
    Field,
    Value,
    Target,
    File,
}

/// A field a kind declares, as `specforge new` offers it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldInfo {
    pub name: String,
    pub field_type: specforge_protocol_types::FieldType,
    pub description: String,
}

/// The fields `kind` declares, in the order a new block writes them (the kind's own, then its enhancements');
/// none for a kind no loaded extension declares.
pub fn fields_of(view: &ProjectView<'_>, kind: &str) -> Vec<FieldInfo> {
    block::order(view, kind)
        .into_iter()
        .filter_map(|name| {
            let entry = view.registries().fields().get(kind, &name)?;
            Some(FieldInfo {
                field_type: entry.field_type(),
                description: entry.declared().description.clone().unwrap_or_default(),
                name,
            })
        })
        .collect()
}

/// What a plan will write.
#[derive(Debug, Clone, PartialEq, Serialize, Shape)]
pub struct EditReport {
    /// The entities the edits created, changed or deleted, sorted.
    pub entities: Vec<String>,
    /// The files the edits change, relative to the spec root, sorted.
    pub affected_files: Vec<String>,
    /// A unified diff of every affected file (spec-root-relative headers), in file order. Empty when nothing changes.
    pub diff: String,
    /// The warnings and infos the edited project reports that the project did not, in report order.
    pub added: DiagnosticList,
}

/// The files a list of edits changes, and what they would write.
#[derive(Debug, Clone)]
pub struct EditPlan {
    change: PlannedChange,
    report: EditReport,
}

impl EditPlan {
    pub fn report(&self) -> &EditReport {
        &self.report
    }

    /// What the plan writes (ADR 0184).
    pub fn change(&self) -> &PlannedChange {
        &self.change
    }
}

/// The part of the plan being made: the edited texts, and what the earlier edits created or deleted.
struct Planner<'v> {
    view: &'v ProjectView<'v>,
    /// The plan's one configuration lookup, through the view's disk (ADR 0169).
    configs: Configs<'v>,
    texts: BTreeMap<String, String>,
    entities: BTreeSet<String>,
    created: BTreeMap<String, (String, String)>,
    deleted: BTreeSet<String>,
}

/// Where an entity is.
struct Located {
    key: String,
    kind: String,
}

impl<'v> Planner<'v> {
    /// The format configuration of the file `key` names (relative to the spec root).
    fn config_of(&mut self, key: &str) -> FormatConfig {
        let path = self.view.env().spec_root().join(key);
        self.configs.layout(Place::File(&path))
    }

    fn text(&self, key: &str) -> Option<String> {
        match self.texts.get(key) {
            Some(text) => Some(text.clone()),
            None => self.view.project().source_text(key).map(|t| t.to_string()),
        }
    }

    fn locate(&self, id: &str) -> Result<Located, Refusal<EditField>> {
        if let Some((kind, key)) = self.created.get(id) {
            return Ok(Located {
                key: key.clone(),
                kind: kind.clone(),
            });
        }
        let graph = self.view.graph();
        match graph.node(id).filter(|_| !self.deleted.contains(id)) {
            Some(node) => Ok(Located {
                key: node.source_span.file.to_string(),
                kind: node.kind.raw.to_string(),
            }),
            None => Err(crate::view::not_found(graph, id).about(EditField::Entity)),
        }
    }

    /// `change` applied to the text of `id`'s file; the file and the entity are recorded when it changed it.
    fn on(
        &mut self,
        id: &str,
        change: impl FnOnce(&block::Site<'_>, &str) -> Result<String, Refusal<EditField>>,
    ) -> Result<(), Refusal<EditField>> {
        let located = self.locate(id)?;
        let text = self.text(&located.key).ok_or_else(|| {
            OpError::new(
                OpErrorKind::Internal,
                crate::FILE_UNREADABLE,
                format!("no text was read for {}", located.key),
            )
        })?;
        let site = block::Site {
            view: self.view,
            key: &located.key,
            id,
            kind: &located.kind,
            config: self.config_of(&located.key),
        };
        let new = change(&site, &text)?;
        if new != text {
            self.texts.insert(located.key, new);
            self.entities.insert(id.to_string());
        }
        Ok(())
    }

    fn apply(&mut self, edit: &Edit) -> Result<(), Refusal<EditField>> {
        match edit {
            Edit::Set {
                entity_id,
                field,
                value,
                annotations,
            } => self.on(entity_id, |site, text| {
                block::set(site, text, field, value, annotations.as_deref())
            }),
            Edit::Unset { entity_id, field } => {
                self.on(entity_id, |site, text| block::unset(site, text, field))
            }
            Edit::Retitle { entity_id, title } => {
                self.on(entity_id, |site, text| block::retitle(site, text, title))
            }
            Edit::Link {
                entity_id,
                field,
                target,
            } => self.on(entity_id, |site, text| {
                block::link(site, text, field, target, true)
            }),
            Edit::Unlink {
                entity_id,
                field,
                target,
            } => self.on(entity_id, |site, text| {
                block::link(site, text, field, target, false)
            }),
            Edit::AddVerify {
                entity_id,
                kind,
                text: description,
            } => self.on(entity_id, |site, text| {
                block::verify(site, text, kind.as_deref(), description, true)
            }),
            Edit::RemoveVerify {
                entity_id,
                kind,
                text: description,
            } => self.on(entity_id, |site, text| {
                block::verify(site, text, kind.as_deref(), description, false)
            }),
            Edit::Create {
                kind,
                id,
                title,
                file,
                fields,
                verify,
            } => self.create(kind, id, title, file, fields, verify),
            Edit::Delete {
                entity_id,
                unlink_references,
            } => self.delete(entity_id, *unlink_references),
        }
    }

    fn finish(self) -> Result<EditPlan, Refusal<EditField>> {
        let project = self.view.project();
        let change = PlannedChange::new(
            self.view.env().spec_root().to_path_buf(),
            "cannot apply the edits",
            self.texts
                .into_iter()
                .map(|(key, after)| (project.source_text(&key), key, after))
                .map(|(before, key, after)| (key, before, after)),
        );
        let added = if change.is_empty() {
            Vec::new()
        } else {
            validate::check(self.view, &change.after())?
        };
        let report = EditReport {
            entities: self.entities.into_iter().collect(),
            affected_files: change.affected_files().map(String::from).collect(),
            diff: change.diff(),
            added: DiagnosticList(added),
        };
        Ok(EditPlan { change, report })
    }
}

/// What an edit is, for the refusal that names it.
fn describe(edit: &Edit) -> String {
    match edit {
        Edit::Create { kind, id, .. } => format!("create {kind} {id}"),
        Edit::Set {
            entity_id, field, ..
        } => format!("set {entity_id} {field}"),
        Edit::Unset { entity_id, field } => format!("unset {entity_id} {field}"),
        Edit::Retitle { entity_id, .. } => format!("retitle {entity_id}"),
        Edit::Link {
            entity_id, field, ..
        } => format!("link {entity_id} {field}"),
        Edit::Unlink {
            entity_id, field, ..
        } => format!("unlink {entity_id} {field}"),
        Edit::AddVerify { entity_id, .. } => format!("add_verify {entity_id}"),
        Edit::RemoveVerify { entity_id, .. } => format!("remove_verify {entity_id}"),
        Edit::Delete { entity_id, .. } => format!("delete {entity_id}"),
    }
}

/// Plan `edits`, in order, over the project `view` reads: each edit reads the text the ones before it left.
/// Refused with the first edit that cannot be made (with several edits its message starts with
/// `edit <n> (<op> <entity>): `). Each file is laid out with its format configuration, read once per plan
/// through the view's disk (ADR 0169).
pub fn plan(view: &ProjectView<'_>, edits: &[Edit]) -> Result<EditPlan, Refusal<EditField>> {
    crate::config::refuse_rewrite(view.env().config_problems())?;
    view.project_root()?;
    let disk = view.containment().disk();
    let mut planner = Planner {
        view,
        configs: Configs::new(None, &disk),
        texts: BTreeMap::new(),
        entities: BTreeSet::new(),
        created: BTreeMap::new(),
        deleted: BTreeSet::new(),
    };
    for (n, edit) in edits.iter().enumerate() {
        planner.apply(edit).map_err(|mut refusal| {
            if edits.len() > 1 {
                refusal.error =
                    refusal
                        .error
                        .prefixed(format!("edit {} ({}): ", n + 1, describe(edit)));
            }
            refusal
        })?;
    }
    planner.finish()
}
