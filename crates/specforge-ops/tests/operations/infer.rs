//! The `inferred` lint profile and the inference sessions, through the
//! operations the surfaces call (ADR 0015, "Management operations").

use specforge_installed::Installed;
use specforge_ops::check::{CheckOptions, check};
use specforge_project::{Environment, ExtensionLoad, LintProfile};
use specforge_test::prelude::*;

use crate::view_support::{Project, config_read};

/// A project with an `inferred` check over the manifest `manifest` (or no
/// manifest) and the source file `src/tiny.rs` of two lines, whose compile
/// read `config` as its `specforge.json`.
fn project(manifest: Option<&str>, config: serde_json::Value) -> Project {
    let project = Project::assembled("behavior a \"A\" {\n}\n", |root| {
        Environment::assemble(
            config_read(root, config),
            Installed::unread(root),
            ExtensionLoad::NoRuntime,
        )
    });
    let root = project.dir.path();
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(root.join("src/tiny.rs"), "pub fn a() {}\npub fn b() {}\n").unwrap();
    if let Some(manifest) = manifest {
        std::fs::write(root.join("specforge-infer.json"), manifest).unwrap();
    }
    project
}

/// A manifest indexing `src/tiny.rs` as the source of `entities` entities.
fn indexing(entities: usize) -> String {
    let produced: Vec<String> = (0..entities).map(|i| format!("e{i}")).collect();
    serde_json::json!({
        "version": 1,
        "source_roots": ["src"],
        "source_index": [{
            "path": "src/tiny.rs",
            "content_hash": "h",
            "entities_produced": produced,
            "analyzed_at": "2026-10-01T00:00:00Z",
        }],
    })
    .to_string()
}

/// What `specforge check --lint inferred` reports for `project`, beyond
/// nothing.
fn inferred(project: &Project) -> Vec<String> {
    let options = CheckOptions {
        lint_profiles: vec![LintProfile::Inferred],
        ..Default::default()
    };
    let outcome = check(&project.view().reporting(&[]), &options).unwrap();
    outcome.reported.iter().map(|d| d.code.clone()).collect()
}

/// A lint profile adds nothing when its input is absent: no
/// specforge-infer.json, no I200/I202.
#[test]
fn the_inferred_lint_adds_nothing_without_a_manifest() {
    assert!(inferred(&project(None, serde_json::json!({}))).is_empty());
}

/// An unusable manifest is an error the check reports (and fails on), not
/// a false pass (plan 06 R4).
#[specforge_test(
    behavior = "detect_stale_source_anchor",
    verify = "the inferred profile reports a manifest it cannot use as E071"
)]
fn the_inferred_lint_reports_an_unusable_manifest() {
    let project = project(Some("{ nope"), serde_json::json!({}));
    let options = CheckOptions {
        lint_profiles: vec![LintProfile::Inferred],
        ..Default::default()
    };
    let outcome = check(&project.view().reporting(&[]), &options).unwrap();
    assert_eq!(outcome.reported.len(), 1, "{:?}", outcome.reported);
    assert_eq!(outcome.reported[0].code, "E071");
    assert!(
        outcome.reported[0]
            .message
            .starts_with("failed to parse specforge-infer.json:")
    );
    assert_eq!(outcome.counts.errors, 1);
    assert!(!outcome.ok());
}

/// The density threshold is the config the compile read, not a second
/// read of `specforge.json`: 2 entities from 2 lines are over the default
/// threshold and not over 1.0, and 3 are over it.
#[specforge_test(
    behavior = "detect_high_inference_density",
    verify = "I202 threshold is configurable via specforge.json"
)]
fn the_density_threshold_is_the_compiled_config() {
    let dense = project(Some(&indexing(2)), serde_json::json!({}));
    assert!(
        inferred(&dense).contains(&"I202".to_string()),
        "over the default threshold"
    );

    let dense = project(
        Some(&indexing(2)),
        serde_json::json!({"inference": {"density_threshold": 1.0}}),
    );
    assert!(!inferred(&dense).contains(&"I202".to_string()));

    let denser = project(
        Some(&indexing(3)),
        serde_json::json!({"inference": {"density_threshold": 1.0}}),
    );
    assert!(inferred(&denser).contains(&"I202".to_string()));
}

// --- inference sessions: `infer::session` ---

use specforge_ops::infer::{
    self, EndStatus, Recorded, SESSION_ACTIVE, SESSION_NOT_ACTIVE, SOURCE_UNREADABLE,
    SessionStatus, SessionStep, UNKNOWN_SESSION,
};
use specforge_ops::{OpError, OpErrorKind};

