//! The schema operation (`specforge schema`, `specforge.schema`): the
//! document it answers and the JSON Schema it publishes. Moved from the
//! emitter's serve-schema tests when the CLI began printing the
//! operation's outcome (ADR 0027's round, ADR 0015 D8).

use serde_json::{Value, json};
use specforge_emitter::GraphProtocolSchema;
use specforge_ops::read::export::Format;
use specforge_ops::read::schema::{SchemaRequest, json_schema, schema};
use specforge_protocol_types::{
    EdgeTypeDescriptor, EntityKindDescriptor, ExtensionDeclaration, FieldDescriptor,
    HandshakeResponse, ValidationRuleDescriptor,
};
use specforge_registry::build_registries;
use specforge_test_macros::test as specforge_test;

use crate::view_support::Project;

/// A project whose one extension, `@t/soft` 1.0.0, declares `behavior`
/// (testable, a `contract` and a `features` reference), `feature` and
/// `event`; `implements` runs behavior → feature, `emits` feature →
/// event; one validation rule targets behavior.
fn project() -> Project {
    let field = |name: &str, field_type: &str| FieldDescriptor {
        name: name.into(),
        field_type: field_type.into(),
        ..FieldDescriptor::default()
    };
    let edge = |label: &str, source: &str, target: &str| EdgeTypeDescriptor {
        label: label.into(),
        source_kind: Some(source.into()),
        target_kind: Some(target.into()),
        ..EdgeTypeDescriptor::default()
    };
    let declaration = ExtensionDeclaration {
        handshake: HandshakeResponse {
            name: "@t/soft".into(),
            version: "1.0.0".into(),
            ..HandshakeResponse::default()
        },
        entities: vec![
            EntityKindDescriptor {
                name: "behavior".into(),
                testable: true,
                supports_verify: true,
                fields: vec![
                    field("contract", "string"),
                    FieldDescriptor {
                        edge: Some("implements".into()),
                        target_kind: Some("feature".into()),
                        ..field("features", "reference_list")
                    },
                ],
                ..EntityKindDescriptor::default()
            },
            EntityKindDescriptor {
                name: "feature".into(),
                ..EntityKindDescriptor::default()
            },
            EntityKindDescriptor {
                name: "event".into(),
                ..EntityKindDescriptor::default()
            },
        ],
        edges: vec![
            edge("implements", "behavior", "feature"),
            edge("emits", "feature", "event"),
        ],
        validation_rules: vec![ValidationRuleDescriptor {
            code: "W901".into(),
            message_template: "{id} has no contract".into(),
            check: "field_required".into(),
            target_kind: Some("behavior".into()),
            field: Some("contract".into()),
            ..ValidationRuleDescriptor::default()
        }],
        ..ExtensionDeclaration::default()
    };
    Project::new("", build_registries(vec![declaration]))
}

/// The labels of a document's edge types.
fn labels(document: &Value) -> Vec<&str> {
    let mut labels: Vec<&str> = document["edge_types"]
        .as_array()
        .unwrap()
        .iter()
        .map(|edge| edge["label"].as_str().unwrap())
        .collect();
    labels.sort_unstable();
    labels
}

#[specforge_test(
    behavior = "serve_schema_resource",
    verify = "specforge schema outputs full schema as JSON"
)]
fn the_outcome_serializes_the_whole_schema() {
    let project = project();
    let outcome = schema(&project.view(), &SchemaRequest::default()).unwrap();
    let text = serde_json::to_string_pretty(&outcome).unwrap();
    let parsed: Value = serde_json::from_str(&text).unwrap();

    assert_eq!(
        parsed["extensions"],
        json!([{ "name": "@t/soft", "version": "1.0.0" }])
    );
    let mut kinds: Vec<&str> = parsed["entity_kinds"]
        .as_array()
        .unwrap()
        .iter()
        .map(|k| k["name"].as_str().unwrap())
        .collect();
    kinds.sort_unstable();
    assert_eq!(kinds, ["behavior", "event", "feature"]);
    let behavior = parsed["entity_kinds"]
        .as_array()
        .unwrap()
        .iter()
        .find(|k| k["name"] == "behavior")
        .unwrap();
    assert_eq!(behavior["source_extension"], "@t/soft");
    assert_eq!(behavior["testable"], true);
    assert!(
        behavior["fields"]
            .as_array()
            .unwrap()
            .iter()
            .any(|f| f["name"] == "contract" && f["field_type"] == "string"),
        "{behavior}"
    );
    assert_eq!(labels(&parsed), ["emits", "implements"]);
    assert!(parsed.get("validation_rules").is_none(), "not asked for");

    // Nothing is lost: it reads back as the schema, and is the schema's own
    // serialization, keys in order.
    let back: GraphProtocolSchema = serde_json::from_str(&text).unwrap();
    assert_eq!(back, outcome.schema);
    assert_eq!(text, serde_json::to_string_pretty(&outcome.schema).unwrap());
}

