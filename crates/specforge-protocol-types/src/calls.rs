//! The operational payloads: what the host sends each export it calls on a
//! loaded extension, and what the export answers (ADR 0013).
//!
//! | Export                        | Input                  | Answer                       |
//! |-------------------------------|------------------------|------------------------------|
//! | `cmd__<id>` (command)         | [`CommandInput`]       | [`CommandOutput`]            |
//! | `mcp__<name>` (MCP tool)      | its input schema's JSON | its output schema's JSON    |
//! | `mcp__<name>` (MCP resource)  | [`McpResourceRequest`] | [`McpResourceContent`]       |
//! | `__pass_<name>` (pass)        | [`PassInput`]          | [`PassAnswer`]               |
//! | `collect__<name>` (collector) | [`CollectInput`]       | [`CollectOutput`]            |
//! | a rule's `wasm_function`      | [`crate::ValidatorContext`] | [`crate::ValidatorVerdict`] |
//! | an analyzer's `scan_export`   | [`crate::ScanRequest`] | [`crate::ScanResponse`]      |
//! | the migration hook            | [`MigrationInput`]     | not read                     |
//!
//! The rules every type here follows: an optional field is absent from the
//! wire when unset, never `null` (ADR 0011), and reads as its default when
//! absent; a required field stays required, so an answer without it does
//! not decode; an unknown field is ignored, so a newer peer may add one.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::value::RawValue;
use serde_json::{Map, Value};

// ── Command (`cmd__<id>`) ──────────────────────────────────────────────────

/// The output a command is asked for: `human` (the CLI default) or `json`
/// (always, over MCP). The host's, not the command's: no command declares
/// an arg named `format` (ADR 0011).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CommandFormat {
    #[default]
    Human,
    Json,
}

impl CommandFormat {
    /// Every format, as the CLI's `--format` takes them.
    pub const ALL: [CommandFormat; 2] = [CommandFormat::Human, CommandFormat::Json];

    /// The format's name on the wire and the command line.
    pub fn as_str(self) -> &'static str {
        match self {
            CommandFormat::Human => "human",
            CommandFormat::Json => "json",
        }
    }

    /// The format named `value`, if there is one.
    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|f| f.as_str() == value)
    }
}

/// What a `cmd__<id>` export receives: the declared args the caller set,
/// the project root, the format the caller asked for, the host's date and
/// the compiled graph. `G` is how the graph is held: the host sends a
/// [`RawGraph`] (the graph export, already rendered), the SDK reads its
/// indexed `CommandGraph`, and [`GraphWire`] is the plain shape.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct CommandInput<G = GraphWire> {
    /// The declared args the caller set, by name: strings (string, path and
    /// enum args), integers and booleans. An arg the caller left out is
    /// absent unless the host applied its declared default.
    #[serde(default)]
    pub args: Map<String, Value>,
    /// The project root, absolute and canonical (the host resolves symlinks).
    #[serde(default)]
    pub cwd: String,
    /// The format the caller asked for (the host's `--format`).
    #[serde(default)]
    pub format: CommandFormat,
    /// The host's date when the command was called, UTC, `YYYY-MM-DD`;
    /// empty when the host passed none.
    #[serde(default)]
    pub today: String,
    /// The compiled project's graph, in the graph export's shape
    /// (`specforge export --format graph` without the schema).
    #[serde(default)]
    pub graph: G,
    /// What the project's recorded test report (`specforge-report.json`)
    /// proves, per entity, as the host's coverage rule scores it; absent
    /// (`none`) when no report is recorded or the host sends none.
    #[serde(default, skip_serializing_if = "CommandEvidence::is_none")]
    pub evidence: CommandEvidence,
}

/// What a command's input says the recorded tests prove. On the wire an
/// object tagged by `state`: `{"state":"recorded","entities":{...}}`,
/// `{"state":"unreadable","reason":"..."}`; `none` is the field's absence.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum CommandEvidence {
    /// No test report is recorded (or the host is older than the field).
    #[default]
    None,
    /// The recorded report, scored: one entry per entity that counts toward
    /// coverage (ADR 0004 D2-b), keyed by id.
    Recorded {
        entities: BTreeMap<String, EntityEvidence>,
    },
    /// A report is recorded and could not be read; `reason` says why.
    Unreadable { reason: String },
}

