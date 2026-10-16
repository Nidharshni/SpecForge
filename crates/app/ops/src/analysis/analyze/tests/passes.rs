use super::*;

#[test]
fn default_options_run_every_pass_in_order() {
    let outcome = Project::new().run(&AnalyzeOptions::default()).unwrap();
    assert_eq!(names(&outcome), vec!["contracts", "@t/x:scan"]);
    assert!(outcome.ok(), "a warning is not an error");
}

#[test]
fn strict_promotes_warnings_and_ok_follows() {
    let outcome = Project::new()
        .run(&AnalyzeOptions {
            strict: true,
            ..Default::default()
        })
        .unwrap();
    assert!(!outcome.ok());
    assert_eq!(outcome.passes[1].findings[0].severity, Severity::Error);
}

#[test]
fn json_shape_is_ok_and_passes() {
    let outcome = Project::new().run(&pass("contracts")).unwrap();
    assert_eq!(
        json_of(&outcome),
        json!({
            "ok": true,
            "passes": [{
                "pass": "contracts",
                "findings": [],
                "summary": {
                    "contract_entities": 0,
                    "unconstrained_entities": 0,
                    "obligation_references": 0
                }
            }]
        })
    );
}

#[specforge_test(
    behavior = "call_extension_exports",
    verify = "an analyze pass that traps is reported as an E028 finding of that pass"
)]
fn an_analyze_pass_that_traps_is_an_e028_finding_and_the_analysis_fails() {
    for answer in [
        WasmCallResult::Trap(WasmTrapInfo {
            kind: "call_failed".into(),
            message: "unreachable: the pass panicked".into(),
            export_name: "__pass_scan".into(),
        }),
        WasmCallResult::Ok(b"not diagnostics".to_vec()),
    ] {
        let project = Project::serving(
            InProcessRuntime::new()
                .with(scanning)
                .answer_raw(EXT, "__pass_scan", answer),
            &[EXT],
        );
        let outcome = analyze(&project.view(), &AnalyzeOptions::default()).unwrap();
        assert!(!outcome.ok(), "a failed pass fails the analysis");
        let scan = outcome
            .passes
            .iter()
            .find(|p| p.name == "@t/x:scan")
            .expect("the failed pass has its report");
        assert_eq!(scan.findings.len(), 1, "{:?}", scan.findings);
        assert_eq!(scan.findings[0].code, "E028");
        assert_eq!(scan.findings[0].severity, Severity::Error);
        assert!(
            scan.findings[0]
                .message
                .starts_with("compiler pass __pass_scan() of '@t/x' "),
            "{}",
            scan.findings[0].message
        );
        assert_eq!(scan.summary["failed"], true);
    }
}

#[test]
fn a_declared_extension_pass_runs_alone_by_its_full_name() {
    let outcome = Project::new().run(&pass("@t/x:scan")).unwrap();
    assert_eq!(names(&outcome), vec!["@t/x:scan"]);
}

/// The `__pass_*` exports the runtime was called for.
fn pass_calls(runtime: &InProcessRuntime) -> Vec<String> {
    runtime
        .calls()
        .into_iter()
        .map(|c| c.export)
        .filter(|e| e.starts_with("__pass_"))
        .collect()
}

#[test]
fn a_builtin_pass_runs_no_extension_pass() {
    let project = Project::new();
    let outcome = project.run(&pass("contracts")).unwrap();
    assert_eq!(names(&outcome), vec!["contracts"]);
    assert!(pass_calls(&project.runtime).is_empty());

    project.runtime.clear_calls();
    let outcome = project.run(&pass("@t/x:scan")).unwrap();
    assert_eq!(names(&outcome), vec!["@t/x:scan"]);
    assert_eq!(pass_calls(&project.runtime), ["__pass_scan"]);

    project.runtime.clear_calls();
    let outcome = project.run(&pass("all")).unwrap();
    assert_eq!(names(&outcome), vec!["contracts", "@t/x:scan"]);
    assert_eq!(pass_calls(&project.runtime), ["__pass_scan"]);
}

#[test]
fn coverage_is_an_alias_for_the_testing_pass() {
    let outcome = Project::new().run(&pass("coverage")).unwrap();
    assert!(outcome.passes.is_empty(), "no testing extension declared");
}

#[test]
fn unknown_and_check_phase_passes_are_refused_with_the_available_ones() {
    for requested in ["nope", "@t/x:hidden", "@t/x:typo"] {
        let err = Project::new().run(&pass(requested)).unwrap_err();
        assert_eq!(err.error.code, UNKNOWN_PASS);
        assert_eq!(
            err.error.message,
            format!(
                "Unknown analysis pass '{requested}' (available: all, coverage, contracts, @t/x:scan)"
            )
        );
        assert_eq!(err.subject, Some(AnalyzeField::Pass));
    }
}

