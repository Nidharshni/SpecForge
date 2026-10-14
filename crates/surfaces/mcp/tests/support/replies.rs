//! The fixture and the calls that observe every core tool's reply: the
//! pins of `tool_replies.rs` and the conformance probe of `revision.rs`
//! read the same calls (plan 11).

use serde_json::{Value, json};
use tempfile::TempDir;

use super::{Serve, Served, TestProject, call_tool};

/// A project holding `alpha`, `beta`, an untitled entity that refers to a
/// ghost, a Rust source file and an inference manifest, served with the
/// builtin components.
pub fn reading() -> Served {
    TestProject::new()
        .enabling(&["@specforge/software", "@specforge/testing"])
        .file(
            "test.spec",
            "behavior alpha \"Alpha\" {\n  category command\n  contract \"MUST work\"\n  verify unit \"works\"\n}\n\nfeature beta \"Beta\" {\n  behaviors [alpha]\n}\n\nbehavior untitled_one {\n  category command\n  refines [ghost_ref]\n}\n",
        )
        .file("src/lib.rs", "pub fn f() {}\n")
        .file(
            "specforge-infer.json",
            json!({"version": 1, "source_roots": ["src"]}).to_string(),
        )
        .serve_components()
}

/// [`reading`] plus a misformatted file: a fresh one per mutation call.
pub fn writing() -> Served {
    let served = reading();
    served.write(
        "misformatted.spec",
        "behavior gamma \"Gamma\" {\n      contract    \"The system MUST work\"\n}\n",
    );
    served
}

/// A project `@specforge/cargo-test` collects from, with the report an
/// earlier `cargo test` wrote.
pub fn collected() -> TempDir {
    TestProject::new()
        .enabling(&[
            "@specforge/software",
            "@specforge/testing",
            "@specforge/cargo-test",
        ])
        .file(
            "app.spec",
            "behavior alpha \"Alpha\" {\n  verify unit \"works\"\n}\n",
        )
        .file("Cargo.toml", "")
        .file(
            "target/specforge/t.json",
            json!({"entries": [{"entity_id": "alpha", "test_name": "works", "status": "passed"}]})
                .to_string(),
        )
        .into_dir()
}

