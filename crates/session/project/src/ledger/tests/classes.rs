//! The rules every recorded input follows, one table over its four classes: an
//! environment input, a check input, a directory the checks listed and a hop of a
//! symlinked source (ADR 0114 D3, ADR 0129 D3, ADR 0133).

#![cfg(unix)]

use std::fs;
use std::path::{Path, PathBuf};

use specforge_common::ProjectConfig;
use specforge_registry::{Disk, PlainDisk};
use specforge_test_macros::test as specforge_test;

use crate::ledger::disk::RunDisk;
use crate::ledger::inputs::{WatchRoot, Watched};
use crate::ledger::recorded::Recording;
use crate::ledger::{Changes, Ledger, Reads, SessionInputs};

/// `t/root` (the project and its spec root), `t/mid` (where middle hops live),
/// `t/out` (targets) and `t/far` (exists, empty), all canonical.
struct Layout {
    _t: tempfile::TempDir,
    root: PathBuf,
    mid: PathBuf,
    out: PathBuf,
    far: PathBuf,
}

fn layout() -> Layout {
    let t = tempfile::TempDir::new().unwrap();
    let base = fs::canonicalize(t.path()).unwrap();
    let dirs = ["root", "mid", "out", "far"].map(|name| {
        let dir = base.join(name);
        fs::create_dir(&dir).unwrap();
        dir
    });
    let [root, mid, out, far] = dirs;
    Layout {
        _t: t,
        root,
        mid,
        out,
        far,
    }
}

/// `root/<name> -> mid/<name> -> out/<name>`, and `out/other-<name>`, longer.
fn chain(l: &Layout, name: &str) -> PathBuf {
    fs::write(l.out.join(name), "first").unwrap();
    fs::write(l.out.join(format!("other-{name}")), "another, longer").unwrap();
    std::os::unix::fs::symlink(l.out.join(name), l.mid.join(name)).unwrap();
    std::os::unix::fs::symlink(l.mid.join(name), l.root.join(name)).unwrap();
    l.root.join(name)
}

fn fresh(l: &Layout) -> Ledger {
    Ledger::over(SessionInputs::opened(
        &l.root,
        &ProjectConfig::default(),
        Vec::new(),
    ))
}

/// Which list of `Changes` a change lands in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Slot {
    Sources,
    Environment,
    Checks,
}

struct Case {
    name: &'static str,
    /// The input's file name in `root`, `mid` and `out` (a listing: unused).
    file: &'static str,
    /// Lay the input out and return its own path.
    lay: fn(&Layout) -> PathBuf,
    /// Record the input as a session reads it.
    record: fn(&Layout, &mut Ledger, &Path),
    slot: Slot,
    /// Whether a read follows it through links (not a listing).
    follows_links: bool,
}

fn record_environment(l: &Layout, ledger: &mut Ledger, input: &Path) {
    let run = ledger.environment_run();
    let _ = RunDisk(&run, &PlainDisk).read(input);
    run.finish(Recording::Environment {
        root: &l.root,
        config: &ProjectConfig::default(),
    });
}

fn record_check(_: &Layout, ledger: &mut Ledger, input: &Path) {
    let run = ledger.checks();
    RunDisk(&run, &PlainDisk).exists(input);
    run.finish(Recording::Checks);
}

fn record_listing(_: &Layout, ledger: &mut Ledger, input: &Path) {
    let run = ledger.checks();
    RunDisk(&run, &PlainDisk).list(input);
    run.finish(Recording::Checks);
}

fn record_hop(l: &Layout, ledger: &mut Ledger, _: &Path) {
    ledger.source(&l.root, "a.spec");
}

fn lay_listing(l: &Layout) -> PathBuf {
    let dir = l.out.join("listed");
    fs::create_dir(&dir).unwrap();
    fs::write(dir.join("a.x"), "a").unwrap();
    dir
}

