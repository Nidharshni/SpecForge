use specforge_extension_sdk::{ContributionsBuilder, ExtensionMeta};
use specforge_protocol_types::{
    CommandArgDescriptor, CommandDescriptor, DeclaredCategory, ExtensionDeclaration,
    McpResourceDescriptor, McpToolDescriptor, SurfaceDescriptor,
};
use specforge_registry::{
    CommandArgType, CommandRoutes, SurfaceRegistryEntry, SurfaceType,
    register_surface_contributions,
};
use specforge_test_macros::test as specforge_test;

fn make_surfaces(
    commands: Vec<CommandDescriptor>,
    tools: Vec<McpToolDescriptor>,
    resources: Vec<McpResourceDescriptor>,
) -> SurfaceDescriptor {
    SurfaceDescriptor {
        commands,
        mcp_tools: tools,
        mcp_resources: resources,
    }
}

/// The declarations of `(name, short, surfaces)`, in order.
fn declarations(
    extensions: Vec<(&str, Option<&str>, SurfaceDescriptor)>,
) -> Vec<ExtensionDeclaration> {
    extensions
        .into_iter()
        .map(|(name, short, surfaces)| ExtensionDeclaration {
            handshake: specforge_protocol_types::HandshakeResponse {
                name: name.into(),
                version: "1.0.0".into(),
                ext_short: short.map(str::to_string),
                ..Default::default()
            },
            surfaces,
            ..Default::default()
        })
        .collect()
}

fn make_command(id: &str, export: &str) -> CommandDescriptor {
    CommandDescriptor {
        id: id.to_string(),
        title: id.to_string(),
        description: format!("{} command", id),
        category: None,
        export: export.to_string(),
        args: vec![],
    }
}

fn make_tool(name: &str, export: &str) -> McpToolDescriptor {
    McpToolDescriptor {
        name: name.to_string(),
        description: format!("{} tool", name),
        category: None,
        export: export.to_string(),
        input_schema: serde_json::json!({"type": "object"}),
        output_schema: None,
    }
}

fn make_resource(name: &str, export: &str) -> McpResourceDescriptor {
    McpResourceDescriptor {
        uri_template: format!("spec://{}", name),
        name: name.to_string(),
        description: None,
        export: export.to_string(),
        mime_type: "application/json".to_string(),
    }
}

/// The declaration of `@ext/test`, its `surfaces` category served as the
/// raw JSON `surfaces` (absent when `None`).
fn declared_with(surfaces: Option<serde_json::Value>) -> ExtensionDeclaration {
    let mut c = ContributionsBuilder::new(ExtensionMeta::new("@ext/test", "1.0.0"));
    if let Some(surfaces) = surfaces {
        c.raw_category(DeclaredCategory::Surfaces, serde_json::json!([surfaces]));
    }
    c.declaration()
}

// B:surface_contributions_types — verify unit "SurfaceDescriptor round-trip JSON serialization"
#[test]
fn test_surface_contributions_round_trip_json() {
    let surfaces = make_surfaces(
        vec![make_command("analyze", "cmd__analyze")],
        vec![make_tool("search", "mcp__search")],
        vec![make_resource("graph", "mcp__graph")],
    );

    let json = serde_json::to_string(&surfaces).unwrap();
    let parsed: SurfaceDescriptor = serde_json::from_str(&json).unwrap();
    assert_eq!(surfaces, parsed);
}

// B:surface_contributions_types — verify unit "a declaration with surfaces parses"
#[test]
fn test_declaration_with_surfaces_parses() {
    let declaration = declared_with(Some(serde_json::json!({
        "commands": [{
            "id": "analyze",
            "title": "Analyze",
            "description": "Run analysis",
            "export": "cmd__analyze"
        }],
        "mcp_tools": [{
            "name": "search",
            "description": "Search entities",
            "export": "mcp__search",
            "input_schema": {"type": "object"}
        }]
    })));

    let surfaces = declaration.surfaces;
    assert_eq!(surfaces.commands.len(), 1);
    assert_eq!(surfaces.commands[0].id, "analyze");
    assert_eq!(surfaces.mcp_tools.len(), 1);
    assert_eq!(surfaces.mcp_tools[0].name, "search");
}