#[specforge_test(
    behavior = "call_extension_exports",
    verify = "an analyze pass runs in the view's runtime whether or not the view has a root"
)]
fn extension_passes_run_without_a_root() {
    let project = Project::new();
    let options = AnalyzeOptions::default();
    let rooted = project.run(&options).unwrap();
    let rootless = analyze(&project.assembled.rootless_view(), &options).unwrap();
    assert_eq!(names(&rootless), names(&rooted));
    assert!(names(&rooted).contains(&"@t/x:scan"));
}

#[test]
fn prove_runs_last_and_tells_the_passes_it_ran() {
    let project = Project::new();
    analyze(&project.view(), &AnalyzeOptions::default()).unwrap();
    assert_eq!(proved_seen(&project.runtime), vec![Value::Null]);

    project.runtime.clear_calls();
    let solver = ScriptedSolver::ready();
    let options = AnalyzeOptions {
        prove: Some(&solver),
        ..Default::default()
    };
    let outcome = analyze(&project.view(), &options).unwrap();
    assert_eq!(names(&outcome), vec!["contracts", "@t/x:scan", "prove"]);
    assert_eq!(proved_seen(&project.runtime), vec![json!([])]);
}

#[test]
fn prove_without_a_solver_is_a_last_w098_report_with_empty_proved_claims() {
    let project = Project::new();
    let missing = ScriptedSolver::not_found();
    let mut options = AnalyzeOptions {
        prove: Some(&missing),
        ..Default::default()
    };
    let lenient = analyze(&project.view(), &options).unwrap();
    assert_eq!(names(&lenient).last().copied(), Some("prove"));
    let report = lenient.passes.last().unwrap();
    assert!(report.findings.iter().any(|f| f.code == "W098"));
    assert!(lenient.ok());
    assert_eq!(proved_seen(&project.runtime), vec![json!([])]);

    options.strict = true;
    let strict = analyze(&project.view(), &options).unwrap();
    assert!(!strict.ok(), "strict promotes the prove report too");
}

/// `@t/x` as [`scanning`] declares it, plus a `claim` kind whose `holds`
/// field is a claim.
fn claiming() -> ContributionsBuilder {
    let mut c = scanning();
    c.kind("Claim", |k| {
        k.keyword("claim");
        k.field("holds", |f| {
            f.field_type(FieldType::String).proof_role("claim");
        });
    });
    c
}

#[specforge_test(
    behavior = "prove_declared_bounds_and_claims",
    verify = "a claim the bounds entail reaches the extension passes as a proved claim"
)]
fn a_claim_the_solver_entails_reaches_the_extension_passes() {
    let mut project = Project::serving(InProcessRuntime::new().with(claiming), &[EXT]);
    add_entity_with(
        &mut project,
        "fast",
        "claim",
        [(
            "holds",
            specforge_parser::FieldValue::String("latency <= latency".into()),
        )],
    );
    let solver = ScriptedSolver::answering([Ok("unsat\n".to_string())]);
    let outcome = analyze(
        &project.view(),
        &AnalyzeOptions {
            prove: Some(&solver),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(names(&outcome).last().copied(), Some("prove"));
    assert_eq!(proved_seen(&project.runtime), vec![json!(["fast"])]);
    assert_eq!(
        solver.scripts().len(),
        1,
        "no bounds: one entailment check, no consistency check"
    );
}

#[specforge_test(
    invariant = "mcp_structured_error_responses",
    verify = "an operation states the request field each refusal is about, and a refusal of the project or the environment is about none"
)]
fn analyze_states_which_field_a_refusal_is_about() {
    let project = Project::new();
    let subject = |options: AnalyzeOptions| project.run(&options).unwrap_err().subject;
    assert_eq!(subject(pass("nope")), Some(AnalyzeField::Pass));
    let min = AnalyzeOptions {
        min: Some(150.0),
        ..Default::default()
    };
    assert_eq!(subject(min), Some(AnalyzeField::Min));
    // A named results file that is not there is about the report; one that is unusable is a
    // refusal of the file, and so is an unusable recorded report.
    let report = |path: std::path::PathBuf| AnalyzeOptions {
        report: ReportSource::Named {
            path,
            containment: crate::Containment::unbounded(),
        },
        ..Default::default()
    };
    let missing = project.assembled.root().join("missing.json");
    assert_eq!(subject(report(missing)), Some(AnalyzeField::Report));
    let unusable = project.assembled.root().join("unusable.json");
    std::fs::write(&unusable, "{not json").unwrap();
    assert_eq!(subject(report(unusable)), None);
    std::fs::write(
        project.assembled.root().join("specforge-report.json"),
        "{not json",
    )
    .unwrap();
    assert_eq!(subject(AnalyzeOptions::default()), None);
    // A refusal of the project is about no field.
    let no_report = AnalyzeOptions {
        min: Some(50.0),
        report: ReportSource::None,
        ..Default::default()
    };
    let refusal = project.run(&no_report).unwrap_err();
    assert_eq!(refusal.subject, None, "{refusal:?}");
}
