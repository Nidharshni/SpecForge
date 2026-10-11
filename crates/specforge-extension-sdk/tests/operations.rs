//! Operational contributions are declared with their handlers: a pass, a
//! collector, a custom rule, a scanner and the migration hook each answer
//! their export through the handler declared with them, reading and
//! writing the protocol's types, and a declaration without its handler is
//! refused when the extension is built.

use serde_json::json;
use specforge_extension_sdk::answer_export;
use specforge_extension_sdk::prelude::*;
use specforge_protocol_types::pass_export;

fn extension() -> ContributionsBuilder {
    let mut c = ContributionsBuilder::new(ExtensionMeta::new("@acme/ops", "1.0.0"));
    c.pass("audit", |p| {
        p.phase("check").run(|input: &PassInput| {
            input
                .entities
                .iter()
                .filter(|e| !e.owes_none)
                .map(|e| {
                    PassDiagnostic::warning("W1", format!("{} audited", e.id)).with_entity(&e.id)
                })
                .collect::<Vec<_>>()
        });
    });
    c.pass("count", |p| {
        p.run(|input: &PassInput| {
            let mut summary = serde_json::Map::new();
            summary.insert("entities".into(), input.entities.len().into());
            PassOutput {
                diagnostics: Vec::new(),
                summary,
            }
        });
    });
    c.collector("acme-test", |k| {
        k.report("out.txt").collect(|input: &CollectInput| {
            Ok(CollectOutput {
                entity_results: vec![CollectEntityResult {
                    entity_id: input.reports[0].content.trim().to_string(),
                    test_results: vec![CollectTestResult {
                        name: "t".into(),
                        status: "passed".into(),
                        verify: None,
                        duration_ms: None,
                    }],
                }],
                unlinked: Vec::new(),
            })
        });
    });
    c.rule("E9", |r| {
        r.check(CheckKind::Custom)
            .message_template("bad {id}")
            .validate(|context: &ValidatorContext| {
                if context.entity.id == "bad" {
                    ValidatorVerdict::Fail {
                        field: Some("x".into()),
                        value: None,
                    }
                } else {
                    ValidatorVerdict::Pass
                }
            });
    });
    c.analyzer("acme", |a| {
        a.file_extensions(&[".acme"])
            .scan(|request: &ScanRequest| ScanResponse {
                items: request
                    .content
                    .lines()
                    .enumerate()
                    .map(|(i, line)| ScannedItem {
                        name: line.to_string(),
                        item_kind: "line".into(),
                        line: i + 1,
                        visibility: None,
                        signature: None,
                    })
                    .collect(),
                language: Some("acme".into()),
            });
    });
    c.migration_hook_handler("migrate__acme", |input: &MigrationInput| {
        if input.files.is_empty() {
            Err("nothing to migrate".to_string())
        } else {
            Ok(())
        }
    });
    c
}

fn call(export: &str, input: serde_json::Value) -> Result<serde_json::Value, String> {
    extension()
        .dispatch_export(export, input.to_string().as_bytes())
        .unwrap_or_else(|| panic!("{export} is not routed"))
        .map(|bytes| {
            if bytes.is_empty() {
                serde_json::Value::Null
            } else {
                serde_json::from_slice(&bytes).unwrap()
            }
        })
}

#[specforge_test_macros::test(
    behavior = "call_extension_exports",
    verify = "a pass, collector, custom rule, scanner or migration hook is declared with its handler, and its export answers through it"
)]
fn every_operation_answers_through_its_declared_handler() {
    let declaration = extension().declaration();
    // The declarations name the exports the handlers answer.
    assert_eq!(declaration.passes.len(), 2);
    assert_eq!(declaration.collectors[0].export, "collect__acme_test");
    assert_eq!(
        declaration.validation_rules[0].wasm_function.as_deref(),
        Some("validate__e9")
    );
    assert_eq!(declaration.analyzers[0].scan_export, "scan__acme");
    assert_eq!(
        declaration.handshake.migration_hook.as_deref(),
        Some("migrate__acme")
    );

    let entities = json!({"entities": [
        {"id": "a", "kind": "k"}, {"id": "b", "kind": "k", "owes_none": true}
    ]});
    assert_eq!(
        call("__pass_audit", entities.clone()).unwrap(),
        json!([{"code": "W1", "severity": "Warning", "message": "a audited", "entity": "a"}])
    );
    assert_eq!(
        call("__pass_count", entities).unwrap(),
        json!({"diagnostics": [], "summary": {"entities": 2}})
    );
    assert_eq!(
        call(
            "collect__acme_test",
            json!({"reports": [{"path": "out.txt", "content": "e1\n"}]})
        )
        .unwrap(),
        json!({"entity_results": [{"entity_id": "e1",
            "test_results": [{"name": "t", "status": "passed"}]}]})
    );
    let context = |id: &str| {
        json!({"entity": {"id": id, "kind": "k", "fields": [], "methods": []},
               "referenced": [], "declared_types": [], "primitives": []})
    };
    assert_eq!(
        call("validate__e9", context("bad")).unwrap(),
        json!({"verdict": "fail", "field": "x"})
    );
    assert_eq!(
        call("validate__e9", context("good")).unwrap(),
        json!({"verdict": "pass"})
    );
    assert_eq!(
        call(
            "scan__acme",
            json!({"file_path": "a.acme", "content": "x\ny"})
        )
        .unwrap(),
        json!({"items": [
            {"name": "x", "item_kind": "line", "line": 1, "visibility": null, "signature": null},
            {"name": "y", "item_kind": "line", "line": 2, "visibility": null, "signature": null}
        ], "language": "acme"})
    );
    assert_eq!(
        call(
            "migrate__acme",
            json!({"from": "0.9", "to": "1.0", "files": ["a.spec"]})
        ),
        Ok(serde_json::Value::Null)
    );
    assert_eq!(
        call(
            "migrate__acme",
            json!({"from": "0.9", "to": "1.0", "files": []})
        ),
        Err("nothing to migrate".to_string())
    );
    // An input that is not the operation's type is the guest's error.
    let err = call("__pass_audit", json!({"nope": 1})).unwrap_err();
    assert!(err.starts_with("invalid pass input: "), "{err}");
    // An export no declaration names is not routed here.
    assert!(extension().dispatch_export("__pass_other", b"{}").is_none());
}

