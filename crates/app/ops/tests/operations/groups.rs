//! The module groups of `specforge-ops` are a DAG (ADR 0086). `src/lib.rs`
//! writes the groups and what each may use; every path in `src/` that starts at `crate`, `super`
//! or `self` (in a `use`, in code, in a test, in a macro's arguments) names a module of its own
//! group or of a group its group may use. Doc comments are prose, not paths.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use proc_macro2::{TokenStream, TokenTree};
use syn::visit::Visit;

/// The group of every top-level module the table does not name.
const BASE: &str = "base";

/// The table of `lib.rs`: each group and the groups it may use, itself included.
#[derive(Debug)]
struct Groups(BTreeMap<String, BTreeSet<String>>);

impl Groups {
    /// The rows of the `//! | Group | Holds | May use |` table in `lib`, refused when a row is
    /// repeated, names a group with no row, or the groups form a cycle.
    fn of_lib(lib: &str) -> Result<Self, String> {
        let mut rows = BTreeMap::new();
        for line in lib.lines().filter_map(|line| line.strip_prefix("//! |")) {
            let cells: Vec<&str> = line.split('|').map(str::trim).collect();
            let [name, _holds, may_use, ..] = cells[..] else {
                continue;
            };
            if name == "Group" || name.starts_with("---") {
                continue;
            }
            let name = name.trim_matches(['[', ']', '`']).to_string();
            let mut uses: BTreeSet<String> = may_use
                .split(',')
                .map(|group| group.trim().trim_matches('`').to_string())
                .filter(|group| !group.is_empty())
                .collect();
            uses.insert(name.clone());
            if rows.insert(name.clone(), uses).is_some() {
                return Err(format!("the group {name} has two rows"));
            }
        }
        if !rows.contains_key(BASE) {
            return Err(format!("the table has no {BASE} row"));
        }
        for (name, uses) in &rows {
            if let Some(unknown) = uses.iter().find(|group| !rows.contains_key(*group)) {
                return Err(format!("{name} may use {unknown}, which has no row"));
            }
        }
        let groups = Self(rows);
        groups.acyclic()?;
        Ok(groups)
    }

    /// The group of a module, by the first segment of its path from the crate root.
    fn of(&self, first: Option<&str>) -> &str {
        first
            .filter(|first| *first != BASE)
            .and_then(|first| self.0.get_key_value(first))
            .map_or(BASE, |(name, _)| name.as_str())
    }

    fn may_use(&self, group: &str) -> &BTreeSet<String> {
        &self.0[group]
    }

    fn acyclic(&self) -> Result<(), String> {
        fn visit<'g>(
            groups: &'g Groups,
            at: &'g str,
            path: &mut Vec<&'g str>,
            done: &mut BTreeSet<&'g str>,
        ) -> Result<(), String> {
            if done.contains(at) {
                return Ok(());
            }
            if let Some(start) = path.iter().position(|group| *group == at) {
                let mut cycle = path[start..].to_vec();
                cycle.push(at);
                return Err(format!("the groups form a cycle: {}", cycle.join(" -> ")));
            }
            path.push(at);
            for next in groups.0[at].iter().filter(|next| next.as_str() != at) {
                visit(groups, next, path, done)?;
            }
            path.pop();
            done.insert(at);
            Ok(())
        }
        let mut done = BTreeSet::new();
        for at in self.0.keys() {
            visit(self, at, &mut Vec::new(), &mut done)?;
        }
        Ok(())
    }
}

/// The paths of one file that leave the groups its group may use.
struct Check<'a> {
    groups: &'a Groups,
    file: &'a str,
    /// The module being read, from the crate root (`["read", "trace", "tests"]`).
    module: Vec<String>,
    violations: Vec<String>,
}

impl Check<'_> {
    fn path(&mut self, segments: &[String], line: usize) {
        let Some(first) = segments.first() else {
            return;
        };
        let mut absolute = match first.as_str() {
            "crate" => Vec::new(),
            "self" | "super" => self.module.clone(),
            _ => return,
        };
        let mut rest = &segments[1..];
        if first == "super" {
            absolute.pop();
        }
        while rest.first().is_some_and(|segment| segment == "super") {
            absolute.pop();
            rest = &rest[1..];
        }
        absolute.extend(rest.iter().cloned());
        let from = self.groups.of(self.module.first().map(String::as_str));
        let to = self.groups.of(absolute.first().map(String::as_str));
        let allowed = self.groups.may_use(from);
        if !allowed.contains(to) {
            let list: Vec<&str> = allowed.iter().map(String::as_str).collect();
            self.violations.push(format!(
                "src/{}:{line}: `{}` is in {to}, which {from} may not use ({from} may use: {})",
                self.file,
                segments.join("::"),
                list.join(", ")
            ));
        }
    }

    /// The `crate::…`, `super::…` and `self::…` paths a macro's arguments hold.
    fn tokens(&mut self, tokens: TokenStream) {
        let trees: Vec<TokenTree> = tokens.into_iter().collect();
        let mut at = 0;
        while at < trees.len() {
            if let TokenTree::Group(group) = &trees[at] {
                self.tokens(group.stream());
            }
            if let TokenTree::Ident(ident) = &trees[at] {
                let word = ident.to_string();
                if matches!(word.as_str(), "crate" | "super" | "self") && colons(&trees, at + 1) {
                    let line = ident.span().start().line;
                    let mut segments = vec![word];
                    let mut next = at + 1;
                    while colons(&trees, next) {
                        let Some(TokenTree::Ident(segment)) = trees.get(next + 2) else {
                            break;
                        };
                        segments.push(segment.to_string());
                        next += 3;
                    }
                    self.path(&segments, line);
                    at = next;
                    continue;
                }
            }
            at += 1;
        }
    }
}

