use specforge_protocol_types::*;

// ── Step 1: HandshakeRequest tracer bullet ──

#[test]
fn handshake_request_round_trip() {
    let req = HandshakeRequest {
        host_version: "1.0.0".to_string(),
    };
    let json = serde_json::to_string(&req).unwrap();
    let decoded: HandshakeRequest = serde_json::from_str(&json).unwrap();
    assert_eq!(req, decoded);
}

#[test]
fn handshake_request_json_field_names() {
    let req = HandshakeRequest {
        host_version: "1.0.0".to_string(),
    };
    let json = serde_json::to_string(&req).unwrap();
    assert!(
        json.contains("host_version"),
        "expected snake_case field name"
    );
}

#[test]
fn protocol_version_constant() {
    assert_eq!(PROTOCOL_VERSION, "1.2.0");
}

// ── Step 2: HandshakeResponse ──

#[test]
fn handshake_response_round_trip() {
    let resp = HandshakeResponse {
        protocol_version: "1.0".to_string(),
        name: "@specforge/software".to_string(),
        version: "1.0.0".to_string(),
        providers: true,
        peer_dependencies: vec![PeerDependency {
            name: "@specforge/governance".to_string(),
            version: ">=1.0.0".to_string(),
            optional: false,
        }],
        sandbox_policy: Some(SandboxPolicy {
            max_memory_mb: Some(256),
            max_execution_ms: Some(5000),
        }),
        starter_template: Some("spec \"{project}\" {}\n".to_string()),
        theme_color: None,
        migration_hook: None,
        ..Default::default()
    };
    let json = serde_json::to_string(&resp).unwrap();
    let decoded: HandshakeResponse = serde_json::from_str(&json).unwrap();
    assert_eq!(resp, decoded);
}

#[test]
fn handshake_response_rejects_missing_critical_fields() {
    // C7-07: the handshake's critical envelope fields are REQUIRED on the
    // wire. A truncated handshake must fail deserialization instead of
    // silently yielding an empty peer-dependency list.
    for missing in ["protocol_version", "name", "version", "peer_dependencies"] {
        let full = handshake_with_all_fields();
        let mut truncated = full.clone();
        truncated.as_object_mut().unwrap().remove(missing);
        let err = serde_json::from_str::<HandshakeResponse>(&truncated.to_string())
            .expect_err("missing critical handshake field must fail deserialization");
        assert!(
            err.to_string().contains(missing),
            "error should name the missing field '{missing}', got: {err}"
        );
    }
}

#[test]
fn handshake_sandbox_policy_is_fail_safe_optional() {
    // `sandbox_policy` is `Option<SandboxPolicy>`: serde maps absence and
    // explicit `null` alike to `None`, which is fail-safe (the host then
    // substitutes its own deny-by-default policy). Pin that documented
    // semantic so it cannot silently change.
    for json in [
        r#"{"protocol_version":"1.0.0","name":"@t/x","version":"0.1.0","peer_dependencies":[]}"#,
        r#"{"protocol_version":"1.0.0","name":"@t/x","version":"0.1.0","peer_dependencies":[],"sandbox_policy":null}"#,
    ] {
        let resp: HandshakeResponse =
            serde_json::from_str(json).expect("no/absent sandbox_policy is valid");
        assert!(resp.sandbox_policy.is_none());
    }
}

/// A handshake value carrying every envelope field, for truncation tests.
fn handshake_with_all_fields() -> serde_json::Value {
    serde_json::json!({
        "protocol_version": "1.0.0",
        "name": "@specforge/test",
        "version": "0.1.0",
        "peer_dependencies": [],
        "sandbox_policy": null,
    })
}

#[test]
fn peer_dependency_optional_defaults_false() {
    let json = r#"{"name": "@specforge/formal", "version": "^1.0"}"#;
    let dep: PeerDependency = serde_json::from_str(json).unwrap();
    assert!(!dep.optional);
}

#[test]
fn sandbox_policy_default_empty() {
    let policy = SandboxPolicy::default();
    assert!(policy.max_memory_mb.is_none());
    assert!(policy.max_execution_ms.is_none());
}

// ── Step 3: DescribeRequest + DescribeResponse ──

