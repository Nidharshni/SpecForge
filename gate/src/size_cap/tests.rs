use super::*;

use std::process::Command;

use crate::baseline::Baseline;
use crate::repo::Repo;
use crate::run::{Mode, Outcome};

fn sizes(entries: &[(&str, u64)]) -> FileSizes {
    entries.iter().map(|&(p, n)| (p.to_string(), n)).collect()
}

fn renamed(from: &str, to: &str) -> (String, String) {
    (from.to_string(), to.to_string())
}

#[test]
fn lines_are_what_wc_counts() {
    assert_eq!(lines(b""), 0);
    assert_eq!(lines(b"fn f() {}"), 0);
    assert_eq!(lines(b"fn f() {}\n"), 1);
    assert_eq!(lines(b"a\n\n// c\n"), 3);
    assert_eq!(lines(&[0xff, b'\n', 0xfe]), 1, "bytes, not UTF-8");
}

#[test]
fn a_file_over_the_cap_fails_unless_listed_and_the_cap_itself_passes() {
    let found = sizes(&[("a.rs", 801), ("b.rs", 800), ("c.rs", 12)]);
    let verdict = verdict(&found, &FileSizes::new());
    assert_eq!(verdict.problems.len(), 1);
    assert!(
        verdict.problems[0].starts_with(
            "a.rs: 801 lines, over the 800-line cap and not in gate/baseline.json: split it"
        ),
        "{:?}",
        verdict.problems
    );
    assert_eq!(
        verdict.lines[0],
        "3 .rs files, 1 over the 800-line cap, 0 listed in gate/baseline.json"
    );
    assert!(
        super::verdict(&found, &sizes(&[("a.rs", 801)]))
            .problems
            .is_empty()
    );
}

#[test]
fn a_listed_file_may_not_grow_and_one_that_shrank_or_went_asks_for_its_entry() {
    let listed = sizes(&[("a.rs", 900)]);
    let problems = |now: Option<u64>| {
        let found = now.map_or_else(FileSizes::new, |n| sizes(&[("a.rs", n)]));
        verdict(&found, &listed).problems
    };
    assert!(problems(Some(900)).is_empty());
    let rose = problems(Some(901));
    assert!(
        rose[0].starts_with("a.rs rose 900 -> 901 lines: a listed file may not grow"),
        "{rose:?}"
    );
    assert_eq!(
        problems(Some(899)),
        [
            "a.rs fell 900 -> 899 lines: lower it in gate/baseline.json in the same commit (cargo gate baseline --only file-size)"
        ]
    );
    assert_eq!(
        problems(Some(800)),
        [
            "a.rs fell 900 -> 800 lines, under the cap: drop it from gate/baseline.json in the same commit (cargo gate baseline --only file-size)"
        ]
    );
    assert_eq!(
        problems(None),
        [
            "a.rs is gone and gate/baseline.json lists it at 900 lines: drop it in the same commit (cargo gate baseline --only file-size, a moved file carries its entry)"
        ]
    );
}

#[test]
fn problems_come_sorted_by_path() {
    let found = sizes(&[("b.rs", 1000), ("c.rs", 950)]);
    let listed = sizes(&[("a.rs", 900), ("c.rs", 900)]);
    let problems = verdict(&found, &listed).problems;
    let paths: Vec<&str> = problems
        .iter()
        .map(|p| p.split([' ', ':']).next().unwrap())
        .collect();
    assert_eq!(paths, ["a.rs", "b.rs", "c.rs"]);
}

#[test]
fn lowering_tightens_entries_and_drops_the_ones_under_the_cap_or_gone() {
    let found = sizes(&[("a.rs", 850), ("b.rs", 700), ("d.rs", 1000), ("e.rs", 10)]);
    let listed = sizes(&[("a.rs", 900), ("b.rs", 900), ("c.rs", 900), ("d.rs", 1000)]);
    let lowered = lower(&found, &listed, &[]);
    assert_eq!(
        lowered.new_baseline,
        Some(sizes(&[("a.rs", 850), ("d.rs", 1000)]))
    );
    assert_eq!(
        lowered.lines,
        [
            "file-size: 2 files over the 800-line cap",
            "  a.rs lowered 900 -> 850 lines",
            "  b.rs dropped: 700 lines, under the cap",
            "  c.rs dropped: gone (900 lines)",
        ]
    );
}

