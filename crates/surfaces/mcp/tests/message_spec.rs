//! `spec/types/mcp.spec`'s types are the schemas the documents MCP sends
//! derive (ADR 0048 D8, ADR 0058).
//!
//! Each core tool names its reply's spec type (`CoreTool::SPEC`): an `Mcp*`
//! type, a document specified elsewhere, or text. Every other document the
//! server sends is a `Message` naming its spec type
//! (`specforge_mcp::message::documents()`). The spec types reached from a
//! document's type are compared with the schema it derives:
//!
//! 1. a name in [`SHARED`] is a domain type the reply embeds: its schema is a
//!    reference to the document's one definition of that name (ADR 0051), not
//!    a field-by-field copy of the wire;
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
//!
//! An argument whose type is a spec type (`ARGUMENT_TYPES`) is compared the
//! same way: its listed schema against the type.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::OnceLock;

use serde_json::Value;
use specforge_mcp::tool::ReplySpec;
use specforge_parser::FieldValue;
use specforge_test::prelude::*;

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
        FieldValue::TypeUnion(items) if items.iter().all(|i| i.text.starts_with('"')) => Ty::Union(
            items
                .iter()
                .map(|i| i.text.trim_matches('"').to_string())
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

/// The `type` entities of `spec/types/*.spec` and the file each is
/// declared in.
struct Declared {
    types: BTreeMap<String, SpecType>,
    files: BTreeMap<String, String>,
}

fn declared() -> &'static Declared {
    static DECLARED: OnceLock<Declared> = OnceLock::new();
    DECLARED.get_or_init(|| {
        let mut types = BTreeMap::new();
        let mut files = BTreeMap::new();
        for (path, file) in crate::support::spec::spec_files("types") {
            let stem = path
                .file_name()
                .expect("a file name")
                .to_string_lossy()
                .into_owned();
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
                    Some(FieldValue::VariantList(members)) => {
                        SpecType::Alias(members.iter().map(|m| m.text.clone()).collect())
                    }
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
                files.insert(name.clone(), stem.clone());
                types.insert(name, spec);
            }
        }
        Declared { types, files }
    })
}

/// Every `type` entity of `spec/types/*.spec`.
fn spec_types() -> &'static BTreeMap<String, SpecType> {
    &declared().types
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

/// The schema a `#…` reference of `root` names; `schema` itself otherwise.
fn resolve<'v>(root: &'v Value, schema: &'v Value) -> &'v Value {
    schema
        .get("$ref")
        .and_then(Value::as_str)
        .and_then(|reference| reference.strip_prefix('#'))
        .and_then(|pointer| root.pointer(pointer))
        .unwrap_or(schema)
}

fn check_type(name: &str, root: &Value, schema: &Value, report: &mut Report) {
    report.reached.insert(name.to_string());
    if SHARED.contains(&name) {
        // A domain type another spec file declares: the reply refers to its one definition.
        let expected = serde_json::json!({ "$ref": format!("#/$defs/{name}") });
        if schema != &expected {
            report.violate(
                name,
                format!(
                    "a shared domain type is referred to as {expected}, but the schema is {}",
                    shape(schema)
                ),
            );
        }
        return;
    }
    let schema = resolve(root, schema);
    let Some(spec) = spec_types().get(name) else {
        report.violate(name, "no spec file declares this type".to_string());
        return;
    };
    match spec {
        SpecType::Alias(members) => check_alias(name, members, root, schema, report),
        SpecType::Record(fields) => check_record(name, fields, root, schema, report),
    }
}

fn check_alias(name: &str, members: &[String], root: &Value, schema: &Value, report: &mut Report) {
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
            check_type(member, root, branch, report);
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

fn check_record(
    name: &str,
    fields: &[SpecField],
    root: &Value,
    schema: &Value,
    report: &mut Report,
) {
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
            check_ty(name, &field.name, &field.ty, root, property, report);
        }
    }
}

fn check_ty(owner: &str, field: &str, ty: &Ty, root: &Value, written: &Value, report: &mut Report) {
    let schema = resolve(root, written);
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
                check_ty(owner, field, inner, root, &schema["items"], report);
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
            other => check_type(other, root, written, report),
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

/// One walk over every core tool's reply, every core prompt's payload and
/// every message the server sends: per spec type reached, its violations.
fn report() -> &'static Report {
    static REPORT: OnceLock<Report> = OnceLock::new();
    REPORT.get_or_init(|| {
        let mut report = Report::default();
        for document in specforge_mcp::message::documents() {
            check_type(document.spec, document.schema, document.schema, &mut report);
        }
        for prompt in specforge_mcp::prompts::CORE_PROMPTS {
            let schema = prompt.payload_schema();
            check_type(prompt.spec_payload(), schema, schema, &mut report);
        }
        for tool in specforge_mcp::tools::CORE_TOOLS {
            let ReplySpec::Type(name) = tool.spec_reply() else {
                continue;
            };
            // A typed spec is a typed reply: the table does not build otherwise.
            let schema = tool
                .output_schema()
                .expect("a typed reply lists its outputSchema");
            let schema = if tool.is_mutation() {
                without_files_written(schema.clone())
            } else {
                schema.clone()
            };
            check_type(name, &schema, &schema, &mut report);
        }
        report
    })
}

