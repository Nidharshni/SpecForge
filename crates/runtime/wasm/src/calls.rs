//! Every operation the host performs on a loaded extension, typed (ADR
//! 0013).
//!
//! The host calls ten kinds of export: the handshake and describe that read
//! an extension's declaration, a command, an MCP tool, an MCP resource, a
//! compiler pass, a collector, a custom validator, a scanner and the
//! migration hook. [`ExtensionCalls`] has one function per operation: its
//! input and its answer are the protocol's types
//! (`specforge_protocol_types`, the ones the SDK shares), and every failure
//! is one [`CallError`], E028. Callers never see bytes, export prefixes or
//! traps: encoding, strict decoding, the export a pass is called by, the
//! canonical order of a pass's diagnostics and the mapping of every failure
//! live here, over the byte-level [`WasmRuntime`] port.
//!
//! What a failure means stays the caller's: a check pass's is a compile
//! diagnostic, an analyze pass's a finding of that pass, a command's its
//! error, a scanner's a reported failure, a collector's the collect error.

use std::fmt;
use std::marker::PhantomData;

use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;
use specforge_common::{Diagnostic, DiagnosticData, Severity, SourceSpan, Sym, codes};
use specforge_protocol_types::{
    CollectInput, CollectOutput, CommandInput, CommandOutput, DeclaredCategory, DescribeRequest,
    DescribeResponse, HandshakeRequest, HandshakeResponse, McpResourceContent, McpResourceRequest,
    MigrationInput, PROTOCOL_VERSION, PassAnswer, PassDiagnostic, PassInput, PassOutput,
    PassSeverity, PassSpan, RawGraph, ScanRequest, ScanResponse, ValidatorContext,
    ValidatorVerdict, pass_export,
};

use crate::runtime::{WasmCallResult, WasmRuntime};
use crate::sandbox::Sandbox;

/// The operation a call performs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Operation {
    Handshake,
    Describe,
    Command,
    McpTool,
    McpResource,
    Pass,
    Collect,
    Validate,
    Scan,
    Migrate,
}

impl Operation {
    /// Every operation.
    pub const ALL: [Operation; 10] = [
        Operation::Handshake,
        Operation::Describe,
        Operation::Command,
        Operation::McpTool,
        Operation::McpResource,
        Operation::Pass,
        Operation::Collect,
        Operation::Validate,
        Operation::Scan,
        Operation::Migrate,
    ];

    /// How a message names the operation.
    pub fn label(self) -> &'static str {
        match self {
            Operation::Handshake => "handshake",
            Operation::Describe => "describe",
            Operation::Command => "command",
            Operation::McpTool => "MCP tool",
            Operation::McpResource => "MCP resource",
            Operation::Pass => "compiler pass",
            Operation::Collect => "collector",
            Operation::Validate => "custom validator",
            Operation::Scan => "scanner",
            Operation::Migrate => "migration hook",
        }
    }
}

impl fmt::Display for Operation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.label())
    }
}

/// Why a call failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CallFailure {
    /// The runtime has no extension of that name (it did not load it).
    NotLoaded,
    /// The export did not answer: it trapped, the guest returned an error
    /// (`guest_error`, which an export the guest does not route is too), or
    /// it crossed a limit of its sandbox: its time (`deadline_exceeded`), its
    /// fuel (`fuel_exhausted`) or its memory (`memory_limit_exceeded`).
    Trapped { kind: String, message: String },
    /// The export answered, but not the protocol type it owes.
    Malformed {
        expected: &'static str,
        reason: String,
    },
    /// The host's input did not encode (a host bug; never sent).
    Unencodable { reason: String },
}

/// A failed extension call: E028 naming the operation, the export and the
/// extension.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallError {
    pub operation: Operation,
    pub extension: String,
    pub export: String,
    pub failure: CallFailure,
}

impl CallError {
    pub fn new(
        operation: Operation,
        extension: &str,
        export: &str,
        failure: CallFailure,
    ) -> CallError {
        CallError {
            operation,
            extension: extension.to_string(),
            export: export.to_string(),
            failure,
        }
    }

