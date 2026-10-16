use super::*;
use crate::testing::ScriptedSolver;
use crate::testing::Viewed;
use specforge_project::testing::Assembled;
use specforge_test_macros::test as specforge_test;

fn diag(code: &str, message: &str, suggestion: Option<&str>) -> Diagnostic {
    let mut diagnostic = Diagnostic::untyped(code, Severity::Error, message);
    diagnostic.suggestion = suggestion.map(String::from);
    diagnostic
}

/// A declaration of `name` that contributes the entity kind `keyword`.
fn declaring_kind(name: &str, keyword: &str) -> specforge_protocol_types::ExtensionDeclaration {
    let mut declaration = Assembled::declaration(name, "1.0.0");
    declaration.entities = vec![specforge_protocol_types::EntityKindDescriptor {
        name: keyword.into(),
        keyword: Some(keyword.into()),
        ..Default::default()
    }];
    declaration
}

/// A declaration of `name` that contributes the edge type `label`.
fn declaring_edge(name: &str, label: &str) -> specforge_protocol_types::ExtensionDeclaration {
    let mut declaration = Assembled::declaration(name, "1.0.0");
    declaration.edges = vec![specforge_protocol_types::EdgeTypeDescriptor {
        label: label.into(),
        ..Default::default()
    }];
    declaration
}

#[test]
fn enhancements_list_the_edge_types_their_fields_write() {
    use specforge_protocol_types::{EntityEnhancementDescriptor, FieldDescriptor};
    let field = |name: &str, edge: Option<&str>| FieldDescriptor {
        name: name.into(),
        edge: edge.map(str::to_string),
        ..Default::default()
    };
    let mut declaration = Assembled::declaration("@acme/a", "1.0.0");
    declaration.enhancements = vec![EntityEnhancementDescriptor {
        target_kind: "behavior".into(),
        fields: vec![
            field("requires", Some("Requires")),
            field("note", None),
            field("also_requires", Some("Requires")),
            field("ensures", Some("Ensures")),
        ],
        ..Default::default()
    }];
    let fixture = Assembled::new().declarations(vec![declaration]);

    let report = diagnose(&fixture.view(), &ScriptedSolver::ready());

    let entry = &report.enhancements["behavior"][0];
    assert_eq!(entry.edge_types, ["Requires", "Ensures"]);
}

#[specforge_test(
    behavior = "run_doctor_check",
    verify = "doctor detects shadowed grammar-level constructs"
)]
fn a_kind_registered_twice_is_one_conflict_naming_the_keyword() {
    // What the registry build reports for a kind two extensions declare:
    // the keyword is the diagnostic's data, not a word of its message.
    let fixture = Assembled::new().declarations(vec![
        declaring_kind("@acme/a", "memo"),
        declaring_kind("@acme/b", "memo"),
    ]);

    let report = diagnose(&fixture.view(), &ScriptedSolver::ready());

    assert_eq!(report.findings.len(), 1, "{:?}", report.findings);
    let finding = &report.findings[0];
    assert_eq!(
        finding.about,
        About::Conflict {
            keyword: Some("memo".into())
        }
    );
    assert_eq!(finding.code, "E026");
    assert!(
        finding
            .remediation
            .ends_with("Rename the conflicting kind keyword."),
        "the catalogued explanation: {}",
        finding.remediation
    );
    assert_eq!(
        report.shadowed().map(|(k, _)| k).collect::<Vec<_>>(),
        ["memo"]
    );
    assert_eq!(report.conflicts().count(), 1);
    assert_eq!(report.about(Part::Shadowing).count(), 0);
    assert!(!report.ok());
}

