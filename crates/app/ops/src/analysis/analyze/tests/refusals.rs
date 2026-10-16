//! Every refusal of `analyze`, field by field: the kind, the code, the
//! message, the suggestion, the entity and the subject each states, pinned
//! once before any refusal moves (round 13, plan 10).

use super::*;

/// `kind code | message | suggestion | entity | subject`, `-` for none.
fn line(refusal: &Refusal<AnalyzeField>) -> String {
    let error = &refusal.error;
    let none = |text: Option<&str>| text.unwrap_or("-").to_string();
    format!(
        "{:?} {} | {} | {} | {} | {}",
        error.kind,
        error.code,
        error.message,
        none(error.suggestion.as_deref()),
        none(error.entity.as_deref()),
        refusal
            .subject
            .map_or("-".to_string(), |field| format!("Some({field:?})")),
    )
}

fn named(path: std::path::PathBuf) -> ReportSource {
    ReportSource::Named {
        path,
        containment: crate::Containment::unbounded(),
    }
}

#[specforge_test(
    invariant = "mcp_structured_error_responses",
    verify = "an operation states the request field each refusal is about, and a refusal of the project or the environment is about none"
)]
fn analyze_refusals_are_pinned_field_by_field() {
    let refused = |project: &Project, options: AnalyzeOptions| {
        line(&Refusal::<AnalyzeField>::from(
            project.run(&options).unwrap_err(),
        ))
    };
    let project = Project::new();
    let serving = Project::serving(
        InProcessRuntime::new()
            .with(scanning)
            .with(coverage(tally(0, 0))),
        &[EXT, TESTING],
    );
    let root = project.assembled.root().to_path_buf();
    let min = |min: f64| AnalyzeOptions {
        min: Some(min),
        ..Default::default()
    };
    let available = "all, coverage, contracts, @t/x:scan";
    let invalid_min = |got: &str| {
        format!(
            "InvalidInput invalid_min | min must be a percentage between 0 and 100, got {got} | - | - | Some(Min)"
        )
    };
    let missing = root.join("absent.json");
    let missing_line = format!(
        "FileNotFound E045 | cannot read test results {}: No such file or directory (os error 2) | check the path: the named test report does not exist | - | Some(Report)",
        missing.display()
    );
    let unusable = root.join("unusable.json");
    std::fs::write(&unusable, "{not json").unwrap();
    let unusable_message = |path: &std::path::Path| {
        format!(
            "SchemaMismatch E045 | invalid test results {}: key must be a string at line 1 column 2 (expected the RES-15 specforge-report.json shape) | run `specforge collect` again to rewrite the report, or fix or remove the file | - | -",
            path.display()
        )
    };
    let e068 = "PreconditionFailed E068 | a proof-coverage minimum needs the coverage pass of @specforge/testing (pass coverage or all) | enable it with `specforge add @specforge/testing` | - | -";

    assert_eq!(
        refused(&project, pass("nope")),
        format!(
            "InvalidInput unknown_pass | Unknown analysis pass 'nope' (available: {available}) | - | - | Some(Pass)"
        )
    );
    assert_eq!(
        refused(&project, pass("@t/x:scna")),
        format!(
            "InvalidInput unknown_pass | Unknown analysis pass '@t/x:scna' (available: {available}) | did you mean '@t/x:scan'? | - | Some(Pass)"
        )
    );
    assert_eq!(
        refused(&project, pass("Contracts")),
        format!(
            "InvalidInput unknown_pass | Unknown analysis pass 'Contracts' (available: {available}) | did you mean 'contracts'? | - | Some(Pass)"
        )
    );
    for (value, got) in [(150.0, "150"), (-1.0, "-1"), (f64::NAN, "NaN")] {
        assert_eq!(refused(&project, min(value)), invalid_min(got), "{got}");
    }
    assert_eq!(refused(&project, min(10.0)), e068);
    assert_eq!(
        refused(&serving, min(50.0)),
        "PreconditionFailed no_test_results | --min needs test results: run `specforge collect` or pass --test-results | - | - | -"
    );
    let named_missing = AnalyzeOptions {
        report: named(missing.clone()),
        ..Default::default()
    };
    assert_eq!(refused(&project, named_missing.clone()), missing_line);
    let named_unusable = AnalyzeOptions {
        report: named(unusable.clone()),
        ..Default::default()
    };
    assert_eq!(
        refused(&project, named_unusable),
        unusable_message(&unusable)
    );
    let recorded = root.join("specforge-report.json");
    std::fs::write(&recorded, "{not json").unwrap();
    assert_eq!(
        refused(&project, AnalyzeOptions::default()),
        unusable_message(&recorded)
    );
    // The order: the pass, then the minimum, then the coverage pass, then the report.
    let pass_and_min = AnalyzeOptions {
        min: Some(150.0),
        ..pass("nope")
    };
    assert_eq!(
        refused(&project, pass_and_min),
        refused(&project, pass("nope"))
    );
    let min_and_report = AnalyzeOptions {
        min: Some(150.0),
        ..named_missing.clone()
    };
    assert_eq!(refused(&project, min_and_report), invalid_min("150"));
    let coverage_and_report = AnalyzeOptions {
        min: Some(50.0),
        ..named_missing
    };
    assert_eq!(refused(&project, coverage_and_report), e068);
}