#[specforge_test(
    behavior = "serve_schema_resource",
    verify = "--kind filter restricts to single entity kind"
)]
fn a_kind_keeps_its_entry_and_the_edges_that_touch_it() {
    let project = project();
    for (kind, edges) in [
        ("behavior", vec!["implements"]),
        ("feature", vec!["emits", "implements"]),
        ("event", vec!["emits"]),
    ] {
        let request = SchemaRequest {
            kind: Some(kind),
            ..SchemaRequest::default()
        };
        let document = serde_json::to_value(schema(&project.view(), &request).unwrap()).unwrap();
        let entries = document["entity_kinds"].as_array().unwrap();
        assert_eq!(entries.len(), 1, "{kind}: {document}");
        assert_eq!(entries[0]["name"], kind);
        assert_eq!(labels(&document), edges, "{kind}");
    }
}

#[specforge_test(behavior = "serve_schema_resource", verify = "missing kind error")]
fn an_unknown_kind_is_refused_naming_the_closest() {
    let project = project();
    let request = SchemaRequest {
        kind: Some("behaviour"),
        ..SchemaRequest::default()
    };
    let error = schema(&project.view(), &request).unwrap_err();
    assert_eq!(error.error.code, "unknown_kind");
    assert_eq!(error.error.message, "unknown entity kind 'behaviour'");
    assert_eq!(
        error.error.suggestion.as_deref(),
        Some("did you mean 'behavior'?")
    );
}

#[specforge_test(
    behavior = "serve_schema_resource",
    verify = "Serve Schema Resource: schema resource serving holds — validation_complete_fired, full_schema_output, kind_filter_supported, mcp_resource_available"
)]
fn serve_schema_contract() {
    let project = project();
    let view = project.view();

    // ensures: full_schema_output
    let full = serde_json::to_value(schema(&view, &SchemaRequest::default()).unwrap()).unwrap();
    assert_eq!(full["entity_kinds"].as_array().unwrap().len(), 3);
    assert!(full["edge_types"].is_array());

    // ensures: kind_filter_supported
    let request = SchemaRequest {
        kind: Some("behavior"),
        ..SchemaRequest::default()
    };
    let filtered = serde_json::to_value(schema(&view, &request).unwrap()).unwrap();
    assert_eq!(filtered["entity_kinds"][0]["name"], "behavior");

    // Without edges, with the rules: what include_edges and
    // include_validation_rules select.
    let request = SchemaRequest {
        edges: false,
        validation_rules: true,
        ..SchemaRequest::default()
    };
    let trimmed = serde_json::to_value(schema(&view, &request).unwrap()).unwrap();
    assert!(trimmed.get("edge_types").is_none(), "{trimmed}");
    assert_eq!(trimmed["validation_rules"][0]["code"], "W901");
    assert_eq!(trimmed["validation_rules"][0]["extension"], "@t/soft");

    // error for unknown kind
    let request = SchemaRequest {
        kind: Some("nonexistent"),
        ..SchemaRequest::default()
    };
    assert!(schema(&view, &request).is_err());
}

#[test]
fn json_schema_refuses_dot() {
    let project = project();
    let error = json_schema(&project.view(), Format::Dot).unwrap_err();
    assert_eq!(error.code, "invalid_input");
    assert_eq!(
        error.message,
        "Unknown format: dot. Expected: graph, context, brief"
    );

    let published: Value = json_schema(&project.view(), Format::Graph).unwrap();
    assert_eq!(
        published["$schema"],
        "https://json-schema.org/draft/2020-12/schema"
    );
}

#[specforge_test(
    invariant = "mcp_structured_error_responses",
    verify = "an operation states the request field each refusal is about, and a refusal of the project or the environment is about none"
)]
fn schema_states_which_field_a_refusal_is_about() {
    use specforge_ops::read::schema::SchemaField;
    let request = SchemaRequest {
        kind: Some("behaviour"),
        ..SchemaRequest::default()
    };
    let refusal = schema(&project().view(), &request).unwrap_err();
    assert_eq!(refusal.subject, Some(SchemaField::Kind));
}