#[test]
fn a_non_shadowing_conflict_is_listed_but_not_shadowed() {
    let fixture = Assembled::new()
        .declarations(vec![
            declaring_edge("@acme/a", "uses"),
            declaring_edge("@acme/b", "uses"),
        ])
        .reporting(vec![diag("W001", "unrelated", None)]);

    let report = diagnose(&fixture.view(), &ScriptedSolver::ready());

    let conflicts: Vec<&Finding> = report.conflicts().collect();
    assert_eq!(conflicts.len(), 1, "{:?}", report.findings);
    assert_eq!(conflicts[0].code, "W018");
    assert_eq!(conflicts[0].about, About::Conflict { keyword: None });
    assert_eq!(report.shadowed().count(), 0);
    // W018 offers no suggestion: the catalogue's explanation stands in.
    assert!(
        conflicts[0]
            .remediation
            .starts_with("Two extensions register an edge type with the same label"),
        "{}",
        conflicts[0].remediation
    );
    // A source diagnostic that is not E013 is no finding.
    assert!(report.findings.iter().all(|f| f.code != "W001"));
}

/// An entry the load could not load, as the environment records it.
fn failed(
    entry: &str,
    problem: specforge_installed::LoadProblem,
    diagnostic: Diagnostic,
) -> specforge_project::EnabledExtension {
    specforge_project::EnabledExtension {
        failure: Some(specforge_installed::LoadFailure {
            problem,
            diagnostic,
        }),
        ..specforge_project::EnabledExtension::unloaded(entry)
    }
}

fn failing_entry(
    entry: &str,
    problem: specforge_installed::LoadProblem,
) -> specforge_project::EnabledExtension {
    failed(
        entry,
        problem,
        Diagnostic::new(codes::E028, format!("{entry} did not load")),
    )
}

#[specforge_test(
    behavior = "run_doctor_check",
    verify = "doctor lists the extensions the extensions listing lists, each with its version, source and status"
)]
fn doctor_lists_the_extensions_the_listing_lists() {
    use specforge_installed::LoadProblem;
    let fixture = Assembled::new()
        .enabled(vec![
            specforge_project::EnabledExtension::unloaded("@specforge/product"),
            specforge_project::EnabledExtension {
                entry: "greet.wasm".into(),
                name: "@sdk/greet".into(),
                file: Some("greet.wasm".into()),
                failure: None,
                warnings: Vec::new(),
            },
            failing_entry("@acme/missing", LoadProblem::NotInstalled),
            failing_entry(
                "@test/probe=nope.wasm",
                LoadProblem::FileMissing {
                    path: "nope.wasm".into(),
                },
            ),
        ])
        .declarations(vec![
            Assembled::declaration("@specforge/product", "1.0.0"),
            Assembled::declaration("@sdk/greet", "0.1.0"),
            Assembled::declaration("@acme/stray", "3.0.0"),
        ])
        .lock(&[
            ("@test/probe", "0.1.0", "local:/blobs/probe.wasm"),
            ("@m/m", "1.0.0", "registry"),
        ]);
    let view = fixture.view();

    let report = diagnose(&view, &ScriptedSolver::ready());

    let doctor: Vec<(&str, Option<&str>, &str, Status)> = report
        .extensions
        .iter()
        .map(|e| {
            (
                e.name.as_str(),
                e.version.as_deref(),
                e.source.as_str(),
                e.status,
            )
        })
        .collect();
    let listing = crate::extension::list(&view);
    let listed: Vec<(&str, Option<&str>, &str, Status)> = listing
        .extensions
        .iter()
        .map(|e| {
            (
                e.name.as_str(),
                e.version.as_deref(),
                e.source.as_str(),
                e.status,
            )
        })
        .collect();
    assert_eq!(doctor, listed);
    assert_eq!(
        doctor,
        [
            ("@acme/missing", None, "unknown", Status::NotLoaded),
            (
                "@acme/stray",
                Some("3.0.0"),
                "unknown",
                Status::NotConfigured
            ),
            ("@m/m", Some("1.0.0"), "registry", Status::NotConfigured),
            (
                "@sdk/greet",
                Some("0.1.0"),
                "file:greet.wasm",
                Status::Loaded
            ),
            (
                "@specforge/product",
                Some("1.0.0"),
                "builtin",
                Status::Loaded
            ),
            ("@test/probe", None, "file:nope.wasm", Status::NotLoaded),
        ]
    );
}

