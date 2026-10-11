//! `ExtensionCalls`: every operation the host performs on a loaded
//! extension, typed. Each test serves one SDK-declared extension in process
//! (`InProcessRuntime`); its exports decode their input as the SDK's types
//! and answer the SDK's types, so what crosses is exactly what a guest
//! built with the SDK reads and writes. The host's bytes are compared with
//! the goldens in `tests/wire/`.

use std::cell::RefCell;

use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::{Map, Value, json};
use specforge_common::{Severity, SourceSpan, Sym};
use specforge_extension_sdk::prelude::*;
use specforge_extension_sdk::{
    McpResourceContent, McpResourceRequest, MigrationInput, PassAnswer, ScanRequest, ScanResponse,
    ScannedItem, Served,
};
use specforge_protocol_types::{CommandInput as WireCommandInput, DeclaredCategory, RawGraph};
use specforge_wasm::testing::InProcessRuntime;
use specforge_wasm::{
    CallError, CallFailure, ExtensionCalls, Operation, WasmCallResult, WasmRuntime, WasmTrapInfo,
    pass_diagnostics,
};

const EXT: &str = "@calls/x";

thread_local! {
    /// What the guest's exports decoded, re-encoded, in call order.
    static RECEIVED: RefCell<Vec<(String, Value)>> = const { RefCell::new(Vec::new()) };
}

fn received(export: &str) -> Value {
    RECEIVED.with(|r| {
        r.borrow()
            .iter()
            .rev()
            .find(|(e, _)| e == export)
            .map(|(_, v)| v.clone())
            .unwrap_or_else(|| panic!("{export} received nothing"))
    })
}

/// Decode `input` as `T` (what the guest reads), and record it.
fn receive<T: DeserializeOwned + Serialize>(export: &str, input: &[u8]) -> Result<T, String> {
    let value: T = serde_json::from_slice(input).map_err(|e| e.to_string())?;
    RECEIVED.with(|r| {
        r.borrow_mut()
            .push((export.to_string(), serde_json::to_value(&value).unwrap()))
    });
    Ok(value)
}

fn answer<T: Serialize>(value: &T) -> Result<Vec<u8>, String> {
    serde_json::to_vec(value).map_err(|e| e.to_string())
}

// The answers the guest builds with the SDK's types, which the host must
// read back equal.

fn command_answer() -> CommandOutput {
    CommandOutput {
        exit_code: 3,
        stdout: "out\n".into(),
        stderr: "err\n".into(),
    }
}

fn resource_answer(uri: &str) -> McpResourceContent {
    McpResourceContent {
        content: format!("content of {uri}"),
        mime_type: "text/plain".into(),
    }
}

fn pass_answer() -> PassOutput {
    let mut summary = Map::new();
    summary.insert("audited".into(), json!(2));
    PassOutput {
        diagnostics: vec![
            PassDiagnostic::warning("W1", "unspanned").with_entity("a"),
            PassDiagnostic::new("E1", PassSeverity::Error, "spanned")
                .with_span(PassSpan {
                    file: "a.spec".into(),
                    start_line: 2,
                    start_col: 1,
                    end_line: 2,
                    end_col: 9,
                })
                .with_suggestion("fix it"),
        ],
        summary,
    }
}

fn collect_answer() -> CollectOutput {
    CollectOutput {
        entity_results: vec![CollectEntityResult {
            entity_id: "a".into(),
            test_results: vec![CollectTestResult {
                name: "a_works".into(),
                status: "passed".into(),
                verify: Some("a works".into()),
                duration_ms: Some(1.5),
            }],
        }],
        unlinked: vec![CollectUnlinkedTest {
            name: "m::t".into(),
            path: vec!["m".into(), "t".into()],
            status: "failed".into(),
        }],
    }
}

fn verdict_answer() -> ValidatorVerdict {
    ValidatorVerdict::Fail {
        field: Some("needs".into()),
        value: Some("b".into()),
    }
}

fn scan_answer(request: &ScanRequest) -> ScanResponse {
    ScanResponse {
        items: vec![ScannedItem {
            name: request.file_path.clone(),
            item_kind: "function".into(),
            line: 1,
            visibility: Some("pub".into()),
            signature: None,
        }],
        language: Some("rust".into()),
    }
}

