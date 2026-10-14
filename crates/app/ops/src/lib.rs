//! User-level operations shared by the CLI, the MCP server and the LSP.
//!
//! Each surface used to orchestrate the backend crates on its own, so fixes
//! landed on one side only. An operation here takes a typed request and
//! returns a typed outcome or an [`OpError`]; the CLI and MCP only translate
//! arguments in and present the result.
//!
//! Operations never write to stdout: the MCP server owns it (it is the
//! JSON-RPC stream). The crate denies `clippy::print_stdout`.

pub mod analyze;
mod builtin_passes;
pub mod check;
pub mod collect;
pub mod command;
pub mod config;
pub mod coverage;
pub mod doctor;
pub mod explain;
pub mod explore;
pub mod export;
pub mod extension;
pub mod format;
pub mod infer;
pub mod init;
pub mod inspect;
pub mod migrate;
pub mod model;
pub mod navigate;
pub mod options;
pub mod plan;
mod prove;
pub mod publish;
pub mod query;
mod refusal;
pub mod registry;
pub mod rename;
mod report;
pub mod review;
mod scan;
pub mod schema;
pub mod schema_version;
pub mod solver;
pub mod stats;
#[cfg(feature = "testing")]
pub mod testing;
pub mod trace;
pub mod view;
mod writes;

pub use refusal::Refusal;
pub use writes::{Writes, resolves_outside};

use specforge_common::Code;
use specforge_diagnostics::Level;
use std::borrow::Cow;

/// What a run that judges the project concluded (CONTEXT "Run verdict",
/// ADR 0029 D3/D4): passed, failed, or it could not judge (a measuring
/// operation that lacks what it measures against). The CLI exits 0, 1 or
/// 2 by it; MCP returns `ok` as `verdict == Passed`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunVerdict {
    Passed,
    Failed,
    Unjudged,
}

impl RunVerdict {
    /// `Passed` when `ok`, else `Failed`.
    pub const fn of(ok: bool) -> Self {
        if ok { Self::Passed } else { Self::Failed }
    }
}

/// What kind of failure an operation reports: the closed set every surface
/// maps its own codes from (MCP's `ErrorCode`), decided where the failure is
/// raised (ADR 0024 D7). [`OpError::code`] stays what the CLI prints
/// (`error[E027]`, `error[invalid_input]`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpErrorKind {
    /// The request is wrong: an unknown name or format, a malformed
    /// specifier, a budget too small for the export.
    InvalidInput,
    /// An entity the request names is not in the graph.
    EntityNotFound,
    /// A file or directory the request names, or the project's config, is
    /// missing.
    FileNotFound,
    /// The extension the request names is not installed, built in, or
    /// published.
    ExtensionNotFound,
    /// The request contradicts what is there: a name taken, a project in
    /// place, an unsatisfiable peer, a version diamond.
    Conflict,
    /// A file the operation reads is not what it should hold: the config,
    /// the lock, a manifest, a test report, a signature's metadata.
    SchemaMismatch,
    /// The project the operation produced does not compile, or compiles to
    /// another graph: a migration whose migrated sources report errors or
    /// change the graph (W054).
    CompilationFailed,
    /// Something the operation needs is not set up: no project, no
    /// registry, no collector, no lock file.
    PreconditionFailed,
    /// The operation was not allowed: an unapproved command, a write the OS
    /// refused.
    PermissionDenied,
    /// A registry or a command did not answer in time.
    Timeout,
    /// The operation failed on its own side: a write that failed for
    /// another reason, an extension that trapped, a serialization.
    Internal,
}