#[test]
fn doctor_lists_extensions_by_name() {
    let fixture = Assembled::new()
        .config(&["@z/z", "@a/a"])
        .declarations(vec![
            Assembled::declaration("@z/z", "1.0.0"),
            Assembled::declaration("@a/a", "1.0.0"),
        ])
        .lock(&[("@m/m", "1.0.0", "registry")]);

    let report = diagnose(&fixture.view(), &ScriptedSolver::ready());

    let names: Vec<&str> = report.extensions.iter().map(|e| e.name.as_str()).collect();
    assert_eq!(names, ["@a/a", "@m/m", "@z/z"]);
}

#[test]
fn doctor_lists_load_failures_in_check_order() {
    use specforge_installed::LoadProblem;
    let fixture = Assembled::new().enabled(vec![
        failing_entry("@z/z", LoadProblem::NotInstalled),
        failing_entry("@a/a", LoadProblem::NotInstalled),
    ]);

    let report = diagnose(&fixture.view(), &ScriptedSolver::ready());

    let checks: Vec<&str> = report.about(Part::Load).map(|f| f.check.as_str()).collect();
    assert_eq!(checks, ["@z/z did not load", "@a/a did not load"]);
}

#[specforge_test(
    behavior = "run_doctor_check",
    verify = "a finding without its own suggestion quotes the catalogued explanation"
)]
fn a_finding_without_a_suggestion_quotes_the_catalogue() {
    use specforge_installed::LoadProblem;
    let fixture = Assembled::new().enabled(vec![
        // No suggestion: the explanation of E028 is quoted.
        failed(
            "@acme/x",
            LoadProblem::NotInstalled,
            diag("E028", "extension '@acme/x' is not installed", None),
        ),
        // Its own suggestion wins.
        failed(
            "@acme/y",
            LoadProblem::NoDeclaration {
                reason: "no handshake".into(),
            },
            diag(
                "E028",
                "protocol loading failed",
                Some("run `specforge add @acme/y`"),
            ),
        ),
    ]);

    let report = diagnose(&fixture.view(), &ScriptedSolver::ready());

    let remedies: Vec<&str> = report
        .about(Part::Load)
        .map(|f| f.remediation.as_str())
        .collect();
    assert_eq!(remedies.len(), 2, "{remedies:?}");
    assert!(
        remedies[0].contains("confirm the extension is installed and up to date"),
        "E028's catalogued explanation: {}",
        remedies[0]
    );
    assert!(
        !remedies[0].contains("specforge explain"),
        "{}",
        remedies[0]
    );
    assert_eq!(remedies[1], "run `specforge add @acme/y`");
    let finding = report.findings.iter().find(|f| f.code == "E028").unwrap();
    assert_eq!(finding.remediation, remedies[0]);
}

#[test]
fn an_entity_id_that_is_a_kind_keyword_is_shadowed_but_not_a_conflict() {
    let mut e013 = diag(
        "E013",
        "entity ID 'behavior' collides with a reserved keyword at project.spec",
        Some("rename the entity"),
    );
    e013.data = Some(Box::new(DiagnosticData::ShadowedKeyword {
        keyword: "behavior".into(),
    }));
    let diagnostics = [e013];

    let fixture = Assembled::new().reporting(diagnostics.to_vec());
    let report = diagnose(&fixture.view(), &ScriptedSolver::ready());

    assert_eq!(report.about(Part::Conflict).count(), 0);
    let shadowed: Vec<&Finding> = report.about(Part::Shadowing).collect();
    assert_eq!(
        shadowed[0].about,
        About::Shadowing {
            keyword: "behavior".into()
        }
    );
    assert_eq!(shadowed[0].remediation, "rename the entity");
    assert!(report.findings.iter().any(|f| f.code == "E013"));
}

