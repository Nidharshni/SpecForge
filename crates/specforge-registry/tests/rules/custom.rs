//! `check: "custom"` rules: the verdict comes through the `CustomVerdicts`
//! port, once per entity at check time and once per rule at load (the
//! probe).

use std::cell::RefCell;

use specforge_common::{CustomRuleFailure, DiagnosticData, Severity};
use specforge_protocol_types::{ValidationRuleDescriptor, ValidationSeverity};
use specforge_registry::rules::{CustomCall, NoVerdicts, Subject, Verdict, VerdictError};
use specforge_test_macros::test as spec;

use super::{entity, messages, one, over, rules};

/// A custom rule `code` calling `function`, on `target`.
fn custom(code: &str, function: &str, target: Option<&str>) -> ValidationRuleDescriptor {
    ValidationRuleDescriptor {
        code: code.to_string(),
        severity: ValidationSeverity::Error,
        message_template: "{kind} '{id}' fails {field} ({value})".to_string(),
        check: "custom".to_string(),
        target_kind: target.map(str::to_string),
        wasm_function: Some(function.to_string()),
        ..Default::default()
    }
}

fn failed() -> Verdict {
    Verdict::Fail {
        field: None,
        value: None,
    }
}

#[spec(
    behavior = "execute_validation_pattern",
    verify = "custom pattern dispatches to registered Wasm function"
)]
fn custom_pattern_dispatches_to_registered_wasm_function() {
    let built = one(custom("E200", "validate_naming", Some("behavior")));
    let calls = RefCell::new(Vec::new());
    let verdicts = |call: CustomCall<'_>| {
        let Subject::Entity(record) = call.subject else {
            panic!("a check asks about entities");
        };
        calls.borrow_mut().push(format!(
            "{} {} {}",
            call.extension, call.function, record.id
        ));
        Ok(if record.id == "bad_name" {
            Verdict::Fail {
                field: Some("name".to_string()),
                value: Some("Bad Name".to_string()),
            }
        } else {
            Verdict::Pass
        })
    };
    let entities = [
        entity("bad_name", "behavior", 1, 0),
        entity("good_name", "behavior", 1, 0),
        entity("an_event", "event", 1, 0),
    ];

    let diagnostics = built.rules.check(&over(&entities), &verdicts);

    assert_eq!(
        messages(&diagnostics),
        ["behavior 'bad_name' fails name (Bad Name)"]
    );
    // Each entity of the target kind is asked once, from the declaring
    // extension.
    assert_eq!(
        *calls.borrow(),
        [
            "@test validate_naming bad_name",
            "@test validate_naming good_name"
        ]
    );
}

#[spec(
    behavior = "register_custom_validation_patterns",
    verify = "custom pattern dispatched to Wasm runtime during validation"
)]
fn custom_pattern_dispatched_to_wasm_runtime_during_validation() {
    let mut declared = custom("E200", "check", None);
    declared.message_template = "{id} failed".to_string();
    let built = one(declared);
    let verdicts = |call: CustomCall<'_>| match call.subject {
        Subject::Entity(record) if record.id == "bad" => Ok(failed()),
        _ => Ok(Verdict::Pass),
    };
    let entities = [
        entity("bad", "behavior", 1, 0),
        entity("good", "event", 1, 0),
    ];
    let diagnostics = built.rules.check(&over(&entities), &verdicts);
    assert_eq!(messages(&diagnostics), ["bad failed"]);
}

#[spec(
    behavior = "register_custom_validation_patterns",
    verify = "custom pattern failure emits configured diagnostic"
)]
fn custom_pattern_failure_emits_configured_diagnostic() {
    let mut declared = custom("E201", "always_fail", None);
    declared.message_template = "{kind} '{id}' custom check failed".to_string();
    let built = one(declared);
    let verdicts = |_: CustomCall<'_>| Ok(failed());
    let diagnostics = built
        .rules
        .check(&over(&[entity("b1", "behavior", 1, 0)]), &verdicts);
    assert_eq!(diagnostics[0].code, "E201");
    assert_eq!(diagnostics[0].severity, Severity::Error);
    assert_eq!(diagnostics[0].message, "behavior 'b1' custom check failed");
}