#[test]
fn lowering_never_takes_a_new_file_or_a_larger_size() {
    let listed = sizes(&[("a.rs", 900)]);
    let new = lower(&sizes(&[("a.rs", 900), ("n.rs", 801)]), &listed, &[]);
    assert_eq!(new.new_baseline, None);
    assert!(
        new.lines[0]
            .starts_with("  n.rs: 801 lines, over the 800-line cap and not in gate/baseline.json")
    );
    assert_eq!(
        new.lines.last().unwrap(),
        "gate/baseline.json not written: split the files above first, or stage the move git should see"
    );
    let rose = lower(&sizes(&[("a.rs", 901)]), &listed, &[]);
    assert_eq!(rose.new_baseline, None);
    assert_eq!(rose.lines[0], "  a.rs rose 900 -> 901 lines: split it");
}

#[test]
fn a_moved_file_carries_its_entry_at_its_new_size() {
    let listed = sizes(&[("crates/x/src/old.rs", 1200)]);
    let moves = [renamed("crates/x/src/old.rs", "crates/x/src/group/old.rs")];
    let carried = lower(
        &sizes(&[("crates/x/src/group/old.rs", 1100)]),
        &listed,
        &moves,
    );
    assert_eq!(
        carried.new_baseline,
        Some(sizes(&[("crates/x/src/group/old.rs", 1100)]))
    );
    assert_eq!(
        carried.lines[1],
        "  crates/x/src/group/old.rs: carried from crates/x/src/old.rs, 1200 -> 1100 lines"
    );
    assert_eq!(
        carried.lines.len(),
        2,
        "the old entry is carried, not dropped: {:?}",
        carried.lines
    );

    // A move that grew the file, a rename from an unlisted file, a "from" that still exists: refused.
    let grew = lower(
        &sizes(&[("crates/x/src/group/old.rs", 1201)]),
        &listed,
        &moves,
    );
    assert_eq!(grew.new_baseline, None);
    assert_eq!(
        grew.lines[0],
        "  crates/x/src/group/old.rs rose 1200 -> 1201 lines (moved from crates/x/src/old.rs): split it"
    );
    let unlisted = lower(
        &sizes(&[("b.rs", 900)]),
        &FileSizes::new(),
        &[renamed("a.rs", "b.rs")],
    );
    assert_eq!(unlisted.new_baseline, None);
    let both = sizes(&[
        ("crates/x/src/old.rs", 1200),
        ("crates/x/src/group/old.rs", 900),
    ]);
    assert_eq!(lower(&both, &listed, &moves).new_baseline, None);
}

#[test]
fn a_split_carries_the_entry_to_the_part_git_pairs_with_the_old_file() {
    // `add.rs` became `add/mod.rs` (under the cap) and `add/tests.rs` (git: R073 add.rs -> add/tests.rs).
    let listed = sizes(&[("ops/src/extension/add.rs", 3191)]);
    let found = sizes(&[
        ("ops/src/extension/add/mod.rs", 875),
        ("ops/src/extension/add/tests.rs", 2313),
    ]);
    let moves = [renamed(
        "ops/src/extension/add.rs",
        "ops/src/extension/add/tests.rs",
    )];
    let lowered = lower(&found, &listed, &moves);
    assert_eq!(
        lowered.new_baseline, None,
        "add/mod.rs is over the cap with no entry"
    );
    let found = sizes(&[
        ("ops/src/extension/add/mod.rs", 640),
        ("ops/src/extension/add/plan.rs", 235),
        ("ops/src/extension/add/tests.rs", 2313),
    ]);
    assert_eq!(
        lower(&found, &listed, &moves).new_baseline,
        Some(sizes(&[("ops/src/extension/add/tests.rs", 2313)]))
    );
}

#[test]
fn renames_are_read_from_git_name_status_z() {
    let out = "M\0a.rs\0R087\0old.rs\0new.rs\0A\0c.rs\0C075\0x.rs\0y.rs\0D\0gone.rs\0R100\0p/q.rs\0r/q.rs\0";
    assert_eq!(
        renames(out),
        [renamed("old.rs", "new.rs"), renamed("p/q.rs", "r/q.rs")]
    );
    assert!(renames("").is_empty());
}