#[specforge_test(
    behavior = "run_doctor_check",
    verify = "doctor reports a solver that does not run as the W098 analyze --prove reports, as a toolchain warning"
)]
fn a_solver_that_does_not_run_is_the_w098_prove_reports_as_a_toolchain_warning() {
    let report = diagnose(&Assembled::new().view(), &ScriptedSolver::not_found());
    assert_eq!(
        finding_codes(&report),
        [("I002", FindingStatus::Info), ("W098", FindingStatus::Warn)]
    );
    let w098 = crate::solver::Z3::not_on_path();
    let toolchain: Vec<&Finding> = report.about(Part::Toolchain).collect();
    assert_eq!(toolchain.len(), 1);
    assert_eq!(toolchain[0].check, w098.message);
    assert_eq!(
        Some(toolchain[0].remediation.as_str()),
        w098.suggestion.as_deref()
    );
    assert!(report.ok() && !report.z3_available() && !report.document().z3_available);
    assert_eq!(report.cache_status(), CacheStatus::Ok);
}

#[specforge_test(
    behavior = "management_operations_over_the_project_view",
    verify = "doctor reads the setup diagnostics of its view's environment, and E013 from what its view reports"
)]
fn doctor_reads_the_diagnostics_its_view_reports() {
    let e028 = diag("E028", "extension '@acme/x' is not installed", None);
    let failing = Assembled::new().enabled(vec![failed(
        "@acme/x",
        specforge_installed::LoadProblem::NotInstalled,
        e028,
    )]);
    let report = diagnose(&failing.view(), &ScriptedSolver::ready());
    let failures: Vec<&str> = report.about(Part::Load).map(|f| f.code.as_str()).collect();
    assert_eq!(failures, ["E028"]);
    assert!(!report.ok());

    let clean = Assembled::new();
    let report = diagnose(&clean.view(), &ScriptedSolver::ready());
    assert_eq!(report.about(Part::Load).count(), 0);
    assert!(report.ok());
}

#[test]
fn a_rootless_view_skips_the_installation_checks() {
    let fixture = Assembled::new()
        .declarations(vec![Assembled::declaration("@acme/loaded", "2.0.0")])
        .lock(&[("@acme/locked", "1.0.0", "registry")]);

    let report = diagnose(&fixture.rootless_view(), &ScriptedSolver::ready());

    assert_eq!(report.installed_count, 0);
    assert_eq!(report.cache_status(), CacheStatus::Ok);
    let names: Vec<&str> = report.extensions.iter().map(|e| e.name.as_str()).collect();
    assert_eq!(names, ["@acme/loaded"], "no lock is read without a root");
    assert_eq!(report.extensions[0].status, Status::NotConfigured);

    // Rooted, the lock is read and its binary checked (missing here).
    let report = diagnose(&fixture.view(), &ScriptedSolver::ready());
    assert_eq!(report.installed_count, 1);
    assert_eq!(report.cache_status(), CacheStatus::Stale);
}

fn finding_codes(report: &DoctorReport) -> Vec<(&str, FindingStatus)> {
    report
        .findings
        .iter()
        .map(|f| (f.code.as_str(), f.status))
        .collect()
}

#[specforge_test(
    behavior = "run_doctor_check",
    verify = "doctor in a directory without specforge.json reports config_missing as a warning"
)]
fn doctor_without_specforge_json_says_so() {
    let fixture = Assembled::new().without_config_file();

    let report = diagnose(&fixture.view(), &ScriptedSolver::ready());

    assert_eq!(
        finding_codes(&report),
        [
            (CONFIG_MISSING, FindingStatus::Warn),
            ("I002", FindingStatus::Info)
        ]
    );
    assert!(report.ok(), "a warning: doctor stays healthy");
    assert!(
        report.findings[0]
            .check
            .contains(&fixture.root().display().to_string()),
        "{:?}",
        report.findings[0]
    );
    assert!(report.findings[0].remediation.contains("specforge init"));

    // A project with specforge.json gets no such finding.
    let found = Assembled::new();
    assert_eq!(
        finding_codes(&diagnose(&found.view(), &ScriptedSolver::ready())),
        [("I002", FindingStatus::Info)]
    );
}