#[test]
fn describe_request_round_trip() {
    let req = DescribeRequest {
        category: "entities".to_string(),
    };
    let json = serde_json::to_string(&req).unwrap();
    let decoded: DescribeRequest = serde_json::from_str(&json).unwrap();
    assert_eq!(req, decoded);
}

#[test]
fn describe_response_parse_items_typed() {
    let resp = DescribeResponse {
        category: "entities".to_string(),
        items: serde_json::json!([
            {"name": "behavior", "testable": true},
            {"name": "invariant"}
        ]),
    };
    let entities: Vec<EntityKindDescriptor> = resp.parse_items().unwrap();
    assert_eq!(entities.len(), 2);
    assert_eq!(entities[0].name, "behavior");
    assert!(entities[0].testable);
    assert_eq!(entities[1].name, "invariant");
    assert!(!entities[1].testable);
}

#[test]
fn describe_response_parse_items_invalid_json() {
    let resp = DescribeResponse {
        category: "entities".to_string(),
        items: serde_json::json!({"not": "an array"}),
    };
    let result: Result<Vec<EntityKindDescriptor>, _> = resp.parse_items();
    assert!(result.is_err());
}

#[test]
fn describe_response_round_trip() {
    let resp = DescribeResponse {
        category: "edges".to_string(),
        items: serde_json::json!([{"label": "enforces"}]),
    };
    let json = serde_json::to_string(&resp).unwrap();
    let decoded: DescribeResponse = serde_json::from_str(&json).unwrap();
    assert_eq!(resp, decoded);
}

// ── Step 4: EntityKindDescriptor (full) ──

#[test]
fn entity_kind_descriptor_full_round_trip() {
    let entity = EntityKindDescriptor {
        name: "behavior".to_string(),
        keyword: Some("behavior".to_string()),
        description: Some("A testable behavior specification".to_string()),
        fields: vec![
            FieldDescriptor {
                name: "contract".to_string(),
                field_type: "block".to_string(),
                required: false,
                description: Some("Structured contract block".to_string()),
                edge: None,
                target_kind: None,
                file_reference: false,
                default_value: None,
                enum_values: vec![],
                inverse_of: None,
                normative: false,
                exempts_obligations: false,
                headline: false,
                derived_from: None,
                proof_role: None,
            },
            FieldDescriptor {
                name: "invariants".to_string(),
                field_type: "reference_list".to_string(),
                required: false,
                description: None,
                edge: Some("enforces".to_string()),
                target_kind: Some("invariant".to_string()),
                file_reference: false,
                default_value: None,
                enum_values: vec![],
                inverse_of: None,
                normative: false,
                exempts_obligations: false,
                headline: false,
                derived_from: None,
                proof_role: None,
            },
        ],
        testable: true,
        singleton: false,
        supports_verify: true,
        incremental: Some(true),
        has_body_parser: false,
        open_fields: false,
        contract_target: false,
        declares_types: false,
        lifecycle_field: None,
        semantic_token: Some("function".to_string()),
        lsp_icon: Some("Method".to_string()),
        dot_shape: Some("ellipse".to_string()),
        dot_color: Some("#4a90d9".to_string()),
        dot_fillcolor: Some("#e8f0fe".to_string()),
        verify_kinds: vec!["smoke".to_string(), "contract".to_string()],
        inference_guide: None,
    };
    let json = serde_json::to_string(&entity).unwrap();
    let decoded: EntityKindDescriptor = serde_json::from_str(&json).unwrap();
    assert_eq!(entity, decoded);
}

#[test]
fn entity_kind_descriptor_minimal_defaults() {
    let json = r#"{"name": "constraint"}"#;
    let entity: EntityKindDescriptor = serde_json::from_str(json).unwrap();
    assert_eq!(entity.name, "constraint");
    assert!(!entity.testable);
    assert!(!entity.singleton);
    assert!(!entity.supports_verify);
    assert!(entity.fields.is_empty());
    assert!(entity.verify_kinds.is_empty());
    assert!(entity.keyword.is_none());
    assert!(entity.incremental.is_none());
}

// ── Step 5: FieldDescriptor ──

