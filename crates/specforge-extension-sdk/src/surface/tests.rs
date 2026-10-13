use crate::{CommandFormat, CommandInput, CommandOutput, ContributionsBuilder, ExtensionMeta};
use serde_json::json;

fn builder() -> ContributionsBuilder {
    let mut c = ContributionsBuilder::new(ExtensionMeta::new("@acme/widgets", "1.0.0"));
    c.command_prefix("acme");
    c.command("widgets", |cmd| {
        cmd.title("List widgets")
            .description("Every widget")
            .category("query")
            .arg("widget", |a| {
                a.required().description("The widget id");
            })
            .arg("limit", |a| {
                a.count();
            })
            .arg("shift", |a| {
                a.integer();
            })
            .arg("all", |a| {
                a.flag();
            })
            .arg("color", |a| {
                a.one_of(&["red", "blue"]);
            })
            .handler(|call| {
                let out = json!({
                    "widget": call.str("widget"),
                    "limit": call.count("limit"),
                    "shift": call.integer("shift"),
                    "all": call.flag("all"),
                    "color": call.str("color"),
                    "nodes": call.graph().nodes().len(),
                    "today": call.today(),
                });
                call.render(&out, |s| s.push_str("human\n"))
            });
    });
    c
}

fn call(c: &ContributionsBuilder, args: serde_json::Value) -> CommandOutput {
    let input = CommandInput {
        args: args.as_object().unwrap().clone(),
        format: CommandFormat::Json,
        today: "2026-10-04".into(),
        ..Default::default()
    };
    c.call_command("cmd__acme_widgets", &input).unwrap()
}

#[test]
fn a_declared_command_is_described_with_its_args_in_order() {
    let c = builder();
    let described: serde_json::Value = serde_json::from_str(
        &crate::testing::MockHost::new(c)
            .describe_json("surfaces")
            .unwrap(),
    )
    .unwrap();
    assert_eq!(
        described["items"],
        json!([{"commands": [{
            "id": "widgets", "title": "List widgets", "description": "Every widget",
            "category": "query", "export": "cmd__acme_widgets",
            "args": [
                {"name": "widget", "arg_type": "string", "required": true,
                 "description": "The widget id"},
                {"name": "limit", "arg_type": "integer", "required": false, "minimum": 0},
                {"name": "shift", "arg_type": "integer", "required": false},
                {"name": "all", "arg_type": "bool", "required": false},
                {"name": "color", "arg_type": {"enum": {"values": ["red", "blue"]}},
                 "required": false}
            ]
        }]}])
    );
}

#[test]
fn no_surfaces_describe_as_none() {
    let c = ContributionsBuilder::new(ExtensionMeta::new("@acme/x", "1.0.0"));
    let described: serde_json::Value = serde_json::from_str(
        &crate::testing::MockHost::new(c)
            .describe_json("surfaces")
            .unwrap(),
    )
    .unwrap();
    assert_eq!(described, json!({"category": "surfaces", "items": []}));
}

#[test]
fn the_handler_reads_the_declared_args_typed() {
    let out = call(
        &builder(),
        json!({"widget": "w1", "limit": "3", "shift": -2, "all": "true", "color": "red"}),
    );
    assert_eq!(out.exit_code, 0, "{out:?}");
    let out: serde_json::Value = serde_json::from_str(&out.stdout).unwrap();
    assert_eq!(
        out,
        json!({"widget": "w1", "limit": 3, "shift": -2, "all": true, "color": "red",
                   "nodes": 0, "today": "2026-10-04"})
    );
}

#[test]
fn a_value_the_declaration_refuses_is_invalid_input() {
    let c = builder();
    for (args, says) in [
        (json!({}), "missing required arg 'widget'"),
        (
            json!({"widget": "w", "limit": -1}),
            "limit must be a non-negative integer, got -1",
        ),
        (
            json!({"widget": "w", "shift": "x"}),
            "shift must be an integer, got 'x'",
        ),
        (
            json!({"widget": "w", "undeclared": 1}),
            "unknown argument 'undeclared'",
        ),
        (
            json!({"widget": "w", "all": 1}),
            "all must be true or false, got 1",
        ),
        (
            json!({"widget": "w", "color": "green"}),
            "color must be one of red, blue, got 'green'",
        ),
        (json!({"widget": 5}), "widget must be a string, got 5"),
    ] {
        let out = call(&c, args.clone());
        assert_eq!((out.exit_code, out.stdout.as_str()), (2, ""), "{args}");
        let error: serde_json::Value = serde_json::from_str(&out.stderr).unwrap();
        assert_eq!(error["code"], "INVALID_INPUT", "{args}");
        assert_eq!(error["message"], says, "{args}");
    }
}