const ENVIRONMENT: Case = Case {
    name: "ENVIRONMENT",
    file: "env.json",
    lay: |l| chain(l, "env.json"),
    record: record_environment,
    slot: Slot::Environment,
    follows_links: true,
};
const CHECK: Case = Case {
    name: "CHECK",
    file: "doc.md",
    lay: |l| chain(l, "doc.md"),
    record: record_check,
    slot: Slot::Checks,
    follows_links: true,
};
const LISTING: Case = Case {
    name: "LISTING",
    file: "",
    lay: lay_listing,
    record: record_listing,
    slot: Slot::Checks,
    follows_links: false,
};
const HOP: Case = Case {
    name: "HOP",
    file: "a.spec",
    lay: |l| chain(l, "a.spec"),
    record: record_hop,
    slot: Slot::Sources,
    follows_links: true,
};
const CASES: [Case; 4] = [ENVIRONMENT, CHECK, LISTING, HOP];

/// The cases whose input follows links.
fn linked() -> impl Iterator<Item = Case> {
    CASES.into_iter().filter(|case| case.follows_links)
}

/// The one list of `changes` that is not empty; panics, naming the case, on none or several.
fn slot(case: &Case, changes: &Changes) -> Slot {
    let lists = [
        (Slot::Sources, !changes.sources.is_empty()),
        (Slot::Environment, !changes.environment.is_empty()),
        (Slot::Checks, !changes.check_inputs.is_empty()),
    ];
    let filled: Vec<Slot> = lists.iter().filter(|l| l.1).map(|l| l.0).collect();
    assert_eq!(filled.len(), 1, "{}: {changes:?}", case.name);
    filled[0]
}

/// A change behind the input: a longer write (a listing: a new file in it).
fn change_behind(case: &Case, l: &Layout, input: &Path) {
    if case.follows_links {
        fs::write(l.out.join(case.file), "changed, and a good deal longer").unwrap();
    } else {
        fs::write(input.join("b.x"), "b").unwrap();
    }
}

/// Every path whose change reaches the input.
fn reaching(case: &Case, l: &Layout, input: &Path) -> Vec<PathBuf> {
    if case.follows_links {
        vec![
            input.to_path_buf(),
            l.mid.join(case.file),
            l.out.join(case.file),
            l.out.clone(),
        ]
    } else {
        vec![input.to_path_buf(), input.join("b.x"), l.out.clone()]
    }
}

#[test]
fn every_input_is_reached_by_its_name_each_hop_and_a_directory_above() {
    for case in CASES {
        let l = layout();
        let input = (case.lay)(&l);
        let mut ledger = fresh(&l);
        (case.record)(&l, &mut ledger, &input);
        change_behind(&case, &l, &input);
        for path in reaching(&case, &l, &input) {
            let changes = ledger.changes([path.as_path()]);
            assert_eq!(slot(&case, &changes), case.slot, "{}: {path:?}", case.name);
        }
        let beside = l.out.join("sibling.x");
        fs::write(&beside, "x").unwrap();
        assert!(
            ledger.changes([beside.as_path()]).is_empty(),
            "{}: a sibling of the target is not an input",
            case.name
        );
    }
}

#[specforge_test(
    behavior = "classify_project_changes",
    verify = "an input that is a symlink pointed elsewhere is its change"
)]
fn an_input_pointed_elsewhere_is_its_change() {
    for case in linked() {
        let l = layout();
        let input = (case.lay)(&l);
        let mut ledger = fresh(&l);
        (case.record)(&l, &mut ledger, &input);
        let hop = l.mid.join(case.file);
        fs::remove_file(&hop).unwrap();
        std::os::unix::fs::symlink(l.out.join(format!("other-{}", case.file)), &hop).unwrap();
        let changes = ledger.changes([hop.as_path()]);
        assert_eq!(slot(&case, &changes), case.slot, "{}", case.name);
    }
}