/// The exports no declaration answers, as a guest's `handler` serves them:
/// each decodes its input as the SDK's type and answers the SDK's type.
fn guest(export: &str, input: &[u8]) -> Option<Result<Vec<u8>, String>> {
    Some(match export {
        "cmd__raw" => {
            receive::<CommandInput>(export, input).and_then(|_| answer(&command_answer()))
        }
        "mcp__tool" => receive::<Value>(export, input)
            .and_then(|arguments| answer(&json!({ "echo": arguments }))),
        "mcp__resource" => receive::<McpResourceRequest>(export, input)
            .and_then(|request| answer(&resource_answer(&request.uri))),
        "__pass_audit" => receive::<PassInput>(export, input)
            .and_then(|_| answer(&PassAnswer::from(pass_answer()))),
        "__pass_bare" => receive::<PassInput>(export, input)
            .and_then(|_| answer(&PassAnswer::from(pass_answer().diagnostics))),
        "collect__x" => {
            receive::<CollectInput>(export, input).and_then(|_| answer(&collect_answer()))
        }
        "validate__x" => {
            receive::<ValidatorContext>(export, input).and_then(|_| answer(&verdict_answer()))
        }
        "scan__x" => {
            receive::<ScanRequest>(export, input).and_then(|request| answer(&scan_answer(&request)))
        }
        _ => return None,
    })
}

fn extension() -> ContributionsBuilder {
    let mut c = ContributionsBuilder::new(ExtensionMeta::new(EXT, "1.0.0"));
    c.kind("widget", |k| {
        k.description("w");
    });
    // Declared with its handler, which records what it decoded.
    c.migration_hook_handler("migrate__x", |input: &MigrationInput| {
        RECEIVED.with(|r| {
            r.borrow_mut().push((
                "migrate__x".to_string(),
                serde_json::to_value(input).unwrap(),
            ))
        });
        Ok(())
    });
    c
}

fn runtime() -> InProcessRuntime {
    RECEIVED.with(|r| r.borrow_mut().clear());
    InProcessRuntime::new().with_handler(extension, guest)
}

fn wire(name: &str) -> Value {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/wire")
        .join(name);
    serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap()
}

fn typed<T: DeserializeOwned>(value: &Value) -> T {
    serde_json::from_value(value.clone()).unwrap()
}

/// The bytes the host sent with its last call, as JSON.
fn sent(runtime: &InProcessRuntime) -> Value {
    runtime
        .calls()
        .last()
        .expect("a call was made")
        .input
        .clone()
}

/// The host's command input of the golden: its graph spliced as rendered.
fn command_input() -> WireCommandInput<RawGraph> {
    let golden = wire("command.input.json");
    WireCommandInput {
        args: golden["args"].as_object().unwrap().clone(),
        cwd: golden["cwd"].as_str().unwrap().to_string(),
        format: typed(&golden["format"]),
        today: golden["today"].as_str().unwrap().to_string(),
        graph: RawGraph::new(golden["graph"].to_string()).unwrap(),
        evidence: Default::default(),
    }
}

fn validator_context() -> ValidatorContext {
    typed(&wire("validate.input.json")[1])
}

