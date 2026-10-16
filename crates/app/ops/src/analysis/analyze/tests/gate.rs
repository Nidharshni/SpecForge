use super::*;

#[test]
fn an_unusable_report_is_refused_as_e045() {
    let project = Project::new();
    std::fs::write(
        project.assembled.root().join("specforge-report.json"),
        "{not json",
    )
    .unwrap();
    let AnalyzeError::UnusableReport(e) = project.run(&AnalyzeOptions::default()).unwrap_err()
    else {
        panic!("expected UnusableReport");
    };
    assert_eq!(e.code, "E045");

    let missing = AnalyzeOptions {
        report: ReportSource::File(project.assembled.root().join("absent.json")),
        ..Default::default()
    };
    assert!(matches!(
        project.run(&missing).unwrap_err(),
        AnalyzeError::UnusableTestResults(_)
    ));
}

#[test]
fn a_report_source_of_none_ignores_a_broken_recorded_report() {
    let project = Project::new();
    std::fs::write(
        project.assembled.root().join("specforge-report.json"),
        "{not json",
    )
    .unwrap();
    let options = AnalyzeOptions {
        report: ReportSource::None,
        ..Default::default()
    };
    assert!(project.run(&options).is_ok());
}

#[specforge_test(
    behavior = "read_views_over_the_project_view",
    verify = "the recorded test report is read at the view's root, never an ancestor's"
)]
fn a_sub_path_does_not_read_the_ancestors_report() {
    let project = Project::serving(
        InProcessRuntime::new()
            .with(scanning)
            .with(coverage(tally(0, 0))),
        &[EXT, TESTING],
    );
    std::fs::write(
        project.assembled.root().join("specforge-report.json"),
        r#"{"results":{}}"#,
    )
    .unwrap();
    // A project of its own below: it reads its root and nothing above.
    let sub = project.assembled.root().join("sub");
    let nested = Assembled::assembling_at(&sub, |root| {
        loaded_at(root, &project.runtime, &[EXT, TESTING])
    });
    let view = nested.view();
    let min = AnalyzeOptions {
        min: Some(50.0),
        ..Default::default()
    };
    // The view rooted at the sub-path reads its root and nothing above.
    assert_eq!(
        analyze(&view, &min).unwrap_err(),
        AnalyzeError::MinNeedsTestResults
    );
    assert!(analyze(&project.view(), &min).is_ok());
}

#[test]
fn min_needs_a_report_and_comes_after_the_other_checks() {
    let project = Project::serving(
        InProcessRuntime::new()
            .with(scanning)
            .with(coverage(tally(0, 0))),
        &[EXT, TESTING],
    );
    let min = AnalyzeOptions {
        min: Some(50.0),
        ..Default::default()
    };
    assert_eq!(
        project.run(&min).unwrap_err(),
        AnalyzeError::MinNeedsTestResults
    );

    let both = AnalyzeOptions {
        pass: "nope".to_string(),
        ..min.clone()
    };
    assert!(matches!(
        project.run(&both).unwrap_err(),
        AnalyzeError::UnknownPass { .. }
    ));

    std::fs::write(
        project.assembled.root().join("specforge-report.json"),
        r#"{"results":{}}"#,
    )
    .unwrap();
    assert!(project.run(&min).is_ok());
}

/// A project with a (blank) recorded report, gated at `min`, whose
/// testing extension reports `proven` of `total`.
fn gate_of(pass_name: &str, min: Option<f64>, summary: Value) -> Option<Gate> {
    let project = Project::serving(InProcessRuntime::new().with(coverage(summary)), &[TESTING]);
    std::fs::write(
        project.assembled.root().join("specforge-report.json"),
        r#"{"results":{}}"#,
    )
    .unwrap();
    let options = AnalyzeOptions {
        pass: pass_name.to_string(),
        min,
        ..Default::default()
    };
    analyze(&project.view(), &options).unwrap().gate
}

#[specforge_test(
    behavior = "te_coverage_gate",
    verify = "the analysis's ok is the run verdict, the gate included, and its JSON says where the gate landed"
)]
fn the_gate_is_not_requested_without_min() {
    assert_eq!(gate_of("all", None, tally(0, 4)), None);
}

#[specforge_test(
    behavior = "te_coverage_gate",
    verify = "the analysis's ok is the run verdict, the gate included, and its JSON says where the gate landed"
)]
fn the_gate_is_met_at_or_above_the_minimum() {
    assert_eq!(
        gate_of("all", Some(50.0), tally(2, 4)),
        Some(Gate::Met {
            pct: 50.0,
            min: 50.0,
            proven: 2,
            total: 4
        })
    );
    assert_eq!(
        gate_of("coverage", Some(50.0), tally(3, 4)),
        Some(Gate::Met {
            pct: 75.0,
            min: 50.0,
            proven: 3,
            total: 4
        })
    );
}

#[specforge_test(
    behavior = "te_coverage_gate",
    verify = "the analysis's ok is the run verdict, the gate included, and its JSON says where the gate landed"
)]
fn nothing_testable_satisfies_any_minimum() {
    assert!(matches!(
        gate_of("all", Some(100.0), tally(0, 0)),
        Some(Gate::Met { .. })
    ));
}

#[specforge_test(
    behavior = "te_coverage_gate",
    verify = "the analysis's ok is the run verdict, the gate included, and its JSON says where the gate landed"
)]
fn the_gate_is_below_with_the_numbers_to_print() {
    assert_eq!(
        gate_of("all", Some(75.0), tally(1, 4)),
        Some(Gate::Below {
            pct: 25.0,
            min: 75.0,
            proven: 1,
            total: 4
        })
    );
}