impl CommandEvidence {
    /// Whether the input carries no evidence state at all.
    pub fn is_none(&self) -> bool {
        matches!(self, CommandEvidence::None)
    }

    /// The scored entities, when a report was recorded and read.
    pub fn entities(&self) -> Option<&BTreeMap<String, EntityEvidence>> {
        match self {
            CommandEvidence::Recorded { entities } => Some(entities),
            _ => None,
        }
    }
}

/// One entity's proof, as the coverage rule scores it: the obligations it
/// declares, how many a passing test names, and how many of its recorded
/// tests fail.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntityEvidence {
    pub obligations: usize,
    pub proven: usize,
    #[serde(default)]
    pub failing: usize,
}

impl EntityEvidence {
    /// The coverage rule's "proven" (ADR 0004 D2-a): at least one
    /// obligation, every one proven, no failing test.
    pub fn is_proven(&self) -> bool {
        self.obligations > 0 && self.proven == self.obligations && self.failing == 0
    }
}

impl<G> CommandInput<G> {
    /// Whether the caller asked for `json`.
    pub fn is_json(&self) -> bool {
        self.format == CommandFormat::Json
    }
}

/// A command input's graph as it is on the wire: its entities sorted by id,
/// its resolved references sorted by (source, target, label). Keys of the
/// graph export it does not name (`format_version`, a node's `file` and
/// `line`) are ignored.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct GraphWire {
    #[serde(default)]
    pub nodes: Vec<GraphNode>,
    #[serde(default)]
    pub edges: Vec<GraphEdge>,
}

/// The host's graph, already rendered by the graph export: spliced into
/// the command input as it is, never parsed back into a value.
#[derive(Debug, Clone, Serialize)]
#[serde(transparent)]
pub struct RawGraph(pub Box<RawValue>);

impl RawGraph {
    /// `json`, which must be one JSON value (the graph export's text).
    pub fn new(json: String) -> Result<Self, serde_json::Error> {
        RawValue::from_string(json).map(RawGraph)
    }
}

impl Default for RawGraph {
    fn default() -> Self {
        RawGraph::new("{}".to_string()).expect("{} is JSON")
    }
}

/// One entity of a command's graph: fields as the graph export writes them
/// (text as strings, lists as arrays).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct GraphNode {
    pub id: String,
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default)]
    pub fields: BTreeMap<String, Value>,
}

impl GraphNode {
    /// A text field's value (a string or an identifier); `None` when the
    /// field is absent or holds a list, a number or a block.
    pub fn text(&self, field: &str) -> Option<&str> {
        self.fields.get(field).and_then(|v| v.as_str())
    }

    /// Whether the entity sets `field`, whatever its value.
    pub fn has_field(&self, field: &str) -> bool {
        self.fields.contains_key(field)
    }

    /// A list field's string items, in declaration order; empty when the
    /// field is absent or not a list.
    pub fn list(&self, field: &str) -> Vec<&str> {
        self.fields
            .get(field)
            .and_then(|v| v.as_array())
            .map(|items| items.iter().filter_map(|i| i.as_str()).collect())
            .unwrap_or_default()
    }
}

/// One resolved reference of a command's graph; `label` is the field it
/// was declared in.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GraphEdge {
    pub source: String,
    pub target: String,
    pub label: String,
}

/// What a `cmd__<id>` export answers: the exit code the CLI exits with
/// (nonzero fails the MCP call), and the text for stdout and stderr.
/// `exit_code` is required: an answer without one is not a command output.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandOutput {
    pub exit_code: i32,
    #[serde(default)]
    pub stdout: String,
    #[serde(default)]
    pub stderr: String,
}

impl CommandOutput {
    /// Success, printing `stdout`.
    pub fn ok(stdout: impl Into<String>) -> Self {
        CommandOutput {
            exit_code: 0,
            stdout: stdout.into(),
            stderr: String::new(),
        }
    }

    /// Failure with exit code 1, printing `stderr`.
    pub fn fail(stderr: impl Into<String>) -> Self {
        CommandOutput {
            exit_code: 1,
            stdout: String::new(),
            stderr: stderr.into(),
        }
    }