#[specforge_test_macros::test(
    behavior = "call_extension_exports",
    verify = "every extension call encodes its input as the protocol type the SDK decodes"
)]
fn every_call_encodes_its_input_as_the_protocol_type() {
    let runtime = runtime();
    let calls = ExtensionCalls::new(&runtime);

    calls.handshake(EXT).unwrap();
    assert_eq!(sent(&runtime), wire("handshake.input.json"));
    calls.describe(EXT, DeclaredCategory::Entities).unwrap();
    assert_eq!(sent(&runtime), wire("describe.input.json"));

    // The graph is spliced as the host rendered it; the guest reads the
    // nodes and edges it names.
    calls
        .run_command(EXT, "cmd__raw", &command_input())
        .unwrap();
    let golden = wire("command.input.json");
    assert_eq!(sent(&runtime), golden);
    let mut read = golden.clone();
    read["graph"] = json!({"nodes": [{"id": "f1", "kind": "feature", "title": "One", "fields": {}}],
                           "edges": []});
    assert_eq!(received("cmd__raw"), read);

    let arguments = json!({"query": "x", "limit": 2});
    calls.call_mcp_tool(EXT, "mcp__tool", &arguments).unwrap();
    assert_eq!(sent(&runtime), arguments);
    assert_eq!(received("mcp__tool"), arguments);

    calls
        .read_mcp_resource(EXT, "mcp__resource", "u://r")
        .unwrap();
    assert_eq!(sent(&runtime), wire("resource.input.json"));
    assert_eq!(received("mcp__resource"), wire("resource.input.json"));

    for golden in ["pass.input.json", "pass.check.input.json"] {
        let input: PassInput = typed(&wire(golden));
        let encoded = ExtensionCalls::encode(&input);
        calls.run_pass(EXT, "audit", &encoded).unwrap();
        assert_eq!(sent(&runtime), wire(golden), "{golden}");
        assert_eq!(received("__pass_audit"), wire(golden), "{golden}");
    }

    let input: CollectInput = typed(&wire("collect.input.json"));
    calls.collect(EXT, "collect__x", &input).unwrap();
    assert_eq!(sent(&runtime), wire("collect.input.json"));
    assert_eq!(received("collect__x"), wire("collect.input.json"));

    calls
        .validate(EXT, "validate__x", &validator_context())
        .unwrap();
    assert_eq!(sent(&runtime), wire("validate.input.json")[1]);
    assert_eq!(received("validate__x"), wire("validate.input.json")[1]);

    let request: ScanRequest = typed(&wire("scan.input.json"));
    calls.scan(EXT, "scan__x", &request).unwrap();
    assert_eq!(sent(&runtime), wire("scan.input.json"));
    assert_eq!(received("scan__x"), wire("scan.input.json"));

    let input: MigrationInput = typed(&wire("migrate.input.json"));
    calls.migrate(EXT, "migrate__x", &input).unwrap();
    assert_eq!(sent(&runtime), wire("migrate.input.json"));
    assert_eq!(received("migrate__x"), wire("migrate.input.json"));
}

#[specforge_test_macros::test(
    behavior = "call_extension_exports",
    verify = "every extension call decodes the protocol type the SDK encodes"
)]
fn every_call_decodes_the_protocol_type() {
    let runtime = runtime();
    let calls = ExtensionCalls::new(&runtime);

    let handshake = calls.handshake(EXT).unwrap().response;
    assert_eq!(handshake, extension().declaration().handshake);
    let describe = calls.describe(EXT, DeclaredCategory::Entities).unwrap();
    assert_eq!(describe.category, "entities");
    assert_eq!(describe.items[0]["name"], "widget");
    assert_eq!(
        calls
            .run_command(EXT, "cmd__raw", &command_input())
            .unwrap(),
        command_answer()
    );
    assert_eq!(
        calls
            .call_mcp_tool(EXT, "mcp__tool", &json!({"q": 1}))
            .unwrap(),
        json!({"echo": {"q": 1}})
    );
    assert_eq!(
        calls
            .read_mcp_resource(EXT, "mcp__resource", "u://r")
            .unwrap(),
        resource_answer("u://r")
    );
    let input = ExtensionCalls::encode(&PassInput::default());
    assert_eq!(calls.run_pass(EXT, "audit", &input).unwrap(), pass_answer());
    assert_eq!(
        calls
            .collect(EXT, "collect__x", &CollectInput::default())
            .unwrap(),
        collect_answer()
    );
    assert_eq!(
        calls
            .validate(EXT, "validate__x", &validator_context())
            .unwrap(),
        verdict_answer()
    );
    let request = ScanRequest {
        file_path: "a.rs".into(),
        content: String::new(),
    };
    assert_eq!(
        calls.scan(EXT, "scan__x", &request).unwrap(),
        scan_answer(&request)
    );
    assert_eq!(
        calls.migrate(EXT, "migrate__x", &MigrationInput::default()),
        Ok(())
    );
}

