//! Runtime-free testing of an extension's contributions. Lets authors pin
//! their handshake/describe wire output in plain unit tests without building
//! wasm or loading a host.

use std::borrow::Cow;

use crate::{CommandFormat, CommandGraph, CommandInput, CommandOutput, ContributionsBuilder};
use specforge_protocol_types::CommandArgType;

/// Wraps a [`ContributionsBuilder`] and asserts on its wire output.
pub struct MockHost(pub ContributionsBuilder);

impl MockHost {
    pub fn new(builder: ContributionsBuilder) -> Self {
        Self(builder)
    }

    /// Everything this extension declares, as a host loads it.
    pub fn declaration(&self) -> specforge_protocol_types::ExtensionDeclaration {
        self.0.declaration()
    }

    /// The handshake wire JSON this extension reports (pretty, every field
    /// written: what `Served` answers `__handshake` with).
    pub fn handshake_json(&self) -> String {
        self.0.declaration().handshake_json()
    }

    /// The `__describe` wire JSON for `category`, as `Served` answers it: a
    /// raw category as given, else the declaration's items; `None` for a
    /// category the protocol does not define.
    pub fn describe_json(&self, category: &str) -> Option<String> {
        crate::describe_answer(&self.0.raw, || Cow::Owned(self.0.declaration()), category)
    }

    /// Assert the describe output for `category` equals `expected` as JSON
    /// (order-insensitive object comparison via serde_json::Value).
    pub fn assert_describe(&self, category: &str, expected: &str) {
        let actual = self.describe_json(category).unwrap_or_default();
        let a: serde_json::Value = serde_json::from_str(&actual).expect("actual is valid JSON");
        let e: serde_json::Value = serde_json::from_str(expected).expect("expected is valid JSON");
        assert_eq!(a, e, "describe '{category}' wire output mismatch");
    }

    /// Assert the handshake output equals `expected` as JSON.
    pub fn assert_handshake(&self, expected: &str) {
        let a: serde_json::Value =
            serde_json::from_str(&self.handshake_json()).expect("actual is valid JSON");
        let e: serde_json::Value = serde_json::from_str(expected).expect("expected is valid JSON");
        assert_eq!(a, e, "handshake wire output mismatch");
    }
}

/// Run every command `b` declares once over `graph`, as the host would
/// under `--format json`, with every arg it declares set to a value of its
/// declared type: an enum's first value, `1` for an integer or a count (its
/// minimum when that is more),
/// `true` for a flag, and `text(command_id, arg_name)` for a string or a
/// path. A handler that reads an arg its command does not declare, or reads
/// one as another type, panics here, in a test, rather than trapping in the
/// host (E028). Each command's id comes back with its output, to assert on:
/// a handler only reads what the path its args take it down reads, so args
/// naming entities of `graph`, and outputs that exit 0, cover the most.
pub fn call_every_command(
    b: &ContributionsBuilder,
    graph: &CommandGraph,
    today: &str,
    text: impl Fn(&str, &str) -> String,
) -> Vec<(String, CommandOutput)> {
    b.surfaces
        .command_descriptors()
        .map(|command| {
            let args = command
                .args
                .iter()
                .map(|arg| {
                    let value = match &arg.arg_type {
                        CommandArgType::Enum { values } => serde_json::json!(values[0]),
                        CommandArgType::Integer => {
                            serde_json::json!(arg.minimum.map_or(1, |minimum| minimum.max(1)))
                        }
                        CommandArgType::Bool => serde_json::json!(true),
                        CommandArgType::String | CommandArgType::Path => {
                            serde_json::json!(text(&command.id, &arg.name))
                        }
                    };
                    (arg.name.clone(), value)
                })
                .collect();
            let input = CommandInput {
                args,
                cwd: String::new(),
                graph: graph.clone(),
                format: CommandFormat::Json,
                today: today.to_string(),
                evidence: Default::default(),
            };
            let output = b
                .call_command(&command.export, &input)
                .expect("a declared command answers its export");
            (command.id.clone(), output)
        })
        .collect()
}
