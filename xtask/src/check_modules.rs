//! Holds `spec/product/modules.spec` to the workspace (ADR 0091). Every member under `crates/` or
//! `integrations/rust/` is one `module` entity titled with its package name. Its `description` is the manifest's
//! `description`, and its `depends_on` names exactly the modules of the members it depends on through a normal or a
//! build dependency.
//!
//!   cargo run -p xtask --bin check-modules
//!
//! Prints one line per difference (`MISSING`, `DUPLICATE`, `DESCRIPTION`, `DEPENDS_ON`); exits 1 on any, 2 when
//! `cargo metadata` or the project cannot be read.
#![expect(
    clippy::print_stdout,
    reason = "a developer tool: its report is its stdout"
)]

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use gate::crate_map::MODULES_SPEC;
use gate::{DependencyKind, Member, Repo};
use specforge_common::ProjectRoot;
use specforge_component::ComponentRuntime;
use specforge_project::CompiledProject;

/// The members the spec models: the product crates and the published Rust test integration, not the repository's
/// tooling (`gate`, `xtask`).
const MODELLED: [&str; 2] = ["crates/", "integrations/rust/"];
/// The kind a crate is modelled as (`@specforge/product`'s module).
const KIND: &str = "module";

/// One `module` entity as the spec declares it.
struct Declared {
    id: String,
    description: Option<String>,
    depends_on: BTreeSet<String>,
}

fn main() {
    let repo = Repo::discover().unwrap_or_else(|e| fail(&e));
    let found = differences(&repo).unwrap_or_else(|e| fail(&e));
    for line in &found {
        println!("{line}");
    }
    if !found.is_empty() {
        std::process::exit(1);
    }
}

fn fail(why: &str) -> ! {
    eprintln!("check-modules: {why}");
    std::process::exit(2);
}

/// The modules the spec declares, by their title (the package name they model).
fn declared(project: &CompiledProject) -> BTreeMap<String, Vec<Declared>> {
    let mut by_title: BTreeMap<String, Vec<Declared>> = BTreeMap::new();
    for record in project
        .entities()
        .records()
        .iter()
        .filter(|r| r.kind == KIND)
    {
        let Some(title) = project
            .graph()
            .node(&record.id)
            .and_then(|n| n.title.clone())
        else {
            continue;
        };
        by_title.entry(title).or_default().push(Declared {
            id: record.id.clone(),
            description: record
                .fields
                .iter()
                .find(|f| f.key == "description")
                .map(|f| f.text.clone()),
            depends_on: record
                .fields
                .iter()
                .filter(|f| f.key == "depends_on")
                .flat_map(|f| f.items.iter().flatten().cloned())
                .collect(),
        });
    }
    by_title
}

fn differences(repo: &Repo) -> Result<Vec<String>, String> {
    let root =
        ProjectRoot::of(&repo.root).map_err(|e| format!("{}: {e:?}", repo.root.display()))?;
    let project =
        CompiledProject::compile(&root, Some(Arc::new(ComponentRuntime::with_user_cache())));
    let by_title = declared(&project);
    let modelled: Vec<&Member> = repo
        .members
        .iter()
        .filter(|m| MODELLED.iter().any(|dir| m.dir.starts_with(dir)))
        .collect();
    let mut found = Vec::new();
    let mut module_of: BTreeMap<&str, &Declared> = BTreeMap::new();
    for member in &modelled {
        match by_title
            .get(&member.name)
            .map(Vec::as_slice)
            .unwrap_or_default()
        {
            [] => found.push(format!(
                "MISSING {}: {MODULES_SPEC} has no `module` titled \"{}\"",
                member.name, member.name
            )),
            [one] => {
                module_of.insert(member.name.as_str(), one);
            }
            many => found.push(format!(
                "DUPLICATE {}: the modules {} are all titled \"{}\"",
                member.name,
                many.iter()
                    .map(|d| d.id.as_str())
                    .collect::<Vec<_>>()
                    .join(", "),
                member.name
            )),
        }
    }
    for member in &modelled {
        let Some(module) = module_of.get(member.name.as_str()) else {
            continue;
        };
        if module.description != member.description {
            found.push(format!(
                "DESCRIPTION {}: {MODULES_SPEC} says {:?}; {}/Cargo.toml says {:?}",
                module.id,
                module.description.as_deref().unwrap_or_default(),
                member.dir,
                member.description.as_deref().unwrap_or_default()
            ));
        }
        let expected: BTreeSet<String> = member
            .dependencies
            .iter()
            .filter(|d| d.kind != DependencyKind::Dev && d.name != member.name)
            .filter_map(|d| module_of.get(d.name.as_str()))
            .map(|d| d.id.clone())
            .collect();
        let add: Vec<&str> = expected
            .difference(&module.depends_on)
            .map(String::as_str)
            .collect();
        let remove: Vec<&str> = module
            .depends_on
            .difference(&expected)
            .map(String::as_str)
            .collect();
        if !add.is_empty() || !remove.is_empty() {
            found.push(format!(
                "DEPENDS_ON {}: add [{}], remove [{}] (its manifest's normal and build dependencies)",
                module.id,
                add.join(", "),
                remove.join(", ")
            ));
        }
    }
    Ok(found)
}
