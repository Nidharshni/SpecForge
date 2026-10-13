//! Graph export: the one function behind `specforge export`, the MCP
//! `specforge.export` tool, `specforge.render` and the `specforge://graph`,
//! `context` and `brief` resources with their scoped forms and
//! `specforge://graph/{entity_id}` (ADR 0004 D3-a, ADR 0024 D4).
//!
//! Every caller gets the same Graph Protocol document for the same request,
//! under the CLI's schema policy: a full `graph` export embeds the schema
//! (format 2.0), a scoped one references it (`schema_ref`), and `context`,
//! `brief` and any export under a token budget leave it out unless asked
//! for it. `dot` never carries a schema.

use crate::navigate;
use crate::options::{Choice, OptionTable};
use crate::schema_version::{VersionedSchema, negotiate_version};
use crate::view::ProjectView;
use crate::{OpError, OpErrorKind};
use specforge_common::{Diagnostic, codes};
use specforge_emitter::{
    EmitFormat, EmitOptions, EmitterError, GraphProtocolSchema, SchemaVersion, emit,
};
use std::path::PathBuf;

/// An export format.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Graph,
    Context,
    Brief,
    Dot,
}

impl Format {
    /// The emitter format this export renders with.
    pub fn emit_format(self) -> EmitFormat {
        match self {
            Self::Graph => EmitFormat::Json,
            Self::Context => EmitFormat::Context,
            Self::Brief => EmitFormat::Brief,
            Self::Dot => EmitFormat::Dot,
        }
    }
}

const GRAPH: Choice<Format> = Choice {
    name: "graph",
    aliases: &["json"],
    help: "every entity with its fields, file and line (Graph Protocol)",
    value: Format::Graph,
};
const CONTEXT: Choice<Format> = Choice {
    name: "context",
    aliases: &[],
    help: "headline and normative fields and verify, for an agent",
    value: Format::Context,
};
const BRIEF: Choice<Format> = Choice {
    name: "brief",
    aliases: &[],
    help: "id, kind and title",
    value: Format::Brief,
};
const DOT: Choice<Format> = Choice {
    name: "dot",
    aliases: &[],
    help: "Graphviz",
    value: Format::Dot,
};

/// `specforge export --format`, `specforge.render`'s `format`.
pub const FORMAT: OptionTable<Format> = OptionTable {
    argument: "format",
    choices: &[GRAPH, CONTEXT, BRIEF, DOT],
    default: Some(Format::Graph),
};

/// The formats an agent reads and a published schema describes:
/// `specforge.export`, `specforge.query`, `specforge schema --publish
/// --format`.
pub const AGENT_FORMAT: OptionTable<Format> = OptionTable {
    argument: "format",
    choices: &[GRAPH, CONTEXT, BRIEF],
    default: Some(Format::Graph),
};

/// Whether the export carries the schema.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Schema {
    /// The policy: embed for a full `graph` export (a scoped one references
    /// it), leave it out for `context`, `brief` and under a token budget.
    #[default]
    Default,
    /// Attach it to every format but `dot` (`--with-schema`). Under a
    /// budget it counts toward the budget, and a budget it doesn't fit in
    /// fails with E062.
    With,
    /// Never attach it (`--no-schema`): Graph Protocol 1.0.
    Without,
}

/// The code of the refusal of `depth` without a `scope`: depth limits a
/// scoped export.
pub const DEPTH_WITHOUT_SCOPE: &str = "depth_without_scope";

/// What to export.
#[derive(Debug, Clone, Default)]
pub struct Request<'a> {
    pub format: Option<Format>,
    /// Restrict to the subgraph reachable from this entity.
    pub scope: Option<&'a str>,
    /// With `scope`, how far to traverse; without one it is refused
    /// ([`DEPTH_WITHOUT_SCOPE`]).
    pub depth: Option<usize>,
    /// Keep only nodes of these kinds (the scoped root always stays).
    pub kinds: Vec<&'a str>,
    /// Token budget: keep the most central entities that fit.
    pub max_tokens: Option<usize>,
    pub schema: Schema,
    /// A schema version to negotiate (same major as the produced schema).
    pub schema_version: Option<&'a str>,
}

