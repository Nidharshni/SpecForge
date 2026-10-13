//! `specforge.inspect`: one entity's facts (`specforge_ops::inspect`), as
//! JSON. The tool renders the read view; it reads no edge, coverage row or
//! attribution itself (ADR 0015, section "Inspect").

use crate::target::View;
use crate::tool::{CoreTool, Effect, ReplySpec, ToolGroup};
use serde::Serialize;
use serde_json::Value;
use specforge_common::shape::Shape;
use specforge_common::{DiagnosticList, SourceSpan};
use specforge_ops::inspect::{EntityCoverage, EntityFacts, obligation_text};

use crate::args::Arguments;
use crate::refusal::ArgumentOf;
use crate::reply::Answered;
use crate::tool::McpError;
use specforge_ops::navigate::EntityField;
use specforge_ops::view::ProjectView;

/// `specforge.inspect`'s arguments.
#[derive(Debug, Arguments)]
pub struct Args {
    /// Entity ID to inspect
    entity_id: String,
}

/// `specforge.inspect`'s reply: the one presenter of
/// an entity's facts as JSON.
#[derive(Debug, Serialize, Shape)]
pub struct Reply {
    entity_id: String,
    kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    title: Option<String>,
    /// Its kind's testability, the standing the hover shows (ADR 0004
    /// D2-d).
    testable: bool,
    /// Whether it declares obligations.
    declared: bool,
    /// It does not count toward coverage (the coverage row's `exempt`).
    exempt: bool,
    /// Whether its kind must declare obligations: why it is exempt.
    obligated: bool,
    /// The extension that declares its kind.
    #[serde(skip_serializing_if = "Option::is_none")]
    source_extension: Option<String>,
    source_span: SourceSpan,
    /// The statement the extension declares (headline and normative): a
    /// behavior's `contract`.
    #[serde(skip_serializing_if = "Option::is_none")]
    contract: Option<String>,
    /// Every field, whatever the kind names its text: an open value.
    fields: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    verify_declarations: Option<Vec<String>>,
    referenced_by: Vec<String>,
    refers_to: Vec<String>,
    #[shape(names = specforge_ops::coverage::STATUS)]
    coverage_status: String,
    /// The diagnostics about the entity, in the shape `specforge.validate`
    /// reports them.
    diagnostics: DiagnosticList,
}

pub(crate) struct Tool;

impl ArgumentOf<EntityField> for Tool {
    fn argument(field: EntityField) -> Option<&'static str> {
        match field {
            EntityField::EntityId => Some("entity_id"),
        }
    }
}

impl CoreTool for Tool {
    const NAME: &'static str = "specforge.inspect";
    const DESCRIPTION: &'static str = "Get full detail for a specific entity";
    const EFFECT: Effect = Effect::Reads {
        group: ToolGroup::Navigation,
    };
    const SPEC: ReplySpec = ReplySpec::Type("McpInspectResult");
    const OPEN_VALUES: &'static [&'static str] = &["$.fields"];
    type Args = Args;
    type Input = View;
    type Returns = Answered<Reply>;

    fn call(view: ProjectView<'_>, args: Args) -> Answered<Reply> {
        let facts =
            specforge_ops::inspect::inspect(&view, &args.entity_id).map_err(Self::refused)?;
        // A recorded report that cannot be read fails the call (ADR 0004 D2-e).
        let coverage =
            specforge_ops::inspect::coverage(&view, &args.entity_id).map_err(McpError::from)?;
        Ok(Reply::of(&facts, &coverage).into())
    }
}

impl Reply {
    fn of(facts: &EntityFacts, coverage: &EntityCoverage) -> Self {
        let node = facts.node;
        let refs = &facts.references;
        let declared = facts.standing.declares_obligations();
        Reply {
            entity_id: node.id.raw.to_string(),
            kind: node.kind.raw.to_string(),
            title: node.title.as_ref().map(ToString::to_string),
            testable: facts.standing.testable,
            declared,
            exempt: facts.standing.exempt(),
            obligated: facts.standing.obligated(),
            source_extension: facts.kind.map(|kind| kind.source_extension.to_string()),
            source_span: node.source_span.clone(),
            contract: facts.headline.as_ref().map(ToString::to_string),
            fields: Value::Object(
                specforge_graph::field_form::write_fields(&node.fields)
                    .into_iter()
                    .map(|(key, written)| (key, Value::from(written)))
                    .collect(),
            ),
            verify_declarations: declared
                .then(|| facts.obligations.iter().map(obligation_text).collect()),
            referenced_by: refs.referenced_by().into_iter().map(String::from).collect(),
            refers_to: refs.refers_to().into_iter().map(String::from).collect(),
            coverage_status: specforge_ops::coverage::STATUS
                .name_of(coverage.status())
                .to_string(),
            // The diagnostics about the entity: those its data names it in,
            // else those inside its block (ADR 0016); never by its message.
            diagnostics: DiagnosticList(facts.diagnostics.iter().map(|d| (*d).clone()).collect()),
        }
    }
}