#[specforge_test(
    behavior = "run_doctor_check",
    verify = "doctor in a directory without specforge.json reports config_missing as a warning"
)]
fn a_rootless_view_has_no_config_missing_finding() {
    let fixture = Assembled::new().without_config_file();

    let report = diagnose(&fixture.rootless_view(), &ScriptedSolver::ready());

    assert_eq!(
        finding_codes(&report),
        [("I002", FindingStatus::Info)],
        "{:?}",
        report.findings
    );
}

/// A project with `@sdk/greet` installed from a file in a second
/// directory (returned with it: the lock names its path), whose module
/// was then replaced by other bytes.
fn project_with_a_changed_greet() -> (tempfile::TempDir, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("specforge.json"),
        r#"{"name": "p", "version": "0.1.0", "extensions": []}"#,
    )
    .unwrap();
    let files = tempfile::tempdir().unwrap();
    let blob = files.path().join("greet.wasm");
    std::fs::write(&blob, crate::testing::GREET).unwrap();
    crate::extension::add(
        &crate::extension::AddRequest {
            root: &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
            extensions: vec![blob.display().to_string()],
            base: None,
            allow_unsigned: false,
            trust: crate::registry::Trust::Refuse,
            dry_run: false,
            containment: &crate::Containment::unbounded(),
        },
        &crate::registry::Unconfigured("add"),
        &crate::testing::candidates(),
    )
    .unwrap();
    let module = dir
        .path()
        .join(".specforge/extensions/@sdk/greet/extension.wasm");
    let mut bytes = std::fs::read(&module).unwrap();
    bytes.extend_from_slice(b"changed after install");
    std::fs::write(module, bytes).unwrap();
    (dir, files)
}

#[specforge_test(
    behavior = "run_doctor_check",
    verify = "doctor reports a missing or changed installed binary once, with the remedy its load gives"
)]
fn doctor_reports_a_changed_binary_once() {
    let (dir, _files) = project_with_a_changed_greet();
    let runtime = std::sync::Arc::new(crate::testing::candidates());
    let compiled = specforge_project::CompiledProject::compile(
        &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
        Some(runtime.clone()),
    );

    let report = diagnose(&ProjectView::of(&compiled), &ScriptedSolver::ready());

    let about_greet: Vec<(&str, &str)> = report
        .findings
        .iter()
        .filter(|f| f.check.contains("@sdk/greet"))
        .map(|f| (f.code.as_str(), f.remediation.as_str()))
        .collect();
    assert_eq!(about_greet.len(), 1, "{:?}", report.findings);
    let (code, remedy) = about_greet[0];
    assert_eq!(code, "stale_hash");
    assert!(
        remedy.starts_with("run `specforge add ") && remedy.ends_with(".wasm` to reinstall it"),
        "{remedy}"
    );
    // The binary is the one finding: the load failure it causes (E070)
    // is not listed again.
    assert_eq!(report.about(Part::Binary).count(), 1);
    assert_eq!(report.about(Part::Load).count(), 0);
    let greet = report
        .extensions
        .iter()
        .find(|e| e.name == "@sdk/greet")
        .unwrap();
    assert_eq!(greet.status, Status::NotLoaded);
    assert!(report.findings.iter().all(|f| f.code != "E070"));
    assert!(matches!(
        report.findings[0].about,
        About::Binary {
            issue: BinaryIssue::StaleHash { .. }
        }
    ));

    // Running the remedy reinstalls the pinned binary: doctor is clean.
    let command = remedy
        .strip_prefix("run `specforge add ")
        .and_then(|r| r.strip_suffix("` to reinstall it"))
        .unwrap();
    crate::extension::add(
        &crate::extension::AddRequest {
            root: &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
            extensions: vec![command.to_string()],
            base: None,
            allow_unsigned: false,
            trust: crate::registry::Trust::Refuse,
            dry_run: false,
            containment: &crate::Containment::unbounded(),
        },
        &crate::registry::Unconfigured("add"),
        runtime.as_ref(),
    )
    .unwrap();
    let compiled = specforge_project::CompiledProject::compile(
        &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
        Some(runtime),
    );
    let report = diagnose(&ProjectView::of(&compiled), &ScriptedSolver::ready());
    assert!(report.findings.is_empty(), "{:?}", report.findings);
}