/// The end-to-end schema tests, which `specforge-cli` ran through the binary: the same requests, over a compiled
/// project. The `published-schemas` gate check runs this module.
mod published {
    use crate::compiled_support::{
        TestProject, built, checked, exported, exported_text, exported_with, published, schema_of,
        specs,
    };
    use specforge_ops::analysis::check::CheckOptions;
    use specforge_ops::read::export::{FORMAT, Format, Request, Schema};
    use specforge_ops::read::schema::{self, SchemaRequest};
    use specforge_ops::view::ProjectView;
    use specforge_project::CompiledProject;
    use specforge_test_macros::test as specforge_test;
    use std::collections::BTreeSet;

    /// SOFTWARE in a project that enables @specforge/software, so the
    /// schema has registered kinds and edge types.
    fn software_project() -> (TestProject, CompiledProject) {
        let project = TestProject::new()
            .enabling(&["@specforge/software"])
            .config(|c| c["spec_root"] = "spec".into())
            .file("spec/main.spec", specs::SOFTWARE);
        let compiled = project.compile();
        (project, compiled)
    }

    /// The kinds @specforge/software registers.
    const SOFTWARE_KINDS: [&str; 5] = ["behavior", "event", "invariant", "port", "type"];

    /// The format `name` is, as `--format` parses it.
    fn format_named(name: &str) -> Format {
        FORMAT
            .parse(name)
            .unwrap_or_else(|_| panic!("a format: {name}"))
    }

    /// `specforge export --format <name>`, parsed.
    fn export_in(project: &CompiledProject, name: &str) -> serde_json::Value {
        exported_with(
            project,
            &Request {
                format: Some(format_named(name)),
                ..Default::default()
            },
        )
    }

    /// `specforge schema --publish --format <name>`.
    fn published_in(project: &CompiledProject, name: &str) -> serde_json::Value {
        published(project, format_named(name))
    }

    fn strings(value: &serde_json::Value) -> BTreeSet<String> {
        value
            .as_array()
            .unwrap_or_else(|| panic!("not an array: {value}"))
            .iter()
            .map(|v| v.as_str().unwrap().to_string())
            .collect()
    }

    fn names(list: &serde_json::Value, key: &str) -> BTreeSet<String> {
        list.as_array()
            .unwrap()
            .iter()
            .map(|v| v[key].as_str().unwrap().to_string())
            .collect()
    }

    /// Whether `specforge schema --kind <kind>` is refused (exit 1).
    fn schema_refused(project: &CompiledProject, kind: &str) -> bool {
        schema::schema(
            &ProjectView::of(project),
            &SchemaRequest {
                kind: Some(kind),
                ..Default::default()
            },
        )
        .is_err()
    }

    // --- Phase 1d: Schema command tests ---

    #[specforge_test(
        behavior = "serve_schema_resource",
        verify = "specforge schema outputs full schema as JSON"
    )]
    fn schema_command_outputs_valid_json() {
        let (_project, compiled) = software_project();
        let parsed = schema_of(&compiled, &SchemaRequest::default());

        assert_eq!(
            parsed["schema_version"],
            serde_json::json!({"major": 1, "minor": 0, "patch": 0})
        );
        assert_eq!(
            names(&parsed["extensions"], "name"),
            strings(&serde_json::json!(["@specforge/software"]))
        );
        // Every kind the extension registers, with its field definitions.
        assert_eq!(
            names(&parsed["entity_kinds"], "name"),
            SOFTWARE_KINDS.iter().map(|s| s.to_string()).collect()
        );
        let behavior = parsed["entity_kinds"]
            .as_array()
            .unwrap()
            .iter()
            .find(|k| k["name"] == "behavior")
            .unwrap();
        let contract = behavior["fields"]
            .as_array()
            .unwrap()
            .iter()
            .find(|f| f["name"] == "contract")
            .expect("behavior.contract");
        assert_eq!(contract["field_type"], "string");
        assert_eq!(contract["source_extension"], "@specforge/software");
        // And its edge types.
        let edge_types = names(&parsed["edge_types"], "label");
        for label in [
            "BehaviorEnforcesInvariant",
            "BehaviorProducesEvent",
            "BehaviorUsesPort",
        ] {
            assert!(
                edge_types.contains(label),
                "{label} missing: {edge_types:?}"
            );
        }
    }

    #[test]
    fn schema_kind_filter_returns_single_kind() {
        // Note: with GraphProtocolSchema::empty(), entity_kinds is empty.
        // This test verifies the --kind flag behavior: unknown kind exits 1
        let (_project, compiled) = built(&[("main.spec", specs::SOFTWARE)]);

        // Any kind filter on empty schema is refused (kind not found; the command exits 1)
        assert!(schema_refused(&compiled, "behavior"));
    }

    #[test]
    fn schema_kind_filter_unknown_exits_one() {
        let (_project, compiled) = built(&[("main.spec", specs::SOFTWARE)]);

        assert!(schema_refused(&compiled, "nonexistent_kind"));
    }

    #[specforge_test(
        behavior = "publish_schema_specification",
        verify = "published schema is valid JSON Schema"
    )]
    fn schema_publish_produces_json_schema_draft() {
        let (_software, with_software) = software_project();
        let (_bare, bare) = built(&[("main.spec", specs::SOFTWARE)]);
        for compiled in [&with_software, &bare] {
            let parsed = published(compiled, Format::Graph);
            assert_eq!(
                parsed["$schema"], "https://json-schema.org/draft/2020-12/schema",
                "published schema should have $schema field"
            );
            assert_eq!(parsed["title"], "SpecForge Graph Protocol");
            // Every keyword is one the in-repo checker reads, or an annotation.
            assert_eq!(
                specforge_common::shape::unchecked_keywords(&parsed),
                Vec::<String>::new()
            );
        }
    }

    /// A project with an edge from nearly every reference field the software,
    /// product and governance extensions declare.
    const EDGES_SPEC: &str = r#"