    /// A command that cannot answer: `error` on stderr, nothing on stdout,
    /// exit code `exit_code`. Under `json` the error object
    /// (`{code, message, entity_id?, suggestion?}`); under `human` the line
    /// `error: <message>`, then `did you mean '<id>'?` when there is a
    /// suggestion.
    pub fn error(format: CommandFormat, error: &CommandError, exit_code: i32) -> Self {
        let stderr = match format {
            CommandFormat::Json => {
                let mut out =
                    serde_json::to_string(error).expect("command error serialization cannot fail");
                out.push('\n');
                out
            }
            CommandFormat::Human => {
                let mut out = format!("error: {}\n", error.message);
                if let Some(suggestion) = &error.suggestion {
                    out.push_str(&format!("did you mean '{suggestion}'?\n"));
                }
                out
            }
        };
        CommandOutput {
            exit_code,
            stdout: String::new(),
            stderr,
        }
    }

    /// The wire bytes the export returns.
    pub fn to_bytes(&self) -> Vec<u8> {
        serde_json::to_vec(self).expect("command output serialization cannot fail")
    }
}

/// Why a command could not answer, as it writes it under `json`: a code
/// (`ENTITY_NOT_FOUND`, `INVALID_INPUT`, ...), a message, and the entity it
/// was asked about and the nearest id of the same kind, when there are.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandError {
    pub code: String,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entity_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub suggestion: Option<String>,
}

impl CommandError {
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        CommandError {
            code: code.into(),
            message: message.into(),
            ..Default::default()
        }
    }
}

// ── MCP (`mcp__<name>`) ────────────────────────────────────────────────────
// A tool's input and answer are the JSON values its declared input and
// output schemas describe: no type of their own.

/// What an MCP resource's export receives: the URI the client read.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpResourceRequest {
    pub uri: String,
}

/// What an MCP resource's export answers: the content and its MIME type.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpResourceContent {
    pub content: String,
    pub mime_type: String,
}

// ── Compiler pass (`__pass_<name>`) ────────────────────────────────────────

/// The export a compiler pass named `pass` answers at: `__pass_<pass>`. A
/// pass's descriptor names the pass only; the SDK routes this export to the
/// pass's handler and the host calls it, so both read this one rule.
pub fn pass_export(pass: &str) -> String {
    format!("__pass_{pass}")
}

/// What a `__pass_<name>` export receives: the compiled project's entity
/// snapshot and its resolved references, with the recorded test results,
/// the claims the prove pass entailed and the previous build's statuses
/// when the host has them.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PassInput {
    pub entities: Vec<PassEntity>,
    /// Resolved references between snapshot entities. Defaults keep passes
    /// written against the entities-only input compatible.
    #[serde(default)]
    pub edges: Vec<PassEdge>,
    /// The recorded test report, without each test's runner, when the host
    /// has one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub test_results: Option<TestResults>,
    /// Entity ids whose formal claims the prove pass entailed, sorted;
    /// absent when the prove pass did not run.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proved_claims: Option<Vec<String>>,
    /// The build cache (`specforge-cache.json`, written by `specforge check
    /// --cache`): the statuses of the build that wrote it. Check-phase
    /// passes only; absent without the file (a first build), when it is
    /// invalid (the host warns W144), and for analyze passes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub previous: Option<PassBuildCache>,
}

/// One entity in the snapshot handed to a compiler pass: its id, kind,
/// field texts, edge counts and span, and how the coverage rule sees it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PassEntity {
    pub id: String,
    pub kind: String,
    /// Every field the entity writes, by name, as its field text (ADR 0019,
    /// protocol 1.1.0): scalars as written; lists of strings or references
    /// and mixed lists joined by `", "`; variant lists and type unions by
    /// `" | "`; expressions by `", "`; verify statements by `"; "`; a
    /// block's keys by `", "`. A written empty list or block is `""`, never
    /// absent. A name written twice keeps its last text.
    #[serde(default)]
    pub fields: BTreeMap<String, String>,
    #[serde(default)]
    pub incoming_edge_count: usize,
    #[serde(default)]
    pub outgoing_edge_count: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span: Option<PassSpan>,
    /// Whether the entity's kind is testable (its kind registry entry's
    /// `testable` flag), so coverage counts it.
    #[serde(default)]
    pub testable: bool,
    /// The entity owes no obligations of its own (ADR 0004, D2-b): no
    /// `no_verify_statements` rule applies to its kind, or a union body or
    /// a field that exempts it does. Decided by the host's one obligation
    /// rule (ADR 0019).
    #[serde(default)]
    pub owes_none: bool,
    /// One entry per `verify` statement, in order: its kind, or `""` for a
    /// bare `verify "..."`. Empty when the entity declares no obligations.
    #[serde(default)]
    pub verify_kinds: Vec<String>,
    /// The obligations' texts, parallel to `verify_kinds`.
    #[serde(default)]
    pub verify_texts: Vec<String>,
}