    /// The E028 diagnostic: `<operation> <export>() of '<extension>'`, then
    /// what went wrong, with the suggestion to report it to the extension's
    /// author.
    pub fn diagnostic(&self) -> Diagnostic {
        Diagnostic::new(codes::E028, self.to_string()).with_suggestion(format!(
            "report the failure to the author of '{}', or check it is installed and up to date",
            self.extension
        ))
    }
}

impl fmt::Display for CallError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} {}() of '{}' ",
            self.operation, self.export, self.extension
        )?;
        match &self.failure {
            CallFailure::NotLoaded => f.write_str("is not loaded"),
            CallFailure::Trapped { kind, message } => write!(f, "trapped: {kind}: {message}"),
            CallFailure::Malformed { expected, reason } => {
                write!(f, "answered output that is not a {expected}: {reason}")
            }
            CallFailure::Unencodable { reason } => {
                write!(f, "was not called: its input does not encode: {reason}")
            }
        }
    }
}

impl std::error::Error for CallError {}

/// An input encoded once, to send to many exports (one pass input, every
/// pass), or why it did not encode (a host bug): a call given it then fails
/// with [`CallFailure::Unencodable`] naming that call's export, and nothing
/// is sent.
#[derive(Debug, Clone)]
pub struct Encoded<T> {
    bytes: Result<Vec<u8>, CallFailure>,
    _type: PhantomData<fn(&T)>,
}

impl<T> Encoded<T> {
    /// Why the input did not encode, if it did not.
    pub fn failure(&self) -> Option<&CallFailure> {
        self.bytes.as_ref().err()
    }
}

/// An extension's handshake, and the sandbox its `sandbox_policy` reads as
/// (the limits the loader applies, and what the host does not honour).
#[derive(Debug, Clone)]
pub struct Handshake {
    pub response: HandshakeResponse,
    pub sandbox: Sandbox,
}

/// The operations on loaded extensions, over a runtime; see the module
/// docs.
#[derive(Clone, Copy)]
pub struct ExtensionCalls<'r> {
    runtime: &'r dyn WasmRuntime,
}

impl<'r> ExtensionCalls<'r> {
    pub fn new(runtime: &'r dyn WasmRuntime) -> Self {
        ExtensionCalls { runtime }
    }

    /// `value` encoded once, for [`Self::run_pass`]. Never fails: a value
    /// that does not encode is kept as its failure, for each call to report.
    pub fn encode<T: Serialize>(value: &T) -> Encoded<T> {
        Encoded {
            bytes: serde_json::to_vec(value).map_err(|e| CallFailure::Unencodable {
                reason: e.to_string(),
            }),
            _type: PhantomData,
        }
    }

    /// `__handshake`: the extension's identity and what it declares, with the
    /// sandbox reading its `sandbox_policy` gives (its warnings name the
    /// extension as its handshake does). Reading it applies
    /// nothing; the loader applies the limits
    /// ([`crate::protocol::load_declaration`]).
    pub fn handshake(&self, extension: &str) -> Result<Handshake, CallError> {
        let request = HandshakeRequest {
            host_version: PROTOCOL_VERSION.to_string(),
        };
        let export = "__handshake";
        let operation = Operation::Handshake;
        let bytes = encode(operation, extension, export, &request)?;
        let answer = self.call_raw(operation, extension, export, &bytes)?;
        let response: HandshakeResponse =
            decode(operation, extension, export, &answer, "HandshakeResponse")?;
        // The same bytes read as a value cannot fail once the typed decode
        // succeeded: the sandbox reads the policy as sent.
        let wire: Value = serde_json::from_slice(&answer).expect("a decoded handshake is JSON");
        let sandbox = Sandbox::of(
            &response.name,
            wire.get("sandbox_policy")
                .filter(|policy| !policy.is_null()),
        );
        Ok(Handshake { response, sandbox })
    }

    /// `__describe` of `category`.
    pub fn describe(
        &self,
        extension: &str,
        category: DeclaredCategory,
    ) -> Result<DescribeResponse, CallError> {
        let request = DescribeRequest {
            category: category.name().to_string(),
        };
        self.call_typed(
            Operation::Describe,
            extension,
            "__describe",
            &request,
            "DescribeResponse",
        )
    }

