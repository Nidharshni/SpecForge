//! Reports a test's `verify` text its spec entity does not declare (A016)
//! without running a test.
//!
//! Every `#[spec(...)]` / `#[specforge_test_macros::test(...)]` attribute is
//! read from the Rust sources with `syn`; the project is compiled; and the
//! pairs go through the coverage crate's own rule (`Verdict::of`), the one
//! `specforge analyze coverage` applies to a recorded report, so the two
//! cannot disagree.
//!
//!   cargo run -p xtask --bin check-links                     the repository's tests against its spec
//!   cargo run -p xtask --bin check-links -- ROOT [DIR...]    ROOT's project, scanning DIR... under it
//!
//! Prints `A016 <file>:<line> <entity> "<verify>"` per finding (and the
//! closest declared text), `UNKNOWN <file>:<line> <entity>` for a link to an
//! entity the project does not have; exits 1 on any.

use specforge_common::ProjectRoot;
use specforge_component::ComponentRuntime;
use specforge_coverage::{RecordedTest, TestResults, assess};
use specforge_project::CompiledProject;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use syn::ext::IdentExt;
use syn::visit::Visit;
use syn::{Attribute, Ident, LitStr, Token};

const DEFAULT_SCAN: [&str; 3] = ["crates", "extensions", "integrations/rust"];
/// The macro's own self-tests link to entities invented for them.
const SELF_TEST_DIR: &str = "integrations/rust/specforge-test";
const SKIPPED_DIRS: [&str; 3] = ["target", ".git", "node_modules"];

/// One test attribute: `<kind> = "<entity id>"` and its `verify` text.
#[derive(Debug)]
struct Link {
    file: String,
    line: usize,
    id: String,
    verify: Option<String>,
}

fn main() {
    let mut args = std::env::args().skip(1);
    let root = PathBuf::from(args.next().unwrap_or_else(|| ".".to_string()));
    let mut scan: Vec<String> = args.collect();
    if scan.is_empty() {
        scan = DEFAULT_SCAN.iter().map(|s| s.to_string()).collect();
    }
    let findings = match check(&root, &scan) {
        Ok(findings) => findings,
        Err(message) => {
            eprintln!("check-links: {message}");
            std::process::exit(2);
        }
    };
    for finding in &findings {
        println!("{finding}");
    }
    if !findings.is_empty() {
        std::process::exit(1);
    }
}

fn check(root: &Path, scan: &[String]) -> Result<Vec<String>, String> {
    let links = read_links(root, scan);
    let project_root = ProjectRoot::of(root).map_err(|e| format!("{}: {e:?}", root.display()))?;
    let project = CompiledProject::compile(
        &project_root,
        Some(Arc::new(ComponentRuntime::with_user_cache())),
    );
    let entities = project.entities().coverage_entities();
    let known: BTreeMap<&str, &specforge_coverage::Entity> =
        entities.iter().map(|e| (e.id.as_str(), e)).collect();

    let mut results = TestResults::default();
    for link in links.iter().filter(|l| known.contains_key(l.id.as_str())) {
        results
            .entities
            .entry(link.id.clone())
            .or_default()
            .push(RecordedTest {
                name: None,
                status: specforge_coverage::PASSING_STATUS.to_string(),
                verify: link.verify.clone(),
            });
    }
    let assessment = assess(&entities, Some(&results), None, None);

    let mut findings = Vec::new();
    let mut reported: BTreeSet<(String, usize)> = BTreeSet::new();
    for link in &links {
        if !reported.insert((link.file.clone(), link.line)) {
            continue;
        }
        let Some(entity) = known.get(link.id.as_str()) else {
            findings.push(format!("UNKNOWN {}:{} {}", link.file, link.line, link.id));
            continue;
        };
        let Some(text) = &link.verify else { continue };
        let undeclared = assessment
            .verdicts
            .get(&link.id)
            .is_some_and(|v| v.undeclared.contains(text));
        if undeclared {
            let mut line = format!("A016 {}:{} {} {text:?}", link.file, link.line, link.id);
            if let Some(near) = closest(text, &entity.verify_texts) {
                line.push_str(&format!("\n     declared: {near:?}"));
            }
            findings.push(line);
        }
    }
    Ok(findings)
}

/// The declared text nearest `text`, when it is near enough to be a typo.
fn closest<'a>(text: &str, declared: &'a [String]) -> Option<&'a String> {
    declared
        .iter()
        .map(|d| (strsim::normalized_levenshtein(text, d), d))
        .filter(|(score, _)| *score >= 0.5)
        .max_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, d)| d)
}

fn read_links(root: &Path, scan: &[String]) -> Vec<Link> {
    let mut links = Vec::new();
    for dir in scan {
        let walk = walkdir::WalkDir::new(root.join(dir))
            .sort_by_file_name()
            .into_iter()
            .filter_entry(|e| {
                !e.file_type().is_dir()
                    || !SKIPPED_DIRS.contains(&e.file_name().to_string_lossy().as_ref())
            });
        for entry in walk.filter_map(Result::ok) {
            let path = entry.path();
            if path.extension().is_none_or(|e| e != "rs") {
                continue;
            }
            let Ok(source) = std::fs::read_to_string(path) else {
                continue;
            };
            let Ok(file) = syn::parse_file(&source) else {
                eprintln!("check-links: skipped {}: does not parse", path.display());
                continue;
            };
            let relative = path.strip_prefix(root).unwrap_or(path);
            if relative.starts_with(SELF_TEST_DIR) {
                continue;
            }
            let mut visitor = Links {
                file: relative.display().to_string(),
                links: &mut links,
            };
            visitor.visit_file(&file);
        }
    }
    links
}

struct Links<'a> {
    file: String,
    links: &'a mut Vec<Link>,
}

impl<'ast> Visit<'ast> for Links<'_> {
    fn visit_attribute(&mut self, attr: &'ast Attribute) {
        if !is_test_attribute(attr) {
            return;
        }
        let Ok(parsed) = attr.parse_args_with(parse_args) else {
            return;
        };
        let line = attr.pound_token.spans[0].start().line;
        self.links.push(Link {
            file: self.file.clone(),
            line,
            id: parsed.0,
            verify: parsed.1,
        });
    }
}

/// The names the test attribute goes by (the macro's own `registration`).
fn is_test_attribute(attr: &Attribute) -> bool {
    let names: Vec<String> = attr
        .path()
        .segments
        .iter()
        .map(|s| s.ident.to_string())
        .collect();
    let names: Vec<&str> = names.iter().map(String::as_str).collect();
    matches!(
        names.as_slice(),
        ["spec"]
            | ["specforge_test"]
            | [
                ..,
                "specforge_test_macros" | "specforge_test" | "specforge",
                "test"
            ]
    )
}

/// `<kind> = "<id>" [, verify = "<text>"] [, __guard_only]`.
fn parse_args(input: syn::parse::ParseStream) -> syn::Result<(String, Option<String>)> {
    let _kind = Ident::parse_any(input)?;
    let _eq: Token![=] = input.parse()?;
    let id: LitStr = input.parse()?;
    let mut verify = None;
    while !input.is_empty() {
        let _comma: Token![,] = input.parse()?;
        if input.is_empty() {
            break;
        }
        let key: Ident = input.parse()?;
        if key == "verify" {
            let _eq: Token![=] = input.parse()?;
            let text: LitStr = input.parse()?;
            verify = Some(text.value());
        }
    }
    Ok((id.value(), verify))
}