#[test]
fn field_descriptor_with_edge_and_target() {
    let field = FieldDescriptor {
        name: "features".to_string(),
        field_type: "reference_list".to_string(),
        required: false,
        description: Some("Related features".to_string()),
        edge: Some("Implements".to_string()),
        target_kind: Some("feature".to_string()),
        file_reference: false,
        default_value: None,
        enum_values: vec![],
        inverse_of: None,
        normative: false,
        exempts_obligations: false,
        headline: false,
        derived_from: None,
        proof_role: None,
    };
    let json = serde_json::to_string(&field).unwrap();
    let decoded: FieldDescriptor = serde_json::from_str(&json).unwrap();
    assert_eq!(field, decoded);
}

#[test]
fn field_descriptor_with_enum_values() {
    let field = FieldDescriptor {
        name: "status".to_string(),
        field_type: "enum".to_string(),
        required: true,
        description: None,
        edge: None,
        target_kind: None,
        file_reference: false,
        default_value: Some("draft".to_string()),
        enum_values: vec![
            "draft".to_string(),
            "active".to_string(),
            "done".to_string(),
        ],
        inverse_of: None,
        normative: false,
        exempts_obligations: false,
        headline: false,
        derived_from: None,
        proof_role: None,
    };
    let json = serde_json::to_string(&field).unwrap();
    assert!(json.contains("enum_values"));
    let decoded: FieldDescriptor = serde_json::from_str(&json).unwrap();
    assert_eq!(field, decoded);
}

#[test]
fn field_descriptor_enum_values_skipped_when_empty() {
    let field = FieldDescriptor {
        name: "title".to_string(),
        field_type: "string".to_string(),
        required: true,
        description: None,
        edge: None,
        target_kind: None,
        file_reference: false,
        default_value: None,
        enum_values: vec![],
        inverse_of: None,
        normative: false,
        exempts_obligations: false,
        headline: false,
        derived_from: None,
        proof_role: None,
    };
    let json = serde_json::to_string(&field).unwrap();
    assert!(
        !json.contains("enum_values"),
        "empty enum_values should be skipped"
    );
}

// ── Step 6: EdgeTypeDescriptor ──

#[test]
fn edge_type_descriptor_round_trip() {
    let edge = EdgeTypeDescriptor {
        label: "Implements".to_string(),
        description: Some("Behavior implements feature".to_string()),
        source_kind: Some("behavior".to_string()),
        target_kind: Some("feature".to_string()),
        edge_style: Some("solid".to_string()),
        edge_color: Some("#333".to_string()),
        edge_arrowhead: Some("normal".to_string()),
    };
    let json = serde_json::to_string(&edge).unwrap();
    let decoded: EdgeTypeDescriptor = serde_json::from_str(&json).unwrap();
    assert_eq!(edge, decoded);
}

#[test]
fn edge_type_descriptor_minimal() {
    let json = r#"{"label": "DependsOn"}"#;
    let edge: EdgeTypeDescriptor = serde_json::from_str(json).unwrap();
    assert_eq!(edge.label, "DependsOn");
    assert!(edge.source_kind.is_none());
    assert!(edge.target_kind.is_none());
}

// ── Step 7: SharedFieldDescriptor (type alias) ──

#[test]
fn shared_field_descriptor_is_field_descriptor() {
    let shared: SharedFieldDescriptor = FieldDescriptor {
        name: "status".to_string(),
        field_type: "enum".to_string(),
        required: true,
        description: Some("Entity lifecycle status".to_string()),
        edge: None,
        target_kind: None,
        file_reference: false,
        default_value: Some("draft".to_string()),
        enum_values: vec!["draft".to_string(), "active".to_string()],
        inverse_of: None,
        normative: false,
        exempts_obligations: false,
        headline: false,
        derived_from: None,
        proof_role: None,
    };
    let json = serde_json::to_string(&shared).unwrap();
    let decoded: SharedFieldDescriptor = serde_json::from_str(&json).unwrap();
    assert_eq!(shared, decoded);
}

// ── Step 8: EntityEnhancementDescriptor ──