impl OpErrorKind {
    /// The kind's name, the code an operation reports a failure of this kind
    /// under when it names none of its own (`invalid_input` for an unknown
    /// name): MCP's `ErrorCode` carries the same names.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::InvalidInput => "invalid_input",
            Self::EntityNotFound => "entity_not_found",
            Self::FileNotFound => "file_not_found",
            Self::ExtensionNotFound => "extension_not_found",
            Self::Conflict => "conflict",
            Self::SchemaMismatch => "schema_mismatch",
            Self::CompilationFailed => "compilation_failed",
            Self::PreconditionFailed => "precondition_failed",
            Self::PermissionDenied => "permission_denied",
            Self::Timeout => "timeout",
            Self::Internal => "internal_error",
        }
    }

    /// Whether a refusal of this kind rejects a value the caller gave: the
    /// request is wrong (`InvalidInput`), names an entity, file or extension
    /// that is not there (`EntityNotFound`, `FileNotFound`,
    /// `ExtensionNotFound`), or contradicts what is there (`Conflict`). Every
    /// other kind is about the project or the environment, and a refusal of
    /// it names no field ([`Refusal::about`]). One arm per kind, no wildcard.
    pub const fn rejects_a_value(self) -> bool {
        match self {
            Self::InvalidInput
            | Self::EntityNotFound
            | Self::FileNotFound
            | Self::ExtensionNotFound
            | Self::Conflict => true,
            Self::SchemaMismatch
            | Self::CompilationFailed
            | Self::PreconditionFailed
            | Self::PermissionDenied
            | Self::Timeout
            | Self::Internal => false,
        }
    }

    /// The kind of a failed file operation: `PermissionDenied` when the OS
    /// refused, `FileNotFound` for `NotFound`, else `Internal`.
    pub fn of_io(error: &std::io::Error) -> Self {
        Self::of_io_kind(error.kind())
    }

    /// [`Self::of_io`] for the kind of an error already taken apart.
    pub fn of_io_kind(kind: std::io::ErrorKind) -> Self {
        match kind {
            std::io::ErrorKind::PermissionDenied => Self::PermissionDenied,
            std::io::ErrorKind::NotFound => Self::FileNotFound,
            _ => Self::Internal,
        }
    }
}

/// Why an operation failed. `code` is data for the surface to present (a
/// CLI `error[CODE]`, an MCP error's `data.code`), never folded into
/// `message`; `kind` is what the failure is, which each surface maps to its
/// own codes.
#[derive(Debug, Clone, PartialEq)]
pub struct OpError {
    pub kind: OpErrorKind,
    pub code: Cow<'static, str>,
    pub message: String,
    pub suggestion: Option<String>,
    /// What else the failure says, as the operation's own JSON. Boxed, like
    /// `entity`, so the error stays small enough to be returned whole
    /// (`clippy.toml`'s `large-error-threshold`).
    pub data: Option<Box<serde_json::Value>>,
    /// The entity the failure is about (`EntityNotFound`, a rename's
    /// `Conflict`).
    pub entity: Option<Box<str>>,
    /// The files the operation changed on disk before it failed and did
    /// not restore (`extension::add` when the lock or config write fails
    /// after the module was placed): a surface reports them as written.
    /// An operation that rolls back what it wrote leaves this empty.
    pub writes: Writes,
}

impl OpError {
    /// A failure the operation names itself: `code` is its own identifier
    /// (`export_failed`, `unknown_pass`), not a catalogued diagnostic
    /// code. A catalogued code goes through [`OpError::coded`] or [`OpError::of`], so its constant carries it.
    pub fn new(
        kind: OpErrorKind,
        code: impl Into<Cow<'static, str>>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            kind,
            code: code.into(),
            message: message.into(),
            suggestion: None,
            data: None,
            entity: None,
            writes: Writes::none(),
        }
    }

    /// `no_project` ([`OpErrorKind::PreconditionFailed`]): the operation
    /// needs a project and `message` says where there is none.
    pub fn no_project(message: impl Into<String>) -> Self {
        Self::new(OpErrorKind::PreconditionFailed, "no_project", message)
    }

    /// A failure reported as the catalogued diagnostic `code`, of the `kind`
    /// its raiser states. It is printed as `error[CODE]`, so the code is an
    /// error's.
    pub fn coded(kind: OpErrorKind, code: Code, message: impl Into<String>) -> Self {
        debug_assert!(
            code.level() == Level::Error,
            "{code} is not an error code, but an OpError is printed as error[CODE]"
        );
        Self::new(kind, code.id(), message)
    }

    /// A failure reported as `diagnostic` (its code, message and suggestion), of the kind its
    /// raiser states: the one way a diagnostic a lower crate made becomes an operation's failure.
    pub fn of(kind: OpErrorKind, diagnostic: specforge_common::Diagnostic) -> Self {
        let mut error = Self::new(kind, Cow::Owned(diagnostic.code), diagnostic.message);
        error.suggestion = diagnostic.suggestion;
        error
    }

    /// Whether this failure is reported as diagnostic `code`.
    pub fn is(&self, code: Code) -> bool {
        code.matches(&self.code)
    }

    /// The same error, having left `writes` changed on disk.
    pub fn with_writes(mut self, writes: Writes) -> Self {
        self.writes = writes;
        self
    }

    /// This failure with `prefix` ahead of its message; its code, kind and suggestion are kept.
    pub fn prefixed(mut self, prefix: impl AsRef<str>) -> Self {
        self.message = format!("{}{}", prefix.as_ref(), self.message);
        self
    }

    pub fn with_suggestion(mut self, suggestion: impl Into<String>) -> Self {
        self.suggestion = Some(suggestion.into());
        self
    }

    /// The same error, about `entity`.
    pub fn with_entity(mut self, entity: impl Into<String>) -> Self {
        self.entity = Some(entity.into().into_boxed_str());
        self
    }

    /// The same error, carrying `data`.
    pub fn with_data(mut self, data: serde_json::Value) -> Self {
        self.data = Some(Box::new(data));
        self
    }

    /// This refusal about `field` ([`Refusal::about`]).
    pub fn about<F>(self, field: F) -> Refusal<F> {
        Refusal::about(field, self)
    }
}

