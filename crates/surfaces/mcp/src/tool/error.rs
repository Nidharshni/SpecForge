use crate::protocol::{JsonRpcError, error_codes};
use serde::Serialize;
use serde_json::{Value, json};
use specforge_common::shape::Shape;
use specforge_common::{Diagnostic, DiagnosticDocument, Severity};
use specforge_graph::Graph;
use specforge_ops::{OpError, OpErrorKind};

/// The spec's `McpErrorCode`: the small closed set agents branch on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Shape)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    InvalidInput,
    CompilationFailed,
    EntityNotFound,
    FileNotFound,
    ExtensionNotFound,
    PermissionDenied,
    Timeout,
    NotInitialized,
    SchemaMismatch,
    InternalError,
    Conflict,
    PreconditionFailed,
}

impl ErrorCode {
    pub fn as_str(self) -> &'static str {
        match self {
            ErrorCode::InvalidInput => "invalid_input",
            ErrorCode::CompilationFailed => "compilation_failed",
            ErrorCode::EntityNotFound => "entity_not_found",
            ErrorCode::FileNotFound => "file_not_found",
            ErrorCode::ExtensionNotFound => "extension_not_found",
            ErrorCode::PermissionDenied => "permission_denied",
            ErrorCode::Timeout => "timeout",
            ErrorCode::NotInitialized => "not_initialized",
            ErrorCode::SchemaMismatch => "schema_mismatch",
            ErrorCode::InternalError => "internal_error",
            ErrorCode::Conflict => "conflict",
            ErrorCode::PreconditionFailed => "precondition_failed",
        }
    }

    /// The JSON-RPC error code a refusal is sent with where there is no
    /// `isError` result (`prompts/get`): invalid params for input the
    /// client can fix, invalid request before `initialize`, internal error
    /// for a failure on the server's side.
    pub fn rpc_code(self) -> i64 {
        match self {
            ErrorCode::InvalidInput
            | ErrorCode::EntityNotFound
            | ErrorCode::ExtensionNotFound
            | ErrorCode::Conflict => error_codes::INVALID_PARAMS,
            ErrorCode::NotInitialized => error_codes::INVALID_REQUEST,
            ErrorCode::CompilationFailed
            | ErrorCode::FileNotFound
            | ErrorCode::PermissionDenied
            | ErrorCode::Timeout
            | ErrorCode::SchemaMismatch
            | ErrorCode::InternalError
            | ErrorCode::PreconditionFailed => error_codes::INTERNAL_ERROR,
        }
    }
}

/// The code an operation's failure kind is reported as: total, one arm per
/// kind (ADR 0024 D7).
impl From<OpErrorKind> for ErrorCode {
    fn from(kind: OpErrorKind) -> Self {
        match kind {
            OpErrorKind::InvalidInput => ErrorCode::InvalidInput,
            OpErrorKind::EntityNotFound => ErrorCode::EntityNotFound,
            OpErrorKind::FileNotFound => ErrorCode::FileNotFound,
            OpErrorKind::ExtensionNotFound => ErrorCode::ExtensionNotFound,
            OpErrorKind::Conflict => ErrorCode::Conflict,
            OpErrorKind::SchemaMismatch => ErrorCode::SchemaMismatch,
            OpErrorKind::CompilationFailed => ErrorCode::CompilationFailed,
            OpErrorKind::PreconditionFailed => ErrorCode::PreconditionFailed,
            OpErrorKind::PermissionDenied => ErrorCode::PermissionDenied,
            OpErrorKind::Timeout => ErrorCode::Timeout,
            OpErrorKind::Internal => ErrorCode::InternalError,
        }
    }
}

/// A failed call or read: the spec's `McpError`. It is the text (and, for an
/// untyped tool, the structuredContent) of an `isError` tool result, and the
/// `data` of a prompt's or a resource read's JSON-RPC error; serialized from
/// this one definition everywhere. The diagnostic code, when there is one,
/// is `diagnostic.code`, never only message text.
#[derive(Debug, Clone, Serialize, Shape)]
pub struct McpError {
    pub code: ErrorCode,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool: Option<String>,
    /// The prompt that refused: a `prompts/get` answered with an error.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt: Option<String>,
    /// The URI of the resource whose read failed: a `resources/read`
    /// answered with an error (the URI read).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uri: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entity_id: Option<String>,
    /// The spec file the failure is about, as the request spelled it
    /// (`file_not_found`, `not_a_source`): what the no-project refusal names,
    /// rather than reading it back out of the message.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub argument: Option<String>,
    /// The diagnostic behind the failure, as every surface presents one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub diagnostic: Option<DiagnosticDocument>,
    /// More about the failure: the open value `McpError` reserves.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
    /// Other diagnostics the call reported on its way to failing; they
    /// ride in `_meta.diagnostics`, as a successful call's do.
    #[serde(skip)]
    pub reported: Vec<Diagnostic>,
}

