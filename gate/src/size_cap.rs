//! The `file-size` check: no tracked `.rs` file is over [`CAP`] lines, except the files the `file-size` section of
//! `gate/baseline.json` lists, each at most at its listed size. The list only shrinks. A listed file that grew fails;
//! one that shrank, fell under the cap or is gone fails until its entry is lowered or dropped in the same commit. When
//! `cargo gate baseline` lowers the list, a listed file that moved (git's rename detection, from a revision to the
//! working tree) passes its entry to its new path, at its new size; no other file ever gains an entry (ADR 0085).
//!
//! Pure but for [`measure`]; tested on literal input.

use std::collections::BTreeMap;
use std::path::Path;

use crate::baseline::{FileSizes, PATH};

/// The check's name, and its section of `gate/baseline.json`.
pub const CHECK: &str = "file-size";

/// The most lines a tracked `.rs` file may have. Lines as `wc -l` counts them: blank lines, comments and inline
/// `#[cfg(test)]` modules count.
pub const CAP: u64 = 800;

/// The lines of `bytes` as `wc -l` counts them: its newline bytes.
pub fn lines(bytes: &[u8]) -> u64 {
    bytes.iter().filter(|&&b| b == b'\n').count() as u64
}

/// The line count of each of `paths` (repo-relative, `/`-separated) under `root`. A path that cannot be read (in the
/// index, deleted from the working tree) is left out.
pub fn measure<'a>(root: &Path, paths: impl IntoIterator<Item = &'a str>) -> FileSizes {
    paths
        .into_iter()
        .filter_map(|path| {
            let bytes = std::fs::read(root.join(path)).ok()?;
            Some((path.to_string(), lines(&bytes)))
        })
        .collect()
}

/// What the check found.
pub struct Verdict {
    /// For the log: a summary line, then every problem, indented.
    pub lines: Vec<String>,
    /// One line per problem, sorted by path; the check passes when there is none.
    pub problems: Vec<String>,
}

/// The check: `found` (every tracked `.rs` file and its lines) against `listed` (the baseline's section).
pub fn verdict(found: &FileSizes, listed: &FileSizes) -> Verdict {
    let lower = format!("cargo gate baseline --only {CHECK}");
    let mut problems = Vec::new();
    for (path, &now) in found.iter().filter(|&(_, &now)| now > CAP) {
        match listed.get(path) {
            None => problems.push(format!(
                "{path}: {now} lines, over the {CAP}-line cap and not in {PATH}: split it along its sections, or put new code in a new module"
            )),
            Some(&was) if now > was => problems.push(format!(
                "{path} rose {was} -> {now} lines: a listed file may not grow, so put new code or tests in a new module, or split it"
            )),
            Some(&was) if now < was => problems.push(format!(
                "{path} fell {was} -> {now} lines: lower it in {PATH} in the same commit ({lower})"
            )),
            Some(_) => {}
        }
    }
    for (path, &was) in listed {
        match found.get(path) {
            None => problems.push(format!(
                "{path} is gone and {PATH} lists it at {was} lines: drop it in the same commit ({lower}, a moved file carries its entry)"
            )),
            Some(&now) if now <= CAP => problems.push(format!(
                "{path} fell {was} -> {now} lines, under the cap: drop it from {PATH} in the same commit ({lower})"
            )),
            Some(_) => {}
        }
    }
    problems.sort();
    let over = found.values().filter(|&&now| now > CAP).count();
    let mut lines = vec![format!(
        "{} .rs files, {over} over the {CAP}-line cap, {} listed in {PATH}",
        found.len(),
        listed.len()
    )];
    lines.extend(problems.iter().map(|p| format!("  {p}")));
    Verdict { lines, problems }
}

/// What `cargo gate baseline` writes for this check.
pub struct Lowered {
    /// What changed, then why nothing was written when it was not.
    pub lines: Vec<String>,
    /// The new section; `None` when a file rose, or is over the cap with no entry of its own or carried to it.
    pub new_baseline: Option<FileSizes>,
}

/// Lower `listed` to `found`. A file over the cap keeps its entry at today's size, never a larger one. A file over
/// the cap with no entry takes the entry of the listed file it was renamed from (`renames`: from, to), if that file
/// is gone and the entry is not smaller. Entries of files at or under the cap, or gone, are dropped. Refuses (no
/// section) when a file rose or is over the cap with no entry to take.
pub fn lower(found: &FileSizes, listed: &FileSizes, renames: &[(String, String)]) -> Lowered {
    let mut lines = Vec::new();
    let mut refused = false;
    let mut next = FileSizes::new();
    let mut carried: BTreeMap<&str, &str> = BTreeMap::new(); // from -> to
    for (path, &now) in found.iter().filter(|&(_, &now)| now > CAP) {
        let source = match listed.get(path) {
            Some(&was) => Some((was, None)),
            None => renames
                .iter()
                .find(|(from, to)| {
                    to == path && !found.contains_key(from) && !carried.contains_key(from.as_str())
                })
                .and_then(|(from, _)| listed.get(from).map(|&was| (was, Some(from.as_str())))),
        };
        let Some((was, from)) = source else {
            lines.push(format!(
                "  {path}: {now} lines, over the {CAP}-line cap and not in {PATH}: split it (the baseline takes no new file; a moved one is carried when git sees the move)"
            ));
            refused = true;
            continue;
        };
        if now > was {
            let moved = from.map_or(String::new(), |from| format!(" (moved from {from})"));
            lines.push(format!(
                "  {path} rose {was} -> {now} lines{moved}: split it"
            ));
            refused = true;
            continue;
        }
        match from {
            Some(from) => {
                carried.insert(from, path);
                lines.push(format!(
                    "  {path}: carried from {from}, {was} -> {now} lines"
                ));
            }
            None if now < was => lines.push(format!("  {path} lowered {was} -> {now} lines")),
            None => {}
        }
        next.insert(path.clone(), now);
    }
    for (path, &was) in listed {
        if carried.contains_key(path.as_str()) {
            continue;
        }
        match found.get(path) {
            None => lines.push(format!("  {path} dropped: gone ({was} lines)")),
            Some(&now) if now <= CAP => {
                lines.push(format!("  {path} dropped: {now} lines, under the cap"))
            }
            Some(_) => {}
        }
    }
    let new_baseline = if refused {
        lines.push(format!(
            "{PATH} not written: split the files above first, or stage the move git should see"
        ));
        None
    } else {
        lines.insert(
            0,
            format!("{CHECK}: {} files over the {CAP}-line cap", next.len()),
        );
        Some(next)
    };
    Lowered {
        lines,
        new_baseline,
    }
}

/// The renames in `git diff -M --name-status -z` output: (from, to) for every `R<score>` record. Copies (`C`) are
/// not renames: their source still exists.
pub fn renames(name_status: &str) -> Vec<(String, String)> {
    let mut fields = name_status.split('\0').filter(|f| !f.is_empty());
    let mut found = Vec::new();
    while let Some(status) = fields.next() {
        let two = status.starts_with('R') || status.starts_with('C');
        let from = fields.next();
        let to = if two { fields.next() } else { None };
        if status.starts_with('R')
            && let (Some(from), Some(to)) = (from, to)
        {
            found.push((from.to_string(), to.to_string()));
        }
    }
    found
}

#[cfg(test)]
mod tests;