impl AsRef<OpError> for OpError {
    fn as_ref(&self) -> &OpError {
        self
    }
}

impl std::fmt::Display for OpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for OpError {}

/// A path that names no project to open: `file_not_found` ("path not found: <path>"),
/// the code and message MCP answers for a call's `path`.
impl From<specforge_common::NoSuchPath> for OpError {
    fn from(error: specforge_common::NoSuchPath) -> Self {
        let kind = OpErrorKind::of_io_kind(error.kind);
        Self::new(kind, kind.as_str(), error.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use specforge_common::codes;

    #[test]
    fn an_operation_states_the_kind_of_a_code_it_raises() {
        for kind in [
            OpErrorKind::InvalidInput,
            OpErrorKind::EntityNotFound,
            OpErrorKind::FileNotFound,
            OpErrorKind::ExtensionNotFound,
            OpErrorKind::Conflict,
            OpErrorKind::SchemaMismatch,
            OpErrorKind::CompilationFailed,
            OpErrorKind::PreconditionFailed,
            OpErrorKind::PermissionDenied,
            OpErrorKind::Timeout,
            OpErrorKind::Internal,
        ] {
            let error = OpError::coded(kind, codes::E027, "x");
            assert_eq!(error.kind, kind);
            assert_eq!(error.code, "E027");
        }
    }

    #[test]
    fn an_error_stays_small_enough_to_return_whole() {
        // clippy.toml's `large-error-threshold` is 153 bytes.
        assert!(std::mem::size_of::<OpError>() <= 129);
    }

    #[test]
    fn failure_kinds_of_io_errors() {
        use std::io::{Error, ErrorKind};
        assert_eq!(
            OpErrorKind::of_io(&Error::from(ErrorKind::PermissionDenied)),
            OpErrorKind::PermissionDenied
        );
        assert_eq!(
            OpErrorKind::of_io(&Error::from(ErrorKind::NotFound)),
            OpErrorKind::FileNotFound
        );
        assert_eq!(
            OpErrorKind::of_io(&Error::from(ErrorKind::NotADirectory)),
            OpErrorKind::Internal
        );
    }

    #[test]
    fn an_error_carries_the_writes_it_left() {
        let error = OpError::coded(
            crate::OpErrorKind::Internal,
            codes::E032,
            "failed to write specforge.lock",
        );
        assert!(error.writes.is_empty());
        let module = "/p/.specforge/extensions/@sdk/greet/extension.wasm";
        let error = error.with_writes(Writes::from_iter([module]));
        let left: Vec<&std::path::Path> = error.writes.paths().collect();
        assert_eq!(left, [std::path::Path::new(module)]);
        // From a diagnostic: nothing written.
        let from = OpError::of(
            OpErrorKind::Internal,
            specforge_common::Diagnostic::new(codes::E032, "x"),
        );
        assert!(from.writes.is_empty());
    }
}