impl McpError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            tool: None,
            prompt: None,
            uri: None,
            entity_id: None,
            file: None,
            argument: None,
            diagnostic: None,
            data: None,
            reported: Vec::new(),
        }
    }

    /// Invalid input: an argument the tool cannot use.
    pub fn invalid_input(argument: &str, message: impl Into<String>) -> Self {
        McpError::new(ErrorCode::InvalidInput, message).with_argument(argument)
    }

    pub fn with_entity(mut self, entity_id: impl Into<String>) -> Self {
        self.entity_id = Some(entity_id.into());
        self
    }

    pub fn with_file(mut self, file: impl Into<String>) -> Self {
        self.file = Some(file.into());
        self
    }

    pub fn with_argument(mut self, argument: impl Into<String>) -> Self {
        self.argument = Some(argument.into());
        self
    }

    pub fn with_diagnostic(mut self, diagnostic: &Diagnostic) -> Self {
        self.diagnostic = Some(DiagnosticDocument(diagnostic.clone()));
        self
    }

    pub fn with_data(mut self, data: Value) -> Self {
        self.data = Some(data);
        self
    }

    /// The same error with `value` under `key` in its `data` object
    /// (created when it has none; a `data` that is no object is left as
    /// it is).
    pub fn with_data_field(mut self, key: &str, value: Value) -> Self {
        self.set_data_field(key, value);
        self
    }

    pub(crate) fn set_data_field(&mut self, key: &str, value: Value) {
        if let Value::Object(data) = self.data.get_or_insert_with(|| json!({})) {
            data.insert(key.to_string(), value);
        }
    }

    /// This refusal as the JSON-RPC error of a request that has no `isError`
    /// result (`prompts/get`, `resources/read`): -32602 when the client can
    /// fix it (it names an argument the client sent, or
    /// [`ErrorCode::rpc_code`] marks its code as input), else -32603; its
    /// data is this McpError (ADR 0004 D4-d, ADR 0024 D5).
    pub fn into_rpc_error(self) -> JsonRpcError {
        let code = if self.argument.is_some() {
            error_codes::INVALID_PARAMS
        } else {
            self.code.rpc_code()
        };
        let data = serde_json::to_value(&self).expect("an McpError serializes");
        JsonRpcError::new(code, self.message).with_data(data)
    }
}

/// An operation's failure as an `McpError`: its kind picks the code. A
/// diagnostic code (`E027`) rides in `diagnostic`, with its suggestion; a
/// slug's suggestion and the operation's own data ride in `data`; the
/// entity the failure is about is `entity_id`, the spec file it is about is `file`.
impl From<OpError> for McpError {
    fn from(error: OpError) -> Self {
        let entity = error.entity().map(str::to_owned);
        let file = error.file().map(str::to_owned);
        let mut mcp_error = McpError::new(error.kind.into(), error.message.clone());
        let mut data = error.data.map_or_else(|| json!({}), |data| *data);
        if is_diagnostic_code(&error.code) {
            let mut diagnostic =
                Diagnostic::untyped(error.code.as_ref(), Severity::Error, error.message);
            if let Some(suggestion) = error.suggestion {
                diagnostic = diagnostic.with_suggestion(suggestion);
            }
            mcp_error = mcp_error.with_diagnostic(&diagnostic);
        } else if let Some(suggestion) = error.suggestion {
            data["suggestion"] = Value::from(suggestion);
        }
        if data.as_object().is_some_and(|d| !d.is_empty()) {
            mcp_error = mcp_error.with_data(data);
        }
        if let Some(file) = file {
            mcp_error = mcp_error.with_file(file);
        }
        match entity {
            Some(entity) => mcp_error.with_entity(entity),
            None => mcp_error,
        }
    }
}

/// A question about `entity_id`, which no entity of `graph` declares:
/// `specforge_ops::view::not_found` (E003 in `diagnostic`, a
/// did-you-mean when an id is close), the one refusal tools and prompts
/// share with every operation.
pub fn entity_not_found(graph: &Graph, entity_id: &str) -> McpError {
    specforge_ops::view::not_found(graph, entity_id).into()
}

/// Whether `code` is a diagnostic code (`E003`, `R004`, `R-RES-006`):
/// capitals, digits and dashes, not a slug such as `extension_not_found`.
pub fn is_diagnostic_code(code: &str) -> bool {
    code.starts_with(|c: char| c.is_ascii_uppercase())
        && code.contains(|c: char| c.is_ascii_digit())
        && code
            .chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '-')
}
