use specforge_component::{ComponentRuntime, builtins};
use specforge_protocol_types::DeclaredCategory;
use specforge_test_macros::test as specforge_test;
use specforge_wasm::protocol::load_declaration;
use specforge_wasm::runtime::{WasmCallResult, WasmRuntime};

#[test]
fn load_all_builtins() {
    let runtime = ComponentRuntime::new();
    builtins::load_builtins(&runtime).expect("failed to load builtins");
}

#[test]
fn product_handshake() {
    let runtime = ComponentRuntime::new();
    builtins::load_builtins(&runtime).unwrap();

    let result = runtime.call_export("@specforge/product", "__handshake", &[]);
    let WasmCallResult::Ok(bytes) = result else {
        panic!("handshake failed: {:?}", result);
    };
    let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(value["name"], "@specforge/product");
}

#[test]
fn software_describe_entities() {
    let runtime = ComponentRuntime::new();
    builtins::load_builtins(&runtime).unwrap();

    let input = br#"{"category":"entities"}"#;
    let result = runtime.call_export("@specforge/software", "__describe", input);
    let WasmCallResult::Ok(bytes) = result else {
        panic!("describe failed: {:?}", result);
    };
    let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(value["category"], "entities");
    assert!(!value["items"].as_array().unwrap().is_empty());
}

#[test]
fn governance_handshake() {
    let runtime = ComponentRuntime::new();
    builtins::load_builtins(&runtime).unwrap();

    let result = runtime.call_export("@specforge/governance", "__handshake", &[]);
    let WasmCallResult::Ok(bytes) = result else {
        panic!("handshake failed: {:?}", result);
    };
    let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(value["name"], "@specforge/governance");
}

#[test]
fn formal_describe_edges() {
    let runtime = ComponentRuntime::new();
    builtins::load_builtins(&runtime).unwrap();

    let input = br#"{"category":"edges"}"#;
    let result = runtime.call_export("@specforge/formal", "__describe", input);
    let WasmCallResult::Ok(bytes) = result else {
        panic!("describe failed: {:?}", result);
    };
    let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(value["category"], "edges");
}

// SDK adoption: the formal wasm twin is authored with the extension SDK —
// its handshake must still report the extracted builtin contract.
#[test]
fn formal_handshake_from_sdk_authored_twin() {
    let runtime = ComponentRuntime::new();
    builtins::load_builtins(&runtime).unwrap();

    let result = runtime.call_export("@specforge/formal", "__handshake", &[]);
    let WasmCallResult::Ok(bytes) = result else {
        panic!("handshake failed: {:?}", result);
    };
    let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(value["name"], "@specforge/formal");
    assert_eq!(value["version"], "1.0.0");
    let declaration = load_declaration(&runtime, "@specforge/formal")
        .unwrap()
        .declaration;
    assert!(declaration.declares(DeclaredCategory::Entities));
    assert!(declaration.declares(DeclaredCategory::ValidationRules));
    let peers = value["peer_dependencies"].as_array().unwrap();
    assert!(
        peers.iter().any(|p| p["name"] == "@specforge/software"),
        "peer dependency on @specforge/software must survive the migration"
    );
}

// SDK migration: every builtin twin is now SDK-authored; pin each handshake
// contract (peer dependencies, sandbox policy, contribution flags).
#[test]
fn builtin_handshakes_survive_sdk_migration() {
    let runtime = ComponentRuntime::new();
    builtins::load_builtins(&runtime).unwrap();

    let expectations = [
        (
            "@specforge/software",
            &[("@specforge/product", true)][..],
            true,
        ),
        (
            "@specforge/governance",
            &[("@specforge/software", true), ("@specforge/product", true)][..],
            false,
        ),
        ("@specforge/product", &[][..], false),
    ];

    for (name, peers, has_sandbox) in expectations {
        let result = runtime.call_export(name, "__handshake", &[]);
        let WasmCallResult::Ok(bytes) = result else {
            panic!("handshake failed for {name}");
        };
        let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(value["name"], name, "handshake name for {name}");
        let declaration = load_declaration(&runtime, name).unwrap().declaration;
        assert!(
            declaration.declares(DeclaredCategory::Entities),
            "{name}: entities"
        );
        // Governance declares no validation rules: its value checks are
        // E061 on its enum fields (ADR 0071).
        assert_eq!(
            declaration.declares(DeclaredCategory::ValidationRules),
            name != "@specforge/governance",
            "{name}: validation rules"
        );
        let peer_deps = value["peer_dependencies"].as_array().unwrap();
        assert_eq!(
            peer_deps.len(),
            peers.len(),
            "{name}: peer dependency count ({peer_deps:?})"
        );
        for (peer_name, optional) in peers {
            let found = peer_deps
                .iter()
                .find(|p| p["name"] == *peer_name)
                .unwrap_or_else(|| panic!("{name}: missing peer {peer_name}"));
            assert_eq!(found["optional"], *optional, "{name} peer {peer_name}");
        }
        assert_eq!(
            value["sandbox_policy"].is_null(),
            !has_sandbox,
            "{name}: sandbox policy presence"
        );
    }
}

