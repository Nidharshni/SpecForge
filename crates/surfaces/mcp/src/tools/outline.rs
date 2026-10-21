use crate::target::View;
use crate::tool::{CoreTool, Effect, ReplySpec, ToolGroup};
use serde::Serialize;
use specforge_common::SourceSpan;
use specforge_common::shape::Shape;
use specforge_ops::read::navigate::{Navigator, OutlineEntry, outline};

use crate::args::Arguments;
use crate::reply::Answered;
use crate::tool::McpError;
use specforge_ops::view::ProjectView;

/// `specforge.outline`'s arguments.
#[derive(Debug, Arguments)]
pub struct Args {
    /// Spec file, relative to the project's spec root
    file: String,
}

/// `specforge.outline`'s reply: the file's entities in
/// line order.
#[derive(Debug, Serialize, Shape)]
pub struct Reply {
    entries: Vec<Entry>,
}

/// One entity of the outline (`McpOutlineEntry`): `range` is its block,
/// `name_range` its name.
#[derive(Debug, Serialize, Shape)]
pub struct Entry {
    entity_id: String,
    kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    title: Option<String>,
    range: SourceSpan,
    name_range: SourceSpan,
    /// Its method members; absent when it has none.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    children: Vec<Child>,
}

/// A method member of an entity (`McpOutlineChild`): `<entity>.<method>`,
/// titled by its signature.
#[derive(Debug, Serialize, Shape)]
pub struct Child {
    entity_id: String,
    kind: ChildKind,
    title: String,
    range: SourceSpan,
    name_range: SourceSpan,
}

/// The one kind a child has.
#[derive(Debug, Serialize, Shape)]
#[serde(rename_all = "lowercase")]
pub enum ChildKind {
    Method,
}

impl Entry {
    fn of(entry: &OutlineEntry) -> Self {
        Entry {
            entity_id: entry.id.to_string(),
            kind: entry.kind.to_string(),
            title: entry.title.clone(),
            range: entry.block.clone(),
            name_range: entry.name.clone(),
            children: entry
                .children
                .iter()
                .map(|m| Child {
                    entity_id: format!("{}.{}", entry.id, m.name),
                    kind: ChildKind::Method,
                    title: m.signature.clone(),
                    range: m.block.clone(),
                    name_range: m.name_span.clone(),
                })
                .collect(),
        }
    }
}

/// `specforge.outline`: the entities a file declares, in line order, each
/// with its method members, each selecting its name: the tree the LSP's
/// document symbols are (`specforge_ops::read::navigate::outline`). `file` is a
/// spec file of the project, relative to its spec root, as spans name it.
pub(crate) struct Tool;

impl CoreTool for Tool {
    const NAME: &'static str = "specforge.outline";
    const DESCRIPTION: &'static str = "Get entity outline for a file";
    const EFFECT: Effect = Effect::Reads {
        group: ToolGroup::Navigation,
    };
    const SPEC: ReplySpec = ReplySpec::Type("McpOutlineResult");
    type Args = Args;
    type Input = View;
    type Returns = Answered<Reply>;

    fn call(view: ProjectView<'_>, args: Args) -> Answered<Reply> {
        let file = args.file.as_str();
        // The view reads the file into its source key and refuses it naming it (ADR 0195 D8); with nothing served
        // the dispatcher makes that refusal the no-project one (ADR 0025).
        let entries = outline(&Navigator::new(view), file)
            .map_err(|refused| McpError::from(refused).with_argument("file"))?;
        Ok(Reply {
            entries: entries.iter().map(Entry::of).collect(),
        }
        .into())
    }
}