    /// A command's `cmd__` export on `input`, whose graph the host already
    /// rendered.
    pub fn run_command(
        &self,
        extension: &str,
        export: &str,
        input: &CommandInput<RawGraph>,
    ) -> Result<CommandOutput, CallError> {
        self.call_typed(
            Operation::Command,
            extension,
            export,
            input,
            "CommandOutput",
        )
    }

    /// An MCP tool's export on `arguments`: any JSON value is its answer
    /// (the tool's output schema is the caller's to check).
    pub fn call_mcp_tool(
        &self,
        extension: &str,
        export: &str,
        arguments: &Value,
    ) -> Result<Value, CallError> {
        self.call_typed(
            Operation::McpTool,
            extension,
            export,
            arguments,
            "JSON value",
        )
    }

    /// An MCP resource's export, reading `uri`.
    pub fn read_mcp_resource(
        &self,
        extension: &str,
        export: &str,
        uri: &str,
    ) -> Result<McpResourceContent, CallError> {
        let request = McpResourceRequest {
            uri: uri.to_string(),
        };
        self.call_typed(
            Operation::McpResource,
            extension,
            export,
            &request,
            "McpResourceContent",
        )
    }

    /// The pass `pass`'s export ([`pass_export`]) on `input`: its
    /// diagnostics, with its summary (empty for a bare answer). An input
    /// that did not encode is this call's `Unencodable` failure.
    pub fn run_pass(
        &self,
        extension: &str,
        pass: &str,
        input: &Encoded<PassInput>,
    ) -> Result<PassOutput, CallError> {
        let export = pass_export(pass);
        let bytes = input.bytes.as_ref().map_err(|failure| {
            CallError::new(Operation::Pass, extension, &export, failure.clone())
        })?;
        let answer: PassAnswer =
            self.call(Operation::Pass, extension, &export, bytes, "PassAnswer")?;
        Ok(answer.into_output())
    }

    /// A collector's `collect__` export on the report files.
    pub fn collect(
        &self,
        extension: &str,
        export: &str,
        input: &CollectInput,
    ) -> Result<CollectOutput, CallError> {
        self.call_typed(
            Operation::Collect,
            extension,
            export,
            input,
            "CollectOutput",
        )
    }

    /// A custom rule's `function` on one entity's context.
    pub fn validate(
        &self,
        extension: &str,
        function: &str,
        context: &ValidatorContext,
    ) -> Result<ValidatorVerdict, CallError> {
        self.call_typed(
            Operation::Validate,
            extension,
            function,
            context,
            "ValidatorVerdict",
        )
    }

    /// An analyzer's scan export on one source file.
    pub fn scan(
        &self,
        extension: &str,
        export: &str,
        request: &ScanRequest,
    ) -> Result<ScanResponse, CallError> {
        self.call_typed(Operation::Scan, extension, export, request, "ScanResponse")
    }

    /// The migration hook `hook`. Its answer is not read: the protocol
    /// defines none.
    pub fn migrate(
        &self,
        extension: &str,
        hook: &str,
        input: &MigrationInput,
    ) -> Result<(), CallError> {
        let bytes = encode(Operation::Migrate, extension, hook, input)?;
        self.call_raw(Operation::Migrate, extension, hook, &bytes)
            .map(|_| ())
    }

    fn call_typed<I: Serialize, O: DeserializeOwned>(
        &self,
        operation: Operation,
        extension: &str,
        export: &str,
        input: &I,
        expected: &'static str,
    ) -> Result<O, CallError> {
        let bytes = encode(operation, extension, export, input)?;
        self.call(operation, extension, export, &bytes, expected)
    }

    /// The one place bytes cross: call the export, map a trap, decode the
    /// answer as `O`.
    fn call<O: DeserializeOwned>(
        &self,
        operation: Operation,
        extension: &str,
        export: &str,
        bytes: &[u8],
        expected: &'static str,
    ) -> Result<O, CallError> {
        let answer = self.call_raw(operation, extension, export, bytes)?;
        decode(operation, extension, export, &answer, expected)
    }

    fn call_raw(
        &self,
        operation: Operation,
        extension: &str,
        export: &str,
        bytes: &[u8],
    ) -> Result<Vec<u8>, CallError> {
        match self.runtime.call_export(extension, export, bytes) {
            WasmCallResult::Ok(answer) => Ok(answer),
            WasmCallResult::Trap(trap) => {
                let failure = if trap.kind == "extension_not_found" {
                    CallFailure::NotLoaded
                } else {
                    CallFailure::Trapped {
                        kind: trap.kind,
                        message: trap.message,
                    }
                };
                Err(CallError::new(operation, extension, export, failure))
            }
        }
    }
}