#[specforge_test(
    behavior = "pe_declare_surface_contributions",
    verify = "manifest surfaces declares the specforge product commands, each answered by its export"
)]
fn product_declares_its_commands_and_exports_them() {
    let runtime = ComponentRuntime::new();
    builtins::load_builtins(&runtime).unwrap();

    let input = br#"{"category":"surfaces"}"#;
    let WasmCallResult::Ok(bytes) = runtime.call_export("@specforge/product", "__describe", input)
    else {
        panic!("describe surfaces failed");
    };
    let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let commands = value["items"][0]["commands"].as_array().unwrap();
    let ids: Vec<&str> = commands.iter().map(|c| c["id"].as_str().unwrap()).collect();
    assert_eq!(
        ids,
        [
            "features",
            "journeys",
            "deliverables",
            "milestones",
            "modules",
            "terms",
            "personas",
            "channels",
            "releases",
            "milestone_completion",
            "journey_coverage",
            "feature_impact",
            "feature_dependents",
            "persona_features",
            "channel_features",
            "deliverable_traceability",
            "feature_deliverables",
            "persona_channels",
            "deliverable_personas",
            "deliverable_completion",
            "release_completion",
            "deliverable_priority",
            "unscheduled_features",
            "owner_workload",
            "feature_ordering",
            "critical_path",
            "module_depth",
            "module_coupling",
            "deliverable_dependents",
            "coverage_matrix",
            "channel_coverage_matrix",
            "feature_overlap",
            "term_graph",
            "term_clusters",
            "term_density",
            "milestone_timeline",
            "milestone_velocity",
            "weighted_milestone_completion",
            "bulk_status",
            "health",
        ]
    );
    // Every command answers over an empty graph: a list, or, without the
    // entity id it requires, INVALID_INPUT (exit 2).
    let empty = br#"{"args":{},"cwd":"/p","format":"json","today":"2026-10-03","graph":{"nodes":[],"edges":[]}}"#;
    for command in commands {
        let export = command["export"].as_str().unwrap();
        let WasmCallResult::Ok(bytes) = runtime.call_export("@specforge/product", export, empty)
        else {
            panic!("{export} is declared but not exported");
        };
        let output: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        let positional = command["args"]
            .as_array()
            .unwrap()
            .iter()
            .any(|a| a["required"] == true);
        assert_eq!(
            output["exit_code"],
            if positional { 2 } else { 0 },
            "{export}: {output}"
        );
    }
}

#[specforge_test(
    behavior = "pe_declare_surface_contributions",
    verify = "manifest surfaces declares the 40 commands surfaces-cli.spec specifies"
)]
fn product_declares_the_commands_its_cli_surface_specifies() {
    let runtime = ComponentRuntime::new();
    builtins::load_builtins(&runtime).unwrap();
    let input = br#"{"category":"surfaces"}"#;
    let WasmCallResult::Ok(bytes) = runtime.call_export("@specforge/product", "__describe", input)
    else {
        panic!("describe surfaces failed");
    };
    let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let mut declared: Vec<String> = value["items"][0]["commands"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["export"].as_str().unwrap().to_string())
        .collect();
    declared.sort();
    // Each surface behavior in surfaces-cli.spec names its export once.
    let spec = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../spec/extensions/product/surfaces-cli.spec"
    ))
    .unwrap();
    let mut specified: Vec<String> = spec
        .lines()
        .filter_map(|l| l.trim().strip_prefix("Wasm export: "))
        .map(|e| e.trim_end_matches('.').to_string())
        .collect();
    specified.sort();
    assert_eq!(specified.len(), 40);
    assert_eq!(declared, specified);
}
