use crate::target::Project;
use crate::tool::{CoreTool, Effect, ReplySpec, ToolGroup};
use serde::Serialize;
use specforge_common::shape::Shape;
use specforge_graph::Graph;
use specforge_ops::navigate::{
    FileAnchors, FileMatch, SourceAnchor, anchors_of_file, source_anchors,
};

use crate::args::Arguments;
use crate::reply::Answered;
use crate::target::ProjectRef;
use crate::tool::McpError;

/// `specforge.find_spec_for_source`'s arguments.
#[derive(Debug, Arguments)]
pub struct Args {
    /// Path of the source file, relative to the project root (a directory lists the files under it; a trailing part of a path matches it)
    file_path: String,
}

/// `specforge.find_spec_for_source`'s reply.
#[derive(Debug, Serialize, Shape)]
pub struct Reply {
    file_path: String,
    match_mode: MatchMode,
    entities: Vec<Anchored>,
    count: usize,
}

/// How a source-file query matched its anchors, as MCP spells it: the file
/// itself, a directory the files are under, or a trailing part of their
/// paths.
#[derive(Debug, Clone, Copy, Serialize, Shape)]
#[serde(rename_all = "snake_case")]
pub enum MatchMode {
    Exact,
    Directory,
    SuffixPath,
    None,
}

impl From<FileMatch> for MatchMode {
    fn from(mode: FileMatch) -> Self {
        match mode {
            FileMatch::Exact => MatchMode::Exact,
            FileMatch::Under => MatchMode::Directory,
            FileMatch::Suffix => MatchMode::SuffixPath,
            FileMatch::None => MatchMode::None,
        }
    }
}

/// One entity anchored to a source file, as MCP spells it: the anchor, and
/// the entity's kind when the graph has it.
#[derive(Debug, Serialize, Shape)]
pub struct Anchored {
    entity_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    kind: Option<String>,
    file: String,
    line: usize,
    symbol_name: String,
    item_kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    confidence: Option<f64>,
}

impl Anchored {
    pub(crate) fn of(anchor: &SourceAnchor, graph: &Graph) -> Self {
        Anchored {
            entity_id: anchor.entity_id.clone(),
            kind: graph
                .node(&anchor.entity_id)
                .map(|n| n.kind.raw.to_string()),
            file: anchor.file.clone(),
            line: anchor.line,
            symbol_name: anchor.symbol_name.clone(),
            item_kind: anchor.item_kind.clone(),
            confidence: anchor.confidence,
        }
    }
}

/// The entities anchored to the source files `file_path` names, under the
/// one file rule (`specforge_ops::navigate::anchors_of_file`).
pub(crate) struct Tool;

impl CoreTool for Tool {
    const NAME: &'static str = "specforge.find_spec_for_source";
    const DESCRIPTION: &'static str = "Find specforge entities anchored to a source file";
    const EFFECT: Effect = Effect::Reads {
        group: ToolGroup::Navigation,
    };
    const SPEC: ReplySpec = ReplySpec::Type("McpSpecForSourceResult");
    type Args = Args;
    type Input = Project;
    type Returns = Answered<Reply>;

    fn call(project: ProjectRef<'_>, args: Args) -> Answered<Reply> {
        let manifest = source_anchors(&project.view()).map_err(McpError::from)?;
        let FileAnchors { mode, anchors } = anchors_of_file(&manifest, &args.file_path);
        let entities: Vec<Anchored> = anchors
            .iter()
            .map(|anchor| Anchored::of(anchor, project.graph()))
            .collect();
        Ok(Reply {
            file_path: args.file_path,
            match_mode: mode.into(),
            count: entities.len(),
            entities,
        }
        .into())
    }
}