#[specforge_test(
    behavior = "te_coverage_gate",
    verify = "the analysis's ok is the run verdict, the gate included, and its JSON says where the gate landed"
)]
fn the_verdict_folds_the_gate_in() {
    let outcome = |gate: Option<Gate>| AnalyzeOutcome {
        passes: Vec::new(),
        gate,
        stray_records: Vec::new(),
    };
    let landed = |pct: f64| (pct, 50.0, 1, 4);
    let (pct, min, proven, total) = landed(25.0);
    let below = outcome(Some(Gate::Below {
        pct,
        min,
        proven,
        total,
    }));
    assert!(below.findings_ok(), "the findings are fine");
    assert_eq!(below.verdict(), RunVerdict::Failed);
    assert!(!below.ok());
    assert_eq!(json_of(&below)["ok"], false);
    assert_eq!(below.gate_failure().unwrap().code, "E048");

    let unjudged = outcome(Some(Gate::Unjudged {
        min: 50.0,
        reason: "no figure".into(),
    }));
    assert_eq!(unjudged.verdict(), RunVerdict::Unjudged);
    assert_eq!(json_of(&unjudged)["gate"]["status"], "unjudged");
    assert_eq!(unjudged.gate_failure().unwrap().code, "E068");

    let met = outcome(Some(Gate::Met {
        pct: 80.0,
        min: 50.0,
        proven: 4,
        total: 5,
    }));
    assert_eq!(met.verdict(), RunVerdict::Passed);
    assert!(met.gate_failure().is_none());
    assert!(json_of(&outcome(None)).get("gate").is_none());
}

#[specforge_test(
    behavior = "te_coverage_gate",
    verify = "the analysis's ok is the run verdict, the gate included, and its JSON says where the gate landed"
)]
fn the_gate_document_is_byte_stable() {
    let written = |gate: Option<Gate>| {
        serde_json::to_string(
            &AnalyzeOutcome {
                passes: Vec::new(),
                gate,
                stray_records: Vec::new(),
            }
            .document(),
        )
        .unwrap()
    };
    assert!(
        written(Some(Gate::Below {
            pct: 25.0,
            min: 75.0,
            proven: 1,
            total: 4
        }))
        .contains(r#""gate":{"status":"below","min":75.0,"pct":25.0,"proven":1,"total":4}"#)
    );
    assert!(
        written(Some(Gate::Unjudged {
            min: 50.0,
            reason: "r".into()
        }))
        .contains(r#""gate":{"status":"unjudged","min":50.0,"reason":"r"}"#)
    );
    assert!(
        written(Some(Gate::Met {
            pct: 100.0,
            min: 50.0,
            proven: 2,
            total: 2
        }))
        .contains(r#""gate":{"status":"met","min":50.0,"pct":100.0,"proven":2,"total":2}"#)
    );
    assert!(!written(None).contains(r#""gate""#));
}

#[specforge_test(
    behavior = "te_coverage_gate",
    verify = "a gate without the coverage pass exits 2 with E068"
)]
fn a_minimum_without_the_coverage_pass_is_refused_before_any_pass_runs() {
    let project = Project::serving(
        InProcessRuntime::new()
            .with(scanning)
            .with(coverage(tally(0, 0))),
        &[TESTING, EXT],
    );
    std::fs::write(
        project.assembled.root().join("specforge-report.json"),
        r#"{"results":{}}"#,
    )
    .unwrap();
    let options = AnalyzeOptions {
        min: Some(10.0),
        ..pass("contracts")
    };
    assert_eq!(
        analyze(&project.view(), &options).unwrap_err(),
        AnalyzeError::MinWithoutCoveragePass
    );
    assert!(project.runtime.calls().is_empty(), "no pass ran");

    // Not declared at all: the same refusal.
    let project = Project::new();
    let all = AnalyzeOptions {
        min: Some(10.0),
        ..Default::default()
    };
    assert_eq!(
        analyze(&project.view(), &all).unwrap_err(),
        AnalyzeError::MinWithoutCoveragePass
    );
    let error = OpError::from(AnalyzeError::MinWithoutCoveragePass);
    assert_eq!(error.code, "E068");
}

#[specforge_test(
    behavior = "te_coverage_gate",
    verify = "a minimum that is not a percentage is invalid input"
)]
fn a_minimum_outside_0_to_100_is_invalid_input() {
    let project = Project::new();
    for min in [-1.0, 100.5, f64::NAN] {
        let options = AnalyzeOptions {
            min: Some(min),
            ..Default::default()
        };
        let error = project.run(&options).unwrap_err();
        assert!(matches!(error, AnalyzeError::MinOutOfRange(_)), "{min}");
        let error = OpError::from(error);
        assert_eq!(error.kind, OpErrorKind::InvalidInput);
        assert_eq!(error.code, INVALID_MIN);
    }
}

#[specforge_test(
    behavior = "te_coverage_gate",
    verify = "a gate without a readable coverage pass is not met"
)]
fn a_summary_of_another_shape_is_unreadable_not_zero() {
    assert_eq!(
        gate_of("all", Some(10.0), json!({"testable_total": "many"})),
        Some(Gate::Unjudged {
            min: 10.0,
            reason: "the summary is not the shape this specforge reads (invalid type: string \"many\", expected usize)".into()
        })
    );
}
