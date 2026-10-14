//! The host and the SDK normalize a command's args by one rule
//! (`specforge_protocol_types::command_args`, ADR 0017): what the host
//! sends a `cmd__` export on either surface is what an SDK command reads,
//! and a value the host refuses the SDK refuses with the same error.

use serde_json::{Map, Value, json};
use specforge_extension_sdk::prelude::*;
use specforge_protocol_types::command_args::normalize_args;
use specforge_test_macros::test as specforge_test;

/// `@acme/x`'s command `t`, one arg of each kind; its handler answers the
/// args it read, through the typed accessors.
fn extension() -> ContributionsBuilder {
    let mut c = ContributionsBuilder::new(ExtensionMeta::new("@acme/x", "1.0.0"));
    c.command("t", |cmd| {
        cmd.arg("name", |a| {
            a.required();
        })
        .arg("text", |_| {})
        .arg("file", |a| {
            a.path();
        })
        .arg("n", |a| {
            a.integer();
        })
        .arg("count", |a| {
            a.count();
        })
        .arg("all", |a| {
            a.flag();
        })
        .arg("order", |a| {
            a.one_of(&["asc", "desc"]).default_value("desc");
        })
        .handler(|call| {
            let read = json!({
                "name": call.str("name"),
                "text": call.str("text"),
                "file": call.str("file"),
                "n": call.integer("n"),
                "count": call.count("count"),
                "all": call.flag("all"),
                "order": call.str("order"),
            });
            CommandOutput::ok(read.to_string())
        });
    });
    c
}

/// What the handler reads for `args`, the host's normalized args: an arg
/// absent from them reads as nothing (`null`).
fn as_read(args: &Map<String, Value>) -> Value {
    let read: Map<String, Value> = ["name", "text", "file", "n", "count", "all", "order"]
        .into_iter()
        .map(|name| {
            (
                name.to_string(),
                args.get(name).cloned().unwrap_or(Value::Null),
            )
        })
        .collect();
    Value::Object(read)
}

#[specforge_test(
    behavior = "dispatch_surface_command",
    verify = "the host and the SDK normalize a command's args by the same rule"
)]
fn the_host_and_the_sdk_normalize_args_alike() {
    let c = extension();
    let declared = c
        .declaration()
        .surfaces
        .commands
        .into_iter()
        .next()
        .unwrap()
        .args;
    let cases: Vec<(Value, Result<Value, &str>)> = vec![
        // Absent: a default applies, a flag is false.
        (
            json!({"name": "a"}),
            Ok(json!({"name": "a", "all": false, "order": "desc"})),
        ),
        // Valid, as JSON or as the strings a command line gives.
        (
            json!({"name": "a", "text": "t", "file": "f.md", "n": "-5", "count": "3",
                   "all": "true", "order": "asc"}),
            Ok(
                json!({"name": "a", "text": "t", "file": "f.md", "n": -5, "count": 3,
                   "all": true, "order": "asc"}),
            ),
        ),
        (
            json!({"name": "a", "n": 7, "count": 0, "all": false}),
            Ok(json!({"name": "a", "n": 7, "count": 0, "all": false, "order": "desc"})),
        ),
        // Missing.
        (json!({}), Err("missing required arg 'name'")),
        // An invalid string, and a wrong JSON type, per kind.
        (
            json!({"name": "a", "n": "x"}),
            Err("n must be an integer, got 'x'"),
        ),
        (
            json!({"name": "a", "n": true}),
            Err("n must be an integer, got true"),
        ),
        (
            json!({"name": "a", "count": "many"}),
            Err("count must be a non-negative integer, got 'many'"),
        ),
        (
            json!({"name": "a", "text": 5}),
            Err("text must be a string, got 5"),
        ),
        (
            json!({"name": "a", "file": ["f"]}),
            Err("file must be a string, got [\"f\"]"),
        ),
        (
            json!({"name": "a", "all": "yes"}),
            Err("all must be true or false, got 'yes'"),
        ),
        (
            json!({"name": "a", "all": 1}),
            Err("all must be true or false, got 1"),
        ),
        (
            json!({"name": "a", "order": "up"}),
            Err("order must be one of asc, desc, got 'up'"),
        ),
        (
            json!({"name": "a", "order": 1}),
            Err("order must be one of asc, desc, got 1"),
        ),
        // Below the minimum.
        (
            json!({"name": "a", "count": -1}),
            Err("count must be a non-negative integer, got -1"),
        ),
        (
            json!({"name": "a", "count": "-1"}),
            Err("count must be a non-negative integer, got -1"),
        ),
        // Undeclared.
        (
            json!({"name": "a", "format": "json"}),
            Err("unknown argument 'format'"),
        ),
    ];
    for (given, expected) in cases {
        let given = given.as_object().unwrap().clone();
        // The host's rule.
        let host = normalize_args(&declared, &given);
        // The SDK, called with the same args.
        let input = CommandInput {
            args: given.clone(),
            format: CommandFormat::Json,
            ..Default::default()
        };
        let out = c.call_command("cmd__t", &input).unwrap();
        match expected {
            Ok(args) => {
                let args = args.as_object().unwrap();
                assert_eq!(host.as_ref(), Ok(args), "{given:?}");
                assert_eq!(out.exit_code, 0, "{given:?}: {out:?}");
                let read: Value = serde_json::from_str(&out.stdout).unwrap();
                assert_eq!(read, as_read(args), "{given:?}");
            }
            Err(message) => {
                let error = host.expect_err(message);
                assert_eq!(error.message(), message, "{given:?}");
                assert_eq!((out.exit_code, out.stdout.as_str()), (2, ""), "{given:?}");
                let written: Value = serde_json::from_str(&out.stderr).unwrap();
                assert_eq!(written, error.to_json(), "{given:?}");
                assert_eq!(written["code"], "INVALID_INPUT");
            }
        }
    }
}
