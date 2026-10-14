//! The inference guide: what an agent is told to look for to infer a
//! project's entities from code (CONTEXT.md "Inference guide"), a read view
//! over the project view. The infer prompt's overview, kind and file scopes,
//! `specforge infer-guide` and the LSP's keyword completion render it.

use std::collections::BTreeMap;
use std::path::Path;

use serde::Serialize;
use specforge_common::shape::Shape;
use specforge_protocol_types::{EntityKindDescriptor, ExtensionDeclaration};
use specforge_registry::{FieldRegistryEntry, FieldType};

use crate::Refusal;
use crate::view::ProjectView;

/// One kind's guide.
#[derive(Debug, Clone)]
pub struct KindGuide<'a> {
    /// The keyword entities of the kind are written with.
    pub keyword: &'a str,
    /// The extension the registry build registered the kind for (the first
    /// to declare it: E026's first-wins).
    pub extension: &'a str,
    pub description: Option<&'a str>,
    /// Every field the registry build registered on the kind (its own, its
    /// extension's shared fields, other extensions' enhancements), by name.
    pub fields: Vec<&'a FieldRegistryEntry>,
    /// The extension's `inference_guide`, then the project's own guide for
    /// the kind (`inference.<kind>` in specforge.json) after a blank line and
    /// `**Project-specific:**`; either alone when the other is absent; empty
    /// when neither is.
    pub guide: String,
    /// The kind's entities, in id order.
    pub existing: Vec<&'a str>,
}

impl KindGuide<'_> {
    /// An example entity of the kind: its required fields, then its first
    /// three optional ones, in name order, each written the way its type is
    /// written (`"..."`, `0`, `true`, an enum's declared default else its
    /// first value, `["item1", "item2"]`, `ref_id`, `[ref_1, ref_2]`, `{ }`
    /// for a block).
    pub fn example(&self) -> String {
        let required = self.fields.iter().filter(|f| f.declared().required);
        let optional = self
            .fields
            .iter()
            .filter(|f| !f.declared().required)
            .take(3);
        let mut lines = vec![format!(
            "{} example_{} \"Example Title\" {{",
            self.keyword, self.keyword
        )];
        for field in required.chain(optional) {
            lines.push(format!("  {} {}", field.name(), example_value(field)));
        }
        lines.push("}".to_string());
        lines.join("\n")
    }

    /// The kind scope's data, as `specforge infer-guide <kind> --format json`
    /// and the infer prompt's kind scope write it.
    pub fn document(&self) -> KindGuideDocument {
        KindGuideDocument {
            kind: self.keyword.to_string(),
            extension: self.extension.to_string(),
            description: self.description.map(str::to_string),
            existing_entity_ids: self.existing.iter().map(|id| id.to_string()).collect(),
            fields: self
                .fields
                .iter()
                .map(|field| GuideField {
                    name: field.name().to_string(),
                    field_type: field.field_type().as_str().to_string(),
                    required: field.declared().required,
                    description: field.declared().description.clone(),
                    values: field.enum_values().to_vec(),
                    default: field.declared().default_value.clone(),
                })
                .collect(),
            inference_guide: self.guide.clone(),
            example: self.example(),
        }
    }
}

/// A kind's guide as `specforge infer-guide <kind> --format json` and the
/// infer prompt's kind scope print it.
#[derive(Debug, Clone, PartialEq, Serialize, Shape)]
pub struct KindGuideDocument {
    pub kind: String,
    pub extension: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub existing_entity_ids: Vec<String>,
    pub fields: Vec<GuideField>,
    pub inference_guide: String,
    pub example: String,
}

/// One field of a kind's guide.
#[derive(Debug, Clone, PartialEq, Serialize, Shape)]
pub struct GuideField {
    pub name: String,
    #[serde(rename = "type")]
    pub field_type: String,
    pub required: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// An enum's values, in order; absent for any other type.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub values: Vec<String>,
    /// What an absent value counts as (the declared default), if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default: Option<String>,
}

