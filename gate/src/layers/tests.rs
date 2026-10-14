use super::*;
use crate::repo::this_repo;

use DependencyKind::{Build, Dev, Normal};

/// A member at `dir` with path dependencies `(name, dir, kind)`.
fn member(name: &str, dir: &str, dependencies: &[(&str, &str, DependencyKind)]) -> Member {
    Member {
        name: name.into(),
        dir: dir.into(),
        dependencies: dependencies
            .iter()
            .map(|&(name, dir, kind)| Dependency {
                name: name.into(),
                dir: dir.into(),
                kind,
            })
            .collect(),
    }
}

const COMMON: (&str, &str) = ("specforge-common", "crates/kernel/common");
const OPS: (&str, &str) = ("specforge-ops", "crates/app/ops");
const CLI: (&str, &str) = ("specforge-cli", "crates/surfaces/cli");
const LSP: (&str, &str) = ("specforge-lsp", "crates/surfaces/lsp");
const TEST: (&str, &str) = ("specforge-test", "integrations/rust/specforge-test");

fn edge(
    (name, dir): (&'static str, &'static str),
    kind: DependencyKind,
) -> (&'static str, &'static str, DependencyKind) {
    (name, dir, kind)
}

#[test]
fn a_crates_layer_is_its_folder_under_crates() {
    assert_eq!(place("crates/leaf/coverage"), Place::Layer(0));
    assert_eq!(place("crates/kernel/tree-sitter"), Place::Layer(1));
    assert_eq!(place("crates/surfaces/cli"), Place::Layer(7));
    for outside in ["gate", "xtask", "integrations/rust/specforge-test", ""] {
        assert_eq!(place(outside), Place::Outside, "{outside}");
    }
    for misplaced in [
        "crates/specforge-cli",
        "crates/frontend/cli",
        "crates/kernel/parser/fuzz",
        "crates/kernel/",
        "crates",
    ] {
        assert_eq!(place(misplaced), Place::Misplaced, "{misplaced}");
    }
}

#[test]
fn an_upward_normal_dependency_is_reported() {
    let members = [
        member(COMMON.0, COMMON.1, &[edge(OPS, Normal)]),
        member(OPS.0, OPS.1, &[]),
    ];
    assert_eq!(
        verdict(&members, &[]).violations,
        [
            "crates/kernel/common/Cargo.toml: specforge-common (kernel) has a normal dependency on specforge-ops (app), a higher layer"
        ]
    );
}

#[test]
fn an_upward_build_dependency_is_reported() {
    let members = [
        member(OPS.0, OPS.1, &[edge(CLI, Build)]),
        member(CLI.0, CLI.1, &[]),
    ];
    assert_eq!(
        verdict(&members, &[]).violations,
        [
            "crates/app/ops/Cargo.toml: specforge-ops (app) has a build dependency on specforge-cli (surfaces), a higher layer"
        ]
    );
}

#[test]
fn a_dev_dependency_may_point_up_or_out() {
    // specforge-registry-client's tests run against specforge-registry-server; every crate's tests link
    // specforge-test.
    let members = [
        member(OPS.0, OPS.1, &[edge(CLI, Dev), edge(TEST, Dev)]),
        member(CLI.0, CLI.1, &[]),
    ];
    let found = verdict(&members, &[]);
    assert!(found.violations.is_empty(), "{:?}", found.violations);
    assert_eq!(
        found.lines.last().unwrap(),
        "0 normal and build dependencies of crates in a layer: 0 down, 0 within a layer, 0 up \
         (0 accepted by an ADR); 2 dev-dependencies not judged"
    );
}

