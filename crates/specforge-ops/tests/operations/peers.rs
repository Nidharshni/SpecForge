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

/// What a case adds.
enum Adds {
    /// A `.wasm` file with these bytes.
    Local(&'static [u8]),
    /// A builtin.
    Builtin(&'static str),
}

/// An extension installed by hand: name, module bytes, locked version and locked peers.
type Locked = (
    &'static str,
    &'static [u8],
    &'static str,
    &'static [(&'static str, &'static str)],
);

/// One case of the table: name, config entries, files under the root, extensions installed by
/// hand, what is added, and the code the add is refused with (`None`: it passes).
type Case = (
    &'static str,
    &'static [&'static str],
    &'static [(&'static str, &'static [u8])],
    &'static [Locked],
    Adds,
    Option<&'static str>,
);

/// Rewrite the lock entry of `name` to record `version` and `peers`.
fn lock_record(root: &Path, name: &str, version: &str, peers: &[(&str, &str)]) {
    let path = root.join("specforge.lock");
    let mut lock: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    for entry in lock["entries"].as_array_mut().unwrap() {
        if entry["name"] == name {
            entry["version"] = version.into();
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

/// The table `add_and_check_agree_on_the_project_peer_set` and the dry run's twin walk.
fn add_cases() -> Vec<Case> {
    use specforge_ops::testing::{BASE_1, BASE_2, DEP_1, NEEDS_SOFTWARE_1, NEEDS_SOFTWARE_9};

    vec![
        (
            "builtin out of range",
            &["@specforge/software"],
            &[],
            &[],
            Adds::Local(NEEDS_SOFTWARE_9),
            Some("E027"),
        ),
        (
            "builtin not enabled",
            &[],
            &[],
            &[],
            Adds::Local(NEEDS_SOFTWARE_1),
            None,
        ),
        (
            "enabling breaks an installed one",
            &["@probe/needs-software"],
            &[],
            &[(
                "@probe/needs-software",
                NEEDS_SOFTWARE_9,
                "1.0.0",
                &[("@specforge/software", "^9.0")],
            )],
            Adds::Builtin("@specforge/software"),
            Some("E027"),
        ),
        (
            "breaks a file entry",
            &["dep.wasm"],
            &[("dep.wasm", DEP_1)],
            &[],
            Adds::Local(BASE_2),
            Some("E027"),
        ),
        (
            "file entry does not satisfy",
            &["base.wasm"],
            &[("base.wasm", BASE_2)],
            &[],
            Adds::Local(DEP_1),
            Some("E027"),
        ),
        (
            "missing non-builtin",
            &[],
            &[],
            &[],
            Adds::Local(DEP_1),
            Some("E027"),
        ),
        (
            "satisfied pair",
            &["base.wasm"],
            &[("base.wasm", BASE_1)],
            &[],
            Adds::Local(DEP_1),
            None,
        ),
        (
            "locked, not enabled",
            &[],
            &[],
            &[("@probe/dep", DEP_1, "1.0.0", &[("@probe/base", "^1.0")])],
            Adds::Local(BASE_2),
            None,
        ),
    ]
}

#[specforge_test(
    behavior = "add_extension_to_existing_project",
    verify = "add and check agree: an add check would report E027 or E073 for is refused, and an add that passes leaves none"
)]
fn add_and_check_agree_on_the_project_peer_set() {
    use specforge_ops::extension::{self, AddRequest, Source, Trust};
    use specforge_ops::testing::probes;

    let cases = add_cases();

    for (case, config, files, locked, adds, refused) in cases {
        let dir = project(config);
        for (path, bytes) in files {
            std::fs::write(dir.path().join(path), bytes).unwrap();
        }
        for (name, bytes, version, peers) in locked {
            specforge_installed::testing::install_module(dir.path(), name, bytes);
            lock_record(dir.path(), name, version, peers);
        }
        let outside = tempfile::tempdir().unwrap();
        let source = match adds {
            Adds::Local(bytes) => {
                let file = outside.path().join("added.wasm");
                std::fs::write(&file, bytes).unwrap();
                Source::Local(file)
            }
            Adds::Builtin(name) => Source::Builtin(name),
        };
        let before = snapshot(dir.path());

        let result = extension::add(
            &AddRequest {
                root: &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
                extensions: vec![source.into()],
                allow_unsigned: false,
                trust: Trust::Refuse,
                dry_run: false,
            },
            &specforge_ops::registry::Unconfigured("test"),
            &probes(),
        );

        match refused {
            Some(code) => {
                let error = result.unwrap_err();
                assert_eq!(error.code, code, "{case}: {error:?}");
                assert_eq!(snapshot(dir.path()), before, "{case}: nothing written");
            }
            None => {
                result.unwrap_or_else(|error| panic!("{case}: {error:?}"));
                let compiled = CompiledProject::compile(
                    &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
                    Some(Arc::new(probes())),
                );
                let all = compiled.diagnostics();
                let peers: Vec<_> = all
                    .iter()
                    .filter(|d| matches!(d.code.as_str(), "E027" | "E073"))
                    .collect();
                assert!(peers.is_empty(), "{case}: {peers:?}");
            }
        }
    }
}

/// Every file under `root` with its bytes.
fn snapshot(root: &Path) -> std::collections::BTreeMap<std::path::PathBuf, Vec<u8>> {
    let mut files = std::collections::BTreeMap::new();
    let mut dirs = vec![root.to_path_buf()];
    while let Some(dir) = dirs.pop() {
        for entry in std::fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                dirs.push(path);
            } else {
                files.insert(path.clone(), std::fs::read(&path).unwrap());
            }
        }
    }
    files
}