// B:surface_contributions_types — verify unit "a declaration without surfaces declares none"
#[test]
fn test_declaration_without_surfaces_declares_none() {
    let declaration = declared_with(None);
    assert_eq!(declaration.surfaces, SurfaceDescriptor::default());
}

// B:surface_contributions_types — verify unit "all 5 CommandArgType variants deserialize"
#[test]
fn test_all_command_arg_type_variants_deserialize() {
    let json = r#"[
        {"name": "path", "arg_type": "string"},
        {"name": "file", "arg_type": "path"},
        {"name": "verbose", "arg_type": "bool"},
        {"name": "format", "arg_type": {"enum": {"values": ["json", "text"]}}},
        {"name": "count", "arg_type": "integer"}
    ]"#;
    let args: Vec<CommandArgDescriptor> = serde_json::from_str(json).unwrap();
    assert_eq!(args.len(), 5);
    assert_eq!(args[0].arg_type, CommandArgType::String);
    assert_eq!(args[1].arg_type, CommandArgType::Path);
    assert_eq!(args[2].arg_type, CommandArgType::Bool);
    assert!(matches!(args[3].arg_type, CommandArgType::Enum { .. }));
    assert_eq!(args[4].arg_type, CommandArgType::Integer);
}

/// The routes and registry entries of one declaration, parsed from its
/// JSON, whose `surfaces` category declares a command, an MCP tool and an
/// MCP resource.
fn registered_from_manifest() -> (CommandRoutes, Vec<SurfaceRegistryEntry>) {
    let declaration = declared_with(Some(serde_json::json!({
        "commands": [{
            "id": "analyze",
            "title": "Analyze",
            "description": "Run analysis",
            "export": "cmd__analyze"
        }],
        "mcp_tools": [{
            "name": "search",
            "description": "Search entities",
            "export": "mcp__search",
            "input_schema": {"type": "object"}
        }],
        "mcp_resources": [{
            "uri_template": "spec://graph",
            "name": "graph",
            "export": "mcp__graph",
            "mime_type": "application/json"
        }]
    })));
    let (routes, entries, diags) = register_surface_contributions(&[declaration]);
    assert!(diags.is_empty(), "{diags:?}");
    assert_eq!(entries.len(), 2);
    (routes, entries)
}

/// `(name, export)` of every registered entry of `surface_type`.
fn of_type(entries: &[SurfaceRegistryEntry], surface_type: SurfaceType) -> Vec<(&str, &str)> {
    entries
        .iter()
        .filter(|e| e.surface_type == surface_type)
        .inspect(|e| {
            assert_eq!(e.extension_name, "@ext/test");
        })
        .map(|e| (e.contribution_name.as_str(), e.export_name.as_str()))
        .collect()
}

#[specforge_test(
    behavior = "register_surface_contributions",
    verify = "commands parsed from manifest surfaces field"
)]
fn a_manifests_commands_are_registered() {
    let (routes, _) = registered_from_manifest();
    let routed: Vec<(&str, &str, &str)> = routes
        .routed()
        .iter()
        .map(|r| {
            (
                r.declaration().id.as_str(),
                r.cli_name(),
                r.declaration().export.as_str(),
            )
        })
        .collect();
    assert_eq!(routed, [("analyze", "analyze", "cmd__analyze")]);
}

#[specforge_test(
    behavior = "register_surface_contributions",
    verify = "MCP tools parsed from manifest surfaces field"
)]
fn a_manifests_mcp_tools_are_registered() {
    let (_, entries) = registered_from_manifest();
    assert_eq!(
        of_type(&entries, SurfaceType::McpTool),
        [("search", "mcp__search")]
    );
}

