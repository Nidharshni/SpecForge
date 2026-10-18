//! Typed edits (ADR 0151): create, change and delete entities in a project's `.spec` files. Each value is written in
//! the form its field's declaration decides, each touched block is laid out by the formatter, and nothing is
//! written unless the edited project reports no error it did not report before (E080). The edits are written as
//! one project change (ADR 0128).

mod block;
mod defaults;
pub mod write;

use crate::view::ProjectView;
use crate::{OpError, OpErrorKind, Refusal};
use serde::{Deserialize, Serialize};
use specforge_common::shape::{Definitions, Shape};
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::Arc;

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
    /// `field` (or `field.entry`) is removed. No change when it is absent.
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
        #[serde(default)]
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

/// What a plan will write.
#[derive(Debug, Clone, PartialEq, Serialize, Shape)]
pub struct EditReport {
    /// The entities the edits created, changed or deleted, sorted.
    pub entities: Vec<String>,
    /// The files the edits change, relative to the spec root, sorted.
    pub affected_files: Vec<String>,
    /// A unified diff of every affected file (spec-root-relative headers), in file order. Empty when nothing changes.
    pub diff: String,
}

/// The files a list of edits changes, and what they would write.
#[derive(Debug, Clone)]
pub struct EditPlan {
    /// Per file (relative to the spec root): the text it was planned on (`None`: the file is new) and the text it
    /// will hold.
    files: BTreeMap<String, (Option<Arc<str>>, String)>,
    spec_root: PathBuf,
    report: EditReport,
}

impl EditPlan {
    /// The spec root the files are relative to.
    pub fn spec_root(&self) -> &std::path::Path {
        &self.spec_root
    }

    pub fn report(&self) -> &EditReport {
        &self.report
    }

    /// Nothing to write.
    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }

    /// Each file the plan changes and the text it will hold, by path relative to the spec root.
    pub fn after(&self) -> BTreeMap<String, String> {
        self.files
            .iter()
            .map(|(key, (_, after))| (key.clone(), after.clone()))
            .collect()
    }
}

/// The part of the plan being made: the edited texts, and what the earlier edits created or deleted.
struct Planner<'v> {
    view: &'v ProjectView<'v>,
    texts: BTreeMap<String, String>,
    entities: BTreeSet<String>,
    created: BTreeMap<String, (String, String)>,
    deleted: BTreeSet<String>,
}

/// Where an entity is.
struct Located<'v> {
    key: String,
    kind: String,
    node: Option<&'v specforge_graph::Node>,
}

impl<'v> Planner<'v> {
    fn text(&self, key: &str) -> Option<String> {
        match self.texts.get(key) {
            Some(text) => Some(text.clone()),
            None => self.view.project().source_text(key).map(|t| t.to_string()),
        }
    }

    fn locate(&self, id: &str) -> Result<Located<'v>, Refusal<EditField>> {
        if let Some((kind, key)) = self.created.get(id) {
            return Ok(Located {
                key: key.clone(),
                kind: kind.clone(),
                node: None,
            });
        }
        let graph = self.view.graph();
        match graph.node(id).filter(|_| !self.deleted.contains(id)) {
            Some(node) => Ok(Located {
                key: node.source_span.file.to_string(),
                kind: node.kind.raw.to_string(),
                node: Some(node),
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
                crate::authoring::rename::UNREADABLE,
                format!("no text was read for {}", located.key),
            )
        })?;
        let site = block::Site {
            view: self.view,
            key: &located.key,
            id,
            kind: &located.kind,
            resolved: located.node.map(|n| &n.fields),
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
            Edit::Create { .. } | Edit::Delete { .. } => Err(write::not_editable(
                EditField::Entity,
                "creating and deleting entities is not planned yet",
            )),
        }
    }

    fn finish(self) -> EditPlan {
        let mut files = BTreeMap::new();
        let mut diff = String::new();
        for (key, after) in self.texts {
            let before = self.view.project().source_text(&key);
            if before.as_deref() == Some(after.as_str()) {
                continue;
            }
            diff.push_str(
                &specforge_formatter::unified_diff(&key, before.as_deref().unwrap_or(""), &after)
                    .diff_text,
            );
            files.insert(key, (before, after));
        }
        let report = EditReport {
            entities: self.entities.into_iter().collect(),
            affected_files: files.keys().cloned().collect(),
            diff,
        };
        EditPlan {
            files,
            spec_root: self.view.env().spec_root().to_path_buf(),
            report,
        }
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
/// `edit <n> (<op> <entity>): `).
pub fn plan(view: &ProjectView<'_>, edits: &[Edit]) -> Result<EditPlan, Refusal<EditField>> {
    if let Some(problem) = view
        .env()
        .config_problems()
        .iter()
        .find(|problem| problem.blocks_rewrites())
    {
        return Err(crate::config::refusal(problem).into());
    }
    view.project_root()?;
    let mut planner = Planner {
        view,
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
    Ok(planner.finish())
}
