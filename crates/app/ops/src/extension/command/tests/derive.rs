use super::*;

#[specforge_test(
    behavior = "auto_promote_commands_to_mcp_tools",
    verify = "the derived input_schema states each arg's default and minimum and accepts no undeclared argument"
)]
fn the_input_schema_is_the_declaration() {
    assert_eq!(
        ordered().input_schema(),
        json!({
            "type": "object",
            "properties": {
                "order": {"type": "string", "enum": ["asc", "desc"], "default": "desc"},
                "all": {"type": "boolean", "default": false},
                "limit": {"type": "integer", "minimum": 0, "default": 10,
                          "description": "At most this many"},
                "milestone": {"type": "string"}
            },
            "required": ["milestone"],
            "additionalProperties": false
        })
    );
    // No arg: no property, none required, nothing else accepted.
    assert_eq!(
        routed("check").input_schema(),
        json!({"type": "object", "properties": {}, "additionalProperties": false})
    );
    let refused = ordered()
        .normalize(
            json!({"milestone": "m1", "format": "json"})
                .as_object()
                .unwrap(),
        )
        .unwrap_err();
    assert_eq!(refused.message(), "unknown argument 'format'");
}

#[specforge_test(
    behavior = "auto_promote_commands_to_mcp_tools",
    verify = "a required flag is not required over MCP, as on the command line"
)]
fn a_required_flag_is_a_flag() {
    let strict = strict();
    assert_eq!(
        strict.args()[0].shape,
        ArgShape::Flag {
            long: "strict".into()
        }
    );
    assert!(!strict.args()[0].is_required());
    assert!(strict.input_schema().get("required").is_none());
    assert_eq!(strict.refusal(), None);
    assert_eq!(
        Value::Object(strict.normalize(&Map::new()).unwrap()),
        json!({"strict": false})
    );
}

#[test]
fn the_args_both_surfaces_send_are_normalized_with_their_defaults() {
    let sent = |given: Value| {
        ordered()
            .normalize(given.as_object().unwrap())
            .map(Value::Object)
            .map_err(|e| e.message())
    };
    assert_eq!(
        sent(json!({"milestone": "m1"})),
        Ok(json!({"order": "desc", "all": false, "limit": 10, "milestone": "m1"}))
    );
    assert_eq!(
        sent(json!({"milestone": "m1", "order": "asc", "all": "true", "limit": "3"})),
        Ok(json!({"order": "asc", "all": true, "limit": 3, "milestone": "m1"}))
    );
    assert_eq!(
        sent(json!({"milestone": "m1", "limit": -1})),
        Err("limit must be a non-negative integer, got -1".to_string())
    );
    assert_eq!(
        sent(json!({})),
        Err("missing required arg 'milestone'".to_string())
    );
}

#[test]
fn each_arg_has_its_command_line_shape() {
    let ordered = ordered();
    let shapes: Vec<(&str, &ArgShape, &ArgValue)> = ordered
        .args()
        .iter()
        .map(|a| (a.name.as_str(), &a.shape, &a.value))
        .collect();
    assert_eq!(
        shapes,
        [
            (
                "order",
                &ArgShape::Option {
                    long: "order".into()
                },
                &ArgValue::OneOf(vec!["asc".into(), "desc".into()])
            ),
            (
                "all",
                &ArgShape::Flag { long: "all".into() },
                &ArgValue::Flag
            ),
            (
                "limit",
                &ArgShape::Option {
                    long: "limit".into()
                },
                &ArgValue::Integer { minimum: Some(0) }
            ),
            (
                "milestone",
                &ArgShape::Positional { index: 0 },
                &ArgValue::Text
            ),
        ]
    );
    assert_eq!(ordered.args()[2].default, Some(json!(10)));
    let sorted = declared(
        json!({"id": "s", "title": "s", "description": "", "export": "cmd__s",
        "args": [{"name": "sort_order", "arg_type": "path"}]}),
    );
    assert_eq!(
        sorted.args()[0].shape,
        ArgShape::Option {
            long: "sort-order".into()
        }
    );
}

#[specforge_test(
    behavior = "dispatch_surface_command",
    verify = "a command declaring an arg named format is refused on the command line"
)]
fn an_arg_taking_a_host_option_or_another_args_name_is_refused() {
    let with = |args: Vec<Value>| {
        declared(
            json!({"id": "w", "title": "w", "description": "", "export": "cmd__w",
            "args": args}),
        )
        .refusal()
        .map(ToString::to_string)
    };
    assert_eq!(with(vec![arg("milestone", "string")]), None);
    for name in HOST_OPTIONS {
        assert_eq!(
            with(vec![arg(name, "string")]),
            Some(format!("its arg '{name}' takes the host's --{name}"))
        );
    }
    assert_eq!(
        with(vec![arg("all_kinds", "bool"), arg("all-kinds", "bool")]),
        Some("it declares the arg 'all-kinds' twice".into())
    );
    // The contradictions the SDK refuses to build (D12), refused here
    // too for a guest that declares raw JSON.
    assert_eq!(
        with(vec![
            json!({"name": "n", "arg_type": "integer", "minimum": 0,
            "default_value": "-1"})
        ]),
        Some(
            "its arg 'n' has a default its type refuses: n must be a non-negative integer, got -1"
                .into()
        )
    );
    assert_eq!(
        with(vec![
            json!({"name": "all", "arg_type": "bool", "default_value": "true"})
        ]),
        Some("its flag 'all' has a default, but a flag is false unless set".into())
    );
    assert_eq!(
        with(vec![
            json!({"name": "m", "arg_type": "string", "required": true,
            "default_value": "m1"})
        ]),
        Some("its arg 'm' is both required and with a default".into())
    );
    // A refused default is no default.
    let bad = declared(
        json!({"id": "w", "title": "w", "description": "", "export": "cmd__w",
        "args": [{"name": "o", "arg_type": {"enum": {"values": ["a"]}}, "default_value": "b"}]}),
    );
    assert_eq!(bad.args()[0].default, None);
    assert!(matches!(bad.refusal(), Some(Refusal::BadDefault { .. })));
}

