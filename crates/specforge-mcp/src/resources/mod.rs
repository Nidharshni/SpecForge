mod schema;
mod views;

use serde_json::{Value, json};

use crate::events::{EventSink, McpEvent};
use crate::lifecycle::Revision;
use crate::protocol::{JsonRpcError, JsonRpcResponse};
use crate::registry::Listed;
use crate::state::McpState;
use crate::surface_call::{Found, Invocation, Surface};
use crate::surface_table::ResourceEntry;
use crate::target::{Call, TargetSpec};
use crate::tool::{ErrorCode, McpError};
use specforge_ops::export::Format;
use specforge_ops::view::ProjectView;

/// `resources/read`: the core resources (matched first), then the extension
/// resource whose template names the URI (ADR 0017 D9). The request
/// pipeline's resources adapter ([`crate::surface_call`]).
pub(crate) struct Resources;

impl Surface for Resources {
    const NAMED_BY: &'static str = "uri";
    const TAKES_ARGUMENTS: bool = false;
    const EXTENDED: bool = true;
    const UNKNOWN: &'static str = "Unknown resource URI";

    type Core = &'static ResourceSpec;
    type Extension = ResourceEntry;
    type Outcome = ReadOutcome;

    fn core(uri: &str) -> Option<&'static ResourceSpec> {
        CORE_RESOURCES.iter().find(|r| r.names(uri))
    }

    fn extension(state: &McpState, uri: &str) -> Option<ResourceEntry> {
        state.surfaces().resource(uri).cloned()
    }

    fn target(found: &Found<&'static ResourceSpec, ResourceEntry>) -> TargetSpec {
        match found {
            Found::Core(spec) => spec.target(),
            Found::Extension(_) => TargetSpec::SERVED_PROJECT,
        }
    }

    fn invoked(
        _: &Found<&'static ResourceSpec, ResourceEntry>,
        _: &Invocation,
    ) -> Option<McpEvent> {
        None
    }

    fn run(
        call: &mut Call<'_>,
        found: &Found<&'static ResourceSpec, ResourceEntry>,
        invocation: &Invocation,
    ) -> ReadOutcome {
        match found {
            Found::Core(spec) => (spec.read)(call.view(), &invocation.name),
            Found::Extension(entry) => {
                crate::extension_dispatch::read_resource(call, entry, &invocation.name)
            }
        }
    }

    fn refused(
        _: &Found<&'static ResourceSpec, ResourceEntry>,
        error: McpError,
        _: &mut dyn EventSink,
    ) -> ReadOutcome {
        Err(Box::new(error))
    }

    fn unknown(revision: Revision, uri: &str) -> JsonRpcError {
        unknown_resource(revision.resource_not_found_code(), uri)
    }

    fn refusal_mut(outcome: &mut ReadOutcome) -> Option<&mut McpError> {
        outcome.as_mut().err().map(|refusal| &mut **refusal)
    }

    fn completed(
        _: &Found<&'static ResourceSpec, ResourceEntry>,
        invocation: &Invocation,
        outcome: &ReadOutcome,
    ) -> Option<McpEvent> {
        // A read that returned content: its format is its MIME type.
        let text = outcome.as_ref().ok()?;
        Some(McpEvent::McpResourceRead {
            resource_uri: invocation.name.clone(),
            format: text.mime_type.clone(),
        })
    }

    fn envelope(
        revision: Revision,
        _: &Found<&'static ResourceSpec, ResourceEntry>,
        invocation: &Invocation,
        mut outcome: ReadOutcome,
        id: Option<Value>,
    ) -> JsonRpcResponse {
        // A refusal names the URI read.
        if let Err(refusal) = &mut outcome {
            refusal.uri.get_or_insert_with(|| invocation.name.clone());
        }
        // A read answers under the URI the client read: its own cache key.
        resource_envelope(
            outcome,
            &invocation.name,
            revision.resource_not_found_code(),
            id,
        )
    }
}

/// What a resource read returned: its MIME type and text. The URI is the one
/// the envelope writes.
pub(crate) struct ResourceText {
    pub mime_type: String,
    pub text: String,
}

impl ResourceText {
    pub fn json(text: impl Into<String>) -> Self {
        Self {
            mime_type: "application/json".into(),
            text: text.into(),
        }
    }
}