invariant graph_acyclic "Graph Acyclicity" {
  guarantee "The dependency graph MUST be acyclic"
}

type parse_result "Parse Result" {
  description "Result of a parse"
}

type typed_result "Typed Result" {
  description    "A parse result with types"
  extends        parse_result
  composed_types [parse_result]
}

port file_reader "File Reader" {
  direction inbound
}

event input_parsed "Input Parsed" {
  payload parse_result
}

behavior parse_input "Parse Input" {
  contract   "The system MUST parse all valid input"
  invariants [graph_acyclic]
  types      [parse_result]
  ports      [file_reader]
  produces   [input_parsed]
  features   [fast_parsing]
}

behavior emit_output "Emit Output" {
  contract "The system MUST emit structured output"
  consumes [input_parsed]
}

feature fast_parsing "Fast Parsing" {
  problem    "Users need quick feedback"
  solution   "Incremental parsing"
  depends_on [slow_parsing]
}

feature slow_parsing "Slow Parsing" {
  problem  "Some input is large"
  solution "Batch parsing"
}

persona developer "Developer" {
  description  "Writes specs"
  key_features [fast_parsing]
}

journey developer_workflow "Developer Workflow" {
  description "A developer writes and validates specs"
  flow        ["write a spec", "run check"]
  persona     developer
  features    [fast_parsing]
}

module parser_module "Parser Module" {
  description   "Handles .spec file parsing"
  features      [fast_parsing]
  ports_defined [file_reader]
}

milestone v1_release "V1 Release" {
  description "First stable release"
  behaviors   [parse_input]
  features    [fast_parsing]
  modules     [parser_module]
}

deliverable cli_tool "CLI Tool" {
  description   "The specforge CLI binary"
  artifact_type "cli"
  journeys      [developer_workflow]
  modules       [parser_module]
  milestones    [v1_release]
}

term spec_file "Spec File" {
  definition "A .spec file"
  module     parser_module
}

decision use_treesitter "Use Tree-Sitter" {
  status           accepted
  context          "We need a parser"
  decision         "Use tree-sitter"
  consequences     ["Fast incremental parsing"]
  invariants       [graph_acyclic]
  affects_features [fast_parsing]
}

constraint parse_latency "Parse Latency" {
  category    performance
  priority    critical
  description "Parsing MUST finish in 100ms"
  constrains  [parse_input]
  protects    [graph_acyclic]
}