/// Every read of a core tool, as (tool, arguments as JSON text).
pub const READS: &[(&str, &str)] = &[
    ("specforge.query", r#"{"entity_id":"alpha"}"#),
    (
        "specforge.query",
        r#"{"entity_id":"alpha","format":"brief"}"#,
    ),
    (
        "specforge.query",
        r#"{"entity_id":"alpha","format":"context"}"#,
    ),
    (
        "specforge.query",
        r#"{"entity_id":"alpha","include_coverage":true}"#,
    ),
    ("specforge.validate", "{}"),
    ("specforge.analyze", "{}"),
    ("specforge.export", r#"{"format":"brief"}"#),
    ("specforge.trace", r#"{"entity_id":"alpha"}"#),
    (
        "specforge.trace",
        r#"{"plan":{"plan_id":"p","entries":[{"entity_id":"alpha","action":"implement"},{"entity_id":"nope","action":"implement"}]}}"#,
    ),
    ("specforge.search", r#"{"query":"a"}"#),
    ("specforge.search", r#"{"query":""}"#),
    ("specforge.explain", r#"{"code":"W018"}"#),
    ("specforge.explain", r#"{"code":"E047"}"#),
    ("specforge.schema", r#"{"include_validation_rules":true}"#),
    ("specforge.model", "{}"),
    ("specforge.outline_extensions", "{}"),
    ("specforge.coverage", "{}"),
    ("specforge.stats", "{}"),
    ("specforge.list", "{}"),
    ("specforge.inspect", r#"{"entity_id":"alpha"}"#),
    ("specforge.inspect", r#"{"entity_id":"untitled_one"}"#),
    ("specforge.find_definition", r#"{"entity_id":"alpha"}"#),
    (
        "specforge.find_references",
        r#"{"entity_id":"alpha","direction":"both","include_declaration":true}"#,
    ),
    ("specforge.outline", r#"{"file":"test.spec"}"#),
    ("specforge.suggest_fixes", "{}"),
    ("specforge.extensions", "{}"),
    ("specforge.providers", "{}"),
    ("specforge.doctor", "{}"),
    ("specforge.render", r#"{"format":"brief"}"#),
    ("specforge.infer_progress", "{}"),
    ("specforge.infer_gaps", "{}"),
    ("specforge.find_implementation", r#"{"entity_id":"alpha"}"#),
    (
        "specforge.find_spec_for_source",
        r#"{"file_path":"src/lib.rs"}"#,
    ),
];

/// Every mutation of a core tool, as (tool, arguments); `<elsewhere>` is a
/// fresh directory outside the served project, `<greet>` the greet
/// fixture's blob and `<collected>` [`collected`]'s directory. Each call
/// runs on a fresh [`writing`] server (see [`write_all`] for the repeats).
pub const WRITES: &[(&str, &str)] = &[
    ("specforge.format", r#"{"check":true,"diff":true}"#),
    ("specforge.format", "{}"),
    (
        "specforge.rename",
        r#"{"entity_id":"alpha","new_name":"gamma2","dry_run":true}"#,
    ),
    (
        "specforge.rename",
        r#"{"entity_id":"alpha","new_name":"gamma2"}"#,
    ),
    ("specforge.init", r#"{"path":"<elsewhere>/new"}"#),
    (
        "specforge.add_extension",
        r#"{"specifiers":["@specforge/product"],"dry_run":true}"#,
    ),
    (
        "specforge.add_extension",
        r#"{"specifiers":["@specforge/governance"]}"#,
    ),
    ("specforge.add_extension", r#"{"specifiers":["<greet>"]}"#),
    ("specforge.add_extension", r#"{"specifiers":["<greet>"]}"#),
    (
        "specforge.remove_extension",
        r#"{"name":"@specforge/testing","dry_run":true}"#,
    ),
    ("specforge.migrate", r#"{"dry_run":true}"#),
    (
        "specforge.infer_session",
        r#"{"action":"start","source_roots":["src"]}"#,
    ),
    (
        "specforge.infer_session",
        r#"{"action":"mark_analyzed","source_file":"src/lib.rs"}"#,
    ),
    ("specforge.collect", r#"{"path":"<collected>"}"#),
];

/// One observed call.
pub struct Observed {
    pub tool: &'static str,
    pub arguments: String,
    pub response: Value,
}

impl Observed {
    /// The reply as the pins read it ([`reply`]).
    pub fn reply(&self) -> Value {
        reply(&self.response)
    }
}

/// `structuredContent` when present, else the first text block parsed as
/// JSON, else the text as a string; for an `isError` result,
/// `{"error": <McpError>}`.
pub fn reply(response: &Value) -> Value {
    let result = &response["result"];
    let text = result["content"][0]["text"].as_str().unwrap_or_default();
    let parsed = || serde_json::from_str::<Value>(text).unwrap_or_else(|_| json!(text));
    if result["isError"] == true {
        return json!({ "error": parsed() });
    }
    match result.get("structuredContent") {
        Some(structured) => structured.clone(),
        None => parsed(),
    }
}

/// Every read of [`READS`], on one served [`reading`] project.
pub fn read_all() -> Vec<Observed> {
    let mut served = reading();
    READS
        .iter()
        .map(|(tool, arguments)| Observed {
            tool,
            arguments: (*arguments).to_string(),
            response: call_tool(&mut served, tool, parse(arguments)),
        })
        .collect()
}

/// Every mutation of [`WRITES`]. Each runs on a fresh [`writing`] server,
/// except the second greet install (the already-present reply), which
/// repeats the first on the same server, and `mark_analyzed`, which follows
/// `start` on the same server.
pub fn write_all() -> Vec<Observed> {
    let elsewhere = TempDir::new().expect("a temp dir");
    let greet = specforge_component::committed::GREET.path();
    // The project a collect names lies inside the served one (ADR 0092), so
    // it is built for the call, not before the server.
    let fill = |arguments: &str, served: &Served| {
        let arguments = arguments
            .replace("<elsewhere>", elsewhere.path().to_str().expect("UTF-8"))
            .replace("<greet>", greet.to_str().expect("UTF-8"));
        if arguments.contains("<collected>") {
            let collected = super::within(served.root(), collected());
            return arguments.replace("<collected>", collected.path().to_str().expect("UTF-8"));
        }
        arguments
    };
    let mut observed = Vec::new();
    let mut served = writing();
    let mut previous: Option<(&str, &str)> = None;
    for &(tool, arguments) in WRITES {
        let continues = previous.is_some_and(|(before_tool, before)| {
            before_tool == tool
                && ((tool == "specforge.add_extension" && before == arguments)
                    || (tool == "specforge.infer_session" && before.contains("start")))
        });
        if !continues {
            served = writing();
        }
        let filled = fill(arguments, &served);
        // Init creates the first project of a session: none is served for it.
        let response = if tool == "specforge.init" {
            call_tool(&mut super::unserved(), tool, parse(&filled))
        } else {
            call_tool(&mut served, tool, parse(&filled))
        };
        observed.push(Observed {
            tool,
            response,
            arguments: arguments.to_string(),
        });
        previous = Some((tool, arguments));
    }
    observed
}

fn parse(arguments: &str) -> Value {
    serde_json::from_str(arguments).expect("the arguments are JSON")
}