#[test]
fn entity_enhancement_descriptor_round_trip() {
    let enh = EntityEnhancementDescriptor {
        verify_kinds: None,
        target_kind: "behavior".to_string(),
        source_extension: "@specforge/formal".to_string(),
        fields: vec![FieldDescriptor {
            name: "requires".to_string(),
            field_type: "block".to_string(),
            required: false,
            description: Some("Precondition block".to_string()),
            edge: None,
            target_kind: None,
            file_reference: false,
            default_value: None,
            enum_values: vec![],
            inverse_of: None,
            normative: false,
            exempts_obligations: false,
            headline: false,
            derived_from: None,
            proof_role: None,
        }],
        edge_types: vec![EdgeTypeDescriptor {
            label: "RequiresCondition".to_string(),
            description: None,
            source_kind: Some("behavior".to_string()),
            target_kind: Some("condition".to_string()),
            edge_style: None,
            edge_color: None,
            edge_arrowhead: None,
        }],
    };
    let json = serde_json::to_string(&enh).unwrap();
    let decoded: EntityEnhancementDescriptor = serde_json::from_str(&json).unwrap();
    assert_eq!(enh, decoded);
}

// ── Step 9: ValidationRuleDescriptor ──

#[test]
fn validation_rule_descriptor_declarative() {
    let rule = ValidationRuleDescriptor {
        code: "W001".to_string(),
        severity: ValidationSeverity::Warning,
        message_template: "behavior '{{name}}' has no verify declarations".to_string(),
        check: "no_incoming_edges".to_string(),
        target_kind: Some("behavior".to_string()),
        edge_type: None,
        field: None,
        constraint: None,
        wasm_function: None,
        target_extension: None,
    };
    let json = serde_json::to_string(&rule).unwrap();
    let decoded: ValidationRuleDescriptor = serde_json::from_str(&json).unwrap();
    assert_eq!(rule, decoded);
}

#[test]
fn validation_rule_descriptor_with_constraint() {
    let rule = ValidationRuleDescriptor {
        code: "W095".to_string(),
        severity: ValidationSeverity::Warning,
        message_template: "invalid effort value".to_string(),
        check: "field_value_constraint".to_string(),
        target_kind: Some("feature".to_string()),
        edge_type: None,
        field: Some("effort".to_string()),
        constraint: Some(FieldConstraintDescriptor {
            kind: "enum".to_string(),
            pattern: None,
            values: vec![
                "xs".to_string(),
                "s".to_string(),
                "m".to_string(),
                "l".to_string(),
                "xl".to_string(),
            ],
        }),
        wasm_function: None,
        target_extension: None,
    };
    let json = serde_json::to_string(&rule).unwrap();
    let decoded: ValidationRuleDescriptor = serde_json::from_str(&json).unwrap();
    assert_eq!(rule, decoded);
}

#[test]
fn validation_rule_descriptor_custom_wasm() {
    let rule = ValidationRuleDescriptor {
        code: "E050".to_string(),
        severity: ValidationSeverity::Error,
        message_template: "custom check failed".to_string(),
        check: "custom".to_string(),
        target_kind: None,
        edge_type: None,
        field: None,
        constraint: None,
        wasm_function: Some("validate__custom_check".to_string()),
        target_extension: None,
    };
    let json = serde_json::to_string(&rule).unwrap();
    assert!(
        json.contains("\"error\""),
        "severity should serialize as lowercase"
    );
    let decoded: ValidationRuleDescriptor = serde_json::from_str(&json).unwrap();
    assert_eq!(rule, decoded);
}

#[test]
fn validation_severity_round_trip() {
    for (severity, expected) in [
        (ValidationSeverity::Error, "\"error\""),
        (ValidationSeverity::Warning, "\"warning\""),
        (ValidationSeverity::Info, "\"info\""),
    ] {
        let json = serde_json::to_string(&severity).unwrap();
        assert_eq!(json, expected);
        let decoded: ValidationSeverity = serde_json::from_str(&json).unwrap();
        assert_eq!(severity, decoded);
    }
}

// ── Step 10: SurfaceDescriptor + CommandDescriptor + McpToolDescriptor + McpResourceDescriptor ──

