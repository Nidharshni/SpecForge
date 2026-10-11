//! A core tool's reply: one typed definition per tool (ADR 0048).
//!
//! A handler answers its module's `Reply` (a struct, or an untagged union
//! of structs, deriving `Serialize` and `Shape`), or [`Text`] for a tool
//! whose result is a document in text. A tool's module names the type once
//! (`type Returns = Answered<Reply>`, ADR 0049): the listed
//! outputSchema is ``shape::build::<Reply>()``, the result is the value
//! serialized, and [`conforming`] checks every structured result
//! against that schema before it is sent: a reply its schema refuses is a
//! `schema_mismatch` failure, never structured content (the rule extension
//! tools follow).

use serde::Serialize;
use serde_json::{Map, Value, json};
use specforge_common::Diagnostic;
pub use specforge_common::shape::{Object, Shape};

use crate::tool::{ErrorCode, McpError, Payload, ToolOutcome};

/// What a core tool's handler answers: its reply, the diagnostics that ride
/// in `_meta.diagnostics`, and other `_meta` entries (`specforge/`-prefixed).
#[derive(Debug)]
pub struct Answer<R> {
    pub reply: R,
    pub diagnostics: Vec<Diagnostic>,
    pub meta: Map<String, Value>,
}

impl<R> Answer<R> {
    pub fn new(reply: R) -> Self {
        Answer {
            reply,
            diagnostics: Vec::new(),
            meta: Map::new(),
        }
    }

    /// The same answer with `extra` in its `_meta.diagnostics`.
    pub fn with_diagnostics(mut self, extra: Vec<Diagnostic>) -> Self {
        self.diagnostics.extend(extra);
        self
    }

    /// The same answer with `value` under `key` in its `_meta`.
    pub fn with_meta(mut self, key: &str, value: Value) -> Self {
        self.meta.insert(key.to_string(), value);
        self
    }
}

impl<R> From<R> for Answer<R> {
    fn from(reply: R) -> Self {
        Answer::new(reply)
    }
}

/// A handler's result: its answer, or the refusal it raised (`?` on an
/// `McpError`).
pub type Answered<R> = Result<Answer<R>, Box<McpError>>;

/// A reply in text: a document in the format the call asked for (export,
/// model, outline_extensions) or validate's diagnostics (ADR 0018 D4). It
/// lists no outputSchema.
#[derive(Debug)]
pub struct Text(pub String);

impl From<String> for Text {
    fn from(text: String) -> Self {
        Text(text)
    }
}

/// The outputSchema of a tool whose reply is `R`: `R`'s document, built once
/// per process (ADR 0051).
pub fn output_schema<R: Object + 'static>() -> &'static Value {
    specforge_common::shape::document::<R>()
}

/// `answered` as the outcome the pipeline sends: the reply serialized as a
/// structured payload, its diagnostics and `_meta`; a refusal as it is.
pub fn structured<R: Object + Serialize>(answered: Answered<R>) -> ToolOutcome {
    match answered {
        Ok(Answer {
            reply,
            diagnostics,
            meta,
        }) => ToolOutcome::Done {
            payload: Payload::Json(serde_json::to_value(&reply).expect("a reply serializes")),
            is_error: false,
            diagnostics,
            meta,
        },
        Err(refused) => ToolOutcome::Refused(refused),
    }
}

/// `answered` as a text outcome.
pub fn text(answered: Answered<Text>) -> ToolOutcome {
    match answered {
        Ok(Answer {
            reply: Text(text),
            diagnostics,
            meta,
        }) => ToolOutcome::Done {
            payload: Payload::Text(vec![text]),
            is_error: false,
            diagnostics,
            meta,
        },
        Err(refused) => ToolOutcome::Refused(refused),
    }
}

/// Where a tool whose structured result is checked comes from: the `source`
/// `tools/list` gives it.
#[derive(Debug, Clone, Copy)]
pub(crate) enum Source<'a> {
    Core,
    /// The contributing extension's name.
    Extension(&'a str),
}

/// `outcome` when its structured payload conforms to `schema`; otherwise the
/// failure `schema_mismatch` with `data.violations`: "core tool '<tool>'
/// answered a reply its output schema refuses: <violations>", or for an
/// extension's tool "tool '<tool>' of '<extension>' answered a reply its
/// output schema refuses: <violations>". A failure or a text payload is
/// returned as it is; a refused core reply keeps its diagnostics.
pub(crate) fn conforming(
    tool: &str,
    source: Source<'_>,
    schema: &Value,
    outcome: ToolOutcome,
) -> ToolOutcome {
    let ToolOutcome::Done {
        payload: Payload::Json(value),
        is_error: false,
        ..
    } = &outcome
    else {
        return outcome;
    };
    let violations = specforge_common::shape::violations(schema, value);
    if violations.is_empty() {
        return outcome;
    }
    let whose = match source {
        Source::Core => format!("core tool '{tool}'"),
        Source::Extension(extension) => format!("tool '{tool}' of '{extension}'"),
    };
    let mut refused = McpError::new(
        ErrorCode::SchemaMismatch,
        format!(
            "{whose} answered a reply its output schema refuses: {}",
            violations.join("; ")
        ),
    )
    .with_data(json!({ "violations": violations }));
    if let ToolOutcome::Done { diagnostics, .. } = outcome {
        refused.reported = diagnostics;
    }
    ToolOutcome::Refused(Box::new(refused))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[specforge_test_macros::test(
        behavior = "follow_negotiated_mcp_revision",
        verify = "a core tool's reply its output schema refuses is a schema_mismatch error, never structured content"
    )]
    fn a_reply_its_schema_refuses_is_a_schema_mismatch() {
        let schema = json!({
            "type": "object",
            "properties": {"a": {"type": "string"}},
            "required": ["a"],
            "additionalProperties": false,
        });
        let conforming_outcome = conforming(
            "specforge.probe",
            Source::Core,
            &schema,
            ToolOutcome::ok(json!({"a": "x"})),
        );
        assert!(conforming_outcome.succeeded());

        let outcome = conforming(
            "specforge.probe",
            Source::Core,
            &schema,
            ToolOutcome::ok(json!({"a": "x", "b": 1})),
        );
        let ToolOutcome::Refused(error) = outcome else {
            panic!("a reply its schema refuses is a failure: {outcome:?}");
        };
        assert_eq!(error.code, ErrorCode::SchemaMismatch);
        assert_eq!(
            error.message,
            "core tool 'specforge.probe' answered a reply its output schema refuses: $.b: undeclared key"
        );
        assert_eq!(
            error.data,
            Some(json!({"violations": ["$.b: undeclared key"]}))
        );

        // A text outcome and a refusal pass through unchanged.
        let text = conforming(
            "specforge.probe",
            Source::Core,
            &schema,
            ToolOutcome::text("anything"),
        );
        assert!(text.succeeded());
        let refusal = conforming(
            "specforge.probe",
            Source::Core,
            &schema,
            ToolOutcome::error(ErrorCode::InvalidInput, "no"),
        );
        assert!(matches!(refusal, ToolOutcome::Refused(e) if e.code == ErrorCode::InvalidInput));
    }
}
