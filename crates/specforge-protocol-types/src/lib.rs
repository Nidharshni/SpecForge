//! Wire types for the SpecForge extension protocol, at the version
//! [`PROTOCOL_VERSION`] names.
//!
//! Shared single source of truth: the host (specforge-wasm) and the
//! plugin-side SDK (specforge-extension-sdk) both consume these types, so the
//! wire format cannot drift between them. Serde-only, so it compiles for the
//! host and for wasm32-wasip2 guests.
//!
//! Two families:
//! - the declaration: the `__handshake` and `__describe` payloads and the
//!   descriptors an extension declares ([`ExtensionDeclaration`], ADR 0012);
//! - the operations ([`calls`], ADR 0013): what the host sends each export
//!   it calls and what the export answers — a command ([`CommandInput`],
//!   [`CommandOutput`]), an MCP resource ([`McpResourceRequest`],
//!   [`McpResourceContent`]), a compiler pass ([`PassInput`],
//!   [`PassAnswer`]), a collector ([`CollectInput`], [`CollectOutput`]), a
//!   custom validator ([`ValidatorContext`], [`ValidatorVerdict`]), a
//!   scanner ([`ScanRequest`], [`ScanResponse`]) and the migration hook
//!   ([`MigrationInput`]).
//!
//! And one rule both sides run: [`command_args`], what a command's declared
//! args admit and what its export receives (ADR 0017).
//!
//! What sits here is what both sides compile: every builtin blob is built from this crate, so an
//! item only the host reads lives host-side (the package names and the peer rule are
//! `specforge-package`, ADR 0050).

use std::fmt;

use serde::{Deserialize, Serialize, de::DeserializeOwned};

pub mod calls;
pub mod command_args;
mod declaration;
mod vocabulary;
pub use calls::*;
pub use declaration::{
    DECLARED_CATEGORIES, DeclaredCategory, ExtensionDeclaration, UnknownKey, is_valid_short,
};
pub use vocabulary::{CheckKind, ConstraintKind, FieldType, ProofRole};

/// Protocol version for the extension wire format (semver). The host loads
/// every guest of its major version. The minor moves when a payload's
/// values change meaning or an optional field is added; the major moves
/// only when an older guest could no longer be decoded or answered.
/// `docs/extension-protocol.md` ("Protocol versions") lists what each
/// version guarantees; 1.2.0 is ADR 0050's: the declared categories are the
/// only describe categories, and the handshake says `providers` instead of
/// carrying contribution flags; 1.3.0 is ADR 0060's: an enhancement declares
/// no edge types (an edge type is declared once, in `edges`).
pub const PROTOCOL_VERSION: &str = "1.3.0";

/// Errors that can occur during the extension protocol lifecycle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProtocolError {
    /// Host and extension protocol versions are incompatible.
    IncompatibleVersion {
        host_version: String,
        extension_version: String,
    },
    /// The __handshake export failed or returned invalid data.
    HandshakeFailed(String),
    /// The __describe export failed or returned invalid data.
    DescribeFailed { category: String, reason: String },
    /// JSON deserialization of a protocol message failed.
    DeserializationError(String),
}

impl fmt::Display for ProtocolError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::IncompatibleVersion {
                host_version,
                extension_version,
            } => write!(
                f,
                "protocol version mismatch: host={}, extension={}",
                host_version, extension_version
            ),
            Self::HandshakeFailed(reason) => write!(f, "handshake failed: {}", reason),
            Self::DescribeFailed { category, reason } => {
                write!(f, "describe '{}' failed: {}", category, reason)
            }
            Self::DeserializationError(msg) => write!(f, "deserialization error: {}", msg),
        }
    }
}

impl std::error::Error for ProtocolError {}

impl From<serde_json::Error> for ProtocolError {
    fn from(e: serde_json::Error) -> Self {
        Self::DeserializationError(e.to_string())
    }
}

// ── Handshake ──

/// Sent by the host to initiate the protocol handshake.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HandshakeRequest {
    pub host_version: String,
}