#[specforge_test(
    behavior = "add_extension_to_existing_project",
    verify = "a dry run answers what the add answers, refusals and the builtins it enables included, and writes nothing"
)]
#[ignore = "fixed by T4"]
fn a_dry_run_answers_what_the_add_answers() {
    use specforge_ops::extension::{self, AddRequest, Source, Trust};
    use specforge_ops::testing::probes;

    for (case, config, files, locked, adds, _refused) in add_cases() {
        let dir = project(config);
        for (path, bytes) in files {
            std::fs::write(dir.path().join(path), bytes).unwrap();
        }
        for (name, bytes, version, peers) in locked {
            specforge_installed::testing::install_module(dir.path(), name, bytes);
            lock_record(dir.path(), name, version, peers);
        }
        let outside = tempfile::tempdir().unwrap();
        let source = match adds {
            Adds::Local(bytes) => {
                let file = outside.path().join("added.wasm");
                std::fs::write(&file, bytes).unwrap();
                Source::Local(file)
            }
            Adds::Builtin(name) => Source::Builtin(name),
        };
        let root = specforge_common::ProjectRoot::of(dir.path()).unwrap();
        let ask = |dry_run| {
            extension::add(
                &AddRequest {
                    root: &root,
                    extensions: vec![source.clone().into()],
                    allow_unsigned: false,
                    trust: Trust::Refuse,
                    dry_run,
                },
                &specforge_ops::registry::Unconfigured("test"),
                &probes(),
            )
        };
        let before = snapshot(dir.path());

        let dry = ask(true);
        assert_eq!(
            snapshot(dir.path()),
            before,
            "{case}: a dry run writes nothing"
        );
        let real = ask(false);

        match (dry, real) {
            (Err(dry), Err(real)) => {
                assert_eq!((dry.code, dry.message), (real.code, real.message), "{case}");
            }
            (Ok(dry), Ok(real)) => {
                let peers = |added: &extension::Added| {
                    let document = serde_json::to_value(added.document()).unwrap();
                    document["added"][0]["peers_enabled"].clone()
                };
                assert_eq!(
                    serde_json::to_value(dry.document()).unwrap()["added"][0]["action"],
                    "planned",
                    "{case}"
                );
                assert_eq!(peers(&dry), peers(&real), "{case}");
            }
            (dry, real) => panic!("{case}: dry run {dry:?}, real add {real:?}"),
        }
    }
}