/// A core tool's argument whose type is a spec type: `(tool, argument, type)`.
const ARGUMENT_TYPES: &[(&str, &str, &str)] = &[("specforge.trace", "plan", "AgentPlan")];

/// One walk over [`ARGUMENT_TYPES`]: each argument's listed schema, compared
/// with the spec type it names.
fn argument_report() -> &'static Report {
    static REPORT: OnceLock<Report> = OnceLock::new();
    REPORT.get_or_init(|| {
        let mut report = Report::default();
        for (tool, argument, spec) in ARGUMENT_TYPES {
            let tool = specforge_mcp::tools::CORE_TOOLS
                .iter()
                .find(|candidate| candidate.name() == *tool)
                .unwrap_or_else(|| panic!("no core tool {tool}"));
            let listed = tool
                .arguments()
                .into_iter()
                .find(|candidate| candidate.name == *argument)
                .unwrap_or_else(|| panic!("{} lists no argument {argument}", tool.name()));
            let mut schema = listed.schema;
            if let Some(object) = schema.as_object_mut() {
                object.remove("description");
            }
            check_type(spec, &schema, &schema, &mut report);
        }
        report
    })
}

/// `name` was reached from a tool's argument and has no violation.
fn assert_argument_type(name: &str) {
    let report = argument_report();
    assert!(
        report.reached.contains(name),
        "{name} is reached from no tool argument"
    );
    let violations = report.violations.get(name).cloned().unwrap_or_default();
    assert!(
        violations.is_empty(),
        "{name}:\n  {}",
        violations.join("\n  ")
    );
}

/// `name` was reached from a core tool's reply or a message and has no
/// violation.
fn assert_reply_type(name: &str) {
    let report = report();
    assert!(
        report.reached.contains(name),
        "{name} is reached from no core tool's reply and no message"
    );
    let violations = report.violations.get(name).cloned().unwrap_or_default();
    assert!(
        violations.is_empty(),
        "{name}:\n  {}",
        violations.join("\n  ")
    );
}

#[specforge_test(
    behavior = "state_mcp_messages_as_spec_types",
    verify = "every Mcp type of spec/types/mcp.spec is a tool reply, a prompt payload or a document the server sends"
)]
fn every_mcp_type_is_sent() {
    // The types declared in spec/types/mcp.spec, all reached. The guest-wire
    // types (ADR 0017) are declared in surface.spec and wasm.spec: received
    // from extensions, never sent.
    let report = report();
    let unreached: Vec<&String> = declared()
        .files
        .iter()
        .filter(|(name, file)| {
            file.as_str() == "mcp.spec"
                && name.starts_with("Mcp")
                && !report.reached.contains(*name)
        })
        .map(|(name, _)| name)
        .collect();
    assert!(
        unreached.is_empty(),
        "Mcp types of mcp.spec no tool reply, prompt payload or message reaches: {unreached:?}"
    );
}

#[specforge_test(
    behavior = "serve_mcp_prompt",
    verify = "each prompt's payload is the spec type its module names"
)]
fn every_prompt_payload_is_its_spec_type() {
    let report = report();
    for prompt in specforge_mcp::prompts::CORE_PROMPTS {
        let violations = report
            .violations
            .get(prompt.spec_payload())
            .cloned()
            .unwrap_or_default();
        assert!(
            report.reached.contains(prompt.spec_payload()) && violations.is_empty(),
            "{}:\n  {}",
            prompt.spec_payload(),
            violations.join("\n  ")
        );
    }
}

#[specforge_test(
    behavior = "state_mcp_messages_as_spec_types",
    verify = "every document the server sends besides a tool reply is the schema its type derives, as its spec type states it"
)]
fn every_sent_document_is_its_spec_type() {
    let report = report();
    for document in specforge_mcp::message::documents() {
        let violations = report
            .violations
            .get(document.spec)
            .cloned()
            .unwrap_or_default();
        assert!(
            report.reached.contains(document.spec) && violations.is_empty(),
            "{}:\n  {}",
            document.spec,
            violations.join("\n  ")
        );
    }
}

macro_rules! message_type {
    ($($test:ident => $name:literal, $verify:literal),* $(,)?) => {$(
        #[specforge_test(type = $name, verify = $verify)]
        fn $test() {
            assert_reply_type($name)
        }
    )*};
}