/// The export each operation is called by in these tests.
fn export_of(operation: Operation) -> &'static str {
    match operation {
        Operation::Handshake => "__handshake",
        Operation::Describe => "__describe",
        Operation::Command => "cmd__raw",
        Operation::McpTool => "mcp__tool",
        Operation::McpResource => "mcp__resource",
        Operation::Pass => "__pass_audit",
        Operation::Collect => "collect__x",
        Operation::Validate => "validate__x",
        Operation::Scan => "scan__x",
        Operation::Migrate => "migrate__x",
    }
}

/// Perform `operation` on `extension`, its answer dropped.
fn perform(calls: &ExtensionCalls, operation: Operation, extension: &str) -> Result<(), CallError> {
    let export = export_of(operation);
    match operation {
        Operation::Handshake => calls.handshake(extension).map(drop),
        Operation::Describe => calls
            .describe(extension, DeclaredCategory::Entities)
            .map(drop),
        Operation::Command => calls
            .run_command(extension, export, &command_input())
            .map(drop),
        Operation::McpTool => calls.call_mcp_tool(extension, export, &json!({})).map(drop),
        Operation::McpResource => calls
            .read_mcp_resource(extension, export, "u://r")
            .map(drop),
        Operation::Pass => {
            let input = ExtensionCalls::encode(&PassInput::default());
            calls.run_pass(extension, "audit", &input).map(drop)
        }
        Operation::Collect => calls
            .collect(extension, export, &CollectInput::default())
            .map(drop),
        Operation::Validate => calls
            .validate(extension, export, &validator_context())
            .map(drop),
        Operation::Scan => calls
            .scan(
                extension,
                export,
                &ScanRequest {
                    file_path: "a.rs".into(),
                    content: String::new(),
                },
            )
            .map(drop),
        Operation::Migrate => calls
            .migrate(extension, export, &MigrationInput::default())
            .map(drop),
    }
}

#[specforge_test_macros::test(
    behavior = "call_extension_exports",
    verify = "a call whose export trapped is E028 naming the extension, the operation and the export"
)]
fn a_trapped_call_is_e028_naming_the_operation_the_export_and_the_extension() {
    for operation in Operation::ALL {
        let export = export_of(operation);
        for kind in ["guest_error", "deadline_exceeded", "call_failed"] {
            let runtime = runtime().answer_raw(
                EXT,
                export,
                WasmCallResult::Trap(WasmTrapInfo {
                    kind: kind.into(),
                    message: "it broke".into(),
                    export_name: export.into(),
                }),
            );
            let err = perform(&ExtensionCalls::new(&runtime), operation, EXT).unwrap_err();
            assert_eq!(
                err,
                CallError::new(
                    operation,
                    EXT,
                    export,
                    CallFailure::Trapped {
                        kind: kind.into(),
                        message: "it broke".into()
                    }
                )
            );
            let diagnostic = err.diagnostic();
            assert_eq!(diagnostic.code, "E028");
            assert_eq!(diagnostic.severity, Severity::Error);
            assert_eq!(
                diagnostic.message,
                format!(
                    "{} {export}() of '{EXT}' trapped: {kind}: it broke",
                    operation.label()
                )
            );
            assert_eq!(
                diagnostic.suggestion.as_deref(),
                Some(
                    format!(
                        "report the failure to the author of '{EXT}', or check it is installed and up to date"
                    )
                    .as_str()
                )
            );
        }
        // An extension the runtime did not load.
        let runtime = runtime();
        let err = perform(&ExtensionCalls::new(&runtime), operation, "@calls/absent").unwrap_err();
        assert_eq!(err.failure, CallFailure::NotLoaded, "{operation:?}");
        assert_eq!(
            err.to_string(),
            format!(
                "{} {export}() of '@calls/absent' is not loaded",
                operation.label()
            )
        );
    }
}