/// The compiled project `config` and `files` describe, `locked` extensions installed by hand.
fn removal_project(
    config: &[&str],
    files: &[(&str, &[u8])],
    locked: &[Locked],
) -> tempfile::TempDir {
    let dir = project(config);
    for (path, bytes) in files {
        std::fs::write(dir.path().join(path), bytes).unwrap();
    }
    for (name, bytes, version, peers) in locked {
        specforge_installed::testing::install_module(dir.path(), name, bytes);
        lock_record(dir.path(), name, version, peers);
    }
    dir
}

#[specforge_test(
    behavior = "remove_extension",
    verify = "remove and check agree: a removal check would report E027 for is refused, and a removal that passes leaves none"
)]
#[ignore = "fixed by T3"]
fn remove_and_check_agree_on_the_project_peer_set() {
    use specforge_ops::extension::{self, RemoveRequest};
    use specforge_ops::testing::{BASE_1, DEP_1, NEEDS_SOFTWARE_1, NEEDS_SOFTWARE_9, probes};

    let software_needs_9: Locked = (
        "@probe/needs-software",
        NEEDS_SOFTWARE_9,
        "1.0.0",
        &[("@specforge/software", "^9.0")],
    );
    let software_needs_1: Locked = (
        "@probe/needs-software",
        NEEDS_SOFTWARE_1,
        "1.0.0",
        &[("@specforge/software", "^1.0")],
    );
    let dep_needs_base: Locked = ("@probe/dep", DEP_1, "1.0.0", &[("@probe/base", "^1.0")]);
    // name, config, files, locked, the argument, force, whether it is refused
    #[allow(clippy::type_complexity)]
    let cases: Vec<(
        &str,
        &[&str],
        &[(&str, &[u8])],
        Vec<Locked>,
        &str,
        bool,
        bool,
    )> = vec![
        (
            "a locked extension no entry enables keeps nothing",
            &["@specforge/software"],
            &[],
            vec![software_needs_9],
            "@specforge/software",
            false,
            false,
        ),
        (
            "an enabled dependent refuses",
            &["@specforge/software", "@probe/needs-software"],
            &[],
            vec![software_needs_1],
            "@specforge/software",
            false,
            true,
        ),
        (
            "a file entry another file entry requires refuses",
            &["base.wasm", "dep.wasm"],
            &[("base.wasm", BASE_1), ("dep.wasm", DEP_1)],
            vec![],
            "base.wasm",
            false,
            true,
        ),
        (
            "a file entry only a disabled lock entry requires goes",
            &["base.wasm"],
            &[("base.wasm", BASE_1)],
            vec![dep_needs_base],
            "base.wasm",
            false,
            false,
        ),
        (
            "force removes what is required",
            &["@specforge/software", "@probe/needs-software"],
            &[],
            vec![software_needs_1],
            "@specforge/software",
            true,
            false,
        ),
    ];

    for (case, config, files, locked, name, force, refused) in cases {
        let dir = removal_project(config, files, &locked);
        let root = specforge_common::ProjectRoot::of(dir.path()).unwrap();
        let compiled = CompiledProject::compile(&root, Some(Arc::new(probes())));
        let before = snapshot(dir.path());

        let result = extension::remove(
            &ProjectView::of(&compiled),
            &RemoveRequest {
                name,
                force,
                dry_run: false,
            },
        );

        if refused {
            let error = result.unwrap_err();
            assert_eq!(error.code, "E027", "{case}: {error:?}");
            if name == "@specforge/software" {
                assert!(
                    error.message.contains("'@probe/needs-software'"),
                    "{case}: {error:?}"
                );
            }
            assert_eq!(snapshot(dir.path()), before, "{case}: nothing written");
        } else {
            result.unwrap_or_else(|error| panic!("{case}: {error:?}"));
            if !force {
                let recompiled = CompiledProject::compile(&root, Some(Arc::new(probes())));
                let all = recompiled.diagnostics();
                let peers: Vec<_> = all
                    .iter()
                    .filter(|d| matches!(d.code.as_str(), "E027" | "E073"))
                    .collect();
                assert!(peers.is_empty(), "{case}: {peers:?}");
            }
        }
    }
}
