//! What a core resource read returned, and the one place that turns it into
//! a `resources/read` reply: the resource-side twin of [`crate::tool`] and
//! [`crate::prompt`].
//!
//! Each core resource is one module implementing [`CoreResource`] (ADR 0049
//! D1, applied to resources by ADR 0103): its URI, name, description, MIME
//! type, what it changes with and its read are stated there once.
//! [`ResourceSpec`] is what the listing, the resources adapter, the
//! subscriptions and the reflection tests read of it. [`resource_envelope`]
//! alone builds the `contents` of a read and the error of a refused one.

use serde_json::{Value, json};
use specforge_common::Diagnostic;

use crate::protocol::{JsonRpcError, JsonRpcResponse};
use crate::registry::Listed;
use crate::subscriptions::Watched;
use crate::target::TargetSpec;
use crate::tool::{ErrorCode, McpError};
use crate::uri_template::{Malformed, Matched};
use specforge_ops::Noticed;
use specforge_ops::view::ProjectView;

/// One core resource, stated once in its own module under `resources/`.
pub trait CoreResource: Sync + 'static {
    /// Its URI, or an RFC 6570 template (`specforge://graph/{entity_id}`).
    const URI: &'static str;
    /// Its name as the listing gives it.
    const NAME: &'static str;
    const DESCRIPTION: &'static str;
    const MIME_TYPE: &'static str = "application/json";
    /// What its content changes with (`subscriptions::Watched`): which
    /// updates notify its subscribers.
    const CHANGES_WITH: Watched;
    /// Read the resource at a URI it [names](ResourceSpec::names), whose
    /// placeholders the template has read ([`crate::uri_template::read`]),
    /// over the project view it is given: the served project's, brought up to date,
    /// or the empty session's when nothing is served.
    fn read(view: ProjectView<'_>, uri: &Matched<'_>) -> ReadOutcome;
}

/// What a resource read returned: its MIME type and text. The URI is the one
/// the envelope writes.
pub struct ResourceText {
    pub mime_type: String,
    pub text: String,
    /// What the read noticed (I016, W053, I020; ADR 0111): sent as the
    /// result's `_meta.diagnostics`, the shape a tool result's has. The
    /// content stays text.
    pub notices: Vec<Diagnostic>,
}

impl ResourceText {
    /// An extension's read: its MIME type and text, no notices.
    pub fn new(mime_type: impl Into<String>, text: impl Into<String>) -> Self {
        Self {
            mime_type: mime_type.into(),
            text: text.into(),
            notices: Vec::new(),
        }
    }

    /// A JSON read that noticed: its text, its notices in `_meta.diagnostics` (ADR 0111 D2, ADR 0130).
    pub fn noticed(read: Noticed<String>) -> Self {
        let (text, notices) = read.into_value_and_notices();
        Self {
            mime_type: "application/json".into(),
            text,
            notices,
        }
    }

    pub fn json(text: impl Into<String>) -> Self {
        Self {
            mime_type: "application/json".into(),
            text: text.into(),
            notices: Vec::new(),
        }
    }

    pub fn with_notices(mut self, notices: Vec<Diagnostic>) -> Self {
        self.notices = notices;
        self
    }
}

/// What a resource read produced, or why it was refused (ADR 0024 D4).
pub type ReadOutcome = Result<ResourceText, Box<McpError>>;