/// One resolved reference in the snapshot (label = edge label, e.g.
/// "produces", "consumes", "invariants").
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PassEdge {
    pub source: String,
    pub target: String,
    pub label: String,
}

/// A source location: an entity's in the snapshot, or a pass diagnostic's.
/// Field names mirror the host's `SourceSpan`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PassSpan {
    pub file: String,
    pub start_line: usize,
    pub start_col: usize,
    pub end_line: usize,
    pub end_col: usize,
}

/// The previous build's statuses, handed to check-phase passes.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PassBuildCache {
    /// Per entity id: its kind and status in that build. Entities without
    /// a lifecycle state are absent.
    #[serde(default)]
    pub statuses: BTreeMap<String, PassCachedStatus>,
}

/// One entity's kind and status in the previous build.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PassCachedStatus {
    pub kind: String,
    pub status: String,
}

/// A pass diagnostic's severity, on the wire as the host's (`"Error"`,
/// `"Warning"`, `"Info"`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PassSeverity {
    Error,
    Warning,
    Info,
}

/// A diagnostic a compiler pass reports, in the host's diagnostic shape.
/// A diagnostic may name the entity it is about (`entity`): with no span
/// of its own, the host attaches that entity's.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PassDiagnostic {
    pub code: String,
    pub severity: PassSeverity,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span: Option<PassSpan>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub suggestion: Option<String>,
    /// The id of the entity the diagnostic is about.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entity: Option<String>,
}

impl PassDiagnostic {
    /// A diagnostic with a code, severity, and message; attach a span or
    /// suggestion with [`Self::with_span`] / [`Self::with_suggestion`].
    pub fn new(
        code: impl Into<String>,
        severity: PassSeverity,
        message: impl Into<String>,
    ) -> Self {
        Self {
            code: code.into(),
            severity,
            message: message.into(),
            span: None,
            suggestion: None,
            entity: None,
        }
    }

    /// Convenience constructor for warnings (the common pass finding).
    pub fn warning(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self::new(code, PassSeverity::Warning, message)
    }

    pub fn with_span(mut self, span: PassSpan) -> Self {
        self.span = Some(span);
        self
    }

    pub fn with_suggestion(mut self, suggestion: impl Into<String>) -> Self {
        self.suggestion = Some(suggestion.into());
        self
    }

    /// Name the entity the diagnostic is about (see [`Self::entity`]).
    pub fn with_entity(mut self, id: impl Into<String>) -> Self {
        self.entity = Some(id.into());
        self
    }
}

/// A pass result carrying a summary beside its diagnostics: the summary's
/// keys join the host's report of the pass.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PassOutput {
    pub diagnostics: Vec<PassDiagnostic>,
    #[serde(default)]
    pub summary: Map<String, Value>,
}

/// What a pass may answer: bare diagnostics, or diagnostics with a summary.
/// On the wire an array is the bare form and an object the other; the host
/// reads both as a [`PassOutput`] ([`PassAnswer::into_output`]).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(untagged)]
pub enum PassAnswer {
    Bare(Vec<PassDiagnostic>),
    WithSummary(PassOutput),
}

impl PassAnswer {
    /// The answer as diagnostics and a summary (empty for a bare answer).
    pub fn into_output(self) -> PassOutput {
        match self {
            PassAnswer::Bare(diagnostics) => PassOutput {
                diagnostics,
                summary: Map::new(),
            },
            PassAnswer::WithSummary(output) => output,
        }
    }
}

impl From<Vec<PassDiagnostic>> for PassAnswer {
    fn from(diagnostics: Vec<PassDiagnostic>) -> Self {
        PassAnswer::Bare(diagnostics)
    }
}

impl From<PassOutput> for PassAnswer {
    fn from(output: PassOutput) -> Self {
        PassAnswer::WithSummary(output)
    }
}