#[test]
fn surface_descriptor_full_round_trip() {
    let surface = SurfaceDescriptor {
        commands: vec![CommandDescriptor {
            id: "check".to_string(),
            title: "Check".to_string(),
            description: "Run validation checks".to_string(),
            category: Some("validation".to_string()),
            export: "cmd__check".to_string(),
            args: vec![CommandArgDescriptor {
                name: "path".to_string(),
                arg_type: CommandArgType::Path,
                required: true,
                default_value: None,
                description: Some("Project path".to_string()),
                minimum: None,
            }],
        }],
        mcp_tools: vec![McpToolDescriptor {
            name: "specforge.validate".to_string(),
            description: "Validate project".to_string(),
            category: None,
            export: "mcp__validate".to_string(),
            input_schema: serde_json::json!({"type": "object"}),
            output_schema: None,
        }],
        mcp_resources: vec![McpResourceDescriptor {
            uri_template: "specforge://entities/{kind}".to_string(),
            name: "entities".to_string(),
            description: Some("List entities by kind".to_string()),
            export: "mcp__entities".to_string(),
            mime_type: "application/json".to_string(),
        }],
    };
    let json = serde_json::to_string(&surface).unwrap();
    let decoded: SurfaceDescriptor = serde_json::from_str(&json).unwrap();
    assert_eq!(surface, decoded);
}

#[test]
fn surface_descriptor_default_empty() {
    let surface = SurfaceDescriptor::default();
    assert!(surface.commands.is_empty());
    assert!(surface.mcp_tools.is_empty());
    assert!(surface.mcp_resources.is_empty());
}

#[test]
fn command_arg_type_enum_variant_with_values() {
    let arg = CommandArgDescriptor {
        name: "format".to_string(),
        arg_type: CommandArgType::Enum {
            values: vec!["json".to_string(), "yaml".to_string()],
        },
        required: false,
        default_value: Some("json".to_string()),
        description: None,
        minimum: None,
    };
    let json = serde_json::to_string(&arg).unwrap();
    let decoded: CommandArgDescriptor = serde_json::from_str(&json).unwrap();
    assert_eq!(arg, decoded);
}

#[test]
fn command_arg_type_round_trip_all_variants() {
    for arg_type in [
        CommandArgType::String,
        CommandArgType::Path,
        CommandArgType::Bool,
        CommandArgType::Integer,
        CommandArgType::Enum {
            values: vec!["a".to_string()],
        },
    ] {
        let json = serde_json::to_string(&arg_type).unwrap();
        let decoded: CommandArgType = serde_json::from_str(&json).unwrap();
        assert_eq!(arg_type, decoded);
    }
}

// ── Step 13: CollectorDescriptor ──

#[test]
fn collector_descriptor_with_auto_detect() {
    let collector = CollectorDescriptor {
        name: "rust".to_string(),
        input_formats: vec!["junit-xml".to_string()],
        export: "collect__rust".to_string(),
        auto_detect: Some(AutoDetectConfig {
            file_patterns: vec!["**/target/**/junit.xml".to_string()],
            env_vars: vec!["CARGO_TARGET_DIR".to_string()],
        }),
        run: vec![],
        report: None,
        capture: None,
    };
    let json = serde_json::to_string(&collector).unwrap();
    let decoded: CollectorDescriptor = serde_json::from_str(&json).unwrap();
    assert_eq!(collector, decoded);
}

#[test]
fn collector_descriptor_without_auto_detect() {
    let json = r#"{"name": "jest", "input_formats": ["jest-json"], "export": "collect__jest"}"#;
    let collector: CollectorDescriptor = serde_json::from_str(json).unwrap();
    assert_eq!(collector.name, "jest");
    assert!(collector.auto_detect.is_none());
}

// ── Step 14: CompilerPassDescriptor + FeatureFlagDescriptor ──

#[test]
fn compiler_pass_descriptor_round_trip() {
    let pass = CompilerPassDescriptor {
        name: "condition_check".to_string(),
        after: Some("parse".to_string()),
        before: Some("validate".to_string()),
        phase: Some("analysis".to_string()),
    };
    let json = serde_json::to_string(&pass).unwrap();
    let decoded: CompilerPassDescriptor = serde_json::from_str(&json).unwrap();
    assert_eq!(pass, decoded);
}

#[test]
fn compiler_pass_descriptor_minimal() {
    let json = r#"{"name": "coverage_tracking"}"#;
    let pass: CompilerPassDescriptor = serde_json::from_str(json).unwrap();
    assert_eq!(pass.name, "coverage_tracking");
    assert!(pass.after.is_none());
    assert!(pass.before.is_none());
    assert!(pass.phase.is_none());
}

