//! Doctor reports the setup diagnostics the environment reports, whatever
//! their code: one finding per environment diagnostic, so doctor fails
//! whenever `check` fails on the project's setup.

use specforge_ops::testing::ScriptedSolver;
use specforge_project::testing::compile_at;
use std::sync::Arc;

use specforge_common::Severity;
use specforge_extension_sdk::prelude::{CheckKind, ValidationSeverity};
use specforge_extension_sdk::{CommandOutput, ContributionsBuilder, ExtensionMeta};
use specforge_ops::analysis::check::{CheckOptions, check};
use specforge_ops::analysis::doctor::{DoctorReport, diagnose};
use specforge_ops::view::ProjectView;
use specforge_project::CompiledProject;
use specforge_protocol_types::PeerDependency;
use specforge_test_macros::test as specforge_test;
use specforge_wasm::testing::InProcessRuntime;

type Build = fn(&mut ContributionsBuilder);

/// What one served extension contributes: its name, its `ext_short` and
/// its declarations.
type Served = (&'static str, Option<&'static str>, Build);

fn served(
    name: &'static str,
    short: Option<&'static str>,
    build: Build,
) -> impl Fn() -> ContributionsBuilder + Send + Sync + 'static {
    move || {
        let mut meta = ExtensionMeta::new(name, "1.0.0");
        meta.short = short.map(str::to_string);
        let mut c = ContributionsBuilder::new(meta);
        build(&mut c);
        c
    }
}

fn nothing(_: &mut ContributionsBuilder) {}

fn hello(c: &mut ContributionsBuilder) {
    c.command("hello", |cmd| {
        cmd.title("Hello")
            .description("Say hello")
            .handler(|_| CommandOutput::ok("hello"));
    });
}

fn malformed_tool(c: &mut ContributionsBuilder) {
    c.mcp_tool("bad", |t| {
        t.description("bad")
            .input_schema(serde_json::json!("not an object"))
            .handler(|_| Ok(serde_json::json!({})));
    });
}

fn kind_edge_rule(c: &mut ContributionsBuilder) {
    c.kind("Memo", |k| {
        k.keyword("memo");
    });
    c.edge("uses", |_| {});
    c.rule("W900", |r| {
        r.severity(ValidationSeverity::Warning)
            .message_template("'{id}' has no outgoing edge")
            .check(CheckKind::NoOutgoingEdges)
            .target_kind("memo");
    });
}

/// The project enabling `extensions` (each installed), compiled in a runtime
/// serving them; `peers` rewrites a lock entry's peer requirements.
fn compiled(
    extensions: &[Served],
    config: serde_json::Value,
    peers: &[(&str, &str, &str)],
) -> (tempfile::TempDir, CompiledProject) {
    let dir = tempfile::tempdir().unwrap();
    let names: Vec<&str> = extensions.iter().map(|(name, ..)| *name).collect();
    let mut config = config;
    config["name"] = "p".into();
    config["version"] = "0.1.0".into();
    config["extensions"] = serde_json::json!(names);
    std::fs::write(dir.path().join("specforge.json"), config.to_string()).unwrap();
    let mut runtime = InProcessRuntime::new();
    for (name, short, build) in extensions {
        runtime = runtime.with(served(name, *short, *build));
    }
    if !names.is_empty() {
        specforge_installed::testing::install(dir.path(), &names);
    }
    if !peers.is_empty() {
        let path = dir.path().join("specforge.lock");
        let mut lock: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        for entry in lock["entries"].as_array_mut().unwrap() {
            for (name, peer, range) in peers {
                if entry["name"] == *name {
                    entry["peer_dependencies"] = serde_json::json!([PeerDependency {
                        name: (*peer).into(),
                        version: (*range).into(),
                        optional: false,
                    }]);
                }
            }
        }
        std::fs::write(&path, lock.to_string()).unwrap();
    }
    let project = compile_at(dir.path(), Some(Arc::new(runtime)));
    (dir, project)
}

/// Every setup scenario: the code it raises and the project that raises it.
fn scenarios() -> Vec<(&'static str, (tempfile::TempDir, CompiledProject))> {
    let none = || serde_json::json!({});
    vec![
        (
            "E039",
            compiled(
                &[("@acme/a", None, hello), ("@acme/b", None, hello)],
                none(),
                &[],
            ),
        ),
        (
            "E055",
            compiled(&[("@acme/a", None, malformed_tool)], none(), &[]),
        ),
        (
            "E030",
            compiled(&[("@acme/a", Some("Bad_Short"), nothing)], none(), &[]),
        ),
        (
            "collisions",
            compiled(
                &[
                    ("@acme/a", None, kind_edge_rule),
                    ("@acme/b", None, kind_edge_rule),
                ],
                none(),
                &[],
            ),
        ),
        (
            "E073",
            compiled(
                &[("@acme/base", None, nothing), ("@acme/bad", None, nothing)],
                none(),
                &[("@acme/bad", "@acme/base", "one-ish")],
            ),
        ),
        (
            "W118",
            compiled(
                &[],
                serde_json::json!({"providers": [
                    {"scheme": "gh", "alias": "work", "extension": "@acme/github"}
                ]}),
                &[],
            ),
        ),
    ]
}

/// (code, message, status) of the findings.
fn findings(report: &DoctorReport) -> Vec<(String, String, String)> {
    let mut all: Vec<_> = report
        .findings
        .iter()
        .map(|f| {
            let status = serde_json::to_value(f.status).unwrap();
            (
                f.code.clone(),
                f.check.clone(),
                status.as_str().unwrap().to_string(),
            )
        })
        .collect();
    all.sort();
    all
}

#[specforge_test(
    behavior = "run_doctor_check",
    verify = "doctor reports every diagnostic the environment reports as one finding, with the part of the setup it is about and the diagnostic's severity"
)]
fn doctor_reports_every_setup_diagnostic_the_environment_reports() {
    for (name, (_dir, project)) in scenarios() {
        let view = ProjectView::of(&project);
        let report = diagnose(&view, &ScriptedSolver::ready());
        let mut expected: Vec<_> = view
            .env()
            .diagnostics()
            .map(|d| {
                let status = match d.severity {
                    Severity::Error => "error",
                    Severity::Warning => "warn",
                    _ => "info",
                };
                (d.code.clone(), d.message.clone(), status.to_string())
            })
            .collect();
        expected.sort();
        assert_eq!(findings(&report), expected, "scenario {name}");
    }
}

#[specforge_test(
    behavior = "run_doctor_check",
    verify = "doctor fails whenever check fails on a diagnostic of the project's setup"
)]
fn doctor_fails_whenever_check_fails_on_a_setup_diagnostic() {
    for (name, (_dir, project)) in scenarios() {
        let view = ProjectView::of(&project);
        let checked = check(&view, &CheckOptions::default()).unwrap();
        let report = diagnose(&view, &ScriptedSolver::ready());
        let setup_failed = view
            .env()
            .diagnostics()
            .any(|d| d.severity == Severity::Error);
        if !checked.ok() {
            assert!(!report.ok(), "scenario {name}: check fails, doctor says ok");
        }
        assert_eq!(report.ok(), !setup_failed, "scenario {name}");
    }
}