/// Returned by the extension's `__handshake` export.
///
/// `peer_dependencies` is **required** on the wire: a truncated handshake
/// fails deserialization instead of yielding an empty dependency list (peer
/// checks pass vacuously) — audit C7-07.
///
/// `sandbox_policy` is a documented exception: serde treats `Option<T>`
/// fields as implicitly optional, so absence and explicit `null` both
/// deserialize to `None`. That default is fail-safe, not silent — `None`
/// means the plugin declares no limits and the host holds it to its ceiling
/// (`Limits::CEILING` in `specforge_wasm::sandbox`).
/// All builtin fixtures and every SDK-built extension serialize all six
/// fields (`ContributionsBuilder::handshake_json` never elides them).
///
/// `starter_template` is optional metadata: the text of the starter `.spec`
/// file `specforge init` writes for a project that enables this extension.
/// `{project}` in it stands for the project's entity id and `{version}` for
/// its version. It is omitted from the wire when absent, so handshakes of extensions without one are
/// unchanged.
///
/// `migration_hook` is optional too: the name of the export `specforge
/// migrate` calls after it migrates the project's files. Omitted from the
/// wire when absent; an extension without one has no hook to run.
///
/// `ext_short`, `description` and `keywords` are optional metadata, omitted
/// when absent: the short name that routes the extension's commands
/// (`specforge <ext_short> <command>`, MCP `specforge.<ext_short>.<id>`;
/// the name's last segment when absent), and what a package registry shows
/// for it. Absent fields serialize nothing, so a handshake without them is
/// byte-identical to one from before they existed.
///
/// What the extension contributes is not on the handshake: it is what its
/// declaration declares (`ExtensionDeclaration::declares`). Only `providers`,
/// which no describe category carries, is.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct HandshakeResponse {
    pub protocol_version: String,
    pub name: String,
    pub version: String,
    pub peer_dependencies: Vec<PeerDependency>,
    pub sandbox_policy: Option<SandboxPolicy>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub starter_template: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub migration_hook: Option<String>,
    /// The colour diagrams draw the extension in (`#rrggbb`): its cluster
    /// in `specforge model --format dot`, its node in `specforge outline`.
    /// Omitted from the wire when absent; diagrams then use a neutral grey.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub theme_color: Option<String>,
    /// The short name routing the extension's commands and tools
    /// (lowercase kebab case); the name's last segment when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ext_short: Option<String>,
    /// One line saying what the extension is for (package registries show
    /// it in search results).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Search keywords for package registries.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub keywords: Vec<String>,
    /// The extension implements configured providers: a `specforge.json`
    /// `providers` entry may name it as its extension (W118 otherwise).
    /// Omitted from the wire when false.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub providers: bool,
}

/// Declares a dependency on another extension.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PeerDependency {
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub optional: bool,
}

/// The limits an extension asks its sandbox to hold it to (ADR 0037). The
/// host grants a component no capability, so the policy declares limits
/// only. Each is held to the host's ceiling (`Limits::CEILING` in
/// `specforge_wasm::sandbox`), and an undeclared one is the ceiling.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct SandboxPolicy {
    /// Wall-clock budget of one call, in milliseconds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_execution_ms: Option<u32>,
    /// Ceiling of the instance's linear memory, in MiB.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_memory_mb: Option<u32>,
}

// ── Describe ──

/// Sent by the host to request a specific contribution category.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DescribeRequest {
    pub category: String,
}

/// Returned by the extension's `__describe` export.
/// Items are stored as raw JSON and parsed into typed descriptors via `parse_items`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DescribeResponse {
    pub category: String,
    pub items: serde_json::Value,
}

impl DescribeResponse {
    /// Parse the raw `items` array into a typed `Vec<T>`.
    pub fn parse_items<T: DeserializeOwned>(&self) -> Result<Vec<T>, ProtocolError> {
        serde_json::from_value(self.items.clone()).map_err(ProtocolError::from)
    }

    /// The answer as a guest puts it on the wire (pretty JSON).
    pub fn wire_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("describe serialization cannot fail")
    }
}

// ── Entity Kind Descriptor ──

