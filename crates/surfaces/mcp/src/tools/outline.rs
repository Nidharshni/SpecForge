use crate::target::View;
use crate::tool::{CoreTool, Effect, ReplySpec, ToolGroup};
use serde::Serialize;
use specforge_common::SourceSpan;
use specforge_common::shape::Shape;
use specforge_ops::read::navigate::{Navigator, OutlineEntry, outline};

use crate::args::Arguments;
use crate::containment;
use crate::reply::Answered;
use crate::tool::file_not_found;
use specforge_ops::Containment;
use specforge_ops::view::ProjectView;
use std::path::Path;

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
        // A file of the project is under its spec root, as spans name it:
        // one that leaves it is refused before anything is read (ADR 0092).
        let spec_root = super::spec_root(&view);
        if let Some(spec_root) = spec_root {
            Containment::within(spec_root)
                .resolve(Path::new(file))
                .map_err(|outside| Box::new(containment::refusal(&outside, "file")))?;
        }
        let entries = outline(&Navigator::new(view), file);
        // A file the graph has no entity from is either empty or not there:
        // under the project's spec root. With nothing served no file is the
        // project's (the dispatcher makes the refusal the no-project one,
        // ADR 0025).
        let on_disk = spec_root.is_some_and(|spec_root| spec_root.join(file).exists());
        if entries.is_empty() && !on_disk {
            return Err(file_not_found(file).into());
        }
        Ok(Reply {
            entries: entries.iter().map(Entry::of).collect(),
        }
        .into())
    }
}
