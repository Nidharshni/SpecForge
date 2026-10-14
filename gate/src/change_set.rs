use std::collections::BTreeSet;
use std::process::Command;

use crate::repo::Repo;

/// The paths a hook or quick run judges: repo-relative, `/`-separated.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ChangeSet(BTreeSet<String>);

impl ChangeSet {
    /// `git diff --cached --name-only --diff-filter=ACMR`.
    pub fn staged(repo: &Repo) -> Result<ChangeSet, String> {
        Ok(ChangeSet::of(git_lines(
            repo,
            &["diff", "--cached", "--name-only", "--diff-filter=ACMR"],
        )?))
    }

    /// `git diff --name-only HEAD` plus `git ls-files --others --exclude-standard`.
    pub fn working_tree(repo: &Repo) -> Result<ChangeSet, String> {
        let mut paths = git_lines(repo, &["diff", "--name-only", "HEAD"])?;
        paths.extend(git_lines(
            repo,
            &["ls-files", "--others", "--exclude-standard"],
        )?);
        // A path deleted from the working tree cannot be judged.
        paths.retain(|p| repo.root.join(p).exists());
        Ok(ChangeSet::of(paths))
    }

    pub fn of<I: IntoIterator<Item = S>, S: Into<String>>(paths: I) -> ChangeSet {
        ChangeSet(paths.into_iter().map(Into::into).collect())
    }

    pub fn paths(&self) -> impl Iterator<Item = &str> {
        self.0.iter().map(String::as_str)
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// The changed paths one of `patterns` matches.
    pub fn matching(&self, patterns: &[&str]) -> Vec<&str> {
        self.paths()
            .filter(|path| patterns.iter().any(|p| Pattern(p).matches(path)))
            .collect()
    }
}

fn git_lines(repo: &Repo, args: &[&str]) -> Result<Vec<String>, String> {
    let out = Command::new("git")
        .args(args)
        .current_dir(&repo.root)
        .output()
        .map_err(|e| format!("git {}: {e}", args.join(" ")))?;
    if !out.status.success() {
        return Err(format!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter(|l| !l.is_empty())
        .map(String::from)
        .collect())
}

/// A path pattern: `*` matches within one segment, `**` any number of segments (`**/*.rs`, `crates/**/src/**`).
pub struct Pattern<'a>(pub &'a str);

impl Pattern<'_> {
    pub fn matches(&self, path: &str) -> bool {
        let pattern: Vec<&str> = self.0.split('/').collect();
        let path: Vec<&str> = path.split('/').collect();
        segments(&pattern, &path)
    }
}

fn segments(pattern: &[&str], path: &[&str]) -> bool {
    match pattern.split_first() {
        None => path.is_empty(),
        Some((&"**", rest)) => (0..=path.len()).any(|skip| segments(rest, &path[skip..])),
        Some((head, rest)) => match path.split_first() {
            Some((segment, tail)) => {
                within_segment(head.as_bytes(), segment.as_bytes()) && segments(rest, tail)
            }
            None => false,
        },
    }
}

/// `*` matches any run of characters inside one segment.
fn within_segment(pattern: &[u8], text: &[u8]) -> bool {
    match pattern.split_first() {
        None => text.is_empty(),
        Some((b'*', rest)) => (0..=text.len()).any(|skip| within_segment(rest, &text[skip..])),
        Some((c, rest)) => text
            .split_first()
            .is_some_and(|(t, tail)| t == c && within_segment(rest, tail)),
    }
}

#[cfg(test)]
mod tests;
