//! Typed edits (ADR 0151): create, change and delete entities in a project's `.spec` files. Each value is written in
//! the form its field's declaration decides, each touched block is laid out by the formatter, and nothing is
//! written unless the edited project reports no error it did not report before (E080). The edits are written as
//! one project change (ADR 0128).

pub mod write;

use serde::{Deserialize, Serialize};
use specforge_common::shape::{Definitions, Shape};
use std::collections::BTreeMap;

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