impl Request<'_> {
    fn format(&self) -> Format {
        self.format
            .or(FORMAT.default)
            .expect("the export format has a default")
    }

    /// Whether the export carries the schema, under the policy.
    pub fn attaches_schema(&self) -> bool {
        let format = self.format();
        if format == Format::Dot {
            return false;
        }
        match self.schema {
            Schema::Without => false,
            Schema::With => true,
            Schema::Default => format == Format::Graph && self.max_tokens.is_none(),
        }
    }
}

/// The export text of the view's project for `request`. A schema it
/// carries is the view's versioned schema (`specforge export`'s version,
/// computed against the root's schema cache, which this only reads).
pub fn export(view: &ProjectView, request: &Request) -> Result<String, OpError> {
    export_versioned(view, &view.versioned_schema(), request)
}

fn export_versioned(
    view: &ProjectView,
    versioned: &VersionedSchema,
    request: &Request,
) -> Result<String, OpError> {
    if request.depth.is_some() && request.scope.is_none() {
        return Err(OpError::new(
            OpErrorKind::InvalidInput,
            DEPTH_WITHOUT_SCOPE,
            "depth limits a scoped export: give a scope too",
        ));
    }
    let schema = if request.attaches_schema() {
        Some(negotiated(
            versioned.schema().clone(),
            request.schema_version,
        )?)
    } else {
        None
    };
    let options = EmitOptions {
        format: request.format().emit_format(),
        scope: request.scope,
        depth: request.depth,
        kind_filter: request.kinds.clone(),
        schema: schema.as_ref(),
        token_budget: request.max_tokens,
        kind_registry: Some(view.registries().kinds()),
        field_registry: Some(view.registries().fields()),
    };
    emit(view.graph(), &options).map_err(|error| failure(view, error))
}

/// What `specforge export` did: the export, what versioning reports against
/// the view root's record (W053, found before the export ran), and what
/// became of the record.
#[derive(Debug)]
pub struct RecordedExport {
    pub export: Result<String, OpError>,
    pub diagnostics: Vec<Diagnostic>,
    pub cache: CacheWrite,
}

/// What became of the schema cache after an export.
#[derive(Debug, PartialEq, Eq)]
pub enum CacheWrite {
    /// There is no root, or the export failed: the cache is left as it was.
    NotWritten,
    Written,
    /// The export succeeded; the cache could not be written.
    WriteFailed {
        dir: PathBuf,
        error: String,
    },
}

/// The export `specforge export` makes (ADR 0015 D10): compare the schema
/// the extensions produce with the cache at the view's root, export it
/// carrying the cached version bumped by what changed, and record it after
/// a successful export. MCP never calls it.
pub fn export_recorded(view: &ProjectView, request: &Request) -> RecordedExport {
    let versioned = view.versioned_schema();
    let diagnostics = versioned.diagnostics();
    let export = export_versioned(view, &versioned, request);
    let cache = match (&export, view.schema_cache()) {
        (Ok(_), Some(cache)) => match cache.record(&versioned) {
            Ok(()) => CacheWrite::Written,
            Err(error) => CacheWrite::WriteFailed {
                dir: cache.dir().to_path_buf(),
                error: error.to_string(),
            },
        },
        _ => CacheWrite::NotWritten,
    };
    RecordedExport {
        export,
        diagnostics,
        cache,
    }
}

/// The emitter's failure as the operation's: the kind by variant, the code
/// the variant's ([`EmitterError::code`]); nothing is read from the message.
/// A missing scope entity is the one not-found refusal.
fn failure(view: &ProjectView, error: EmitterError) -> OpError {
    let kind = match &error {
        EmitterError::ScopeNotFound { entity_id } => {
            return navigate::not_found(view.graph(), entity_id);
        }
        EmitterError::BudgetTooSmall { .. } => OpErrorKind::InvalidInput,
        EmitterError::Serialization(_) => OpErrorKind::Internal,
    };
    match error.code() {
        Some(code) => OpError::coded(kind, code, error.to_string()),
        None => OpError::new(kind, "export_failed", error.to_string()),
    }
}