/// Describes an entity kind contributed by an extension.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct EntityKindDescriptor {
    pub name: String,
    #[serde(default)]
    pub keyword: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub fields: Vec<FieldDescriptor>,
    #[serde(default)]
    pub testable: bool,
    #[serde(default)]
    pub singleton: bool,
    #[serde(default)]
    pub supports_verify: bool,
    #[serde(default)]
    pub incremental: Option<bool>,
    #[serde(default)]
    pub has_body_parser: bool,
    #[serde(default)]
    pub open_fields: bool,
    #[serde(default)]
    pub semantic_token: Option<String>,
    #[serde(default)]
    pub lsp_icon: Option<String>,
    #[serde(default)]
    pub dot_shape: Option<String>,
    #[serde(default)]
    pub dot_color: Option<String>,
    #[serde(default)]
    pub dot_fillcolor: Option<String>,
    #[serde(default)]
    pub verify_kinds: Vec<String>,
    #[serde(default)]
    pub inference_guide: Option<String>,
    /// Its entities are contract clauses: a reference field that targets
    /// this kind is a contract obligation of the entity that declares it
    /// (the `contracts` analysis, A010).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub contract_target: bool,
    /// Its entity ids name types: custom validators receive them as
    /// `ValidatorContext::declared_types`.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub declares_types: bool,
    /// The one field (of those this kind declares) holding its entities'
    /// lifecycle state: the build cache records its value for check-phase
    /// passes that compare against the previous build (ADR 0009, C).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lifecycle_field: Option<String>,
}

// ── Field Descriptor ──

/// Describes a field on an entity kind.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct FieldDescriptor {
    pub name: String,
    /// A [`FieldType`] name (kept a string so an unknown name costs this
    /// field a diagnostic, not the whole describe payload).
    pub field_type: String,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub edge: Option<String>,
    #[serde(default)]
    pub target_kind: Option<String>,
    #[serde(default)]
    pub file_reference: bool,
    #[serde(default)]
    pub default_value: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub enum_values: Vec<String>,
    #[serde(default)]
    pub inverse_of: Option<String>,
    /// The field states what the entity promises (a contract, a guarantee)
    /// rather than prose; token-optimized exports keep it.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub normative: bool,
    /// Set on an entity (`true`, or a non-empty value), the entity owes no
    /// obligations of its own: W004, the coverage rule and stats leave it
    /// out (ADR 0004, D2-b). E.g. a specification-only `abstract` flag.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub exempts_obligations: bool,
    /// The context export carries this field at the node's top level, as
    /// the line an agent reads first (a contract, a status), instead of
    /// among the normative `fields`.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub headline: bool,
    /// The host fills this reference field's edges from type names the
    /// entity writes elsewhere: `type_expressions` (its field types) or
    /// `method_signatures` (its method parameter and return types).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub derived_from: Option<String>,
    /// What the prove pass reads this field as: `bound` (a fact the solver
    /// assumes) or `claim` (a statement that must follow from the bounds).
    /// Kept a string, as `derived_from`, so an unknown value costs this
    /// field a diagnostic (ADR 0009, A).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proof_role: Option<String>,
}

// ── Edge Type Descriptor ──

/// Describes an edge type (relationship) between entity kinds.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct EdgeTypeDescriptor {
    pub label: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub source_kind: Option<String>,
    #[serde(default)]
    pub target_kind: Option<String>,
    #[serde(default)]
    pub edge_style: Option<String>,
    #[serde(default)]
    pub edge_color: Option<String>,
    #[serde(default)]
    pub edge_arrowhead: Option<String>,
}

// ── Shared Field Descriptor ──

/// A field applied globally to all entity kinds. Structurally identical to FieldDescriptor.
pub type SharedFieldDescriptor = FieldDescriptor;

// ── Entity Enhancement Descriptor ──

/// Describes fields added to a foreign entity kind.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct EntityEnhancementDescriptor {
    pub target_kind: String,
    pub source_extension: String,
    #[serde(default)]
    pub fields: Vec<FieldDescriptor>,
    /// Makes the target kind testable with these `verify` kinds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verify_kinds: Option<Vec<String>>,
}

// ── Validation Rule Descriptor ──

/// Severity level for validation diagnostics.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ValidationSeverity {
    Error,
    #[default]
    Warning,
    Info,
}

/// Constraint on a field value.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct FieldConstraintDescriptor {
    /// A [`ConstraintKind`] name.
    pub kind: String,
    #[serde(default)]
    pub pattern: Option<String>,
    #[serde(default)]
    pub values: Vec<String>,
}