/// Reads an array as the bare form and an object as the one with a
/// summary, reporting the chosen form's own error (an untagged enum would
/// only say that no variant matched).
impl<'de> Deserialize<'de> for PassAnswer {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        use serde::de::Error;
        match Value::deserialize(deserializer)? {
            items @ Value::Array(_) => Vec::<PassDiagnostic>::deserialize(items)
                .map(PassAnswer::Bare)
                .map_err(D::Error::custom),
            object @ Value::Object(_) => PassOutput::deserialize(object)
                .map(PassAnswer::WithSummary)
                .map_err(D::Error::custom),
            other => Err(D::Error::custom(format!(
                "expected an array of diagnostics or an object with diagnostics, got {other}"
            ))),
        }
    }
}

// ── Recorded tests (the collect wire, specforge-report.json, a pass's input) ──

/// What a run says of one test: the one status vocabulary a collector
/// answers, `specforge-report.json` keeps and a pass reads (ADR 0069).
/// Decoded strictly: any other value is not a `TestStatus`, so a collector
/// answer carrying one is not a `CollectOutput` (E028) and a report
/// carrying one is not a usable report (E045).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TestStatus {
    /// It ran and passed: it proves the obligation it names.
    Passed,
    /// It ran and failed: its entity is not proven (A014).
    Failed,
    /// It did not run (ignored, todo, pending): it proves nothing and
    /// fails nothing. Collect counts it and does not record it.
    Skipped,
}

/// One test: its name, what the run said of it, the obligation it names
/// (`verify`, the statement's exact text) and how long it took. A collector
/// answers it, `specforge-report.json` keeps it with the runner that
/// recorded it, and a pass reads it ([`TestResults`]).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TestResult {
    pub name: String,
    pub status: TestStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verify: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<f64>,
}

/// The recorded tests, per entity id: `specforge-report.json` as a pass
/// receives it (`PassInput::test_results`) and the coverage rule reads it.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct TestResults {
    /// The collectors whose results these are, joined `", "`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runner: Option<String>,
    #[serde(default)]
    pub results: BTreeMap<String, EntityTests>,
}

/// The tests recorded for one entity.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct EntityTests {
    #[serde(default)]
    pub tests: Vec<TestResult>,
}

impl TestResults {
    /// The tests recorded for `entity`; empty when it has none.
    pub fn of(&self, entity: &str) -> &[TestResult] {
        self.results
            .get(entity)
            .map_or(&[], |recorded| recorded.tests.as_slice())
    }
}

// ── Collector (`collect__<name>`) ──────────────────────────────────────────

/// What the host passes to a `collect__<name>` export: the runner's report
/// files, read from the declared report location, and the command's
/// standard output when the collector captures it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CollectInput {
    #[serde(default)]
    pub reports: Vec<CollectReportFile>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stdout: Option<String>,
}

/// One report file: its path relative to the project root, and its text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CollectReportFile {
    pub path: String,
    pub content: String,
}

/// What a `collect__<name>` export returns: test results grouped by the
/// entity each test proves, and the tests the report doesn't link to any
/// entity, which the host links by naming convention when it can.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct CollectOutput {
    pub entity_results: Vec<CollectEntityResult>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub unlinked: Vec<CollectUnlinkedTest>,
}

/// A test the report doesn't link to an entity: its name, the name split
/// into its path segments (`["tests", "add_item", "rejects_a_duplicate"]`,
/// the test's own name last), and `passed`, `failed` or `skipped`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CollectUnlinkedTest {
    pub name: String,
    pub path: Vec<String>,
    pub status: String,
}

/// The tests a report links to one entity.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CollectEntityResult {
    pub entity_id: String,
    pub test_results: Vec<CollectTestResult>,
}

/// One test. `status` is `passed`, `failed` or `skipped`; `verify` names
/// the obligation the test proves, when the test says so.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CollectTestResult {
    pub name: String,
    pub status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verify: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<f64>,
}

// ── Migration hook (the handshake's `migration_hook`) ──────────────────────

/// What the migration hook receives after `specforge migrate` rewrote the
/// project's files: the format versions it moved between and the files it
/// rewrote. The hook's answer is not read.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MigrationInput {
    pub from: String,
    pub to: String,
    pub files: Vec<String>,
}

#[cfg(test)]
mod tests;
