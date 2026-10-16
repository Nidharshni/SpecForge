//! What a trace request asks for (ADR 0101): a trace of one entity or of every entity, or an agent plan's check.

use super::Target;
use crate::read::plan::AgentPlan;
use crate::{OpError, OpErrorKind, Refusal};

/// The code of the refusal of an entity and a plan in one request: they are two traces.
pub const ENTITY_WITH_PLAN: &str = "entity_with_plan";

/// The field of a trace request a refusal of the request itself can be about (ADR 0072). An entity the
/// graph lacks stays the read views' `EntityField::EntityId`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TraceField {
    /// The plan given beside an entity.
    Plan,
}

/// The read a trace request asks for.
#[derive(Debug, Clone, Copy)]
pub enum Ask<'a> {
    /// The trace operation over `Target`: one entity's chain when an entity is given, every entity's when
    /// neither an entity nor a plan is. A surface may decline to offer `Target::Every` (MCP does, ADR 0101 D4).
    Trace(Target<'a>),
    /// The check of an agent plan against the graph (`plan::check`).
    Plan(&'a AgentPlan),
}

impl<'a> Ask<'a> {
    /// The one reading of a trace request's entity and plan: either, or neither; both are refused,
    /// invalid input about the plan.
    pub fn of(
        entity: Option<&'a str>,
        plan: Option<&'a AgentPlan>,
    ) -> Result<Self, Refusal<TraceField>> {
        match (entity, plan) {
            (Some(_), Some(_)) => Err(OpError::new(
                OpErrorKind::InvalidInput,
                ENTITY_WITH_PLAN,
                "an entity and a plan are two traces: give one",
            )
            .about(TraceField::Plan)),
            (Some(entity), None) => Ok(Ask::Trace(Target::Entity(entity))),
            (None, Some(plan)) => Ok(Ask::Plan(plan)),
            (None, None) => Ok(Ask::Trace(Target::Every)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plan() -> AgentPlan {
        AgentPlan::parse(
            r#"{"plan_id":"p","entries":[{"entity_id":"login","action":"implement"}]}"#,
        )
        .unwrap()
    }

    #[specforge_test_macros::test(
        behavior = "validate_agent_plan",
        verify = "an entity and a plan together are invalid input about the plan, on every surface"
    )]
    fn an_entity_with_a_plan_is_refused_about_the_plan() {
        let refusal = Ask::of(Some("login"), Some(&plan())).unwrap_err();
        assert_eq!(refusal.error.kind, OpErrorKind::InvalidInput);
        assert_eq!(refusal.error.code, ENTITY_WITH_PLAN);
        assert_eq!(refusal.subject, Some(TraceField::Plan));
    }

    #[test]
    fn either_or_neither_is_one_trace() {
        let plan = plan();
        assert!(matches!(
            Ask::of(Some("login"), None),
            Ok(Ask::Trace(Target::Entity("login")))
        ));
        assert!(matches!(Ask::of(None, Some(&plan)), Ok(Ask::Plan(_))));
        assert!(matches!(Ask::of(None, None), Ok(Ask::Trace(Target::Every))));
    }
}