/// What the listing, the resources adapter, the subscriptions and the
/// reflection tests read of a core resource; implemented once for every
/// [`CoreResource`].
pub trait ResourceSpec: Sync {
    fn uri(&self) -> &'static str;
    fn name(&self) -> &'static str;
    fn description(&self) -> &'static str;
    fn mime_type(&self) -> &'static str;
    /// What its content changes with ([`CoreResource::CHANGES_WITH`]).
    fn changes_with(&self) -> Watched;
    /// Read the resource at a URI it [names](Self::names): the template
    /// reads its placeholders first, and refuses a malformed one.
    fn read(&self, view: ProjectView<'_>, uri: &str) -> ReadOutcome;

    /// How it reaches its project: every core resource reads the served
    /// project's view ([`TargetSpec::SERVED_VIEW`]).
    fn target(&self) -> TargetSpec {
        TargetSpec::SERVED_VIEW
    }

    /// The resource as `resources/list` (or `resources/templates/list`)
    /// describes it.
    fn descriptor(&self) -> Listed {
        Listed::of(
            self.uri(),
            self.name(),
            Some(self.description()),
            self.mime_type(),
        )
    }

    /// Whether `uri` names this resource, by the one rule extension
    /// resources follow too ([`crate::uri_template::names`]).
    fn names(&self, uri: &str) -> bool {
        crate::uri_template::names(self.uri(), uri)
    }
}

impl<T: CoreResource> ResourceSpec for T {
    fn uri(&self) -> &'static str {
        T::URI
    }

    fn name(&self) -> &'static str {
        T::NAME
    }

    fn description(&self) -> &'static str {
        T::DESCRIPTION
    }

    fn mime_type(&self) -> &'static str {
        T::MIME_TYPE
    }

    fn changes_with(&self) -> Watched {
        T::CHANGES_WITH
    }

    fn read(&self, view: ProjectView<'_>, uri: &str) -> ReadOutcome {
        let matched = crate::uri_template::read(T::URI, uri).map_err(Malformed::refusal)?;
        T::read(view, &matched)
    }
}

/// The `resources/read` reply: the only place that builds `contents` or a
/// resource's error. Its `uri` is the one given. A refusal is a JSON-RPC
/// error whose data is its McpError ([`McpError::into_rpc_error`]); one
/// that says the entity a read names does not exist is *not found*, and
/// carries `not_found`, the code of the revision the request speaks.
pub(crate) fn resource_envelope(
    outcome: ReadOutcome,
    uri: &str,
    not_found: i64,
    id: Option<Value>,
) -> JsonRpcResponse {
    match outcome {
        Ok(text) => {
            let mut result = json!({
                "contents": [{
                    "uri": uri,
                    "mimeType": text.mime_type,
                    "text": text.text,
                }]
            });
            if !text.notices.is_empty() {
                result["_meta"] = json!({
                    "diagnostics": specforge_common::diagnostics_json(&text.notices),
                });
            }
            JsonRpcResponse::success(id, result)
        }
        Err(refusal) => {
            let missing = refusal.code == ErrorCode::EntityNotFound;
            let mut error = refusal.into_rpc_error();
            if missing {
                error.code = not_found;
            }
            JsonRpcResponse::from_error(id, error)
        }
    }
}

/// A read of a URI no core resource and no extension serves: a lookup
/// failure, like "Unknown tool", so it carries no McpError, but it is *not
/// found* in the revision of the request (`not_found`) and names the URI.
pub(crate) fn unknown_resource(not_found: i64, uri: &str) -> JsonRpcError {
    JsonRpcError::new(not_found, format!("Unknown resource URI: {uri}"))
        .with_data(json!({ "uri": uri }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use specforge_common::codes;

    fn result_of(text: ResourceText) -> Value {
        resource_envelope(Ok(text), "specforge://x", -32002, None)
            .result
            .expect("a read is a result")
    }

    #[test]
    fn a_noticed_read_carries_its_notices_in_its_meta() {
        let notice = Diagnostic::new(codes::I016, "the schema cache could not be read");

        let with = result_of(ResourceText::json("{}").with_notices(vec![notice]));
        assert_eq!(with["_meta"]["diagnostics"][0]["code"], "I016", "{with}");
        assert_eq!(with["contents"][0]["text"], "{}");

        let noticed = result_of(ResourceText::noticed(Noticed::new(
            "{}".into(),
            vec![Diagnostic::new(
                codes::I016,
                "the schema cache could not be read",
            )],
        )));
        assert_eq!(
            noticed["_meta"]["diagnostics"][0]["code"], "I016",
            "{noticed}"
        );

        let without = result_of(ResourceText::json("{}"));
        assert!(without.get("_meta").is_none(), "{without}");
    }
}