/// What a resource read produced, or why it was refused (ADR 0024 D4).
pub(crate) type ReadOutcome = Result<ResourceText, Box<McpError>>;

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
        Ok(text) => JsonRpcResponse::success(
            id,
            json!({
                "contents": [{
                    "uri": uri,
                    "mimeType": text.mime_type,
                    "text": text.text,
                }]
            }),
        ),
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

/// One core resource: everything the server lists and reads about it.
pub struct ResourceSpec {
    /// Its URI, or an RFC 6570 template (`specforge://graph/{entity_id}`).
    pub uri: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    pub mime_type: &'static str,
    /// Read the resource at a URI it [matches](Self::matches), over the
    /// project view it is given: the served project's, brought up to date,
    /// or the empty session's when nothing is served.
    pub(crate) read: fn(ProjectView<'_>, &str) -> ReadOutcome,
}

impl ResourceSpec {
    /// How it reaches its project: every core resource reads the served
    /// project's view ([`TargetSpec::SERVED_VIEW`]).
    pub fn target(&self) -> TargetSpec {
        TargetSpec::SERVED_VIEW
    }

    /// The resource as `resources/list` (or `resources/templates/list`)
    /// describes it.
    pub fn descriptor(&self) -> Listed {
        Listed::of(self.uri, self.name, Some(self.description), self.mime_type)
    }

    /// Whether `uri` names this resource, by the one rule extension
    /// resources follow too ([`crate::uri_template::names`]): what stands
    /// for a placeholder or follows as a query is the reader's to read or
    /// refuse ([`views::ViewQuery`]).
    pub fn names(&self, uri: &str) -> bool {
        crate::uri_template::names(self.uri, uri)
    }
}

/// The core resources, in listing order.
pub static CORE_RESOURCES: &[ResourceSpec] = &[
    ResourceSpec {
        uri: "specforge://graph",
        name: "graph",
        description: "Full spec graph in JSON format",
        mime_type: "application/json",
        read: |view, uri| views::export_view(view, uri, Format::Graph, None),
    },
    ResourceSpec {
        uri: "specforge://schema",
        name: "schema",
        description: "Graph schema definition",
        mime_type: "application/json",
        read: |view, uri| views::schema_view(view, uri),
    },
    ResourceSpec {
        uri: "specforge://context",
        name: "context",
        description: "Context-optimized graph (contract, status, verify fields)",
        mime_type: "application/json",
        read: |view, uri| views::export_view(view, uri, Format::Context, None),
    },
    ResourceSpec {
        uri: "specforge://context/{entity_id}",
        name: "context_entity",
        description: "Context-optimized subgraph rooted at an entity",
        mime_type: "application/json",
        read: |view, uri| {
            views::export_view(view, uri, Format::Context, Some("specforge://context/"))
        },
    },
    ResourceSpec {
        uri: "specforge://brief",
        name: "brief",
        description: "Brief graph (id, kind, title, edges only)",
        mime_type: "application/json",
        read: |view, uri| views::export_view(view, uri, Format::Brief, None),
    },
    ResourceSpec {
        uri: "specforge://diagnostics",
        name: "diagnostics",
        description: "Current compilation diagnostics",
        mime_type: "application/json",
        read: |view, uri| views::diagnostics_view(view, uri),
    },
    ResourceSpec {
        uri: "specforge://graph/{entity_id}",
        name: "entity",
        description: "Subgraph rooted at a specific entity",
        mime_type: "application/json",
        read: |view, uri| views::entity_view(view, uri),
    },
    ResourceSpec {
        uri: "specforge://entities/{kind}",
        name: "entities_by_kind",
        description: "All entities of a specific kind (e.g. feature, behavior)",
        mime_type: "application/json",
        read: |view, uri| views::entities_view(view, uri),
    },
];

/// Whether `uri` names a resource the server serves: a core one, or one an
/// extension contributes. The lookup `resources/read` makes
/// ([`crate::surface_call::find`]), so the freshness decision is its.
pub(crate) fn is_served(state: &mut McpState, uri: &str) -> bool {
    crate::surface_call::find::<Resources>(state, uri).is_some()
}