#[specforge_test(
    behavior = "provide_mcp_doctor_tool",
    verify = "specforge.doctor reports an unusable specforge.json (E069) as a finding"
)]
fn an_unusable_config_is_an_error_finding() {
    // What the environment reports for a specforge.json that is not JSON.
    let fixture =
        Assembled::new().config_problems(vec![specforge_common::ConfigProblem::NotJson {
            path: "./specforge.json".into(),
            error: "expected value at line 1 column 41".into(),
        }]);

    let report = diagnose(&fixture.view(), &ScriptedSolver::ready());

    assert_eq!(
        finding_codes(&report),
        [
            ("E069", FindingStatus::Error),
            ("I002", FindingStatus::Info)
        ]
    );
    assert!(!report.ok());
    assert_eq!(
        report.findings[0].remediation,
        "fix specforge.json; `specforge explain E069` says what it must be"
    );
    assert_eq!(report.findings[0].about, About::Config);
    assert_eq!(
        report.about(Part::Load).count(),
        0,
        "E069 is about the config, not an extension"
    );
}

#[specforge_test(
    behavior = "run_doctor_check",
    verify = "doctor reports every diagnostic the environment reports as one finding, with the part of the setup it is about and the diagnostic's severity"
)]
fn a_load_warning_is_a_finding_but_leaves_extensions_ok() {
    let fixture = Assembled::new()
        .enabled(vec![specforge_project::EnabledExtension {
            warnings: vec![Diagnostic::new(codes::W153, "its sandbox asks for more")],
            ..specforge_project::EnabledExtension::unloaded("@acme/x")
        }])
        .declarations(vec![Assembled::declaration("@acme/x", "1.0.0")]);

    let report = diagnose(&fixture.view(), &ScriptedSolver::ready());

    assert_eq!(finding_codes(&report), [("W153", FindingStatus::Warn)]);
    assert_eq!(report.findings[0].about, About::Load);
    assert!(report.ok() && report.extensions_ok());
}

#[specforge_test(
    behavior = "run_doctor_check",
    verify = "doctor reports a missing or changed installed binary once, with the remedy its load gives"
)]
fn without_a_root_a_missing_module_is_a_load_finding() {
    use specforge_installed::LoadProblem;
    let fixture = Assembled::new().enabled(vec![failing_entry(
        "@acme/gone",
        LoadProblem::ModuleMissing {
            path: "gone.wasm".into(),
        },
    )]);

    // With a root the lock's binary check states it (no lock entry here,
    // so nothing); without one nothing is checked on disk, so the
    // environment's own diagnostic is the finding.
    let report = diagnose(&fixture.rootless_view(), &ScriptedSolver::ready());

    assert_eq!(report.about(Part::Binary).count(), 0);
    let loads: Vec<&str> = report.about(Part::Load).map(|f| f.code.as_str()).collect();
    assert_eq!(loads, ["E028"]);
    assert!(!report.ok() && !report.extensions_ok());

    let rooted = diagnose(&fixture.view(), &ScriptedSolver::ready());
    assert_eq!(
        rooted.about(Part::Load).count(),
        0,
        "a module-health failure is its binary finding when there is a root"
    );
}