/// How the example writes a value of `field`'s type.
fn example_value(field: &FieldRegistryEntry) -> String {
    match field.field_type() {
        FieldType::String => "\"...\"".into(),
        FieldType::Integer => "0".into(),
        FieldType::Bool => "true".into(),
        FieldType::Enum => field
            .declared()
            .default_value
            .clone()
            .or_else(|| field.enum_values().first().cloned())
            .unwrap_or_else(|| "\"...\"".into()),
        FieldType::StringList => "[\"item1\", \"item2\"]".into(),
        FieldType::Reference => "ref_id".into(),
        FieldType::ReferenceList => "[ref_1, ref_2]".into(),
        FieldType::Block => "{\n  }".into(),
    }
}

/// The guide of a whole project.
#[derive(Debug, Clone)]
pub struct InferenceGuide<'a> {
    /// The loaded extensions, in load order.
    pub extensions: Vec<&'a str>,
    /// Every declared kind's guide, in declaration order (a kind declared
    /// twice once, as its registering extension declares it).
    pub kinds: Vec<KindGuide<'a>>,
    /// Entities per kind, every kind entities are written with
    /// (`ProjectView::entities_by_kind`).
    pub entities_by_kind: BTreeMap<&'static str, usize>,
    /// The project's global conventions (`inference.global`).
    pub conventions: Option<&'a str>,
    /// Where the project's `.spec` files go: its spec root relative to its
    /// root, with a trailing `/` (`./` when they are the same, or without a
    /// root).
    pub spec_directory: String,
}

impl<'a> InferenceGuide<'a> {
    /// The guide of the kind written `keyword`.
    pub fn kind(&self, keyword: &str) -> Option<&KindGuide<'a>> {
        self.kinds.iter().find(|kind| kind.keyword == keyword)
    }

    /// The overview's data, as `specforge infer-guide --format json` and the
    /// infer prompt's overview write it.
    pub fn document(&self) -> GuideDocument {
        GuideDocument {
            installed_extensions: self
                .extensions
                .iter()
                .map(|name| name.to_string())
                .collect(),
            existing_entities: self
                .entities_by_kind
                .iter()
                .map(|(kind, count)| (kind.to_string(), *count))
                .collect(),
            kinds: self
                .kinds
                .iter()
                .map(|kind| GuideKind {
                    kind: kind.keyword.to_string(),
                    extension: kind.extension.to_string(),
                    description: kind.description.map(str::to_string),
                    fields: kind
                        .fields
                        .iter()
                        .map(|field| {
                            if field.declared().required {
                                format!("{}*", field.name())
                            } else {
                                field.name().to_string()
                            }
                        })
                        .collect(),
                    inference_guide: kind.guide.clone(),
                })
                .collect(),
            project_conventions: self.conventions.unwrap_or("").to_string(),
            spec_directory: self.spec_directory.clone(),
        }
    }
}

/// The project's guide as `specforge infer-guide --format json` and the
/// infer prompt's overview print it.
#[derive(Debug, Clone, PartialEq, Serialize, Shape)]
pub struct GuideDocument {
    pub installed_extensions: Vec<String>,
    pub existing_entities: BTreeMap<String, usize>,
    pub kinds: Vec<GuideKind>,
    pub project_conventions: String,
    pub spec_directory: String,
}

/// One kind in the overview.
#[derive(Debug, Clone, PartialEq, Serialize, Shape)]
pub struct GuideKind {
    pub kind: String,
    pub extension: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Field names, a required one followed by `*`.
    pub fields: Vec<String>,
    pub inference_guide: String,
}

/// The project's inference guide.
pub fn guide<'a>(view: &ProjectView<'a>) -> InferenceGuide<'a> {
    let existing = existing_by_kind(view);
    let kinds = view
        .registries()
        .declarations()
        .iter()
        .flat_map(|declaration| {
            declaration
                .entities
                .iter()
                .map(move |descriptor| (declaration, descriptor))
        })
        .filter(|(declaration, descriptor)| registered_for(view, declaration, descriptor))
        .map(|(declaration, descriptor)| kind_of(view, declaration, descriptor, &existing))
        .collect();
    InferenceGuide {
        extensions: view
            .registries()
            .extension_info()
            .map(|(name, _)| name)
            .collect(),
        kinds,
        entities_by_kind: view.entities_by_kind(),
        conventions: view.env().config().inference.global.as_deref(),
        spec_directory: spec_directory(view),
    }
}

