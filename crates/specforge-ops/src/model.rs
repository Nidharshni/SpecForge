//! `specforge model`/`specforge.model` and `specforge outline`/
//! `specforge.outline_extensions`: the logical data model and the extension
//! architecture diagrams, each one operation over the project view (ADR
//! 0015). Each enumerated argument is an option table here (ADR 0027), so
//! both surfaces list, accept, default and refuse the same names.

// The value types, so surfaces name ops rather than the emitter.
pub use specforge_emitter::model::{FieldLevel, GroupBy, ModelFormat, ModelOptions, ModelRoot};
pub use specforge_emitter::outline::{
    DependencyDepth, OutlineDetail, OutlineFormat, OutlineOptions,
};

use specforge_common::{Diagnostic, find_close_match};
use specforge_protocol_types::ExtensionDeclaration;

use crate::options::{Choice, OptionTable};
use crate::view::ProjectView;
use crate::{OpError, OpErrorKind, Refusal};

/// What `specforge model` and `specforge.model` show.
#[derive(Debug, Clone, PartialEq)]
pub struct ModelOutcome {
    /// The model, drawn in the requested format.
    pub document: String,
    /// I020 for each kind of `ModelOptions::kinds` the project does not
    /// know (ADR 0015 Q3); the filter still drops it.
    pub notices: Vec<Diagnostic>,
}

/// The field of a model request a refusal can be about (ADR 0072).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelField {
    /// `ModelOptions::extension`.
    Extension,
    /// `ModelOptions::root`.
    Root,
}

/// The logical data model of the view's extensions, as `options` asks.
///
/// Err:
/// - `extension_not_found` (`ExtensionNotFound`) when `options.extension`
///   names no extension the project loads. It names the loaded one meant
///   (the one whose short name it is, else the closest), else lists the
///   loaded ones.
/// - `unknown_kind` (`InvalidInput`) when `options.root` names a kind no
///   loaded extension declares, naming the closest declared kind.
pub fn model(
    view: &ProjectView,
    options: &ModelOptions,
) -> Result<ModelOutcome, Refusal<ModelField>> {
    let declarations = view.registries().declarations();
    if let Some(extension) = &options.extension {
        loaded(declarations, extension).map_err(|error| error.about(ModelField::Extension))?;
    }
    let kinds = view.kinds();
    if let Some(root) = &options.root {
        kinds
            .declared(&root.kind)
            .map_err(|error| error.about(ModelField::Root))?;
    }
    let filter: Vec<&str> = options.kinds.iter().map(String::as_str).collect();
    Ok(ModelOutcome {
        notices: kinds.unknown_in(&filter),
        document: specforge_emitter::model::export(
            view.versioned_schema().schema(),
            declarations,
            options,
        ),
    })
}

/// `Ok` when an extension the project loads is named `name`; else
/// `extension_not_found`, with the loaded extension `name` most likely
/// means as its suggestion.
fn loaded(declarations: &[ExtensionDeclaration], name: &str) -> Result<(), OpError> {
    if declarations.iter().any(|d| d.name() == name) {
        return Ok(());
    }
    let names: Vec<&str> = declarations
        .iter()
        .map(ExtensionDeclaration::name)
        .collect();
    let meant = declarations
        .iter()
        .find(|d| d.short() == name)
        .map(ExtensionDeclaration::name)
        .or_else(|| find_close_match(name, names.iter().copied()));
    let suggestion = match meant {
        Some(meant) => format!("did you mean '{meant}'?"),
        None if names.is_empty() => "the project loads no extension".to_string(),
        None => format!("the project loads {}", names.join(", ")),
    };
    Err(OpError::new(
        OpErrorKind::ExtensionNotFound,
        crate::extension::NOT_FOUND,
        format!("extension '{name}' is not loaded by this project"),
    )
    .with_suggestion(suggestion)
    .with_data(serde_json::json!({ "extension": name })))
}

/// The architecture of the view's extensions (dependencies, enhancements,
/// contributions), as `options` asks.
pub fn outline(view: &ProjectView, options: &OutlineOptions) -> String {
    specforge_emitter::outline::export(view.registries().declarations(), options)
}

/// A choice whose name says what it selects.
const fn plain<T>(name: &'static str, value: T) -> Choice<T> {
    Choice {
        name,
        aliases: &[],
        help: "",
        value,
    }
}

/// A choice with a one-line help.
const fn helped<T>(name: &'static str, help: &'static str, value: T) -> Choice<T> {
    Choice {
        name,
        aliases: &[],
        help,
        value,
    }
}

/// `specforge model --format`, `specforge.model`'s `format`.
pub const MODEL_FORMAT: OptionTable<ModelFormat> = OptionTable {
    argument: "format",
    choices: &[
        plain("markdown", ModelFormat::Markdown),
        helped("mermaid", "ER diagram", ModelFormat::Mermaid),
        helped("dot", "Graphviz", ModelFormat::Dot),
        plain("json", ModelFormat::Json),
        helped("dbml", "dbdiagram.io", ModelFormat::Dbml),
    ],
    default: Some(ModelFormat::Markdown),
};

/// `specforge model --group-by`, `specforge.model`'s `group_by`.
pub const GROUP_BY: OptionTable<GroupBy> = OptionTable {
    argument: "group_by",
    choices: &[
        helped(
            "extension",
            "under a header per extension",
            GroupBy::Extension,
        ),
        helped("none", "one flat list", GroupBy::None),
    ],
    default: Some(GroupBy::Extension),
};

/// `specforge model --fields`, `specforge.model`'s `fields`.
pub const MODEL_FIELDS: OptionTable<FieldLevel> = OptionTable {
    argument: "fields",
    choices: &[
        plain("none", FieldLevel::None),
        helped("keys", "key fields", FieldLevel::Keys),
        plain("all", FieldLevel::All),
    ],
    default: Some(FieldLevel::Keys),
};

/// `specforge outline --format`, `specforge.outline_extensions`' `format`.
pub const OUTLINE_FORMAT: OptionTable<OutlineFormat> = OptionTable {
    argument: "format",
    choices: &[
        plain("markdown", OutlineFormat::Markdown),
        helped("mermaid", "flowchart", OutlineFormat::Mermaid),
        helped("dot", "Graphviz", OutlineFormat::Dot),
        plain("json", OutlineFormat::Json),
    ],
    default: Some(OutlineFormat::Markdown),
};

/// `specforge outline --fields`, `specforge.outline_extensions`' `fields`.
pub const OUTLINE_FIELDS: OptionTable<OutlineDetail> = OptionTable {
    argument: "fields",
    choices: &[
        helped("none", "counts only", OutlineDetail::None),
        helped("keys", "names and rule codes", OutlineDetail::Keys),
        helped("all", "full field attribution", OutlineDetail::All),
    ],
    default: Some(OutlineDetail::Keys),
};

/// `specforge outline --deps`, `specforge.outline_extensions`' `deps`.
pub const DEPS: OptionTable<DependencyDepth> = OptionTable {
    argument: "deps",
    choices: &[
        helped("direct", "declared only", DependencyDepth::Direct),
        helped(
            "effective",
            "direct and used transitive",
            DependencyDepth::Effective,
        ),
        helped("full", "all transitive", DependencyDepth::Full),
    ],
    default: Some(DependencyDepth::Direct),
};
