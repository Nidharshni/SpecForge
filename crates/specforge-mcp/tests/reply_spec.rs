//! `spec/types/mcp.spec`'s reply types are the schemas the replies derive
//! (ADR 0048 D8).
//!
//! Each core tool's reply is named once in [`REPLIES`]: an `Mcp*` type, a
//! document specified elsewhere, or text. The spec types reached from a
//! tool's type are compared with the schema its reply derives:
//!
//! 1. a name in [`SHARED`] is a domain type the reply embeds, compared by
//!    identity (it is not a field-by-field copy of the wire);
//! 2. a union of named types is a `oneOf`/`anyOf` of the same length,
//!    compared in order;
//! 3. an alias of string literals is a string `enum` of the same set;
//! 4. a record is a closed object whose properties are exactly its fields
//!    (`verify` excluded), required exactly the fields without `@optional`;
//!    a record meeting a union of tagged objects is compared with the union
//!    merged (the union of the properties, the intersection of the required
//!    keys); a property that can be `null` is a violation (D4);
//! 5. a named type no spec file declares is a violation.
//!
//! A mutation's `files_written` is the pipeline's and is removed first.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::sync::OnceLock;

use serde_json::Value;
use specforge_parser::FieldValue;
use specforge_test::prelude::*;

/// What a core tool's reply is, in the spec.
enum Spec {
    /// An `Mcp*` type of `spec/types/mcp.spec`.
    Type(&'static str),
    /// A document specified elsewhere.
    Document(#[allow(dead_code)] &'static str),
    /// A document in text: no outputSchema.
    Text,
}

use Spec::{Document, Text, Type};

/// Each core tool's reply as the spec names it: an `Mcp*` type, or why it
/// has none.
const REPLIES: &[(&str, Spec)] = &[
    ("specforge.query", Document("the agent export, ADR 0007")),
    ("specforge.validate", Text),
    ("specforge.analyze", Type("McpAnalyzeResult")),
    ("specforge.export", Text),
    ("specforge.trace", Type("McpTraceResult")),
    ("specforge.search", Type("McpSearchResults")),
    ("specforge.explain", Type("McpExplainResult")),
    (
        "specforge.schema",
        Document("specforge schema's GraphProtocolSchema document"),
    ),
    ("specforge.model", Text),
    ("specforge.outline_extensions", Text),
    ("specforge.coverage", Type("McpCoverageResults")),
    ("specforge.stats", Type("McpStatsResult")),
    ("specforge.list", Type("McpListResult")),
    ("specforge.inspect", Type("McpInspectResult")),
    ("specforge.find_definition", Type("McpDefinitionResult")),
    ("specforge.find_references", Type("McpReferenceResult")),
    ("specforge.outline", Type("McpOutlineResult")),
    ("specforge.suggest_fixes", Type("McpFixSuggestions")),
    ("specforge.format", Type("McpFormatResult")),
    ("specforge.rename", Type("McpRenameResult")),
    ("specforge.init", Type("McpInitResult")),
    ("specforge.add_extension", Type("McpAddExtensionResult")),
    (
        "specforge.remove_extension",
        Type("McpRemoveExtensionResult"),
    ),
    ("specforge.migrate", Type("McpMigrateResult")),
    ("specforge.extensions", Type("McpExtensionsResult")),
    ("specforge.providers", Type("McpProvidersResult")),
    ("specforge.doctor", Type("McpDoctorReport")),
    ("specforge.collect", Type("McpCollectResult")),
    ("specforge.render", Type("McpRenderResult")),
    ("specforge.infer_progress", Type("McpInferProgressResult")),
    ("specforge.infer_gaps", Type("McpInferGapsResult")),
    ("specforge.infer_session", Type("McpInferSessionResult")),
    (
        "specforge.find_implementation",
        Type("McpImplementationResult"),
    ),
    (
        "specforge.find_spec_for_source",
        Type("McpSpecForSourceResult"),
    ),
];

/// Domain types a reply embeds, specified in their own spec files: their
/// drift from the wire is not this test's (the spec gives the domain, the
/// reply the wire).
const SHARED: &[&str] = &[
    "Diagnostic",
    "MigrationResult",
    "MigrationDiff",
    "MigrationBackup",
    "RollbackSummary",
];

// ── the spec side ───────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
enum Ty {
    Name(String),
    List(Box<Ty>),
    Literal(String),
    Union(Vec<String>),
    Unsupported(String),
}

#[derive(Debug)]
struct SpecField {
    name: String,
    ty: Ty,
    optional: bool,
}

#[derive(Debug)]
enum SpecType {
    Record(Vec<SpecField>),
    Alias(Vec<String>),
}

fn ty_of(value: &FieldValue) -> Ty {
    match value {
        FieldValue::Identifier(text) => name_or_list(text),
        FieldValue::String(literal) => Ty::Literal(literal.clone()),
        FieldValue::TypeUnion(items) if items.iter().all(|i| i.starts_with('"')) => Ty::Union(
            items
                .iter()
                .map(|i| i.trim_matches('"').to_string())
                .collect(),
        ),
        other => Ty::Unsupported(format!("{other:?}")),
    }
}

fn name_or_list(text: &str) -> Ty {
    match text.strip_suffix("[]") {
        Some(inner) => Ty::List(Box::new(name_or_list(inner))),
        None => Ty::Name(text.to_string()),
    }
}

/// Every `type` entity of `spec/types/*.spec`.
fn spec_types() -> &'static BTreeMap<String, SpecType> {
    static TYPES: OnceLock<BTreeMap<String, SpecType>> = OnceLock::new();
    TYPES.get_or_init(|| {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../spec/types");
        let mut types = BTreeMap::new();
        let mut files: Vec<_> = std::fs::read_dir(&dir)
            .expect("spec/types")
            .map(|e| e.expect("an entry").path())
            .filter(|p| p.extension().is_some_and(|e| e == "spec"))
            .collect();
        files.sort();
        for path in files {
            let source = std::fs::read_to_string(&path).expect("a spec file");
            let file = specforge_parser::parse(&source, &path.display().to_string());
            for entity in file
                .entities
                .iter()
                .filter(|e| e.kind.raw.as_str() == "type")
            {
                let name = entity.id.raw.to_string();
                let spec = match entity
                    .fields
                    .get(specforge_parser::ast::UNION_VARIANTS_FIELD)
                {
                    Some(FieldValue::VariantList(members)) => SpecType::Alias(members.clone()),
                    _ => SpecType::Record(
                        entity
                            .fields
                            .entries()
                            .iter()
                            .filter(|f| f.key.as_str() != "verify")
                            .map(|f| SpecField {
                                name: f.key.to_string(),
                                ty: ty_of(&f.value),
                                optional: f.annotations.iter().any(|a| a.name == "optional"),
                            })
                            .collect(),
                    ),
                };
                types.insert(name, spec);
            }
        }
        types
    })
}

// ── the comparison ──────────────────────────────────────────────────────────

#[derive(Default)]
struct Report {
    reached: BTreeSet<String>,
    violations: BTreeMap<String, Vec<String>>,
}

impl Report {
    fn violate(&mut self, ty: &str, message: String) {
        self.violations
            .entry(ty.to_string())
            .or_default()
            .push(message);
    }
}

/// `schema` without the `files_written` property a mutation adds, at its
/// root or in each branch of its union.
fn without_files_written(mut schema: Value) -> Value {
    fn strip(object: &mut Value) {
        if let Some(properties) = object.get_mut("properties").and_then(Value::as_object_mut) {
            properties.remove("files_written");
        }
    }
    strip(&mut schema);
    for key in ["oneOf", "anyOf"] {
        if let Some(branches) = schema.get_mut(key).and_then(Value::as_array_mut) {
            branches.iter_mut().for_each(strip);
        }
    }
    schema
}

fn can_be_null(schema: &Value) -> bool {
    let typed = match schema.get("type") {
        Some(Value::Array(types)) => types.iter().any(|t| t == "null"),
        Some(Value::String(t)) => t == "null",
        _ => false,
    };
    let composed = ["anyOf", "oneOf"].iter().any(|key| {
        schema
            .get(*key)
            .and_then(Value::as_array)
            .is_some_and(|branches| branches.iter().any(can_be_null))
    });
    typed || composed
}

fn strings(value: Option<&Value>) -> BTreeSet<String> {
    value
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

/// A union of tagged objects as the one record it is read as: the union of
/// the branches' properties (the enum values of a shared tag joined), the
/// keys every branch requires.
fn merged(schema: &Value) -> Value {
    let branches = schema["oneOf"].as_array().cloned().unwrap_or_default();
    let mut properties: BTreeMap<String, Value> = BTreeMap::new();
    let mut required: Option<BTreeSet<String>> = None;
    for branch in &branches {
        for (key, property) in branch["properties"].as_object().into_iter().flatten() {
            match properties.get_mut(key) {
                Some(have) => {
                    if let (Some(a), Some(b)) = (
                        have["enum"].as_array().cloned(),
                        property["enum"].as_array(),
                    ) {
                        let mut all = a;
                        all.extend(b.iter().cloned());
                        have["enum"] = Value::Array(all);
                    }
                }
                None => {
                    properties.insert(key.clone(), property.clone());
                }
            }
        }
        let keys = strings(branch.get("required"));
        required = Some(match required {
            Some(have) => have.intersection(&keys).cloned().collect(),
            None => keys,
        });
    }
    serde_json::json!({
        "type": "object",
        "properties": properties,
        "required": required.unwrap_or_default(),
        "additionalProperties": false,
    })
}

fn check_type(name: &str, schema: &Value, report: &mut Report) {
    report.reached.insert(name.to_string());
    if SHARED.contains(&name) {
        return;
    }
    let Some(spec) = spec_types().get(name) else {
        report.violate(name, "no spec file declares this type".to_string());
        return;
    };
    match spec {
        SpecType::Alias(members) => check_alias(name, members, schema, report),
        SpecType::Record(fields) => check_record(name, fields, schema, report),
    }
}

fn check_alias(name: &str, members: &[String], schema: &Value, report: &mut Report) {
    let named = members
        .iter()
        .all(|m| spec_types().contains_key(m) || SHARED.contains(&m.as_str()));
    if named {
        let branches = schema
            .get("oneOf")
            .or_else(|| schema.get("anyOf"))
            .and_then(Value::as_array);
        let Some(branches) = branches.filter(|b| b.len() == members.len()) else {
            report.violate(
                name,
                format!(
                    "a union of {} types, but the schema is {}",
                    members.len(),
                    shape(schema)
                ),
            );
            return;
        };
        for (member, branch) in members.iter().zip(branches) {
            check_type(member, branch, report);
        }
        return;
    }
    let wanted: BTreeSet<String> = members.iter().cloned().collect();
    if schema["type"] != "string" || strings(schema.get("enum")) != wanted {
        report.violate(
            name,
            format!("the names {wanted:?}, but the schema is {}", shape(schema)),
        );
    }
}

fn check_record(name: &str, fields: &[SpecField], schema: &Value, report: &mut Report) {
    let schema = if schema.get("oneOf").is_some() {
        merged(schema)
    } else {
        schema.clone()
    };
    if schema["type"] != "object" || schema["additionalProperties"] != false {
        report.violate(
            name,
            format!("a closed object, but the schema is {}", shape(&schema)),
        );
        return;
    }
    let properties = schema["properties"]
        .as_object()
        .cloned()
        .unwrap_or_default();
    let reply: BTreeSet<&String> = properties.keys().collect();
    let spec: BTreeSet<&String> = fields.iter().map(|f| &f.name).collect();
    let only_reply: Vec<_> = reply.difference(&spec).collect();
    let only_spec: Vec<_> = spec.difference(&reply).collect();
    if !only_reply.is_empty() {
        report.violate(
            name,
            format!("keys the reply sends and the type lacks: {only_reply:?}"),
        );
    }
    if !only_spec.is_empty() {
        report.violate(
            name,
            format!("fields the type has and the reply never sends: {only_spec:?}"),
        );
    }
    let required = strings(schema.get("required"));
    let spec_required: BTreeSet<String> = fields
        .iter()
        .filter(|f| !f.optional)
        .map(|f| f.name.clone())
        .collect();
    if required != spec_required {
        report.violate(
            name,
            format!(
                "required: the reply {:?}, the type {:?}",
                required.difference(&spec_required).collect::<Vec<_>>(),
                spec_required.difference(&required).collect::<Vec<_>>()
            ),
        );
    }
    for field in fields {
        if let Some(property) = properties.get(&field.name) {
            if can_be_null(property) {
                report.violate(
                    name,
                    format!(
                        "{}: the reply may carry null (an optional value is absent)",
                        field.name
                    ),
                );
            }
            check_ty(name, &field.name, &field.ty, property, report);
        }
    }
}

fn check_ty(owner: &str, field: &str, ty: &Ty, schema: &Value, report: &mut Report) {
    let mismatch = |report: &mut Report, wanted: &str| {
        report.violate(
            owner,
            format!(
                "{field}: the type says {wanted}, the schema is {}",
                shape(schema)
            ),
        );
    };
    match ty {
        Ty::List(inner) => {
            if schema["type"] == "array" && schema.get("items").is_some() {
                check_ty(owner, field, inner, &schema["items"], report);
            } else {
                mismatch(report, "a list");
            }
        }
        Ty::Literal(literal) => {
            if strings(schema.get("enum")) != BTreeSet::from([literal.clone()]) {
                mismatch(report, &format!("the literal {literal:?}"));
            }
        }
        Ty::Union(literals) => {
            let wanted: BTreeSet<String> = literals.iter().cloned().collect();
            if strings(schema.get("enum")) != wanted {
                mismatch(report, &format!("one of {wanted:?}"));
            }
        }
        Ty::Name(name) => match name.as_str() {
            "string" => {
                if schema["type"] != "string" {
                    mismatch(report, "a string");
                }
            }
            "integer" => {
                if schema["type"] != "integer" {
                    mismatch(report, "an integer");
                }
            }
            "float" => {
                if schema["type"] != "number" {
                    mismatch(report, "a number");
                }
            }
            "boolean" => {
                if schema["type"] != "boolean" {
                    mismatch(report, "a boolean");
                }
            }
            "object" => {
                let any = schema.as_object().is_some_and(|o| o.is_empty());
                let composed = schema.get("oneOf").is_some() || schema.get("anyOf").is_some();
                if schema["type"] != "object" && !any && !composed {
                    mismatch(report, "an object");
                }
            }
            other => check_type(other, schema, report),
        },
        Ty::Unsupported(text) => report.violate(
            owner,
            format!("{field}: a type the test cannot read: {text}"),
        ),
    }
}

/// A short account of `schema` for a message.
fn shape(schema: &Value) -> String {
    let text = schema.to_string();
    if text.len() > 160 {
        format!("{}…", &text[..160])
    } else {
        text
    }
}

/// One walk over every core tool: per spec type reached, its violations.
fn report() -> &'static Report {
    static REPORT: OnceLock<Report> = OnceLock::new();
    REPORT.get_or_init(|| {
        let mut report = Report::default();
        for (tool, spec) in REPLIES {
            let Type(name) = spec else { continue };
            let core = specforge_mcp::tools::core_tool(tool).expect("a core tool");
            let schema = core
                .output_schema()
                .unwrap_or_else(|| panic!("{tool} names {name} but lists no outputSchema"));
            let schema = if core.is_mutation() {
                without_files_written(schema)
            } else {
                schema
            };
            check_type(name, &schema, &mut report);
        }
        report
    })
}

/// `name` was reached from a core tool's reply and has no violation.
fn assert_reply_type(name: &str) {
    let report = report();
    assert!(
        report.reached.contains(name),
        "{name} is reached from no core tool's reply"
    );
    let violations = report.violations.get(name).cloned().unwrap_or_default();
    assert!(
        violations.is_empty(),
        "{name}:\n  {}",
        violations.join("\n  ")
    );
}

#[test]
fn every_core_tool_names_its_reply_in_the_spec() {
    let named: BTreeSet<&str> = REPLIES.iter().map(|(tool, _)| *tool).collect();
    assert_eq!(named.len(), REPLIES.len(), "a tool is named twice");
    let core: BTreeSet<&str> = specforge_mcp::tools::CORE_TOOLS
        .iter()
        .map(|t| t.name())
        .collect();
    assert_eq!(named, core, "REPLIES names exactly the core tools");
    for (tool, spec) in REPLIES {
        let listed = specforge_mcp::tools::core_tool(tool)
            .expect("a core tool")
            .output_schema()
            .is_some();
        assert_eq!(
            listed,
            !matches!(spec, Text),
            "{tool}: Text exactly when it lists no outputSchema"
        );
    }
}

#[test]
fn every_spec_reply_type_is_reached_and_none_is_unreached() {
    // The record types the spec holds for replies, all reached.
    let report = report();
    let unreached: Vec<&String> = spec_types()
        .keys()
        .filter(|name| {
            name.starts_with("Mcp")
                && !report.reached.contains(*name)
                && !PROTOCOL_AND_PROMPT_TYPES.contains(&name.as_str())
        })
        .collect();
    assert!(
        unreached.is_empty(),
        "Mcp types no core tool's reply reaches (nor protocol or prompt types): {unreached:?}"
    );
}

/// `Mcp*` types that are no tool reply: the protocol's and the prompts'.
const PROTOCOL_AND_PROMPT_TYPES: &[&str] = &[
    "McpErrorCode",
    "McpError",
    "McpCapabilities",
    "McpResourceDescriptor",
    "McpToolDescriptor",
    "McpToolAnnotations",
    "McpSubscription",
    "McpToolCategory",
    "McpToolGroup",
    "McpPromptDescriptor",
    "McpPromptArgument",
    "McpContextPromptResult",
    "McpReviewPromptResult",
    "McpReviewFinding",
    "McpTracePromptResult",
    "McpTraceGap",
    "McpExplorePromptResult",
    "McpRelationshipPath",
    // The extension surfaces' (ADR 0017).
    "McpResourceContent",
    "McpResourceContribution",
    "McpResourceRequest",
    "McpToolContribution",
];

macro_rules! reply_type {
    ($($test:ident => $name:literal, $verify:literal),* $(,)?) => {$(
        #[specforge_test(type = $name, verify = $verify)]
        fn $test() {
            assert_reply_type($name)
        }
    )*};
}

reply_type! {
    mcp_inspect_result => "McpInspectResult", "McpInspectResult schema is valid",
    mcp_definition_result => "McpDefinitionResult", "McpDefinitionResult schema is valid",
    mcp_reference_location => "McpReferenceLocation", "McpReferenceLocation schema is valid",
    mcp_reference_result => "McpReferenceResult", "McpReferenceResult schema is valid",
    mcp_outline_result => "McpOutlineResult", "McpOutlineResult schema is valid",
    mcp_outline_entry => "McpOutlineEntry", "McpOutlineEntry schema is valid",
    mcp_outline_child => "McpOutlineChild", "McpOutlineChild schema is valid",
    mcp_fix_suggestions => "McpFixSuggestions", "McpFixSuggestions schema is valid",
    mcp_fix_suggestion => "McpFixSuggestion", "McpFixSuggestion schema is valid",
    mcp_stats_result => "McpStatsResult", "McpStatsResult schema is valid",
    mcp_entity_count => "McpEntityCount", "McpEntityCount schema is valid",
    mcp_diagnostic_summary => "McpDiagnosticSummary", "McpDiagnosticSummary schema is valid",
    mcp_list_result => "McpListResult", "McpListResult schema is valid",
    mcp_listed_entity => "McpListedEntity", "McpListedEntity schema is valid",
    mcp_extensions_result => "McpExtensionsResult", "McpExtensionsResult schema is valid",
    mcp_lock_entry => "McpLockEntry", "McpLockEntry schema is valid",
    mcp_extension_info => "McpExtensionInfo", "McpExtensionInfo schema is valid",
    mcp_providers_result => "McpProvidersResult", "McpProvidersResult schema is valid",
    mcp_provider_info => "McpProviderInfo", "McpProviderInfo schema is valid",
    mcp_doctor_finding => "McpDoctorFinding", "McpDoctorFinding schema is valid",
    mcp_doctor_extension => "McpDoctorExtension", "McpDoctorExtension schema is valid",
    mcp_doctor_report => "McpDoctorReport", "McpDoctorReport schema is valid",
    mcp_init_result => "McpInitResult", "McpInitResult schema is valid",
    mcp_format_result => "McpFormatResult", "McpFormatResult schema is valid",
    mcp_format_failure => "McpFormatFailure", "McpFormatFailure schema is valid",
    mcp_search_results => "McpSearchResults", "McpSearchResults schema is valid",
    mcp_search_result => "McpSearchResult", "McpSearchResult schema is valid",
    mcp_coverage_results => "McpCoverageResults", "McpCoverageResults schema is valid",
    mcp_coverage_result => "McpCoverageResult", "McpCoverageResult schema is valid",
    mcp_rename_result => "McpRenameResult", "McpRenameResult schema is valid",
    mcp_rename_edit => "McpRenameEdit", "McpRenameEdit schema is valid",
    mcp_trace_link => "McpTraceLink", "McpTraceLink schema is valid",
    mcp_missing_link => "McpMissingLink", "McpMissingLink schema is valid",
    mcp_analyze_result => "McpAnalyzeResult", "McpAnalyzeResult schema is valid",
    mcp_analyze_pass => "McpAnalyzePass", "McpAnalyzePass schema is valid",
    mcp_gate_met => "McpGateMet", "McpGateMet schema is valid",
    mcp_gate_below => "McpGateBelow", "McpGateBelow schema is valid",
    mcp_gate_unjudged => "McpGateUnjudged", "McpGateUnjudged schema is valid",
    mcp_stray_record => "McpStrayRecord", "McpStrayRecord schema is valid",
    mcp_explain_entry => "McpExplainEntry", "McpExplainEntry schema is valid",
    mcp_explain_retired => "McpExplainRetired", "McpExplainRetired schema is valid",
    mcp_explain_replacement => "McpExplainReplacement", "McpExplainReplacement schema is valid",
    mcp_collect_result => "McpCollectResult", "McpCollectResult schema is valid",
    mcp_collect_runner => "McpCollectRunner", "McpCollectRunner schema is valid",
    mcp_render_result => "McpRenderResult", "McpRenderResult schema is valid",
    mcp_add_extension_enabled => "McpAddExtensionEnabled", "McpAddExtensionEnabled schema is valid",
    mcp_add_extension_installed => "McpAddExtensionInstalled", "McpAddExtensionInstalled schema is valid",
    mcp_add_extension_present => "McpAddExtensionPresent", "McpAddExtensionPresent schema is valid",
    mcp_add_extension_planned => "McpAddExtensionPlanned", "McpAddExtensionPlanned schema is valid",
    mcp_remove_extension_result => "McpRemoveExtensionResult", "McpRemoveExtensionResult schema is valid",
    mcp_stranded_entity => "McpStrandedEntity", "McpStrandedEntity schema is valid",
    mcp_migrate_result => "McpMigrateResult", "McpMigrateResult schema is valid",
    mcp_infer_progress_result => "McpInferProgressResult", "McpInferProgressResult schema is valid",
    mcp_infer_summary => "McpInferSummary", "McpInferSummary schema is valid",
    mcp_infer_session_row => "McpInferSessionRow", "McpInferSessionRow schema is valid",
    mcp_infer_gaps_result => "McpInferGapsResult", "McpInferGapsResult schema is valid",
    mcp_scan_failure => "McpScanFailure", "McpScanFailure schema is valid",
    mcp_directory_gaps => "McpDirectoryGaps", "McpDirectoryGaps schema is valid",
    mcp_gap_item => "McpGapItem", "McpGapItem schema is valid",
    mcp_infer_session_result => "McpInferSessionResult", "McpInferSessionResult schema is valid",
    mcp_implementation_result => "McpImplementationResult", "McpImplementationResult schema is valid",
    mcp_implementation => "McpImplementation", "McpImplementation schema is valid",
    mcp_spec_for_source_result => "McpSpecForSourceResult", "McpSpecForSourceResult schema is valid",
    mcp_spec_for_source_entity => "McpSpecForSourceEntity", "McpSpecForSourceEntity schema is valid",
}

#[specforge_test(
    type = "McpTraceChainResult",
    verify = "Trace tool response when entity_id is provided conforms to schema"
)]
fn mcp_trace_chain_result() {
    assert_reply_type("McpTraceChainResult")
}

#[specforge_test(
    type = "McpTracePlanResult",
    verify = "Trace tool response when plan parameter is provided conforms to schema"
)]
fn mcp_trace_plan_result() {
    assert_reply_type("McpTracePlanResult")
}