/// The field of a kind guide's request a refusal can be about (ADR 0072). (Not `GuideField`:
/// that is a field of the guide's document.)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KindGuideField {
    /// The kind the guide is asked for.
    Kind,
}

/// The guide of the declared kind `kind`; an undeclared one is
/// `unknown_kind` naming the closest declared kind (`KnownKinds::declared`).
pub fn kind_guide<'a>(
    view: &ProjectView<'a>,
    kind: &str,
) -> Result<KindGuide<'a>, Refusal<KindGuideField>> {
    view.kinds()
        .declared(kind)
        .map_err(|error| error.about(KindGuideField::Kind))?;
    let (declaration, descriptor) = view
        .registries()
        .declarations()
        .iter()
        .flat_map(|declaration| {
            declaration
                .entities
                .iter()
                .map(move |descriptor| (declaration, descriptor))
        })
        .find(|(declaration, descriptor)| {
            keyword(descriptor) == kind && registered_for(view, declaration, descriptor)
        })
        .expect("a declared kind has its declaration");
    Ok(kind_of(
        view,
        declaration,
        descriptor,
        &existing_by_kind(view),
    ))
}

/// The keyword a kind is written with: its declared keyword, else its name.
fn keyword(kind: &EntityKindDescriptor) -> &str {
    kind.keyword.as_deref().unwrap_or(&kind.name)
}

/// The kind registry registered `descriptor`'s kind for `declaration`'s
/// extension: the first declaration of a kind wins (E026).
fn registered_for(
    view: &ProjectView,
    declaration: &ExtensionDeclaration,
    descriptor: &EntityKindDescriptor,
) -> bool {
    view.registries()
        .kinds()
        .get(keyword(descriptor))
        .is_some_and(|entry| entry.source_extension == declaration.name())
}

/// The ids of every kind's entities, in id order.
fn existing_by_kind(view: &ProjectView) -> BTreeMap<&'static str, Vec<&'static str>> {
    let mut existing: BTreeMap<&'static str, Vec<&'static str>> = BTreeMap::new();
    for node in view.graph().nodes() {
        existing
            .entry(node.kind.raw.as_str())
            .or_default()
            .push(node.id.raw.as_str());
    }
    existing
}

fn kind_of<'a>(
    view: &ProjectView<'a>,
    declaration: &'a ExtensionDeclaration,
    descriptor: &'a EntityKindDescriptor,
    existing: &BTreeMap<&'static str, Vec<&'static str>>,
) -> KindGuide<'a> {
    let keyword = keyword(descriptor);
    // Every field the registry build registered on the kind: the registry's
    // map has no declaration order, so by name.
    let mut fields = view.registries().fields().fields_for_kind(keyword);
    fields.sort_by(|a, b| a.name().cmp(b.name()));
    KindGuide {
        keyword,
        extension: declaration.name(),
        description: descriptor.description.as_deref(),
        fields,
        guide: merged_guide(
            descriptor.inference_guide.as_deref().unwrap_or(""),
            view.env().config().inference.kinds.get(keyword),
        ),
        existing: existing.get(keyword).cloned().unwrap_or_default(),
    }
}

/// The extension's guide, then the project's own under `**Project-specific:**`.
fn merged_guide(extension_guide: &str, project_guide: Option<&String>) -> String {
    match project_guide {
        Some(project_guide) if !extension_guide.is_empty() => {
            format!("{extension_guide}\n\n**Project-specific:**\n{project_guide}")
        }
        Some(project_guide) => project_guide.clone(),
        None => extension_guide.to_string(),
    }
}

/// The spec root relative to the root, `/`-separated with a trailing `/`;
/// `./` when they are the same, when the spec root is not under the root
/// and without a root.
fn spec_directory(view: &ProjectView) -> String {
    let relative = view
        .root()
        .and_then(|root| view.env().spec_root().strip_prefix(root).ok())
        .filter(|relative| *relative != Path::new(""));
    match relative {
        Some(relative) => {
            let parts: Vec<String> = relative
                .components()
                .map(|part| part.as_os_str().to_string_lossy().into_owned())
                .collect();
            format!("{}/", parts.join("/"))
        }
        None => "./".to_string(),
    }
}