fn colons(trees: &[TokenTree], at: usize) -> bool {
    matches!(
        (trees.get(at), trees.get(at + 1)),
        (Some(TokenTree::Punct(a)), Some(TokenTree::Punct(b))) if a.as_char() == ':' && b.as_char() == ':'
    )
}

fn use_paths(tree: &syn::UseTree, prefix: &mut Vec<String>, out: &mut Vec<(Vec<String>, usize)>) {
    let leaf = |prefix: &Vec<String>, ident: &syn::Ident| {
        let mut path = prefix.clone();
        path.push(ident.to_string());
        (path, ident.span().start().line)
    };
    match tree {
        syn::UseTree::Path(path) => {
            prefix.push(path.ident.to_string());
            use_paths(&path.tree, prefix, out);
            prefix.pop();
        }
        syn::UseTree::Name(name) => out.push(leaf(prefix, &name.ident)),
        syn::UseTree::Rename(rename) => out.push(leaf(prefix, &rename.ident)),
        syn::UseTree::Glob(glob) => out.push((prefix.clone(), glob.star_token.span.start().line)),
        syn::UseTree::Group(group) => {
            for tree in &group.items {
                use_paths(tree, prefix, out);
            }
        }
    }
}

impl<'ast> Visit<'ast> for Check<'_> {
    fn visit_item_use(&mut self, item: &'ast syn::ItemUse) {
        let mut paths = Vec::new();
        use_paths(&item.tree, &mut Vec::new(), &mut paths);
        for (segments, line) in paths {
            self.path(&segments, line);
        }
    }

    fn visit_path(&mut self, path: &'ast syn::Path) {
        if path.leading_colon.is_none() {
            let segments: Vec<String> = path
                .segments
                .iter()
                .map(|segment| segment.ident.to_string())
                .collect();
            self.path(&segments, path.segments[0].ident.span().start().line);
        }
        syn::visit::visit_path(self, path);
    }

    fn visit_macro(&mut self, mac: &'ast syn::Macro) {
        self.tokens(mac.tokens.clone());
        syn::visit::visit_macro(self, mac);
    }

    fn visit_item_mod(&mut self, item: &'ast syn::ItemMod) {
        if item.content.is_some() {
            self.module.push(item.ident.to_string());
            syn::visit::visit_item_mod(self, item);
            self.module.pop();
        }
    }
}

/// The violations of the file at `file` (relative to `src/`, `/`-separated) holding `source`.
fn check(groups: &Groups, file: &str, source: &str) -> Result<Vec<String>, String> {
    let parsed = syn::parse_file(source).map_err(|error| format!("src/{file}: {error}"))?;
    let mut module: Vec<String> = file
        .trim_end_matches(".rs")
        .split('/')
        .map(str::to_string)
        .collect();
    if module
        .last()
        .is_some_and(|last| last == "mod" || last == "lib")
    {
        module.pop();
    }
    let mut check = Check {
        groups,
        file,
        module,
        violations: Vec::new(),
    };
    check.visit_file(&parsed);
    Ok(check.violations)
}

pub(crate) fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            rust_files(&path, out);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            out.push(path);
        }
    }
}

#[test]
fn every_path_in_src_follows_the_module_groups() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let groups = Groups::of_lib(&std::fs::read_to_string(src.join("lib.rs")).unwrap()).unwrap();
    for group in groups.0.keys().filter(|group| group.as_str() != BASE) {
        assert!(
            src.join(group).join("mod.rs").is_file(),
            "the group {group} has no src/{group}/mod.rs"
        );
    }
    let mut files = Vec::new();
    rust_files(&src, &mut files);
    files.sort();
    let mut violations = Vec::new();
    for path in &files {
        let file = path
            .strip_prefix(&src)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        violations.extend(check(&groups, &file, &std::fs::read_to_string(path).unwrap()).unwrap());
    }
    assert!(
        violations.is_empty(),
        "{} paths break the module groups of src/lib.rs:\n{}",
        violations.len(),
        violations.join("\n")
    );
}

const TABLE: &str = "\
//! | Group | Holds | May use |
//! |---|---|---|
//! | base | x | |
//! | [`read`] | x | base |
//! | [`analysis`] | x | base, read |
";

#[test]
fn a_path_into_a_group_its_group_may_not_use_is_reported() {
    let groups = Groups::of_lib(TABLE).unwrap();
    let source = "\
use crate::analysis::check::Counts;
fn f() { assert!(matches!(1, crate::analysis::X)); }
mod tests { use super::super::super::super::analysis::doctor; use super::guide; use crate::view::ProjectView; }
";
    let found = check(&groups, "read/infer/gaps.rs", source).unwrap();
    assert_eq!(
        found,
        [
            "src/read/infer/gaps.rs:1: `crate::analysis::check::Counts` is in analysis, which read may not use (read may use: base, read)",
            "src/read/infer/gaps.rs:2: `crate::analysis::X` is in analysis, which read may not use (read may use: base, read)",
            "src/read/infer/gaps.rs:3: `super::super::super::super::analysis::doctor` is in analysis, which read may not use (read may use: base, read)",
        ]
    );
    assert_eq!(
        check(&groups, "analysis/doctor.rs", source).unwrap(),
        Vec::<String>::new()
    );
}

#[test]
fn a_cycle_between_groups_is_refused() {
    let cyclic = TABLE.replace(
        "| [`read`] | x | base |",
        "| [`read`] | x | base, analysis |",
    );
    assert_eq!(
        Groups::of_lib(&cyclic).unwrap_err(),
        "the groups form a cycle: analysis -> read -> analysis"
    );
}