#[spec(
    behavior = "register_custom_validation_patterns",
    verify = "a custom rule whose function fails on entities produces one W148 per check naming how many were not checked"
)]
fn a_function_failing_on_entities_is_one_w148_per_check() {
    let built = rules(vec![
        custom("E202", "flaky", Some("behavior")),
        custom("E203", "fragile", Some("behavior")),
        custom("E204", "sound", Some("behavior")),
    ]);
    let entities = [
        entity("a", "behavior", 1, 0),
        entity("b", "behavior", 1, 0),
        entity("c", "behavior", 1, 0),
        entity("d", "behavior", 1, 0),
    ];
    let verdicts = |call: CustomCall<'_>| {
        let Subject::Entity(record) = call.subject else {
            unreachable!()
        };
        match (call.function, record.id.as_str()) {
            ("flaky", "b" | "d") => Err(VerdictError::Failed(format!("trap on {}", record.id))),
            ("flaky", "c") => Ok(failed()),
            ("fragile", "a") => Err(VerdictError::Failed("bad answer".into())),
            _ => Ok(Verdict::Pass),
        }
    };

    let diagnostics = built.rules.check(&over(&entities), &verdicts);

    // Each rule's own diagnostics, then its W148 right after them.
    let reported: Vec<(&str, &str)> = diagnostics
        .iter()
        .map(|d| (d.code.as_str(), d.message.as_str()))
        .collect();
    assert_eq!(
        reported,
        [
            ("E202", "behavior 'c' fails {field} ({value})"),
            (
                "W148",
                "extension '@test': rule 'E202': wasm_function 'flaky' failed on 2 entities ('b': trap on b) — they were not checked"
            ),
            (
                "W148",
                "extension '@test': rule 'E203': wasm_function 'fragile' failed on 1 entity ('a': bad answer) — it was not checked"
            ),
        ]
    );
    assert!(
        diagnostics[1..]
            .iter()
            .all(|d| d.severity == Severity::Warning && d.span.is_none())
    );
    // The data carries every failure, by id.
    assert_eq!(
        diagnostics[1].data.as_deref(),
        Some(&DiagnosticData::CustomRuleFailures {
            rule: "E202".into(),
            function: "flaky".into(),
            failures: vec![
                CustomRuleFailure {
                    entity: "b".into(),
                    error: "trap on b".into()
                },
                CustomRuleFailure {
                    entity: "d".into(),
                    error: "trap on d".into()
                },
            ],
        })
    );
    assert_eq!(diagnostics[1].data.as_ref().unwrap().entities(), ["b", "d"]);

    // Without a runtime, the rules check nothing and report nothing.
    assert!(built.rules.check(&over(&entities), &NoVerdicts).is_empty());
}

#[spec(
    behavior = "register_custom_validation_patterns",
    verify = "unresolvable wasm_function produces warning"
)]
fn a_function_that_cannot_answer_the_probe_is_w112_once() {
    let mut built = rules(vec![
        custom("E300", "validate__present", Some("gadget")),
        custom("E301", "validate__absent", Some("gadget")),
        custom("E302", "validate__untargeted", None),
    ]);
    let probed = RefCell::new(Vec::new());
    let verdicts = |call: CustomCall<'_>| {
        let Subject::Probe { kind } = call.subject else {
            panic!("the probe asks about no entity");
        };
        probed
            .borrow_mut()
            .push(format!("{} {:?}", call.function, kind));
        match call.function {
            "validate__absent" => Err(VerdictError::Failed("no export 'validate__absent'".into())),
            _ => Ok(Verdict::Pass),
        }
    };

    built.build.probe(&verdicts);
    let diagnostics: Vec<_> = built
        .build
        .diagnostics()
        .filter(|d| d.code == "W112")
        .collect();

    assert_eq!(
        *probed.borrow(),
        [
            "validate__present Some(\"gadget\")",
            "validate__absent Some(\"gadget\")",
            "validate__untargeted None",
        ]
    );
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].code, "W112");
    assert_eq!(diagnostics[0].severity, Severity::Warning);
    assert_eq!(
        diagnostics[0].message,
        "extension '@test': rule 'E301': wasm_function 'validate__absent' could not be resolved (no export 'validate__absent') — the rule will not fire"
    );
    assert_eq!(
        diagnostics[0].suggestion.as_deref(),
        Some("export 'validate__absent' from '@test', or fix the rule's wasm_function")
    );
    // The rule stays registered.
    assert_eq!(built.codes(), ["E300", "E301", "E302"]);
    // Without a runtime nothing is probed.
    built.build.probe(&NoVerdicts);
    assert!(built.build.diagnostics().all(|d| d.code != "W112"));
}