#[test]
#[should_panic(expected = "pass 'p' declares no handler")]
fn a_pass_without_its_handler_is_refused() {
    let mut c = ContributionsBuilder::new(ExtensionMeta::new("@acme/x", "1.0.0"));
    c.pass("p", |p| {
        p.phase("check");
    });
}

#[test]
#[should_panic(expected = "collector 'k' declares no handler")]
fn a_collector_without_its_handler_is_refused() {
    let mut c = ContributionsBuilder::new(ExtensionMeta::new("@acme/x", "1.0.0"));
    c.collector("k", |k| {
        k.report("r.json");
    });
}

#[test]
#[should_panic(expected = "custom rule 'E1' declares no handler")]
fn a_custom_rule_without_its_handler_is_refused() {
    let mut c = ContributionsBuilder::new(ExtensionMeta::new("@acme/x", "1.0.0"));
    c.rule("E1", |r| {
        r.check(CheckKind::Custom).wasm_function("validate__e1");
    });
}

#[test]
#[should_panic(expected = "rule 'E1' has a validate handler, but its check is not custom")]
fn a_declarative_rule_with_a_handler_is_refused() {
    let mut c = ContributionsBuilder::new(ExtensionMeta::new("@acme/x", "1.0.0"));
    c.rule("E1", |r| {
        r.check(CheckKind::NoEdges)
            .validate(|_| ValidatorVerdict::Pass);
    });
}

#[test]
#[should_panic(expected = "analyzer 'acme' declares no scanner")]
fn an_analyzer_without_its_scanner_is_refused() {
    let mut c = ContributionsBuilder::new(ExtensionMeta::new("@acme/x", "1.0.0"));
    c.analyzer("acme", |a| {
        a.file_extensions(&[".acme"]);
    });
}

#[test]
#[should_panic(expected = "export cmd__dup is already")]
fn an_operation_and_a_surface_cannot_share_an_export() {
    let mut c = ContributionsBuilder::new(ExtensionMeta::new("@acme/x", "1.0.0"));
    c.rule("E1", |r| {
        r.check(CheckKind::Custom)
            .wasm_function("cmd__dup")
            .validate(|_| ValidatorVerdict::Pass);
    });
    c.command("dup", |cmd| {
        cmd.title("Dup").handler(|_| CommandOutput::ok(""));
    });
}

#[specforge_test_macros::test(
    behavior = "call_extension_exports",
    verify = "an export the guest's handler answers decodes its input and encodes its answer as a declared handler does"
)]
fn answer_export_names_a_bad_input_as_a_declared_handler_does() {
    let mut handled = false;
    let by_hand = answer_export::<ScanRequest, ScanResponse>("scan", b"nope", |_| {
        handled = true;
        ScanResponse {
            items: Vec::new(),
            language: None,
        }
    })
    .expect_err("an input that is not a ScanRequest");
    let declared = extension()
        .dispatch_export("scan__acme", b"nope")
        .expect("routed")
        .expect_err("an input that is not a ScanRequest");
    assert!(!handled, "the handler is not called");
    assert_eq!(
        by_hand, declared,
        "one decode: the same words for the same input"
    );
    assert!(by_hand.starts_with("invalid scan input: "), "{by_hand}");

    let ok = answer_export::<ScanRequest, ScanResponse>(
        "scan",
        br#"{"file_path":"a","content":"x"}"#,
        |_| ScanResponse {
            items: Vec::new(),
            language: None,
        },
    );
    assert!(ok.is_ok(), "{ok:?}");
}

#[specforge_test_macros::test(
    behavior = "call_extension_exports",
    verify = "the SDK routes a compiler pass at the export the host calls it by"
)]
fn a_pass_is_routed_at_the_export_the_host_calls() {
    let mut b = ContributionsBuilder::new(ExtensionMeta::new("@acme/pass", "1.0.0"));
    b.pass("audit", |p| {
        p.run(|_: &PassInput| Vec::<PassDiagnostic>::new());
    });
    assert_eq!(pass_export("audit"), "__pass_audit");
    let answer = b.dispatch_export(&pass_export("audit"), br#"{"entities":[],"edges":[]}"#);
    assert!(matches!(answer, Some(Ok(_))), "{answer:?}");
    assert!(b.dispatch_export("__pass_other", b"{}").is_none());
}