#[test]
fn a_dependency_down_or_within_its_layer_passes() {
    let members = [
        member(
            CLI.0,
            CLI.1,
            &[edge(LSP, Normal), edge(OPS, Normal), edge(COMMON, Build)],
        ),
        member(LSP.0, LSP.1, &[edge(COMMON, Normal)]),
        member(OPS.0, OPS.1, &[edge(COMMON, Normal)]),
        member(COMMON.0, COMMON.1, &[]),
    ];
    let found = verdict(&members, &[]);
    assert!(found.violations.is_empty(), "{:?}", found.violations);
    assert_eq!(
        found.lines,
        [
            "surfaces : specforge-cli specforge-lsp",
            "adapters : ",
            "app      : specforge-ops",
            "session  : ",
            "runtime  : ",
            "pipeline : ",
            "kernel   : specforge-common",
            "leaf     : ",
            "no layer : ",
            "5 normal and build dependencies of crates in a layer: 4 down, 1 within a layer, 0 up \
             (0 accepted by an ADR); 0 dev-dependencies not judged",
        ]
    );
}

#[test]
fn a_crate_in_a_layer_may_not_link_a_member_in_no_layer() {
    let members = [
        member(COMMON.0, COMMON.1, &[edge(TEST, Normal)]),
        // A member in no layer may depend on any crate.
        member(
            TEST.0,
            TEST.1,
            &[edge(
                ("specforge-protocol-types", "crates/leaf/protocol-types"),
                Normal,
            )],
        ),
        member(
            "xtask",
            "xtask",
            &[edge(OPS, Normal), edge(("gate", "gate"), Normal)],
        ),
    ];
    assert_eq!(
        verdict(&members, &[]).violations,
        [
            "crates/kernel/common/Cargo.toml: specforge-common (kernel) has a normal dependency on specforge-test (integrations/rust/specforge-test), which is in no layer"
        ]
    );
}

#[test]
fn a_crate_under_crates_outside_a_layer_is_reported_once() {
    let members = [
        member(
            "specforge-cli",
            "crates/specforge-cli",
            &[edge(OPS, Normal)],
        ),
        member(
            OPS.0,
            OPS.1,
            &[edge(("specforge-cli", "crates/specforge-cli"), Normal)],
        ),
    ];
    assert_eq!(
        verdict(&members, &[]).violations,
        [
            "crates/specforge-cli: specforge-cli is under crates/ but not in crates/<layer>/<name>/ (layers: leaf, kernel, pipeline, runtime, session, app, adapters, surfaces)"
        ]
    );
}

#[test]
fn an_accepted_edge_passes_and_an_entry_that_accepts_nothing_fails() {
    let members = [
        member(COMMON.0, COMMON.1, &[edge(OPS, Normal)]),
        member(OPS.0, OPS.1, &[edge(COMMON, Normal)]),
    ];
    let accepts = |from, to| Allowed {
        from,
        to,
        adr: "0081",
    };
    let found = verdict(&members, &[accepts("specforge-common", "specforge-ops")]);
    assert!(found.violations.is_empty(), "{:?}", found.violations);
    assert!(
        found
            .lines
            .last()
            .unwrap()
            .contains("1 up (1 accepted by an ADR)")
    );
    // A downward edge needs no entry, and an entry for it is stale.
    assert_eq!(
        verdict(
            &members,
            &[
                accepts("specforge-common", "specforge-ops"),
                accepts("specforge-ops", "specforge-common"),
            ]
        )
        .violations,
        [
            "gate/src/layers.rs accepts specforge-ops -> specforge-common (ADR 0081), which is no upward normal or build dependency: delete the entry"
        ]
    );
}

#[test]
fn every_accepted_edge_names_an_adr_that_exists() {
    let adrs: Vec<String> = std::fs::read_dir(this_repo().root.join("docs/adr"))
        .unwrap()
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    for entry in ALLOWED {
        assert!(
            entry.adr.len() == 4 && entry.adr.bytes().all(|b| b.is_ascii_digit()),
            "{} -> {}: `{}` is no ADR number",
            entry.from,
            entry.to,
            entry.adr
        );
        assert!(
            adrs.iter()
                .any(|file| file.starts_with(&format!("{}-", entry.adr))),
            "{} -> {}: docs/adr has no ADR {}",
            entry.from,
            entry.to,
            entry.adr
        );
    }
}