message_type! {
    mcp_initialize_result => "McpInitializeResult", "McpInitializeResult schema is valid",
    mcp_discover_result => "McpDiscoverResult", "McpDiscoverResult schema is valid",
    mcp_server_capabilities => "McpServerCapabilities", "McpServerCapabilities schema is valid",
    mcp_list_changed => "McpListChanged", "McpListChanged schema is valid",
    mcp_resources_capability => "McpResourcesCapability", "McpResourcesCapability schema is valid",
    mcp_server_info => "McpServerInfo", "McpServerInfo schema is valid",
    mcp_tool_descriptor => "McpToolDescriptor", "McpToolDescriptor schema is valid",
    mcp_tool_annotations => "McpToolAnnotations", "McpToolAnnotations schema is valid",
    mcp_resource_descriptor => "McpResourceDescriptor", "McpResourceDescriptor schema is valid",
    mcp_resource_template_descriptor => "McpResourceTemplateDescriptor", "McpResourceTemplateDescriptor schema is valid",
    mcp_prompt_descriptor => "McpPromptDescriptor", "McpPromptDescriptor schema is valid",
    mcp_prompt_argument => "McpPromptArgument", "McpPromptArgument schema is valid",
    mcp_context_prompt_result => "McpContextPromptResult", "Context Prompt Result conforms to schema",
    mcp_context_constraint => "McpContextConstraint", "McpContextConstraint schema is valid",
    mcp_review_prompt_result => "McpReviewPromptResult", "Review Prompt Result conforms to schema",
    mcp_review_finding => "McpReviewFinding", "McpReviewFinding schema is valid",
    mcp_trace_prompt_result => "McpTracePromptResult", "Trace Prompt Result conforms to schema",
    mcp_trace_gap => "McpTraceGap", "McpTraceGap schema is valid",
    mcp_explore_prompt_result => "McpExplorePromptResult", "Explore Prompt Result conforms to schema",
    mcp_relationship_path => "McpRelationshipPath", "McpRelationshipPath schema is valid",
    mcp_infer_overview => "McpInferOverview", "Infer Prompt Result conforms to schema",
    mcp_infer_guide_kind => "McpInferGuideKind", "McpInferGuideKind schema is valid",
    mcp_infer_kind_guide => "McpInferKindGuide", "McpInferKindGuide schema is valid",
    mcp_infer_guide_field => "McpInferGuideField", "McpInferGuideField schema is valid",
    mcp_infer_file_guide => "McpInferFileGuide", "McpInferFileGuide schema is valid",
    mcp_infer_file_kind => "McpInferFileKind", "McpInferFileKind schema is valid",
    mcp_infer_plan => "McpInferPlan", "McpInferPlan schema is valid",
    mcp_infer_kind_priority => "McpInferKindPriority", "McpInferKindPriority schema is valid",
    mcp_infer_workflow => "McpInferWorkflow", "McpInferWorkflow schema is valid",
    graph_delta => "GraphDelta", "GraphDelta schema is valid",
    node_change => "NodeChange", "NodeChange schema is valid",
    modified_node_change => "ModifiedNodeChange", "ModifiedNodeChange schema is valid",
    edge_change => "EdgeChange", "EdgeChange schema is valid",
    diagnostics_delta => "DiagnosticsDelta", "DiagnosticsDelta schema is valid",
}

#[specforge_test(
    type = "McpError",
    verify = "MCP Structured Error Response conforms to schema"
)]
fn mcp_error() {
    assert_reply_type("McpError")
}

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
    annotation_json => "AnnotationJson", "AnnotationJson schema is valid",
    parameter_json => "ParameterJson", "ParameterJson schema is valid",
    method_json => "MethodJson", "MethodJson schema is valid",
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
    mcp_edit_result => "McpEditResult", "McpEditResult schema is valid",
    mcp_rename_result => "McpRenameResult", "McpRenameResult schema is valid",
    text_edit => "TextEdit", "TextEdit schema is valid",
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
    mcp_add_extension_result => "McpAddExtensionResult", "McpAddExtensionResult schema is valid",
    mcp_add_extension_duplicate => "McpAddExtensionDuplicate", "McpAddExtensionDuplicate schema is valid",
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

#[specforge_test(type = "AgentPlan", verify = "AgentPlan schema is valid")]
#[ignore = "fixed by T5"]
fn agent_plan() {
    assert_argument_type("AgentPlan")
}

#[specforge_test(type = "AgentPlanEntry", verify = "AgentPlanEntry schema is valid")]
#[ignore = "fixed by T5"]
fn agent_plan_entry() {
    assert_argument_type("AgentPlanEntry")
}
