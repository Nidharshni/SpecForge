pub mod brief;
pub mod context;
pub mod context_entity;
pub mod diagnostics;
pub mod entities;
pub mod entity;
pub mod graph;
pub mod schema;
mod view;

use serde_json::Value;

use crate::events::{EventSink, McpEvent};
use crate::lifecycle::Revision;
use crate::protocol::{JsonRpcError, JsonRpcResponse};
use crate::resource::{ReadOutcome, ResourceSpec, resource_envelope, unknown_resource};
use crate::state::McpState;
use crate::surface_call::{Found, Invocation, Surface};
use crate::surface_table::ResourceEntry;
use crate::target::{Call, TargetSpec};
use crate::tool::McpError;

/// `resources/read`: the core resources (matched first), then the extension
/// resource whose template names the URI (ADR 0017 D9). The request
/// pipeline's resources adapter ([`crate::surface_call`]).
pub(crate) struct Resources;

impl Surface for Resources {
    const NAMED_BY: &'static str = "uri";
    const TAKES_ARGUMENTS: bool = false;
    const EXTENDED: bool = true;
    const UNKNOWN: &'static str = "Unknown resource URI";

    type Core = &'static dyn ResourceSpec;
    type Extension = ResourceEntry;
    type Outcome = ReadOutcome;

    fn core(uri: &str) -> Option<&'static dyn ResourceSpec> {
        core_resource(uri)
    }

    fn extension(state: &McpState, uri: &str) -> Option<ResourceEntry> {
        state.surfaces().resource(uri).cloned()
    }

    fn target(found: &Found<&'static dyn ResourceSpec, ResourceEntry>) -> TargetSpec {
        match found {
            Found::Core(spec) => spec.target(),
            Found::Extension(_) => TargetSpec::SERVED_PROJECT,
        }
    }

    fn invoked(
        _: &Found<&'static dyn ResourceSpec, ResourceEntry>,
        _: &Invocation,
    ) -> Option<McpEvent> {
        None
    }

    fn run(
        call: &mut Call<'_>,
        found: &Found<&'static dyn ResourceSpec, ResourceEntry>,
        invocation: &Invocation,
    ) -> ReadOutcome {
        match found {
            Found::Core(spec) => spec.read(call.view(), &invocation.name),
            Found::Extension(entry) => {
                crate::extension_dispatch::read_resource(call, entry, &invocation.name)
            }
        }
    }

    fn refused(
        _: &Found<&'static dyn ResourceSpec, ResourceEntry>,
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
        _: &Found<&'static dyn ResourceSpec, ResourceEntry>,
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
        _: &Found<&'static dyn ResourceSpec, ResourceEntry>,
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

/// The core resources, in listing order.
pub static CORE_RESOURCES: &[&dyn ResourceSpec] = &[
    &graph::GraphResource,
    &schema::SchemaResource,
    &context::ContextResource,
    &context_entity::ContextEntityResource,
    &brief::BriefResource,
    &diagnostics::DiagnosticsResource,
    &entity::EntityResource,
    &entities::EntitiesResource,
];

/// The core resource that names `uri`, if any.
pub fn core_resource(uri: &str) -> Option<&'static dyn ResourceSpec> {
    CORE_RESOURCES.iter().copied().find(|r| r.names(uri))
}

/// Whether `uri` names a resource the server serves: a core one, or one an
/// extension contributes. The lookup `resources/read` makes
/// ([`crate::surface_call::find`]), so the freshness decision is its.
pub(crate) fn is_served(state: &mut McpState, uri: &str) -> bool {
    crate::surface_call::find::<Resources>(state, uri).is_some()
}