#[test]
fn the_export_routes_to_the_handler_and_nothing_else() {
    let c = builder();
    let input = json!({"args": {"widget": "w1"}, "format": "json"}).to_string();
    let out: CommandOutput = serde_json::from_slice(
        &c.dispatch_export("cmd__acme_widgets", input.as_bytes())
            .unwrap()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(out.exit_code, 0);
    assert!(
        c.dispatch_export("cmd__acme_nope", input.as_bytes())
            .is_none()
    );
    assert!(
        c.dispatch_export("cmd__acme_widgets", b"not json")
            .unwrap()
            .is_err()
    );
}

#[test]
#[should_panic(expected = "command 'w' reads arg 'nope', which it does not declare")]
fn reading_an_undeclared_arg_panics() {
    let mut c = ContributionsBuilder::new(ExtensionMeta::new("@acme/x", "1.0.0"));
    c.command("w", |cmd| {
        cmd.handler(|call| CommandOutput::ok(call.str("nope").unwrap_or_default()));
    });
    c.call_command("cmd__w", &CommandInput::default());
}

#[test]
#[should_panic(expected = "reads arg 'n' as a count, but declares it String")]
fn reading_an_arg_as_another_type_panics() {
    let mut c = ContributionsBuilder::new(ExtensionMeta::new("@acme/x", "1.0.0"));
    c.command("w", |cmd| {
        cmd.arg("n", |_| {})
            .handler(|call| CommandOutput::ok(format!("{:?}", call.count("n"))));
    });
    c.call_command("cmd__w", &CommandInput::default());
}

#[test]
#[should_panic(expected = "is refused by the host: its arg 'format' takes the host's --format")]
fn an_arg_the_host_owns_is_refused() {
    let mut c = ContributionsBuilder::new(ExtensionMeta::new("@acme/x", "1.0.0"));
    c.command("w", |cmd| {
        cmd.arg("format", |_| {});
    });
}

#[test]
#[should_panic(expected = "is refused by the host: it declares the arg 'all-kinds' twice")]
fn two_args_spelling_one_option_are_refused() {
    let mut c = ContributionsBuilder::new(ExtensionMeta::new("@acme/x", "1.0.0"));
    c.command("w", |cmd| {
        cmd.arg("all_kinds", |a| {
            a.flag();
        })
        .arg("all-kinds", |a| {
            a.flag();
        });
    });
}

#[test]
#[should_panic(expected = "command 'w' declares no handler")]
fn a_command_without_a_handler_is_refused() {
    let mut c = ContributionsBuilder::new(ExtensionMeta::new("@acme/x", "1.0.0"));
    c.command("w", |cmd| {
        cmd.title("W");
    });
}

#[test]
fn an_absent_arg_takes_its_default_on_every_surface() {
    let mut c = ContributionsBuilder::new(ExtensionMeta::new("@acme/x", "1.0.0"));
    c.command("w", |cmd| {
        cmd.arg("limit", |a| {
            a.count().default_value("7");
        })
        .arg("order", |a| {
            a.one_of(&["asc", "desc"]).default_value("desc");
        })
        .handler(|call| {
            CommandOutput::ok(format!(
                "{:?} {:?} {}",
                call.count("limit"),
                call.str("order"),
                call.is_set("order")
            ))
        });
    });
    let out = c.call_command("cmd__w", &CommandInput::default()).unwrap();
    assert_eq!(out.stdout, "Some(7) Some(\"desc\") true");
    let input = CommandInput {
        args: json!({"limit": 2, "order": "asc"})
            .as_object()
            .unwrap()
            .clone(),
        ..Default::default()
    };
    let out = c.call_command("cmd__w", &input).unwrap();
    assert_eq!(out.stdout, "Some(2) Some(\"asc\") true");
}

#[test]
fn a_declaration_that_contradicts_itself_is_refused() {
    type Declare = fn(&mut crate::ArgBuilder);
    let cases: [(Declare, &str); 5] = [
        (
            |a| {
                a.required().default_value("x");
            },
            "both required and with a default",
        ),
        (
            |a| {
                a.flag().required();
            },
            "a flag is false unless set",
        ),
        (
            |a| {
                a.flag().default_value("true");
            },
            "a flag is false unless set",
        ),
        (
            |a| {
                a.count().default_value("-1");
            },
            "a default its type refuses: n must be a non-negative integer",
        ),
        (
            |a| {
                a.one_of(&["a"]).default_value("b");
            },
            "a default its type refuses: n must be one of a, got 'b'",
        ),
    ];
    for (declare, says) in cases {
        let panic = std::panic::catch_unwind(|| {
            let mut c = ContributionsBuilder::new(ExtensionMeta::new("@acme/x", "1.0.0"));
            c.command("w", |cmd| {
                cmd.arg("n", declare).handler(|_| CommandOutput::ok(""));
            });
        })
        .expect_err(says);
        let message = panic.downcast_ref::<String>().cloned().unwrap_or_default();
        assert!(message.contains(says), "{message}");
    }
}

#[test]
fn two_surfaces_cannot_share_an_export() {
    type Declare = fn(&mut ContributionsBuilder);
    let cases: [(Declare, &str); 3] = [
        (
            |c| {
                c.command("a_b", |cmd| {
                    cmd.handler(|_| CommandOutput::ok(""));
                });
                c.command("a-b", |cmd| {
                    cmd.handler(|_| CommandOutput::ok(""));
                });
            },
            "command 'a-b''s export cmd__a_b is already 'a_b''s",
        ),
        (
            |c| {
                c.mcp_tool("acme.doc", |t| {
                    t.handler(|_| Ok(json!({})));
                });
                c.mcp_resource("acme-doc", |r| {
                    r.handler(|_| Ok(String::new()));
                });
            },
            "MCP resource 'acme-doc''s export mcp__acme_doc is already 'acme.doc''s",
        ),
        (
            |c| {
                c.command_prefix("acme");
                c.command("x", |cmd| {
                    cmd.handler(|_| CommandOutput::ok(""));
                });
                c.command("x", |cmd| {
                    cmd.handler(|_| CommandOutput::ok(""));
                });
            },
            "command 'x' is declared twice",
        ),
    ];
    for (declare, says) in cases {
        let panic = std::panic::catch_unwind(|| {
            let mut c = ContributionsBuilder::new(ExtensionMeta::new("@acme/x", "1.0.0"));
            declare(&mut c);
        })
        .expect_err(says);
        let message = panic.downcast_ref::<String>().cloned().unwrap_or_default();
        assert!(message.contains(says), "{message}");
    }
}

#[test]
fn calling_every_command_sets_every_arg_with_its_type() {
    let outputs = crate::testing::call_every_command(
        &builder(),
        &Default::default(),
        "2026-10-04",
        |command, arg| format!("{command}.{arg}"),
    );
    assert_eq!(outputs.len(), 1);
    let (id, out) = &outputs[0];
    assert_eq!((id.as_str(), out.exit_code), ("widgets", 0), "{out:?}");
    let out: serde_json::Value = serde_json::from_str(&out.stdout).unwrap();
    assert_eq!(
        out,
        json!({"widget": "widgets.widget", "limit": 1, "shift": 1, "all": true,
                   "color": "red", "nodes": 0, "today": "2026-10-04"})
    );
}

#[test]
#[should_panic(expected = "command 'w' reads arg 'typo', which it does not declare")]
fn calling_every_command_finds_an_undeclared_read() {
    let mut c = ContributionsBuilder::new(ExtensionMeta::new("@acme/x", "1.0.0"));
    c.command("w", |cmd| {
        cmd.arg("n", |a| {
            a.flag();
        })
        .handler(|call| {
            // Only reached with the flag set: a call without args misses it.
            if call.flag("n") {
                let _ = call.str("typo");
            }
            CommandOutput::ok("")
        });
    });
    crate::testing::call_every_command(&c, &Default::default(), "", |_, _| String::new());
}

#[test]
fn tools_and_resources_are_described_and_routed() {
    let mut c = ContributionsBuilder::new(ExtensionMeta::new("@acme/x", "1.0.0"));
    c.mcp_tool("acme.echo", |t| {
        t.description("Echo")
            .handler(|input| Ok(json!({"echo": input})));
    });
    c.mcp_resource("acme-doc", |r| {
        r.uri_template("specforge://ext/acme/{id}")
            .mime_type("text/plain")
            .handler(|uri| Ok(format!("read {uri}")));
    });
    let host = crate::testing::MockHost::new(c);
    let described: serde_json::Value =
        serde_json::from_str(&host.describe_json("surfaces").unwrap()).unwrap();
    let c = host.0;
    assert_eq!(
        described["items"],
        json!([{
            "commands": [],
            "mcp_tools": [{"name": "acme.echo", "description": "Echo",
                "export": "mcp__acme_echo", "input_schema": {"type": "object"}}],
            "mcp_resources": [{"uri_template": "specforge://ext/acme/{id}",
                "name": "acme-doc", "export": "mcp__acme_doc", "mime_type": "text/plain"}]
        }])
    );
    let tool = c
        .dispatch_export("mcp__acme_echo", br#"{"a":1}"#)
        .unwrap()
        .unwrap();
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&tool).unwrap(),
        json!({"echo": {"a": 1}})
    );
    let read = c
        .dispatch_export("mcp__acme_doc", br#"{"uri":"specforge://ext/acme/d1"}"#)
        .unwrap()
        .unwrap();
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&read).unwrap(),
        json!({"content": "read specforge://ext/acme/d1", "mime_type": "text/plain"})
    );
}