/// Describes a validation rule contributed by an extension.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ValidationRuleDescriptor {
    pub code: String,
    pub severity: ValidationSeverity,
    pub message_template: String,
    /// A [`CheckKind`] name (kept a string so an unknown name costs this
    /// rule a diagnostic, not the whole describe payload).
    pub check: String,
    #[serde(default)]
    pub target_kind: Option<String>,
    #[serde(default)]
    pub edge_type: Option<String>,
    #[serde(default)]
    pub field: Option<String>,
    #[serde(default)]
    pub constraint: Option<FieldConstraintDescriptor>,
    #[serde(default)]
    pub wasm_function: Option<String>,
    /// The extension, other than the declaring one or its peers, whose
    /// kinds or edge types this rule's `target_kind` and `edge_type` name
    /// (protocol 1.1.0). While it is not loaded the rule is inert and costs
    /// no W021; loaded, a kind or edge type it does not declare is W021.
    /// Absent: the kind belongs to the extension itself or a declared peer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_extension: Option<String>,
}

// ── Surface Descriptors ──

/// Describes all surface contributions (CLI commands, MCP tools, MCP resources).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct SurfaceDescriptor {
    #[serde(default)]
    pub commands: Vec<CommandDescriptor>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mcp_tools: Vec<McpToolDescriptor>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mcp_resources: Vec<McpResourceDescriptor>,
}

/// Describes a CLI command contributed by an extension.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct CommandDescriptor {
    pub id: String,
    pub title: String,
    pub description: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    pub export: String,
    #[serde(default)]
    pub args: Vec<CommandArgDescriptor>,
}

/// Describes an argument to a CLI command.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct CommandArgDescriptor {
    pub name: String,
    pub arg_type: CommandArgType,
    #[serde(default)]
    pub required: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_value: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// The least value of an integer arg: `0` for a count. Absent, any
    /// integer (wire compatible both ways: a host or guest that does not
    /// know it ignores it).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub minimum: Option<i64>,
}

/// Type of a command argument.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum CommandArgType {
    #[default]
    String,
    Path,
    Bool,
    #[serde(rename = "enum")]
    Enum {
        values: Vec<String>,
    },
    Integer,
}

/// Describes an MCP tool contributed by an extension.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct McpToolDescriptor {
    pub name: String,
    pub description: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    pub export: String,
    pub input_schema: serde_json::Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_schema: Option<serde_json::Value>,
}

/// Describes an MCP resource contributed by an extension.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct McpResourceDescriptor {
    pub uri_template: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub export: String,
    pub mime_type: String,
}

// ── Collector Descriptor ──

/// Auto-detection configuration for a collector.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct AutoDetectConfig {
    pub file_patterns: Vec<String>,
    #[serde(default)]
    pub env_vars: Vec<String>,
}

/// Describes a test result collector.
///
/// A runner extension declares the command that runs its test runner
/// (`run`, an argv whose elements may contain the `{report}` placeholder)
/// and where the runner leaves its report (`report`, a file or directory
/// relative to the project root). The host runs the command, with the
/// user's consent, and passes the report bytes to the pure `export`
/// (ADR 0002). With `capture: "stdout"` the host also keeps the command's
/// standard output and passes it along, for runners whose results only
/// appear there.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct CollectorDescriptor {
    pub name: String,
    pub input_formats: Vec<String>,
    pub export: String,
    #[serde(default)]
    pub auto_detect: Option<AutoDetectConfig>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub run: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub report: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capture: Option<String>,
}

// ── Analyzer Protocol Types ──

/// Request to scan a source file for public items.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ScanRequest {
    pub file_path: String,
    pub content: String,
}

/// A single scanned item from source code.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ScannedItem {
    pub name: String,
    pub item_kind: String,
    pub line: usize,
    #[serde(default)]
    pub visibility: Option<String>,
    #[serde(default)]
    pub signature: Option<String>,
}

/// Response from scanning a source file.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ScanResponse {
    pub items: Vec<ScannedItem>,
    #[serde(default)]
    pub language: Option<String>,
}

/// Request to classify scanned items into specforge entity categories.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ClassifyRequest {
    pub items: Vec<ScannedItem>,
    pub file_path: String,
}

/// Classification result for a scanned item.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ClassifiedItem {
    pub name: String,
    pub item_kind: String,
    pub suggested_entity_kind: Option<String>,
    pub confidence: f64,
    pub line: usize,
}

/// Response from classifying scanned items.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ClassifyResponse {
    pub items: Vec<ClassifiedItem>,
}