/// One `step` over `project`'s view.
fn step(project: &Project, step: SessionStep<'_>) -> Result<infer::SessionOutcome, OpError> {
    infer::session(&project.view(), step)
}

/// The sessions `specforge-infer.json` records at `project`'s root.
fn recorded_sessions(project: &Project) -> Vec<serde_json::Value> {
    let text = std::fs::read_to_string(project.dir.path().join("specforge-infer.json")).unwrap();
    let manifest: serde_json::Value = serde_json::from_str(&text).unwrap();
    manifest["sessions"].as_array().cloned().unwrap_or_default()
}

/// The id a start recorded.
fn started(project: &Project, agent: Option<&str>) -> String {
    let outcome = step(
        project,
        SessionStep::Start {
            agent,
            source_roots: None,
        },
    )
    .unwrap();
    match outcome.recorded {
        Recorded::Started { session_id } => session_id,
        other => panic!("not a start: {other:?}"),
    }
}

#[specforge_test(
    behavior = "start_inference_session",
    verify = "start creates session with active status"
)]
fn start_records_an_active_session() {
    let project = project(None, serde_json::json!({}));
    let id = started(&project, Some("claude"));
    let sessions = recorded_sessions(&project);
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0]["session_id"], id.as_str());
    assert_eq!(sessions[0]["status"], "active");
    assert_eq!(sessions[0]["agent"], "claude");
    assert!(sessions[0].get("ended_at").is_none(), "{sessions:?}");
}

#[specforge_test(
    behavior = "start_inference_session",
    verify = "start rejects when another session is active"
)]
fn a_second_start_is_session_active_and_writes_nothing() {
    let project = project(None, serde_json::json!({}));
    started(&project, None);
    let before = std::fs::read(project.dir.path().join("specforge-infer.json")).unwrap();

    let error = step(
        &project,
        SessionStep::Start {
            agent: Some("other"),
            source_roots: Some(&["lib".to_string()]),
        },
    )
    .unwrap_err();
    assert_eq!(error.code, SESSION_ACTIVE);
    assert_eq!(error.kind, OpErrorKind::Conflict);
    assert_eq!(
        error.message,
        "Another inference session is already active. End it first."
    );
    let after = std::fs::read(project.dir.path().join("specforge-infer.json")).unwrap();
    assert_eq!(before, after);
}

#[specforge_test(
    behavior = "end_inference_session",
    verify = "end sets status to completed"
)]
fn end_completes_or_pauses_and_stamps_ended_at() {
    let project = project(None, serde_json::json!({}));
    for (status, expected) in [
        (EndStatus::Completed, SessionStatus::Completed),
        (EndStatus::Paused, SessionStatus::Paused),
    ] {
        let id = started(&project, None);
        let outcome = step(
            &project,
            SessionStep::End {
                session_id: &id,
                status,
            },
        )
        .unwrap();
        assert_eq!(
            outcome.recorded,
            Recorded::Ended {
                session_id: id.clone(),
                status: expected
            }
        );
        let session = recorded_sessions(&project).pop().unwrap();
        assert_eq!(session["status"], expected.name());
        assert!(session["ended_at"].is_string(), "{session}");

        // An ended session cannot end again.
        let again = step(
            &project,
            SessionStep::End {
                session_id: &id,
                status,
            },
        )
        .unwrap_err();
        assert_eq!(again.code, SESSION_NOT_ACTIVE);
        assert_eq!(again.kind, OpErrorKind::Conflict);
    }
}

#[specforge_test(
    behavior = "end_inference_session",
    verify = "end rejects unknown session_id"
)]
fn end_of_an_unknown_session_is_unknown_session() {
    let project = project(None, serde_json::json!({}));
    let error = step(
        &project,
        SessionStep::End {
            session_id: "nope",
            status: EndStatus::Completed,
        },
    )
    .unwrap_err();
    assert_eq!(error.code, UNKNOWN_SESSION);
    assert_eq!(error.kind, OpErrorKind::InvalidInput);
    assert_eq!(error.message, "Unknown session_id: 'nope'");
    assert!(!project.dir.path().join("specforge-infer.json").exists());
}

/// The index entries `specforge-infer.json` records.
fn source_index(project: &Project) -> Vec<serde_json::Value> {
    let text = std::fs::read_to_string(project.dir.path().join("specforge-infer.json")).unwrap();
    let manifest: serde_json::Value = serde_json::from_str(&text).unwrap();
    manifest["source_index"]
        .as_array()
        .cloned()
        .unwrap_or_default()
}