/// A git repository in a temp dir: `gate/baseline.json` with `listed`, and one file per `(path, lines)`, committed.
fn repo_with(files: &[(&str, u64)], listed: &FileSizes) -> (tempfile::TempDir, Repo) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_path_buf();
    for &(path, n) in files {
        write_lines(&root, path, n);
    }
    std::fs::create_dir_all(root.join("gate")).unwrap();
    Baseline {
        file_size: listed.clone(),
        ..Baseline::default()
    }
    .write(&root)
    .unwrap();
    git(&root, &["init", "-q"]);
    git(&root, &["add", "-A"]);
    git(
        &root,
        &[
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@t",
            "commit",
            "-qm",
            "base",
        ],
    );
    let repo = Repo {
        target_dir: root.join("target"),
        members: vec![],
        root,
    };
    (dir, repo)
}

fn write_lines(root: &Path, path: &str, n: u64) {
    let file = root.join(path);
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    let text: String = (0..n).map(|i| format!("// line {i} of {path}\n")).collect();
    std::fs::write(file, text).unwrap();
}

fn git(root: &Path, args: &[&str]) {
    let status = Command::new("git")
        .args(args)
        .current_dir(root)
        .status()
        .unwrap();
    assert!(status.success(), "git {args:?}");
}

fn outcome(repo: &Repo) -> Result<(), String> {
    let report = crate::run::run(Mode::Full, repo, Some(CHECK)).unwrap();
    let (_, outcome, _) = report
        .outcomes
        .into_iter()
        .find(|(name, _, _)| *name == CHECK)
        .unwrap();
    match outcome {
        Outcome::Passed => Ok(()),
        Outcome::Failed(why) => Err(why),
        _ => panic!("{CHECK} neither passed nor failed"),
    }
}

#[test]
fn the_check_judges_the_tracked_files_of_the_working_tree() {
    let (_dir, repo) = repo_with(
        &[
            ("src/big.rs", 900),
            ("src/small.rs", 10),
            ("notes.txt", 5000),
        ],
        &sizes(&[("src/big.rs", 900)]),
    );
    assert_eq!(outcome(&repo), Ok(()));
    // An untracked file is not judged until it is staged.
    write_lines(&repo.root, "src/new.rs", 801);
    assert_eq!(outcome(&repo), Ok(()));
    git(&repo.root, &["add", "src/new.rs"]);
    let why = outcome(&repo).unwrap_err();
    assert!(why.starts_with("src/new.rs: 801 lines"), "{why}");
}

#[test]
fn baseline_carries_an_entry_through_a_staged_move_and_the_check_then_passes() {
    let (_dir, repo) = repo_with(&[("src/big.rs", 900)], &sizes(&[("src/big.rs", 900)]));
    std::fs::create_dir_all(repo.root.join("src/group")).unwrap();
    git(&repo.root, &["mv", "src/big.rs", "src/group/big.rs"]);
    let why = outcome(&repo).unwrap_err();
    assert!(
        why.contains("src/big.rs is gone") && why.contains("src/group/big.rs: 900 lines"),
        "{why}"
    );

    let lines = crate::run::baseline(&repo, Some(CHECK), None).unwrap();
    assert!(
        lines
            .contains(&"  src/group/big.rs: carried from src/big.rs, 900 -> 900 lines".to_string()),
        "{lines:?}"
    );
    assert_eq!(
        Baseline::read(&repo.root).unwrap().file_size,
        sizes(&[("src/group/big.rs", 900)])
    );
    assert_eq!(outcome(&repo), Ok(()));
}

#[test]
fn baseline_since_a_revision_sees_a_move_already_committed() {
    let (_dir, repo) = repo_with(&[("src/big.rs", 900)], &sizes(&[("src/big.rs", 900)]));
    git(&repo.root, &["mv", "src/big.rs", "src/moved.rs"]);
    git(
        &repo.root,
        &[
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@t",
            "commit",
            "-qm",
            "move",
        ],
    );
    assert!(
        crate::run::baseline(&repo, Some(CHECK), None).is_err(),
        "HEAD sees no move"
    );
    crate::run::baseline(&repo, Some(CHECK), Some("HEAD~1")).unwrap();
    assert_eq!(
        Baseline::read(&repo.root).unwrap().file_size,
        sizes(&[("src/moved.rs", 900)])
    );
}

#[test]
fn baseline_refuses_a_section_no_check_keeps() {
    let (_dir, repo) = repo_with(&[], &FileSizes::new());
    let why = crate::run::baseline(&repo, Some("rustfmt"), None).unwrap_err();
    assert_eq!(why, "`rustfmt` keeps no baseline (file-size, specs)");
}