#[test]
fn a_command_is_routed_once_by_its_short_name_and_cli_name() {
    // R7: two extensions with one short name, and two commands of one
    // CLI name: the first in load order wins, the later is shadowed.
    let commands = ExtensionCommands::build(&build_of(&[
        ("@acme/widgets", None, &["list", "a_b"]),
        ("@other/widgets", None, &["list", "show"]),
        ("@acme/tools", Some("t"), &["a-b", "a_b"]),
    ]));
    let routed: Vec<(&str, String)> = commands
        .all()
        .iter()
        .map(|c| (c.extension(), c.tool_name()))
        .collect();
    assert_eq!(
        routed,
        [
            ("@acme/widgets", "specforge.widgets.list".to_string()),
            ("@acme/widgets", "specforge.widgets.a_b".to_string()),
            ("@other/widgets", "specforge.widgets.show".to_string()),
            ("@acme/tools", "specforge.t.a-b".to_string()),
        ]
    );
    let shadowed: Vec<(&str, &str, &str)> = commands
        .shadowed()
        .map(|(c, winner)| (c.extension(), c.id(), winner.extension()))
        .collect();
    assert_eq!(
        shadowed,
        [
            ("@other/widgets", "list", "@acme/widgets"),
            ("@acme/tools", "a_b", "@acme/tools")
        ]
    );
    assert_eq!(commands.shorts(), ["widgets", "t"]);
    let of_widgets: Vec<&str> = commands.of("widgets").map(|c| c.id()).collect();
    assert_eq!(of_widgets, ["list", "a_b", "show"]);
}

/// The routing table is the registry build's: the same commands in the same
/// order, and the same winners.
#[test]
fn the_routing_table_is_the_registry_builds() {
    let build = build_of(&[
        ("@acme/widgets", None, &["list", "a_b"]),
        ("@other/widgets", None, &["list", "show"]),
        ("@acme/tools", Some("t"), &["a-b", "a_b"]),
    ]);
    let commands = ExtensionCommands::build(&build);
    let routes = build.commands();

    let table: Vec<(&str, &str)> = commands
        .all()
        .iter()
        .map(|c| (c.extension(), c.id()))
        .collect();
    let routed: Vec<(&str, &str)> = routes
        .routed()
        .iter()
        .map(|r| (r.extension(), r.declaration().id.as_str()))
        .collect();
    assert_eq!(table, routed);
    let pairs: Vec<(&str, &str, &str)> = commands
        .shadowed()
        .map(|(s, w)| (s.extension(), s.id(), w.extension()))
        .collect();
    let expected: Vec<(&str, &str, &str)> = routes
        .shadowed()
        .iter()
        .map(|s| {
            (
                s.route().extension(),
                s.route().declaration().id.as_str(),
                routes.winner(s).extension(),
            )
        })
        .collect();
    assert_eq!(pairs, expected);
    assert_eq!(pairs.len(), 2);
}

#[specforge_test(type = "ExtensionCommand", verify = "ExtensionCommand schema is valid")]
fn an_extension_command_carries_every_derived_field() {
    let commands = ExtensionCommands::build(&build_of(&[(
        "@specforge/product",
        None,
        &["milestone_completion"],
    )]));
    let [command] = commands.all() else {
        panic!("one command: {commands:?}")
    };
    assert_eq!(command.extension(), "@specforge/product");
    assert_eq!(command.short(), "product");
    assert_eq!(command.cli_name(), "milestone-completion");
    assert_eq!(
        command.tool_name(),
        "specforge.product.milestone_completion"
    );
    assert!(command.args().is_empty());
    assert_eq!(command.input_schema()["type"], "object");
    assert_eq!(command.refusal(), None);
    assert_eq!(command.export(), "cmd__milestone_completion");
}

#[specforge_test(
    behavior = "auto_promote_commands_to_mcp_tools",
    verify = "auto-promoted tool name follows specforge.{ext}.{cmd} pattern"
)]
fn a_command_is_named_by_its_extension_short_name_and_dashed_id() {
    // The short name is the declared one (the SDK's `short`, on the wire
    // `ext_short`), else the name's last segment.
    let commands = ExtensionCommands::build(&build_of(&[
        ("@specforge/product", None, &["milestone_completion"]),
        ("@acme/widgets", Some("w"), &["list_all"]),
    ]));
    let named: Vec<(&str, String, String)> = commands
        .all()
        .iter()
        .map(|c| (c.short(), c.cli_name(), c.tool_name()))
        .collect();
    assert_eq!(
        named,
        [
            (
                "product",
                "milestone-completion".to_string(),
                "specforge.product.milestone_completion".to_string()
            ),
            (
                "w",
                "list-all".to_string(),
                "specforge.w.list_all".to_string()
            ),
        ]
    );
    // `specforge w list-all` routes to it; `specforge widgets` routes
    // nothing.
    assert_eq!(commands.shorts(), ["product", "w"]);
    assert_eq!(
        commands.of("w").map(|c| c.id()).collect::<Vec<_>>(),
        ["list_all"]
    );
    assert_eq!(commands.of("widgets").count(), 0);
}