#[specforge_test(
    behavior = "classify_project_changes",
    verify = "a session's watch roots cover every input it classifies"
)]
fn the_directory_of_every_input_and_hop_is_watched() {
    for case in linked() {
        let l = layout();
        let input = (case.lay)(&l);
        let mut ledger = fresh(&l);
        (case.record)(&l, &mut ledger, &input);
        let roots = ledger.inputs().watch_roots();
        for dir in [&l.mid, &l.out] {
            let watch = WatchRoot {
                dir: dir.clone(),
                recursive: true,
            };
            assert!(
                roots.contains(&watch),
                "{}: {dir:?} in {roots:?}",
                case.name
            );
        }
    }
}

#[test]
fn watched_names_every_input_and_hop() {
    for case in CASES {
        let l = layout();
        let input = (case.lay)(&l);
        let mut ledger = fresh(&l);
        (case.record)(&l, &mut ledger, &input);
        let watched = ledger.inputs().watched();
        let has = |w: Watched| assert!(watched.contains(&w), "{}: {w:?} in {watched:?}", case.name);
        match case.slot {
            Slot::Environment | Slot::Checks if case.follows_links => {
                has(Watched::File(l.root.join(case.file)));
                has(Watched::File(l.mid.join(case.file)));
                has(Watched::File(l.out.join(case.file)));
            }
            Slot::Sources => {
                has(Watched::File(l.mid.join(case.file)));
                has(Watched::File(l.out.join(case.file)));
                // The sources glob covers the link itself.
                assert!(!watched.contains(&Watched::File(l.root.join(case.file))));
            }
            _ => has(Watched::Listing(input.clone())),
        }
    }
}

#[specforge_test(
    behavior = "classify_project_changes",
    verify = "a directory created on the way to a missing input's directory outside the root is that input's change, a source hop's included"
)]
fn a_missing_directory_on_the_way_to_an_input_is_pending() {
    for case in linked() {
        let l = layout();
        let input = (case.lay)(&l);
        let mut ledger = fresh(&l);
        (case.record)(&l, &mut ledger, &input);
        // Pointed into a directory that does not exist yet, outside the root.
        let hop = l.mid.join(case.file);
        fs::remove_file(&hop).unwrap();
        std::os::unix::fs::symlink(l.far.join("new").join(case.file), &hop).unwrap();
        (case.record)(&l, &mut ledger, &input);

        let ancestor = WatchRoot {
            dir: l.far.clone(),
            recursive: false,
        };
        let roots = ledger.inputs().watch_roots();
        assert!(roots.contains(&ancestor), "{}: {roots:?}", case.name);

        fs::create_dir(l.far.join("beside")).unwrap();
        let beside = l.far.join("beside");
        assert!(
            ledger.changes([beside.as_path()]).is_empty(),
            "{}: a directory beside is nothing",
            case.name
        );
        fs::create_dir(l.far.join("new")).unwrap();
        let created = l.far.join("new");
        let changes = ledger.changes([created.as_path()]);
        assert_eq!(slot(&case, &changes), case.slot, "{}", case.name);
    }
}

#[test]
fn recording_the_same_reads_again_moves_nothing() {
    for case in CASES {
        let l = layout();
        let input = (case.lay)(&l);
        let mut ledger = fresh(&l);
        (case.record)(&l, &mut ledger, &input);
        let first = ledger.inputs().clone();
        (case.record)(&l, &mut ledger, &input);
        assert_eq!(ledger.inputs(), &first, "{}", case.name);
        if case.follows_links {
            let hop = l.mid.join(case.file);
            fs::remove_file(&hop).unwrap();
            std::os::unix::fs::symlink(l.out.join(format!("other-{}", case.file)), &hop).unwrap();
            (case.record)(&l, &mut ledger, &input);
            assert_ne!(ledger.inputs(), &first, "{}", case.name);
        }
    }
}

#[test]
fn stale_sees_a_change_behind_every_input() {
    for case in CASES {
        let l = layout();
        let input = (case.lay)(&l);
        let mut ledger = fresh(&l);
        (case.record)(&l, &mut ledger, &input);
        change_behind(&case, &l, &input);
        assert_eq!(slot(&case, &ledger.stale()), case.slot, "{}", case.name);
    }
}