#[specforge_test(
    behavior = "register_surface_contributions",
    verify = "MCP resources parsed from manifest surfaces field"
)]
fn a_manifests_mcp_resources_are_registered() {
    let (_, entries) = registered_from_manifest();
    assert_eq!(
        of_type(&entries, SurfaceType::McpResource),
        [("graph", "mcp__graph")]
    );
}

#[specforge_test(
    behavior = "register_surface_contributions",
    verify = "two extensions of one short name routing one CLI name produce E039"
)]
fn two_extensions_of_one_short_name_routing_one_name_is_e039() {
    let s1 = make_surfaces(
        vec![make_command("analyze", "cmd__analyze")],
        vec![],
        vec![],
    );
    let s2 = make_surfaces(
        vec![make_command("analyze", "cmd__analyze_v2")],
        vec![],
        vec![],
    );

    let manifests = declarations(vec![
        ("@ext/a", Some("ext"), s1),
        ("@ext/b", Some("ext"), s2),
    ]);
    let (routes, _, diags) = register_surface_contributions(&manifests);
    assert_eq!(diags.len(), 1);
    assert_eq!(diags[0].code, "E039");
    assert!(diags[0].message.contains("analyze"));
    assert_eq!(routes.routed().len(), 1);
    assert_eq!(routes.shadowed().len(), 1);
}

#[specforge_test(
    behavior = "register_surface_contributions",
    verify = "duplicate MCP tool name across extensions produces E039"
)]
fn test_register_duplicate_mcp_tool_e039() {
    let s1 = make_surfaces(vec![], vec![make_tool("search", "mcp__search")], vec![]);
    let s2 = make_surfaces(vec![], vec![make_tool("search", "mcp__search_v2")], vec![]);

    let manifests = declarations(vec![("@ext/a", None, s1), ("@ext/b", None, s2)]);
    let (_, _, diags) = register_surface_contributions(&manifests);
    assert_eq!(diags.len(), 1);
    assert_eq!(diags[0].code, "E039");
    assert!(diags[0].message.contains("search"));
}

#[specforge_test(
    behavior = "register_surface_contributions",
    verify = "registration succeeds with no duplicates"
)]
fn test_register_no_duplicates_clean() {
    let s1 = make_surfaces(
        vec![make_command("analyze", "cmd__analyze")],
        vec![make_tool("search", "mcp__search")],
        vec![],
    );
    let s2 = make_surfaces(
        vec![make_command("report", "cmd__report")],
        vec![],
        vec![make_resource("graph", "mcp__graph")],
    );

    let manifests = declarations(vec![("@ext/a", None, s1), ("@ext/b", None, s2)]);
    let (routes, entries, diags) = register_surface_contributions(&manifests);
    assert!(diags.is_empty());
    assert_eq!(routes.routed().len(), 2);
    assert_eq!(entries.len(), 2);
}

// B:register_surface_contributions — verify contract
#[test]
fn test_register_surface_contributions_contract() {
    // requires: declarations with surface contributions
    // ensures: all contributions registered with correct types
    let surfaces = make_surfaces(
        vec![make_command("run", "cmd__run")],
        vec![make_tool("query", "mcp__query")],
        vec![make_resource("spec", "mcp__spec")],
    );
    let manifests = declarations(vec![("@ext/a", None, surfaces)]);
    let (routes, entries, diags) = register_surface_contributions(&manifests);
    assert!(diags.is_empty());
    assert_eq!(routes.routed().len(), 1);
    assert_eq!(entries.len(), 2);
    assert!(entries.iter().all(|e| e.extension_name == "@ext/a"));

    // ensures: empty surfaces register nothing
    let manifests2 = declarations(vec![("@ext/b", None, SurfaceDescriptor::default())]);
    let (routes2, entries2, diags2) = register_surface_contributions(&manifests2);
    assert!(routes2.routed().is_empty());
    assert!(entries2.is_empty());
    assert!(diags2.is_empty());

    // ensures: duplicates produce E039
    let s1 = make_surfaces(vec![make_command("x", "cmd__x")], vec![], vec![]);
    let s2 = make_surfaces(vec![make_command("x", "cmd__x2")], vec![], vec![]);
    let manifests3 = declarations(vec![
        ("@ext/a", Some("ext"), s1),
        ("@ext/b", Some("ext"), s2),
    ]);
    let (_, _, diags3) = register_surface_contributions(&manifests3);
    assert!(!diags3.is_empty() && diags3.iter().all(|d| d.code == "E039"));
}