#[specforge_test_macros::test(
    behavior = "call_extension_exports",
    verify = "a call whose answer does not decode as its protocol type is E028, never a default"
)]
fn an_answer_that_does_not_decode_is_e028() {
    // Per operation: the type it owes, and answers that are not one —
    // not JSON, the wrong JSON type, a required field missing. Any JSON
    // value is a tool's answer; a migration hook's is not read.
    let table: [(Operation, &str, &[&[u8]]); 9] = [
        (
            Operation::Handshake,
            "HandshakeResponse",
            &[b"not json", b"[]", br#"{"name":"x"}"#],
        ),
        (
            Operation::Describe,
            "DescribeResponse",
            &[b"not json", b"[]", br#"{"category":"entities"}"#],
        ),
        (
            Operation::Command,
            "CommandOutput",
            &[
                b"not json at all",
                b"[1,2]",
                br#"{"exit_code":"3","stdout":"x"}"#,
                b"{}",
                br#"{"stdout":"x"}"#,
            ],
        ),
        (Operation::McpTool, "JSON value", &[b"not json"]),
        (
            Operation::McpResource,
            "McpResourceContent",
            &[
                b"oops",
                br#""text""#,
                br#"{"text":"t"}"#,
                br#"{"content":"c"}"#,
            ],
        ),
        (
            Operation::Pass,
            "PassAnswer",
            &[
                b"not json",
                br#""x""#,
                br#"[{"code":"W1"}]"#,
                br#"{"summary":{}}"#,
            ],
        ),
        (
            Operation::Collect,
            "CollectOutput",
            &[
                b"not json",
                b"[]",
                b"{}",
                br#"{"entity_results":[{"entity_id":"a","test_results":[{"status":"passed"}]}]}"#,
            ],
        ),
        (
            Operation::Validate,
            "ValidatorVerdict",
            &[b"not json", br#""pass""#, br#"{"field":"x"}"#],
        ),
        (
            Operation::Scan,
            "ScanResponse",
            &[b"not json", b"[]", br#"{"language":"rust"}"#],
        ),
    ];
    for (operation, expected, answers) in table {
        let export = export_of(operation);
        for raw in answers {
            let runtime = runtime().answer_raw(EXT, export, WasmCallResult::Ok(raw.to_vec()));
            let err = perform(&ExtensionCalls::new(&runtime), operation, EXT).unwrap_err();
            let shown = String::from_utf8_lossy(raw);
            match &err.failure {
                CallFailure::Malformed { expected: e, .. } => {
                    assert_eq!(*e, expected, "{operation:?} {shown}")
                }
                other => panic!("{operation:?} {shown}: {other:?}"),
            }
            let diagnostic = err.diagnostic();
            assert_eq!(diagnostic.code, "E028");
            assert!(
                diagnostic.message.starts_with(&format!(
                    "{} {export}() of '{EXT}' answered output that is not a {expected}: ",
                    operation.label()
                )),
                "{}",
                diagnostic.message
            );
        }
    }
    // A migration hook's answer is not read: whatever it is, the call
    // succeeded.
    for raw in [&b"not json"[..], b"", b"{}"] {
        let runtime = runtime().answer_raw(EXT, "migrate__x", WasmCallResult::Ok(raw.to_vec()));
        assert_eq!(
            perform(&ExtensionCalls::new(&runtime), Operation::Migrate, EXT),
            Ok(())
        );
    }
}

#[specforge_test_macros::test(
    behavior = "call_extension_exports",
    verify = "an unknown field in an answer is ignored and an absent optional field takes its default"
)]
fn unknown_fields_are_ignored_and_absent_optional_fields_default() {
    let answering = |export: &str, raw: &str| {
        runtime().answer_raw(EXT, export, WasmCallResult::Ok(raw.as_bytes().to_vec()))
    };
    let runtime = answering("cmd__raw", r#"{"exit_code":0,"newer":true}"#);
    let output = ExtensionCalls::new(&runtime)
        .run_command(EXT, "cmd__raw", &command_input())
        .unwrap();
    assert_eq!(output, CommandOutput::ok(""));

    let runtime = answering(
        "mcp__resource",
        r#"{"content":"c","mime_type":"text/plain","etag":"1"}"#,
    );
    let content = ExtensionCalls::new(&runtime)
        .read_mcp_resource(EXT, "mcp__resource", "u://r")
        .unwrap();
    assert_eq!(content.content, "c");

    let runtime = answering(
        "__pass_audit",
        r#"[{"code":"W1","severity":"Warning","message":"m","newer":1}]"#,
    );
    let input = ExtensionCalls::encode(&PassInput::default());
    let output = ExtensionCalls::new(&runtime)
        .run_pass(EXT, "audit", &input)
        .unwrap();
    assert_eq!(output.diagnostics[0].span, None);
    assert!(output.summary.is_empty());

    let runtime = answering(
        "collect__x",
        r#"{"entity_results":[{"entity_id":"a","test_results":[{"name":"t","status":"passed","newer":1}]}]}"#,
    );
    let collected = ExtensionCalls::new(&runtime)
        .collect(EXT, "collect__x", &CollectInput::default())
        .unwrap();
    assert!(collected.unlinked.is_empty());
    assert_eq!(collected.entity_results[0].test_results[0].verify, None);

    let runtime = answering("scan__x", r#"{"items":[],"newer":1}"#);
    let scanned = ExtensionCalls::new(&runtime)
        .scan(
            EXT,
            "scan__x",
            &ScanRequest {
                file_path: "a.rs".into(),
                content: String::new(),
            },
        )
        .unwrap();
    assert_eq!(scanned.language, None);

    let runtime = answering("validate__x", r#"{"verdict":"fail","newer":1}"#);
    let verdict = ExtensionCalls::new(&runtime)
        .validate(EXT, "validate__x", &validator_context())
        .unwrap();
    assert_eq!(
        verdict,
        ValidatorVerdict::Fail {
            field: None,
            value: None
        }
    );

    let runtime = answering(
        "__handshake",
        r#"{"protocol_version":"1.0.0","name":"@calls/x","version":"1.0.0","contribution_flags":{},"peer_dependencies":[],"newer":1}"#,
    );
    let handshake = ExtensionCalls::new(&runtime)
        .handshake(EXT)
        .unwrap()
        .response;
    assert_eq!(handshake.sandbox_policy, None);
    assert_eq!(handshake.starter_template, None);
}

fn span(line: usize) -> SourceSpan {
    SourceSpan {
        file: Sym::new("a.spec"),
        start_line: line,
        start_col: 1,
        end_line: line,
        end_col: 2,
    }
}

#[specforge_test_macros::test(
    behavior = "call_extension_exports",
    verify = "a pass answer may be bare diagnostics or diagnostics with a summary, and its diagnostics come back in canonical order with an entity's span attached"
)]
fn a_pass_answer_is_bare_or_with_a_summary_and_its_diagnostics_are_canonical() {
    let runtime = runtime();
    let calls = ExtensionCalls::new(&runtime);
    let input = ExtensionCalls::encode(&PassInput::default());
    let with_summary = calls.run_pass(EXT, "audit", &input).unwrap();
    assert_eq!(with_summary.summary["audited"], 2);
    let bare = calls.run_pass(EXT, "bare", &input).unwrap();
    assert!(bare.summary.is_empty());
    assert_eq!(bare.diagnostics, with_summary.diagnostics);

    // The guest built W1 (naming `a`, no span) before E1 (spanned): they
    // come back E1 then W1, W1 with `a`'s span; an entity the graph does
    // not have gives no span.
    let span_of = |id: &str| (id == "a").then(|| span(7));
    let diagnostics = pass_diagnostics(EXT, bare, span_of);
    // Each names the extension that reported it.
    assert!(diagnostics.iter().all(|d| d.origin() == Some(EXT)));
    let shown: Vec<(&str, Severity, Option<usize>, Option<&str>)> = diagnostics
        .iter()
        .map(|d| {
            (
                d.code.as_str(),
                d.severity,
                d.span.as_ref().map(|s| s.start_line),
                d.suggestion.as_deref(),
            )
        })
        .collect();
    assert_eq!(
        shown,
        [
            ("E1", Severity::Error, Some(2), Some("fix it")),
            ("W1", Severity::Warning, Some(7), None),
        ]
    );
    // W1 names `a`: the host carries it as the subject (ADR 0016); E1
    // names nothing and carries nothing.
    let data: Vec<_> = diagnostics.iter().map(|d| d.data.as_deref()).collect();
    assert_eq!(
        data,
        [
            None,
            Some(&specforge_common::DiagnosticData::Subject { entity: "a".into() })
        ]
    );

    // Same code: by file and line, then message.
    let output = PassOutput {
        diagnostics: vec![
            PassDiagnostic::warning("X", "b").with_span(PassSpan {
                file: "z.spec".into(),
                start_line: 1,
                start_col: 1,
                end_line: 1,
                end_col: 2,
            }),
            PassDiagnostic::warning("X", "c").with_entity("a"),
            PassDiagnostic::warning("X", "a").with_entity("a"),
            PassDiagnostic::warning("X", "none").with_entity("ghost"),
        ],
        summary: Map::new(),
    };
    let order: Vec<String> = pass_diagnostics(EXT, output, span_of)
        .into_iter()
        .map(|d| d.message)
        .collect();
    assert_eq!(order, ["none", "a", "c", "b"]);
}

#[specforge_test_macros::test(
    behavior = "call_extension_exports",
    verify = "Call Extension Exports: extension calls hold — extension_loaded, one_protocol_type, strict_answers, one_failure, no_silent_failure, runtimes_agree"
)]
fn extension_calls_hold_their_contract() {
    let runtime = runtime();
    let calls = ExtensionCalls::new(&runtime);

    // extension_loaded: an extension the runtime did not load is a failure
    // of the call, never an empty answer.
    assert_eq!(
        calls.handshake("@calls/absent").unwrap_err().failure,
        CallFailure::NotLoaded
    );

    // one_protocol_type: what the guest built with the SDK is what the host
    // reads, typed.
    assert_eq!(
        calls
            .run_command(EXT, "cmd__raw", &command_input())
            .unwrap(),
        command_answer()
    );

    // strict_answers: an answer missing a required field is not defaulted.
    let strict =
        InProcessRuntime::new().answer_raw(EXT, "cmd__raw", WasmCallResult::Ok(b"{}".to_vec()));
    let err = ExtensionCalls::new(&strict)
        .run_command(EXT, "cmd__raw", &command_input())
        .unwrap_err();
    assert!(
        matches!(err.failure, CallFailure::Malformed { .. }),
        "{err}"
    );

    // one_failure: an unrouted export is the guest's error, E028 naming the
    // operation, the export and the extension.
    let err = calls
        .collect(EXT, "collect__unrouted", &CollectInput::default())
        .unwrap_err();
    assert_eq!(
        err.diagnostic().message,
        format!(
            "collector collect__unrouted() of '{EXT}' trapped: guest_error: unknown export 'collect__unrouted'"
        )
    );

    // no_silent_failure: every operation reports a failure as an Err.
    for operation in Operation::ALL {
        let export = export_of(operation);
        let failing = InProcessRuntime::new().answer_raw(
            EXT,
            export,
            WasmCallResult::Trap(WasmTrapInfo {
                kind: "call_failed".into(),
                message: "unreachable".into(),
                export_name: export.into(),
            }),
        );
        assert!(
            perform(&ExtensionCalls::new(&failing), operation, EXT).is_err(),
            "{operation:?}"
        );
    }

    // runtimes_agree: the in-process runtime answers what the guest's own
    // routing (the component's `call`) answers.
    let input = serde_json::to_vec(&command_input()).unwrap();
    let routed = Served::new(extension())
        .call(guest, "cmd__raw", &input)
        .unwrap();
    match runtime.call_export(EXT, "cmd__raw", &input) {
        WasmCallResult::Ok(bytes) => assert_eq!(bytes, routed),
        WasmCallResult::Trap(trap) => panic!("{trap:?}"),
    }
}

#[test]
fn a_host_input_that_does_not_encode_is_never_sent() {
    #[derive(Debug)]
    struct Unencodable;
    impl Serialize for Unencodable {
        fn serialize<S: serde::Serializer>(&self, _: S) -> Result<S::Ok, S::Error> {
            Err(serde::ser::Error::custom("no"))
        }
    }
    let encoded = ExtensionCalls::encode(&Unencodable);
    let failure = CallFailure::Unencodable {
        reason: "no".into(),
    };
    assert_eq!(encoded.failure(), Some(&failure));
    let err = CallError::new(Operation::Pass, EXT, "__pass_audit", failure);
    assert_eq!(
        err.to_string(),
        format!(
            "compiler pass __pass_audit() of '{EXT}' was not called: its input does not encode: no"
        )
    );
}
