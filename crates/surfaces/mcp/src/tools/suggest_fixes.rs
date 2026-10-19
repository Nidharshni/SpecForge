use crate::target::View;
use crate::tool::{CoreTool, Effect, ReplySpec, ToolGroup};
use serde::Serialize;
use specforge_common::shape::Shape;
use specforge_ops::authoring::fix::{self, Fix, FixKind, FixQuery};
use specforge_ops::read::navigate::Navigator;

use crate::args::Arguments;
use crate::reply::Answered;
use specforge_ops::view::ProjectView;

/// `specforge.suggest_fixes`'s arguments.
#[derive(Debug, Arguments)]
pub struct Args {
    /// Entity ID (optional, all if omitted)
    entity_id: Option<String>,
    /// Only diagnostics in this spec file
    file_path: Option<String>,
    /// Only diagnostics with this code, e.g. W001
    diagnostic_code: Option<String>,
}

/// `specforge.suggest_fixes`'s reply.
#[derive(Debug, Serialize, Shape)]
pub struct Reply {
    fixes: Vec<Suggestion>,
}

/// A fix (`McpFixSuggestion`): its edits are the spec's `TextEdit`.
#[derive(Debug, Serialize, Shape)]
pub struct Suggestion {
    /// The LSP code action's title.
    title: String,
    kind: FixKind,
    /// The code of the diagnostic the fix resolves.
    #[serde(skip_serializing_if = "Option::is_none")]
    diagnostic_code: Option<String>,
    /// The entity the fix is about.
    #[serde(skip_serializing_if = "Option::is_none")]
    entity_id: Option<String>,
    edits: Vec<crate::reply::TextEdit>,
}

impl Suggestion {
    fn of(fix: &Fix) -> Self {
        Suggestion {
            title: fix.title.clone(),
            kind: fix.kind,
            diagnostic_code: fix.diagnostic_code.clone(),
            entity_id: fix.subject.as_ref().map(ToString::to_string),
            edits: fix.edits.iter().map(Into::into).collect(),
        }
    }
}

/// `specforge.suggest_fixes`: the fixes the LSP offers as code actions for
/// the same diagnostics and entities, each with its edits (ADR 0016). A
/// diagnostic whose data names no fix contributes none: its suggestion
/// text stays on the diagnostic (validate, inspect).
pub(crate) struct Tool;

impl CoreTool for Tool {
    const NAME: &'static str = "specforge.suggest_fixes";
    const DESCRIPTION: &'static str = "The fixes the LSP offers as code actions for the project's diagnostics and entities, each with its edits";
    const EFFECT: Effect = Effect::Reads {
        group: ToolGroup::Navigation,
    };
    const SPEC: ReplySpec = ReplySpec::Type("McpFixSuggestions");
    type Args = Args;
    type Input = View;
    type Returns = Answered<Reply>;

    fn call(view: ProjectView<'_>, args: Args) -> Answered<Reply> {
        if let Some(entity_id) = args.entity_id.as_deref()
            && view.graph().node(entity_id).is_none()
        {
            return Err(crate::tool::entity_not_found(view.graph(), entity_id)
                .with_argument("entity_id")
                .into());
        }
        let diagnostics = view.reported();
        let query = FixQuery {
            entity: args.entity_id.as_deref(),
            file: args.file_path.as_deref(),
            code: args.diagnostic_code.as_deref(),
            within: None,
        };
        let fixes = fix::fixes(&Navigator::new(view), &diagnostics, &query);
        Ok(Reply {
            fixes: fixes.iter().map(Suggestion::of).collect(),
        }
        .into())
    }
}