failure_mode parser_crash "Parser Crash" {
  description        "The parser panics on bad input"
  cause              "malformed input"
  effect             "no output"
  severity           "high"
  affected_behaviors [parse_input]
  invariant          graph_acyclic
  threatens_features [fast_parsing]
}
"#;

    #[specforge_test(
        behavior = "publish_schema_specification",
        verify = "published schema validates known-good export"
    )]
    fn schema_publish_validates_a_real_export() {
        let project = TestProject::new()
            .enabling(&[
                "@specforge/software",
                "@specforge/product",
                "@specforge/governance",
            ])
            .config(|c| c["spec_root"] = "spec".into())
            .files(&[("spec/main.spec", EDGES_SPEC)]);
        let compiled = project.compile();
        let published = published(&compiled, Format::Graph);
        let export = exported(&compiled);

        // The export carries an edge for every reference field the spec fills,
        // each labelled with the field's name.
        let labels = names(&export["edges"], "label");
        assert_eq!(
            labels,
            strings(&serde_json::json!([
                "affected_behaviors",
                "affects_features",
                "behaviors",
                "composed_types",
                "constrains",
                "consumes",
                "depends_on",
                "extends",
                "features",
                "invariant",
                "invariants",
                "journeys",
                "key_features",
                "milestones",
                "module",
                "modules",
                "payload",
                "persona",
                "ports",
                "ports_defined",
                "produces",
                "protects",
                "threatens_features",
                "types"
            ]))
        );

        let errors = conforms(&published, &export);
        assert!(
            errors.is_empty(),
            "the export does not validate against the published schema:\n{}",
            errors.join("\n")
        );

        // The checker does reject what the schema forbids.
        let mut broken = export.clone();
        broken["edges"][0]["label"] = serde_json::json!(7);
        broken["nodes"][0]["kind"] = serde_json::json!("no_such_kind");
        let rejected = conforms(&published, &broken);
        assert_eq!(rejected.len(), 2, "{rejected:?}");
        assert!(
            rejected.contains(&"$.edges[0].label: expected string, got integer".to_string()),
            "{rejected:?}"
        );
        assert!(
            rejected
                .iter()
                .any(|m| m.starts_with("$.nodes[0]: matches 0 of the ")
                    && m.ends_with(" oneOf schemas")),
            "{rejected:?}"
        );
    }

    #[specforge_test(
        behavior = "check_field_value_types",
        verify = "an export with a coerced string_list validates against the published schema"
    )]
    fn schema_publish_validates_an_export_with_a_coerced_string_list() {
        let project = TestProject::new()
            .enabling(&["@specforge/software", "@specforge/governance"])
            .config(|c| c["spec_root"] = "spec".into())
            .files(&[(
                "spec/main.spec",
                r#"decision d1 "D" {
  status accepted
  context "c"
  decision "x"
  consequences "just a string"
}
failure_mode fm1 "F" {
  cause "disk full"
  effect "writes fail"
  severity high
  rpn "12"
}
"#,
            )]);
        let compiled = project.compile();
        let published = published(&compiled, Format::Graph);
        let export = exported(&compiled);

        let errors = conforms(&published, &export);
        assert!(
            errors.is_empty(),
            "the export does not validate against the published schema:\n{}",
            errors.join("\n")
        );
    }

    #[specforge_test(
        behavior = "publish_schema_specification",
        verify = "published schema describes all registered entity kinds"
    )]
    fn schema_publish_includes_node_kind_enum() {
        let (_project, compiled) = software_project();
        let parsed = published(&compiled, Format::Graph);

        // One node branch per kind an export can hold: the registered ones and
        // the structural `ref` and `spec`.
        let kinds = published_node_kinds(&parsed);
        let structural: BTreeSet<String> = specforge_common::structural::NODE_KINDS
            .iter()
            .map(|k| k.to_string())
            .collect();
        let declared: BTreeSet<String> = kinds.difference(&structural).cloned().collect();
        assert_eq!(
            declared,
            SOFTWARE_KINDS.iter().map(|s| s.to_string()).collect(),
            "the node branches list every registered kind"
        );
        // The same set `specforge schema` reports as registered.
        let registered = schema_of(&compiled, &SchemaRequest::default());
        assert_eq!(declared, names(&registered["entity_kinds"], "name"));
    }

    #[specforge_test(
        behavior = "publish_schema_specification",
        verify = "published schema describes all edge types"
    )]
    fn schema_publish_includes_edge_label_enum() {
        let (_project, compiled) = software_project();
        let parsed = published(&compiled, Format::Graph);

        // The embedded schema block names every registered edge type, no more.
        let edge_type =
            &parsed["$defs"]["GraphProtocolSchema"]["properties"]["edge_types"]["items"];
        let label = &edge_type["properties"]["label"];
        assert_eq!(label["type"], "string");
        let listed = strings(&label["enum"]);
        let registered = schema_of(&compiled, &SchemaRequest::default());
        assert_eq!(listed, names(&registered["edge_types"], "label"));
        for expected in [
            "BehaviorEnforcesInvariant",
            "BehaviorProducesEvent",
            "BehaviorConsumesEvent",
            "BehaviorUsesPort",
            "EventCarriesPayloadType",
        ] {
            assert!(listed.contains(expected), "{expected} missing: {listed:?}");
        }

        // A graph edge is labelled by the field that declared it. The label is
        // open, and the fields that declare an edge type are its examples.
        let edge_label = &parsed["$defs"]["Edge"]["properties"]["label"];
        assert_eq!(edge_label["type"], "string");
        assert!(edge_label.get("enum").is_none(), "{edge_label}");
        let declaring: BTreeSet<String> = registered["entity_kinds"]
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|k| k["fields"].as_array().unwrap())
            .filter(|f| f["edge"].is_string())
            .map(|f| f["name"].as_str().unwrap().to_string())
            .collect();
        assert_eq!(
            declaring,
            strings(&serde_json::json!([
                "composed_types",
                "consumes",
                "extends",
                "features",
                "invariants",
                "payload",
                "ports",
                "produces",
                "types"
            ]))
        );
        assert_eq!(strings(&edge_label["examples"]), declaring);
    }

    #[specforge_test(
        behavior = "embed_schema_in_export",
        verify = "schema embedded as top-level key in full JSON export"
    )]
    fn export_v2_schema_has_entity_kinds() {
        let (_project, compiled) = built(&[("main.spec", specs::SOFTWARE)]);

        let parsed = exported(&compiled);
        assert_eq!(parsed["format_version"], "2.0");
        assert!(
            parsed["schema"]["entity_kinds"].is_array(),
            "V2 schema should have entity_kinds"
        );
    }

    #[specforge_test(
        behavior = "embed_schema_in_export",
        verify = "format_version set to 2.0 with schema"
    )]
    fn export_v2_schema_has_edge_types() {
        let (_project, compiled) = software_project();
        let parsed = exported(&compiled);

        // Format 2.0, carrying the project's schema.
        assert_eq!(parsed["format_version"], "2.0");
        let registered = schema_of(&compiled, &SchemaRequest::default());
        assert_eq!(
            parsed["schema"], registered,
            "the embedded schema is the project's"
        );
        assert!(
            names(&parsed["schema"]["edge_types"], "label").contains("BehaviorEnforcesInvariant"),
            "{}",
            parsed["schema"]["edge_types"]
        );

        // Without the schema, the export is not 2.0.
        let bare = exported_with(
            &compiled,
            &Request {
                format: Some(Format::Graph),
                schema: Schema::Without,
                ..Default::default()
            },
        );
        assert!(bare.get("schema").is_none());
        assert_ne!(bare["format_version"], "2.0");
    }

    #[specforge_test(
        behavior = "negotiate_schema_version",
        verify = "a requested version of the same major, not newer, is admitted and the export carries the current version"
    )]
    fn export_schema_version_negotiation() {
        let (_project, compiled) = built(&[("main.spec", specs::SOFTWARE)]);
        let versioned = |version: &'static str| {
            exported_text(
                &compiled,
                &Request {
                    format: Some(Format::Graph),
                    schema_version: Some(version),
                    ..Default::default()
                },
            )
        };

        // Valid version (1.0.0 matches the current schema version)
        assert!(versioned("1.0.0").is_ok(), "1.0.0 should be accepted");

        // Another major version: refused (the command exits 1 with `error[incompatible_schema_version]`)
        let refused = versioned("99.0.0").expect_err("another major version is refused");
        assert_eq!(refused.error.code, "incompatible_schema_version");
    }

    #[specforge_test(
        behavior = "embed_schema_in_export",
        verify = "scoped exports carry schema_ref (url and content_hash) instead of embedded schema"
    )]
    fn export_scoped_v2_references_schema() {
        let (_project, compiled) = built(&[(
            "main.spec",
            r#"
behavior alpha "A" { contract "first" }
feature gamma "G" { behaviors [alpha] }
"#,
        )]);

        let parsed = exported_with(
            &compiled,
            &Request {
                format: Some(Format::Graph),
                scope: Some("alpha"),
                ..Default::default()
            },
        );
        assert_eq!(parsed["format_version"], "2.0");
        assert!(
            parsed.get("schema").is_none(),
            "scoped V2 must not embed the full schema"
        );
        assert!(
            parsed["schema_ref"]["url"].is_string(),
            "scoped V2 carries a schema_ref url"
        );
        assert_eq!(
            parsed["schema_ref"]["content_hash"].as_str().unwrap().len(),
            64,
            "schema_ref content_hash is a sha256 hex digest"
        );
    }

    #[test]
    fn schema_publish_describes_the_requested_format() {
        let project = TestProject::new()
            .config(|c| {
                c["name"] = "s".into();
                c["spec_root"] = "src".into();
            })
            .enabling(&["@specforge/product"])
            .file("src/a.spec", "type W { id string @unique }");
        let compiled = project.compile();

        let run = |fmt: &str| {
            let v = published_in(&compiled, fmt);
            let items = v["properties"]["nodes"]["items"].clone();
            match items["oneOf"].as_array().and_then(|b| b.first()) {
                // The first branch of a graph document, resolved.
                Some(first) => {
                    let name = first["$ref"]
                        .as_str()
                        .unwrap()
                        .trim_start_matches("#/$defs/");
                    let mut resolved = v["$defs"][name].clone();
                    resolved["oneOf"] = items["oneOf"].clone();
                    resolved
                }
                None => items,
            }
        };

        // A graph node is one branch per kind (`full` is the first, with the
        // branch list), each requiring the full node.
        let full = run("graph");
        assert!(
            full["oneOf"]
                .as_array()
                .is_some_and(|branches| branches.len() > 2),
            "graph schema has a branch per kind"
        );
        assert_eq!(
            full["required"],
            serde_json::json!(["id", "kind", "file", "line", "fields"]),
            "graph schema describes full nodes"
        );

        let context = run("context");
        assert_eq!(
            context["required"],
            serde_json::json!(["id", "kind"]),
            "context schema describes context nodes (no file/line/fields required)"
        );
        assert!(
            context["properties"].get("verify").is_some(),
            "context schema knows the verify field"
        );
        assert!(context["properties"].get("file").is_none());

        let brief = run("brief");
        assert_eq!(
            brief["properties"]
                .as_object()
                .map(|o| o.keys().cloned().collect::<Vec<_>>()),
            Some(vec![
                "coverage_status".into(),
                "id".into(),
                "kind".into(),
                "title".into()
            ]),
            "brief schema exposes exactly id/kind/title and the optional coverage_status"
        );
    }

    // --- Exports validate against what is published (ADR 0064, plan 01) ---

    /// The messages for which `export` does not conform to `published`: the
    /// published schema's own keywords are held to the checker's, the export is
    /// checked against it, and a node that fails is checked against its kind's
    /// branch so a message names the field.
    fn conforms(published: &serde_json::Value, export: &serde_json::Value) -> Vec<String> {
        use specforge_common::shape::{unchecked_keywords, violations};
        let unchecked = unchecked_keywords(published);
        assert!(
            unchecked.is_empty(),
            "keywords the checker ignores: {unchecked:?}"
        );
        let mut messages = violations(published, export);
        if messages.is_empty() {
            return messages;
        }
        for (i, node) in export["nodes"].as_array().into_iter().flatten().enumerate() {
            let Some(kind) = node["kind"].as_str() else {
                continue;
            };
            if published["$defs"].get(format!("node_{kind}")).is_none() {
                continue;
            }
            let branch = serde_json::json!({
                "$ref": format!("#/$defs/node_{kind}"),
                "$defs": published["$defs"],
            });
            messages.extend(
                violations(&branch, node)
                    .into_iter()
                    .map(|m| format!("$.nodes[{i}]{}", &m[1..])),
            );
        }
        messages
    }

    /// The kinds a graph document's node branches name.
    fn published_node_kinds(published: &serde_json::Value) -> BTreeSet<String> {
        published["properties"]["nodes"]["items"]["oneOf"]
            .as_array()
            .expect("a graph document has a branch per kind")
            .iter()
            .map(|branch| {
                branch["$ref"]
                    .as_str()
                    .and_then(|r| r.strip_prefix("#/$defs/node_"))
                    .expect("a node branch")
                    .to_string()
            })
            .collect()
    }

    /// A copy of `<repo>/<rel>`; for the repository itself, only `specforge.json` and `spec/`. `export` records the
    /// schema cache in the project it exports, so a test never exports a committed directory.
    fn copy_project(rel: &str) -> TestProject {
        let repo = specforge_repo_root::repo_root();
        if rel == "." {
            TestProject::new()
                .raw_config(std::fs::read(repo.join("specforge.json")).unwrap())
                .copy_in(repo.join("spec"), "spec")
        } else {
            TestProject::copy_of(repo.join(rel))
        }
    }

    /// The projects whose exports must validate against what they publish.
    const PUBLISHED_PROJECTS: [&str; 9] = [
        ".",
        "integrations/rust/spec",
        "examples/todo-app",
        "examples/shop",
        "crates/surfaces/cli/tests/fixtures/parity/clean",
        "crates/surfaces/cli/tests/fixtures/parity/body_parser_type",
        "crates/surfaces/cli/tests/fixtures/read_views/rv1",
        "crates/surfaces/cli/tests/fixtures/coverage/fx1",
        "xtask/tests/fixtures/links",
    ];

    /// Every named format's export of the project against the format's published
    /// schema; one message per violation, prefixed by the format.
    fn export_violations(project: &CompiledProject, formats: &[&str]) -> Vec<String> {
        let mut messages = Vec::new();
        for format in formats {
            let published = published_in(project, format);
            let export = export_in(project, format);
            messages.extend(
                conforms(&published, &export)
                    .into_iter()
                    .map(|m| format!("{format}: {m}")),
            );
        }
        messages
    }

    #[specforge_test(
        behavior = "publish_schema_specification",
        verify = "a context or brief export validates against the published schema of its format"
    )]
    fn agent_exports_validate_against_their_published_schema() {
        let project = TestProject::new()
        .enabling(&["@specforge/software"])
        .config(|c| c["spec_root"] = "spec".into())
        .files(&[(
            "spec/main.spec",
            "spec \"fixture\" {\n  version \"1.0\"\n}\n\nbehavior register \"Register\" {\n  contract \"registers an id\"\n}\n",
        )]);
        let compiled = project.compile();
        for with_schema in [false, true] {
            for format in ["context", "brief"] {
                let published = published_in(&compiled, format);
                let export = exported_with(
                    &compiled,
                    &Request {
                        format: Some(format_named(format)),
                        schema: if with_schema {
                            Schema::With
                        } else {
                            Schema::Default
                        },
                        ..Default::default()
                    },
                );
                let messages = conforms(&published, &export);
                assert!(
                    messages.is_empty(),
                    "{format} (with_schema {with_schema}):\n{}",
                    messages.join("\n")
                );
            }
        }
    }

    const ADMITTED_SPEC: &str = r#"spec "fixture" {
  version "1.0"
}