/// Decode an export's `answer` as the protocol type `expected` names.
fn decode<O: DeserializeOwned>(
    operation: Operation,
    extension: &str,
    export: &str,
    answer: &[u8],
    expected: &'static str,
) -> Result<O, CallError> {
    serde_json::from_slice(answer).map_err(|e| {
        CallError::new(
            operation,
            extension,
            export,
            CallFailure::Malformed {
                expected,
                reason: e.to_string(),
            },
        )
    })
}

fn encode<T: Serialize>(
    operation: Operation,
    extension: &str,
    export: &str,
    input: &T,
) -> Result<Vec<u8>, CallError> {
    ExtensionCalls::encode(input)
        .bytes
        .map_err(|failure| CallError::new(operation, extension, export, failure))
}

/// The host diagnostics of `extension`'s pass answer, in canonical order
/// (code, then file and line, then message), whatever order the guest built
/// them in. Each names `extension` as its origin
/// ([`Diagnostic::from_extension`]); one without a span that names an entity
/// gets that entity's (`span_of`), and one that names an entity carries it
/// as `DiagnosticData::Subject`.
pub fn pass_diagnostics(
    extension: &str,
    output: PassOutput,
    span_of: impl Fn(&str) -> Option<SourceSpan>,
) -> Vec<Diagnostic> {
    let mut diagnostics: Vec<Diagnostic> = output
        .diagnostics
        .into_iter()
        .map(|diagnostic| {
            let PassDiagnostic {
                code,
                severity,
                message,
                span,
                suggestion,
                entity,
            } = diagnostic;
            let span = span
                .map(source_span)
                .or_else(|| entity.as_deref().and_then(&span_of));
            let severity = match severity {
                PassSeverity::Error => Severity::Error,
                PassSeverity::Warning => Severity::Warning,
                PassSeverity::Info => Severity::Info,
            };
            let mut reported = Diagnostic::from_extension(extension, code, severity, message);
            reported.span = span;
            reported.suggestion = suggestion;
            // The entity the guest names is the diagnostic's subject,
            // even one the graph lacks: navigation reads data, never the
            // message (ADR 0016). A guest's own `data` is not carried.
            reported.data = entity.map(|entity| Box::new(DiagnosticData::Subject { entity }));
            reported
        })
        .collect();
    diagnostics.sort_by(|a, b| {
        a.code
            .cmp(&b.code)
            .then_with(|| {
                a.span
                    .as_ref()
                    .map(|s| (s.file.as_str(), s.start_line))
                    .cmp(&b.span.as_ref().map(|s| (s.file.as_str(), s.start_line)))
            })
            .then_with(|| a.message.cmp(&b.message))
    });
    diagnostics
}

fn source_span(span: PassSpan) -> SourceSpan {
    SourceSpan {
        file: Sym::new(&span.file),
        start_line: span.start_line,
        start_col: span.start_col,
        end_line: span.end_line,
        end_col: span.end_col,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::InProcessRuntime;

    #[specforge_test_macros::test(
        behavior = "call_extension_exports",
        verify = "a pass input that does not encode fails each pass's call, naming its export, and nothing is sent"
    )]
    fn a_pass_input_that_does_not_encode_fails_each_pass_without_a_call() {
        let runtime = InProcessRuntime::new();
        let calls = ExtensionCalls::new(&runtime);
        let input = Encoded::<PassInput> {
            bytes: Err(CallFailure::Unencodable {
                reason: "no".to_string(),
            }),
            _type: PhantomData,
        };
        for (pass, export) in [("a", "__pass_a"), ("b", "__pass_b")] {
            let error = calls.run_pass("@t/x", pass, &input).unwrap_err();
            assert_eq!(error.export, export);
            assert!(
                error
                    .to_string()
                    .ends_with("was not called: its input does not encode: no"),
                "{error}"
            );
        }
        assert!(runtime.calls().is_empty(), "nothing is sent");
    }
}