/// `schema`, at `requested` when one is asked for: the same major as the
/// produced schema, minor and patch from 0 up to the produced version.
fn negotiated(
    mut schema: GraphProtocolSchema,
    requested: Option<&str>,
) -> Result<GraphProtocolSchema, OpError> {
    let Some(requested) = requested else {
        return Ok(schema);
    };
    let requested = requested.parse::<SchemaVersion>().map_err(|e| {
        OpError::new(
            OpErrorKind::SchemaMismatch,
            "invalid_schema_version",
            format!("invalid schema version: {e}"),
        )
    })?;
    let max = schema.schema_version.clone();
    let min = SchemaVersion::new(max.major, 0, 0);
    negotiate_version(&requested, &min, &max)
        .map_err(|e| OpError::coded(OpErrorKind::Conflict, codes::E027, e.reason))?;
    schema.schema_version = requested;
    Ok(schema)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(format: Format) -> Request<'static> {
        Request {
            format: Some(format),
            ..Request::default()
        }
    }

    #[test]
    fn format_names_accept_json_for_graph() {
        assert_eq!(FORMAT.parse("graph"), Ok(Format::Graph));
        assert_eq!(FORMAT.parse("json"), Ok(Format::Graph));
        assert_eq!(FORMAT.parse("dot"), Ok(Format::Dot));
        let error = FORMAT.parse("yaml").unwrap_err();
        assert_eq!(error.code, "invalid_input");
        assert_eq!(
            error.message,
            "Unknown format: yaml. Expected: graph, context, brief, dot"
        );
        assert_eq!(
            AGENT_FORMAT.parse("dot").unwrap_err().message,
            "Unknown format: dot. Expected: graph, context, brief"
        );
    }

    fn failed(request: &Request) -> OpError {
        let fixture = crate::view::testing::Fixture::new();
        export(&fixture.view(), request).unwrap_err()
    }

    #[specforge_test_macros::test(
        behavior = "export_agent_graph_format",
        verify = "depth without a scope is invalid input on every surface"
    )]
    fn a_depth_without_a_scope_is_invalid_input() {
        let error = failed(&Request {
            depth: Some(2),
            ..request(Format::Graph)
        });
        assert_eq!(error.kind, OpErrorKind::InvalidInput);
        assert_eq!(error.code, DEPTH_WITHOUT_SCOPE);

        // With a scope it is a hop limit, and the export goes on.
        let fixture = crate::view::testing::Fixture::new();
        let scoped = export(
            &fixture.view(),
            &Request {
                scope: Some("ghost"),
                depth: Some(2),
                ..request(Format::Graph)
            },
        )
        .unwrap_err();
        assert_eq!(
            scoped.code, "E003",
            "the scope is looked up, not the depth refused"
        );
    }

    #[test]
    fn export_failures_carry_their_kind() {
        let error = failed(&Request {
            scope: Some("ghost"),
            ..request(Format::Graph)
        });
        assert_eq!(error.kind, OpErrorKind::EntityNotFound);
        assert_eq!(error.code, "E003");
        assert_eq!(error.entity.as_deref(), Some("ghost"));
        assert!(!error.message.starts_with("E003"), "{}", error.message);
        assert!(error.message.contains("'ghost'"), "{}", error.message);

        let error = failed(&Request {
            max_tokens: Some(1),
            ..request(Format::Graph)
        });
        assert_eq!(error.kind, OpErrorKind::InvalidInput);
        assert_eq!(error.code, "E062");
        assert!(
            error.message.starts_with("the token budget"),
            "{}",
            error.message
        );
    }

    #[test]
    fn the_policy_embeds_only_for_an_unbudgeted_graph() {
        assert!(request(Format::Graph).attaches_schema());
        assert!(!request(Format::Context).attaches_schema());
        assert!(!request(Format::Brief).attaches_schema());
        assert!(!request(Format::Dot).attaches_schema());
        let budgeted = Request {
            max_tokens: Some(100),
            ..request(Format::Graph)
        };
        assert!(!budgeted.attaches_schema());
    }

    #[test]
    fn with_and_without_override_the_policy_but_not_for_dot() {
        let with = |format| Request {
            schema: Schema::With,
            max_tokens: Some(100),
            ..request(format)
        };
        assert!(with(Format::Graph).attaches_schema());
        assert!(with(Format::Brief).attaches_schema());
        assert!(!with(Format::Dot).attaches_schema());
        let without = Request {
            schema: Schema::Without,
            ..request(Format::Graph)
        };
        assert!(!without.attaches_schema());
    }
}