#[specforge_test(
    behavior = "mark_source_file_analyzed",
    verify = "mark updates existing entry on re-analysis"
)]
fn mark_records_and_replaces_an_entry() {
    let project = project(None, serde_json::json!({}));
    let first = ["a".to_string()];
    let second = ["a".to_string(), "b".to_string()];
    for entities in [&first[..], &second[..]] {
        let outcome = step(
            &project,
            SessionStep::MarkAnalyzed {
                source_file: "src/tiny.rs",
                entities,
            },
        )
        .unwrap();
        assert_eq!(
            outcome.recorded,
            Recorded::Marked {
                source_file: "src/tiny.rs".into(),
                entities: entities.to_vec()
            }
        );
    }
    let index = source_index(&project);
    assert_eq!(index.len(), 1, "{index:?}");
    assert_eq!(index[0]["entities_produced"], serde_json::json!(["a", "b"]));
}

#[specforge_test(
    behavior = "mark_source_file_analyzed",
    verify = "mark computes SHA-256 content hash"
)]
fn mark_hashes_the_file_with_sha256() {
    use sha2::{Digest, Sha256};

    let project = project(None, serde_json::json!({}));
    step(
        &project,
        SessionStep::MarkAnalyzed {
            source_file: "src/tiny.rs",
            entities: &[],
        },
    )
    .unwrap();
    let index = source_index(&project);
    let bytes = std::fs::read(project.dir.path().join("src/tiny.rs")).unwrap();
    assert_eq!(
        index[0]["content_hash"],
        format!("{:x}", Sha256::digest(&bytes)).as_str()
    );

    // A file that is not there is refused, naming it as given.
    let error = step(
        &project,
        SessionStep::MarkAnalyzed {
            source_file: "src/missing.rs",
            entities: &[],
        },
    )
    .unwrap_err();
    assert_eq!(error.code, SOURCE_UNREADABLE);
    assert_eq!(error.kind, OpErrorKind::FileNotFound);
    assert_eq!(error.message, "failed to read src/missing.rs");
}

#[test]
fn a_rootless_view_is_no_project() {
    let project = project(None, serde_json::json!({}));
    let recorded =
        specforge_project::coverage::RecordedCoverage::over(&project.graph, &project.env);
    let rootless =
        specforge_ops::view::ProjectView::new(&project.graph, &project.env, None, &recorded);
    let error = infer::session(
        &rootless,
        SessionStep::Start {
            agent: None,
            source_roots: None,
        },
    )
    .unwrap_err();
    assert_eq!(error.code, "no_project");
}

#[specforge_test(
    behavior = "provide_mcp_infer_session_tool",
    verify = "creates manifest on first write"
)]
fn every_step_reports_the_manifest_as_its_write() {
    let project = project(None, serde_json::json!({}));
    let root = project.dir.path();
    let id = started(&project, None);
    let mut all = vec![
        step(
            &project,
            SessionStep::MarkAnalyzed {
                source_file: "src/tiny.rs",
                entities: &[],
            },
        )
        .unwrap(),
    ];
    all.push(
        step(
            &project,
            SessionStep::End {
                session_id: &id,
                status: EndStatus::Completed,
            },
        )
        .unwrap(),
    );
    for outcome in all {
        assert_eq!(outcome.writes.names_under(root), ["specforge-infer.json"]);
    }
    // The first write created the file; a refusal later writes none.
    let refused = step(
        &project,
        SessionStep::End {
            session_id: "nope",
            status: EndStatus::Completed,
        },
    )
    .unwrap_err();
    assert!(refused.writes.is_empty());
}

#[specforge_test(
    behavior = "mark_source_file_analyzed",
    verify = "mark refuses a file outside the project root"
)]
fn a_source_root_outside_the_root_is_refused() {
    let project = project(None, serde_json::json!({}));
    let error = step(
        &project,
        SessionStep::Start {
            agent: None,
            source_roots: Some(&["src".to_string(), "../x".to_string()]),
        },
    )
    .unwrap_err();
    assert_eq!(error.code, infer::SOURCE_OUTSIDE_ROOT);
    assert_eq!(error.kind, OpErrorKind::InvalidInput);
    assert_eq!(
        error.message,
        "source_roots entry '../x' is not a path inside the project root"
    );
    assert_eq!(error.data.as_ref().unwrap()["argument"], "source_roots");
    assert!(!project.dir.path().join("specforge-infer.json").exists());

    // The project root itself is a root; roots are recorded by the rule.
    step(
        &project,
        SessionStep::Start {
            agent: None,
            source_roots: Some(&[".".to_string(), "./src".to_string()]),
        },
    )
    .unwrap();
    let text = std::fs::read_to_string(project.dir.path().join("specforge-infer.json")).unwrap();
    let manifest: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(manifest["source_roots"], serde_json::json!([".", "src"]));
}