invariant unique_ids "Unique ids" {
  guarantee "ids are unique"
  expression expr {
    duplicates < 1
    collisions <= 0
  }
}

behavior register "Register" {
  contract   "registers an id"
  invariants [unique_ids]
  produces   [registered]
  requires {
    id_free "the id is not taken"
  }
  ensures {
    id_taken "the id is taken"
  }
  maintains {
    unique_ids "ids stay unique"
  }
}

type Registration {
  id     string
  fields RegistrationFields
  kind   string
}

type RegistrationFields {
  entries string[]
}

event registered "Registered" {
  payload Registration
  sync {
    timeout "5s"
  }
}

failure_mode clash "Clash" {
  cause    "two writers"
  effect   "duplicate ids"
  severity "high"
  rpn      12
  post_mitigation {
    severity low
    rpn      2
  }
}

decision d1 "D" {
  status       accepted
  context      "c"
  decision     "x"
  consequences 5
}
"#;

    #[specforge_test(
        behavior = "publish_schema_specification",
        verify = "an export of a project with no errors validates against the published schema"
    )]
    fn an_export_of_a_project_check_admits_validates_against_its_published_schema() {
        let project = TestProject::new()
            .enabling(&[
                "@specforge/software",
                "@specforge/product",
                "@specforge/governance",
                "@specforge/formal",
            ])
            .config(|c| c["spec_root"] = "spec".into())
            .files(&[("spec/main.spec", ADMITTED_SPEC)]);
        let compiled = project.compile();
        assert!(checked(&compiled, CheckOptions::default()).ok());
        let messages = export_violations(&compiled, &["graph", "context", "brief"]);
        assert!(messages.is_empty(), "{}", messages.join("\n"));
    }

    #[specforge_test(
        behavior = "export_agent_graph_format",
        verify = "output conforms to Graph Protocol schema"
    )]
    #[specforge_test(
        behavior = "publish_schema_specification",
        verify = "this repository's and its example projects' exports validate against their published schemas"
    )]
    fn this_repository_and_its_example_projects_export_what_they_publish() {
        for rel in PUBLISHED_PROJECTS {
            let project = copy_project(rel);
            let compiled = project.compile();
            assert!(checked(&compiled, CheckOptions::default()).ok(), "{rel}");
            let messages = export_violations(&compiled, &["graph", "context", "brief"]);
            assert!(messages.is_empty(), "{rel}:\n{}", messages.join("\n"));
        }
    }
}
