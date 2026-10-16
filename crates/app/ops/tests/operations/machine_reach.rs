//! Only the machine's adapters touch the machine (ADR 0079, ADR 0125): no process is spawned and no
//! environment variable is read in `src/` outside the SMT solver's adapter and the collector machine's.
//! A new `Command::new(` or `env::var(` elsewhere fails here, and so does a listed entry that no longer
//! matches.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// The calls that spawn a process or read the environment.
const NEEDLES: &[&str] = &["Command::new(", "env::var(", "env::var_os("];

/// `(file under src/, needle, how many lines, why it may reach the machine)`.
const ALLOWED: &[(&str, &str, usize, &str)] = &[
    (
        "solver.rs",
        "Command::new(",
        2,
        "the SMT solver's adapter, ADR 0079",
    ),
    (
        "analysis/collect/process.rs",
        "Command::new(",
        1,
        "the collector machine's adapter, ADR 0125",
    ),
    (
        "analysis/collect/process.rs",
        "env::var_os(",
        3,
        "the collector machine's adapter, ADR 0125",
    ),
];

/// Test support, which stands for the machine.
fn exempt(file: &str) -> bool {
    file.ends_with("tests.rs")
        || file == "testing.rs"
        || file
            .split('/')
            .any(|part| part == "tests" || part == "testing")
}

/// The needles a non-comment line of `source` (up to its first `#[cfg(test)]` module) contains.
fn reaches(source: &str) -> Vec<(usize, &'static str)> {
    let mut found = Vec::new();
    let lines: Vec<&str> = source.lines().collect();
    for (index, line) in lines.iter().enumerate() {
        if line.starts_with("#[cfg(test)]")
            && lines
                .get(index + 1)
                .is_some_and(|next| next.contains("mod "))
        {
            break;
        }
        if line.trim_start().starts_with("//") {
            continue;
        }
        for needle in NEEDLES {
            if line.contains(needle) {
                found.push((index + 1, *needle));
            }
        }
    }
    found
}

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            rust_files(&path, out);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            out.push(path);
        }
    }
}

/// What the check refuses in `files` (file, source): a reach not on the list, or a listed entry that no
/// longer matches.
fn unlisted(files: &[(String, String)], allowed: &[(&str, &str, usize, &str)]) -> Vec<String> {
    let mut lines: BTreeMap<(String, &str), Vec<usize>> = BTreeMap::new();
    for (file, source) in files.iter().filter(|(file, _)| !exempt(file)) {
        for (line, needle) in reaches(source) {
            lines.entry((file.clone(), needle)).or_default().push(line);
        }
    }
    let mut problems = Vec::new();
    for ((file, needle), found) in &lines {
        let listed = allowed
            .iter()
            .find(|(f, n, ..)| f == file && n == needle)
            .map_or(0, |(_, _, count, _)| *count);
        if found.len() != listed {
            problems.push(format!(
                "src/{file}: {} `{needle}` (lines {found:?}), {listed} listed: reach the machine through a port (CollectorMachine, Solver), or list it in ALLOWED with its reason",
                found.len()
            ));
        }
    }
    for (file, needle, count, reason) in allowed {
        if *count > 0 && !lines.contains_key(&((*file).to_string(), *needle)) {
            problems.push(format!(
                "ALLOWED lists src/{file} `{needle}` ({reason}), which no longer matches: delete the entry"
            ));
        }
    }
    problems
}

#[test]
fn only_the_machine_adapters_reach_the_machine() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut paths = Vec::new();
    rust_files(&src, &mut paths);
    paths.sort();
    let files: Vec<(String, String)> = paths
        .iter()
        .map(|path| {
            let name = path
                .strip_prefix(&src)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            (name, std::fs::read_to_string(path).unwrap())
        })
        .collect();
    let problems = unlisted(&files, ALLOWED);
    assert!(
        problems.is_empty(),
        "{} reaches of the machine outside its adapters:\n{}",
        problems.len(),
        problems.join("\n")
    );
}

#[test]
fn a_new_reach_outside_the_adapters_fails_the_check() {
    let files = vec![(
        "analysis/collect/mod.rs".to_string(),
        "fn f() { std::process::Command::new(\"sh\"); }\n".to_string(),
    )];
    let problems = unlisted(&files, &[]);
    assert_eq!(problems.len(), 1, "{problems:?}");
    assert!(
        problems[0].contains("src/analysis/collect/mod.rs"),
        "{problems:?}"
    );

    // Listed with its count, it passes; a stale entry fails.
    let listed = [("analysis/collect/mod.rs", "Command::new(", 1, "test")];
    assert!(unlisted(&files, &listed).is_empty());
    let stale = [("analysis/collect/gone.rs", "Command::new(", 1, "test")];
    assert_eq!(unlisted(&files, &stale).len(), 2);

    // Comments, and everything after the test module, are not reaches.
    let quiet = vec![(
        "read/quiet.rs".to_string(),
        "// std::env::var(a)\n#[cfg(test)]\nmod t { fn f() { std::env::var(a); } }\n".to_string(),
    )];
    assert!(unlisted(&quiet, &[]).is_empty());
}