#[test]
fn feature_flag_descriptor_round_trip() {
    let flag = FeatureFlagDescriptor {
        name: "strict_mode".to_string(),
        description: Some("Enable strict validation".to_string()),
        default_enabled: false,
    };
    let json = serde_json::to_string(&flag).unwrap();
    let decoded: FeatureFlagDescriptor = serde_json::from_str(&json).unwrap();
    assert_eq!(flag, decoded);
}

#[test]
fn feature_flag_descriptor_default_enabled_false() {
    let json = r#"{"name": "experimental"}"#;
    let flag: FeatureFlagDescriptor = serde_json::from_str(json).unwrap();
    assert!(!flag.default_enabled);
    assert!(flag.description.is_none());
}

// ── Cross-cutting: DescribeResponse.parse_items with various descriptor types ──

#[test]
fn describe_response_parse_edge_descriptors() {
    let resp = DescribeResponse {
        category: "edges".to_string(),
        items: serde_json::json!([
            {"label": "Implements", "source_kind": "behavior", "target_kind": "feature"},
            {"label": "DependsOn"}
        ]),
    };
    let edges: Vec<EdgeTypeDescriptor> = resp.parse_items().unwrap();
    assert_eq!(edges.len(), 2);
    assert_eq!(edges[0].label, "Implements");
    assert_eq!(edges[1].label, "DependsOn");
}

#[test]
fn describe_response_parse_validation_rules() {
    let resp = DescribeResponse {
        category: "validation_rules".to_string(),
        items: serde_json::json!([
            {
                "code": "W001",
                "severity": "warning",
                "message_template": "test",
                "check": "no_incoming_edges",
                "target_kind": "behavior"
            }
        ]),
    };
    let rules: Vec<ValidationRuleDescriptor> = resp.parse_items().unwrap();
    assert_eq!(rules.len(), 1);
    assert_eq!(rules[0].code, "W001");
    assert_eq!(rules[0].severity, ValidationSeverity::Warning);
}

#[test]
fn describe_response_parse_collectors() {
    let resp = DescribeResponse {
        category: "collectors".to_string(),
        items: serde_json::json!([
            {"name": "rust", "input_formats": ["junit-xml"], "export": "collect__rust"}
        ]),
    };
    let collectors: Vec<CollectorDescriptor> = resp.parse_items().unwrap();
    assert_eq!(collectors.len(), 1);
    assert_eq!(collectors[0].name, "rust");
}

// ── Step 15: ProtocolError ──

#[test]
fn protocol_error_display_incompatible_version() {
    let err = ProtocolError::IncompatibleVersion {
        host_version: "1.0".to_string(),
        extension_version: "2.0".to_string(),
    };
    let msg = err.to_string();
    assert!(msg.contains("host=1.0"));
    assert!(msg.contains("extension=2.0"));
}

#[test]
fn protocol_error_display_handshake_failed() {
    let err = ProtocolError::HandshakeFailed("invalid JSON".to_string());
    assert_eq!(err.to_string(), "handshake failed: invalid JSON");
}

#[test]
fn protocol_error_display_describe_failed() {
    let err = ProtocolError::DescribeFailed {
        category: "entities".to_string(),
        reason: "timeout".to_string(),
    };
    assert_eq!(err.to_string(), "describe 'entities' failed: timeout");
}

#[test]
fn protocol_error_display_deserialization() {
    let err = ProtocolError::DeserializationError("missing field 'name'".to_string());
    assert!(err.to_string().contains("missing field 'name'"));
}

#[test]
fn protocol_error_from_serde_json_error() {
    let bad_json = "not valid json";
    let serde_err = serde_json::from_str::<HandshakeRequest>(bad_json).unwrap_err();
    let proto_err: ProtocolError = serde_err.into();
    match proto_err {
        ProtocolError::DeserializationError(msg) => {
            assert!(!msg.is_empty());
        }
        other => panic!("expected DeserializationError, got {:?}", other),
    }
}

#[test]
fn protocol_error_is_std_error() {
    let err = ProtocolError::HandshakeFailed("test".to_string());
    let _: &dyn std::error::Error = &err;
}
