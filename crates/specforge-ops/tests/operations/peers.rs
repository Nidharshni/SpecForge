//! Peer requirements as `doctor` reports them over a compiled project.

use std::path::Path;

use specforge_common::Severity;
use specforge_extension_sdk::{ContributionsBuilder, ExtensionMeta};
use specforge_ops::doctor::{Part, diagnose_with};
use specforge_ops::view::ProjectView;
use specforge_project::CompiledProject;
use specforge_protocol_types::PeerDependency;
use specforge_test_macros::test as specforge_test;
use specforge_wasm::testing::InProcessRuntime;
use std::sync::Arc;

/// `name` at `version`, served in process, declaring `peers` (required).
fn served(
    name: &'static str,
    version: &'static str,
    peers: &'static [(&'static str, &'static str)],
) -> impl Fn() -> ContributionsBuilder + Send + Sync + 'static {
    move || {
        let mut meta = ExtensionMeta::new(name, version);
        meta.peer_dependencies = peers
            .iter()
            .map(|(peer, range)| PeerDependency {
                name: (*peer).into(),
                version: (*range).into(),
                optional: false,
            })
            .collect();
        ContributionsBuilder::new(meta)
    }
}

/// Rewrite `specforge.lock` so `name`'s entry records `peers`.
fn locked_peers(root: &Path, name: &str, peers: &[(&str, &str)]) {
    let path = root.join("specforge.lock");
    let mut lock: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    for entry in lock["entries"].as_array_mut().unwrap() {
        if entry["name"] == name {
            entry["peer_dependencies"] = peers
                .iter()
                .map(|(peer, range)| {
                    serde_json::json!({"name": peer, "version": range, "optional": false})
                })
                .collect();
        }
    }
    std::fs::write(&path, lock.to_string()).unwrap();
}

fn project(extensions: &[&str]) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let config = serde_json::json!({"name": "p", "version": "0.1.0", "extensions": extensions});
    std::fs::write(dir.path().join("specforge.json"), config.to_string()).unwrap();
    dir
}

#[specforge_test(
    behavior = "run_doctor_check",
    verify = "a peer a builtin satisfies is not reported"
)]
fn a_peer_a_builtin_satisfies_is_not_reported_by_doctor() {
    let dir = project(&["@specforge/software", "@acme/app"]);
    let runtime = InProcessRuntime::new()
        .with(served("@specforge/software", "1.0.0", &[]))
        .with(served(
            "@acme/app",
            "1.0.0",
            &[("@specforge/software", "^1.0")],
        ));
    specforge_installed::testing::install(dir.path(), &["@acme/app"]);
    locked_peers(dir.path(), "@acme/app", &[("@specforge/software", "^1.0")]);

    let compiled = CompiledProject::compile(
        &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
        Some(Arc::new(runtime)),
    );
    assert!(
        !compiled.diagnostics().iter().any(|d| d.code == "E027"),
        "{:?}",
        compiled.diagnostics()
    );
    let report = diagnose_with(&ProjectView::of(&compiled), true);

    assert_eq!(report.about(Part::Peer).count(), 0, "{:?}", report.findings);
    assert_eq!(
        report.about(Part::Binary).count(),
        0,
        "{:?}",
        report.findings
    );
    assert!(report.ok(), "{:?}", report.findings);
    assert!(report.extensions_ok());
}

#[specforge_test(
    behavior = "run_doctor_check",
    verify = "doctor reports the peer requirements check reports, with the remedy each suggests"
)]
fn doctor_reports_the_peer_requirements_check_reports() {
    let dir = project(&["@acme/base", "@acme/bad"]);
    let runtime = InProcessRuntime::new()
        .with(served("@acme/base", "1.0.0", &[]))
        .with(served("@acme/bad", "1.0.0", &[("@acme/base", "one-ish")]));
    specforge_installed::testing::install(dir.path(), &["@acme/base", "@acme/bad"]);
    locked_peers(dir.path(), "@acme/bad", &[("@acme/base", "one-ish")]);

    let compiled = CompiledProject::compile(
        &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
        Some(Arc::new(runtime)),
    );
    let all = compiled.diagnostics();
    let reported: Vec<_> = all
        .iter()
        .filter(|d| matches!(d.code.as_str(), "E073" | "E027"))
        .collect();
    assert_eq!(reported.len(), 1, "{reported:?}");
    assert_eq!(reported[0].code, "E073");
    assert_eq!(reported[0].severity, Severity::Error);

    let report = diagnose_with(&ProjectView::of(&compiled), true);
    let peers: Vec<_> = report.about(Part::Peer).collect();
    assert_eq!(peers.len(), 1, "{:?}", report.findings);
    assert_eq!(peers[0].code, "E073");
    assert_eq!(peers[0].check, reported[0].message);
    assert_eq!(
        Some(peers[0].remediation.as_str()),
        reported[0].suggestion.as_deref()
    );
    assert_eq!(format!("{:?}", peers[0].status), "Error");
    assert_eq!(report.about(Part::Binary).count(), 0);
    assert!(!report.ok() && !report.extensions_ok());

    // An unsatisfied range is reported the same way: `@acme/app` wants a
    // base this project does not have.
    let dir = project(&["@acme/base", "@acme/app"]);
    let runtime = InProcessRuntime::new()
        .with(served("@acme/base", "1.0.0", &[]))
        .with(served("@acme/app", "1.0.0", &[("@acme/base", "^2.0")]));
    specforge_installed::testing::install(dir.path(), &["@acme/base", "@acme/app"]);
    let compiled = CompiledProject::compile(
        &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
        Some(Arc::new(runtime)),
    );
    let all = compiled.diagnostics();
    let e027: Vec<_> = all.iter().filter(|d| d.code == "E027").collect();
    assert_eq!(e027.len(), 1, "{e027:?}");
    let report = diagnose_with(&ProjectView::of(&compiled), true);
    let peers: Vec<_> = report.about(Part::Peer).collect();
    assert_eq!(peers.len(), 1, "{:?}", report.findings);
    assert_eq!(peers[0].code, "E027");
    assert_eq!(peers[0].check, e027[0].message);
}

#[specforge_test(
    behavior = "registry_build_load_order",
    verify = "specforge check reports a cycle among required peers"
)]
fn check_reports_a_cycle_among_required_peers() {
    let dir = project(&["@acme/a", "@acme/b"]);
    let runtime = InProcessRuntime::new()
        .with(served("@acme/a", "1.0.0", &[("@acme/b", "^1")]))
        .with(served("@acme/b", "1.0.0", &[("@acme/a", "^1")]));
    specforge_installed::testing::install(dir.path(), &["@acme/a", "@acme/b"]);

    let compiled = CompiledProject::compile(
        &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
        Some(Arc::new(runtime)),
    );
    let all = compiled.diagnostics();
    let cycles: Vec<_> = all.iter().filter(|d| d.code == "E027").collect();
    assert_eq!(cycles.len(), 1, "{cycles:?}");
    assert_eq!(
        cycles[0].message,
        "cycle detected in peer dependencies: @acme/a, @acme/b"
    );

    let outcome = specforge_ops::check::check(
        &ProjectView::of(&compiled),
        all.clone(),
        &specforge_ops::check::CheckOptions::default(),
    )
    .unwrap();
    assert!(!outcome.ok());

    // Doctor lists the same cycle under its peers.
    let report = diagnose_with(&ProjectView::of(&compiled), true);
    let peers: Vec<_> = report.about(Part::Peer).collect();
    assert_eq!(peers.len(), 1, "{:?}", report.findings);
    assert_eq!(peers[0].check, cycles[0].message);
}