/// The registry build over one extension contributing `tools`: the tools
/// it registered (and lists to MCP), and its surface diagnostics.
fn build_with_tools(tools: Vec<McpToolDescriptor>) -> (Vec<String>, Vec<String>, Vec<String>) {
    let mut declaration =
        ContributionsBuilder::new(ExtensionMeta::new("@ext/t", "1.0.0")).declaration();
    declaration.surfaces = make_surfaces(vec![], tools, vec![]);
    let build = specforge_registry::build_registries(vec![declaration]);
    let registered = build
        .surfaces()
        .iter()
        .filter(|e| e.surface_type == SurfaceType::McpTool)
        .map(|e| e.contribution_name.clone())
        .collect();
    let listed = build
        .declarations()
        .iter()
        .flat_map(|d| d.surfaces.mcp_tools.iter().map(|t| t.name.clone()))
        .collect();
    let codes = build
        .diagnostics()
        .filter(|d| ["E055", "E039"].contains(&d.code.as_str()))
        .map(|d| d.code.clone())
        .collect();
    (registered, listed, codes)
}

#[specforge_test(
    behavior = "validate_mcp_tool_schemas",
    verify = "a tool whose input_schema is not a JSON object is E055 and not registered"
)]
fn a_tool_whose_input_schema_is_not_an_object_is_refused() {
    let mut tool = make_tool("bad", "mcp__bad");
    tool.input_schema = serde_json::json!("object");
    let (registered, listed, codes) = build_with_tools(vec![tool, make_tool("ok", "mcp__ok")]);
    assert_eq!(registered, ["ok"]);
    assert_eq!(listed, ["ok"]);
    assert_eq!(codes, ["E055"]);
}

#[specforge_test(
    behavior = "validate_mcp_tool_schemas",
    verify = "a tool whose output_schema is not a JSON object is E055 and not registered"
)]
fn a_tool_whose_output_schema_is_not_an_object_is_refused() {
    let mut tool = make_tool("bad", "mcp__bad");
    tool.output_schema = Some(serde_json::json!([1]));
    let (registered, listed, codes) = build_with_tools(vec![tool]);
    assert!(registered.is_empty() && listed.is_empty());
    assert_eq!(codes, ["E055"]);
}

#[specforge_test(
    behavior = "validate_mcp_tool_schemas",
    verify = "a tool whose schemas are JSON objects is registered"
)]
fn a_tool_whose_schemas_are_objects_is_registered() {
    let mut tool = make_tool("ok", "mcp__ok");
    tool.output_schema = Some(serde_json::json!({"type": "object"}));
    let (registered, listed, codes) = build_with_tools(vec![tool]);
    assert_eq!(registered, ["ok"]);
    assert_eq!(listed, ["ok"]);
    assert!(codes.is_empty(), "{codes:?}");
}

#[specforge_test(
    invariant = "surface_schema_validity",
    verify = "a tool whose schemas are JSON objects is registered"
)]
fn the_invariant_keeps_a_wellformed_tool() {
    let (registered, _, codes) = build_with_tools(vec![make_tool("ok", "mcp__ok")]);
    assert_eq!(registered, ["ok"]);
    assert!(codes.is_empty());
}

#[specforge_test(
    invariant = "surface_schema_validity",
    verify = "a tool whose input_schema is not a JSON object is E055 and not registered"
)]
fn the_invariant_refuses_a_malformed_tool() {
    let mut tool = make_tool("bad", "mcp__bad");
    tool.input_schema = serde_json::json!(null);
    let (registered, _, codes) = build_with_tools(vec![tool]);
    assert!(registered.is_empty());
    assert_eq!(codes, ["E055"]);
}