/// Request to map a source symbol to a specforge entity ID.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MapSymbolRequest {
    pub name: String,
    pub item_kind: String,
    pub file_path: String,
    #[serde(default)]
    pub existing_entity_ids: Vec<String>,
}

/// Response from mapping a symbol to an entity ID.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MapSymbolResponse {
    pub entity_id: Option<String>,
    pub mapping_strategy: String,
}

// ── Analyzer Descriptor ──

/// Describes a language analyzer contributed by an extension.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct AnalyzerDescriptor {
    pub language: String,
    pub file_extensions: Vec<String>,
    #[serde(default)]
    pub excluded_dirs: Vec<String>,
    pub scan_export: String,
    pub classify_export: String,
    pub map_export: String,
    #[serde(default)]
    pub description: Option<String>,
}

// ── Compiler Pass Descriptor ──

/// Describes a compiler pass contributed by an extension.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct CompilerPassDescriptor {
    pub name: String,
    #[serde(default)]
    pub after: Option<String>,
    #[serde(default)]
    pub before: Option<String>,
    #[serde(default)]
    pub phase: Option<String>,
}

// ── Feature Flag Descriptor ──

/// Describes a feature flag contributed by an extension.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct FeatureFlagDescriptor {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub default_enabled: bool,
}

// ── Custom Validator Protocol (v1) ──

/// Context handed to a `validate__*` wasm export for one entity. The host
/// precomputes everything the native custom-rule walks touch
/// the host-side custom-rule walker it replaced, so a guest validator is a
/// pure function of this
/// value: reference resolutions replace graph lookups, declared types and
/// primitives replace registry access.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ValidatorContext {
    /// The entity being validated.
    pub entity: ValidatorEntity,
    /// Resolution of every reference target the entity declares. A target
    /// with no graph node carries `kind: null` (dangling reference).
    pub referenced: Vec<ValidatorRef>,
    /// IDs of every `type`-kind entity in the graph.
    pub declared_types: Vec<String>,
    /// Type names accepted without a declared `type` entity (the host's
    /// primitive-type list).
    pub primitives: Vec<String>,
}

/// One entity in a [`ValidatorContext`].
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ValidatorEntity {
    pub id: String,
    pub kind: String,
    /// Every field the entity writes, in declaration order (a name written
    /// twice appears twice); `value` is the field's text (ADR 0019,
    /// protocol 1.1.0), always a JSON string.
    pub fields: Vec<ValidatorField>,
    /// Declared methods (populated for entity kinds that have them, e.g.
    /// ports).
    pub methods: Vec<ValidatorMethod>,
}

/// One field in a [`ValidatorEntity`].
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ValidatorField {
    pub key: String,
    /// The field's text, always a JSON string (ADR 0019, protocol 1.1.0):
    /// scalars as written; lists of strings or references and mixed lists
    /// joined by `", "`; variant lists and type unions by `" | "`;
    /// expressions by `", "`; verify statements by `"; "`; a block's keys by
    /// `", "`; an empty list or block `""`. Under protocol 1.0.0 a variant
    /// list, mixed list, expression or type union was `null`.
    pub value: serde_json::Value,
    /// Annotation names applied to the field, without the `@`.
    #[serde(default)]
    pub annotations: Vec<String>,
}

/// One method in a [`ValidatorEntity`].
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ValidatorMethod {
    pub name: String,
    pub params: Vec<ValidatorParam>,
    #[serde(default)]
    pub returns: Option<String>,
}

/// One method parameter.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ValidatorParam {
    pub name: String,
    /// Parameter type as written (may be generic, e.g. `Result<A, B>`).
    pub ty: String,
}

/// One resolved reference in a [`ValidatorContext`].
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ValidatorRef {
    pub id: String,
    /// Kind of the referenced node; `None` when the reference dangles
    /// (the ID names no node in the graph).
    #[serde(default)]
    pub kind: Option<String>,
}

/// Verdict of a `validate__*` wasm export; the wire mirror of the host's
/// `CustomVerdict`. Serializes as `{"verdict":"pass"}` or
/// `{"verdict":"fail","field":...,"value":...}` (`field`/`value` omitted
/// when `None`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "verdict", rename_all = "lowercase")]
pub enum ValidatorVerdict {
    Pass,
    Fail {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        field: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        value: Option<String>,
    },
}
