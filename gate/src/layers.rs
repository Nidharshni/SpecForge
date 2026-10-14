//! The layer rule (ADR 0084): a crate's layer is its folder under `crates/`, and no normal or build dependency
//! points to a crate in a higher layer. Dev-dependencies are not judged: they link into no library or binary.
//! A workspace member outside `crates/` (the gate, xtask, the Rust test integration) is in no layer: it may depend
//! on any crate, and no crate in a layer may have a normal or build dependency on it.
//!
//! It also holds the pairs an ADR keeps apart ([`APART`]): the LSP links no package registry client (ADR 0010).

use crate::repo::{Dependency, DependencyKind, Member};

/// The file that holds the rule: the layer order and the accepted edges. The check runs when it changes.
pub const RULE: &str = "gate/src/layers.rs";

/// The layers, bottom to top: the folders under `crates/`. A crate may depend on crates in its own layer and in the
/// layers below it.
pub const LAYERS: &[&str] = &[
    "leaf", "kernel", "pipeline", "runtime", "session", "app", "adapters", "surfaces",
];

/// An upward normal or build dependency that an ADR accepts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Allowed {
    /// The package that depends.
    pub from: &'static str,
    /// The package it depends on, in a higher layer or in none.
    pub to: &'static str,
    /// The four-digit number of the ADR that accepts the edge (`docs/adr/<adr>-*.md`).
    pub adr: &'static str,
}

/// The accepted upward edges. Empty: every normal and build dependency points down or stays in its layer.
pub const ALLOWED: &[Allowed] = &[];

/// Two crates an ADR keeps apart: `from` must not reach `to` through any chain of normal or build dependencies.
/// Dev-dependencies are not followed: they link into no library or binary.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Apart {
    /// The package that must not link `to`.
    pub from: &'static str,
    /// The package it must not link.
    pub to: &'static str,
    /// The four-digit number of the ADR that keeps them apart (`docs/adr/<adr>-*.md`).
    pub adr: &'static str,
}

/// ADR 0010: the LSP links no package registry client, neither the client, nor its HTTP shapes (ADR 0044), nor
/// the adapter over it. A cargo feature cannot give this, because features unify across a workspace build. Only
/// the dependency graph can.
pub const APART: &[Apart] = &[
    Apart {
        from: "specforge-lsp",
        to: "specforge-registry-client",
        adr: "0010",
    },
    Apart {
        from: "specforge-lsp",
        to: "specforge-registry-wire",
        adr: "0010",
    },
    Apart {
        from: "specforge-lsp",
        to: "specforge-ops-registry",
        adr: "0010",
    },
];

/// Each pair of `apart` that `members` link anyway, as the shortest chain that links them, and each entry naming a
/// package that is no member (so the list cannot go stale). `cargo metadata` lists optional dependencies without
/// resolving features, so a chain through an optional dependency is reported too: the rule over-approximates.
pub fn linked_apart(members: &[Member], apart: &[Apart]) -> Vec<String> {
    let known = |name: &str| members.iter().any(|m| m.name == name);
    let mut violations = Vec::new();
    for rule in apart {
        if let Some(missing) = [rule.from, rule.to].into_iter().find(|name| !known(name)) {
            violations.push(format!(
                "{RULE} keeps {} apart from {} (ADR {}), but no workspace member is named {missing}: fix or delete the entry",
                rule.from, rule.to, rule.adr
            ));
            continue;
        }
        if let Some(chain) = chain(members, rule.from, rule.to) {
            violations.push(format!(
                "{} links {} (ADR {} keeps them apart): {}",
                rule.from,
                rule.to,
                rule.adr,
                chain.join(" -> ")
            ));
        }
    }
    violations
}

/// The shortest chain of normal or build dependencies from `from` to `to`, both ends included.
fn chain<'a>(members: &'a [Member], from: &'a str, to: &'a str) -> Option<Vec<&'a str>> {
    use std::collections::{BTreeMap, VecDeque};
    let mut parent: BTreeMap<&str, &str> = BTreeMap::from([(from, from)]);
    let mut queue = VecDeque::from([from]);
    while let Some(name) = queue.pop_front() {
        if name == to {
            let mut path = vec![to];
            let mut at = to;
            while at != from {
                at = parent[at];
                path.push(at);
            }
            path.reverse();
            return Some(path);
        }
        let Some(member) = members.iter().find(|m| m.name == name) else {
            continue;
        };
        for dependency in &member.dependencies {
            if dependency.kind != DependencyKind::Dev
                && !parent.contains_key(dependency.name.as_str())
            {
                parent.insert(dependency.name.as_str(), name);
                queue.push_back(dependency.name.as_str());
            }
        }
    }
    None
}

/// Where a member's directory puts it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Place {
    /// `crates/<layer>/<name>`, by the layer's index in [`LAYERS`].
    Layer(usize),
    /// Outside `crates/`: a tool or test support, in no layer.
    Outside,
    /// Under `crates/`, but not `crates/<layer>/<name>` with a layer of [`LAYERS`].
    Misplaced,
}

/// The place of a repo-relative directory: `crates/kernel/parser` is in `kernel`, `xtask` is outside,
/// `crates/parser` and `crates/kernel/parser/fuzz` are misplaced.
pub fn place(dir: &str) -> Place {
    let mut segments = dir.split('/');
    if segments.next() != Some("crates") {
        return Place::Outside;
    }
    match (segments.next(), segments.next(), segments.next()) {
        (Some(layer), Some(name), None) if !name.is_empty() => LAYERS
            .iter()
            .position(|known| *known == layer)
            .map_or(Place::Misplaced, Place::Layer),
        _ => Place::Misplaced,
    }
}

/// What the layer check found.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct LayerVerdict {
    /// Each violation: one sentence naming the manifest, the directory or the entry to edit.
    pub violations: Vec<String>,
    /// The stack (each layer and its crates, top first), the members in no layer, and the edge counts.
    pub lines: Vec<String>,
}

/// The layer rule over `members` (`cargo metadata`'s, each with its path dependencies), accepting `allowed`. An
/// entry of `allowed` that accepts no upward edge is a violation too, so the list stays exact.
pub fn verdict(members: &[Member], allowed: &[Allowed]) -> LayerVerdict {
    let mut verdict = LayerVerdict::default();
    let mut used = vec![false; allowed.len()];
    let (mut down, mut within, mut up, mut accepted, mut dev) = (0, 0, 0, 0, 0);
    for member in members {
        let from = match place(&member.dir) {
            Place::Layer(layer) => layer,
            Place::Outside => continue,
            Place::Misplaced => {
                verdict.violations.push(format!(
                    "{}: {} is under crates/ but not in crates/<layer>/<name>/ (layers: {})",
                    member.dir,
                    member.name,
                    LAYERS.join(", ")
                ));
                continue;
            }
        };
        for dependency in &member.dependencies {
            if dependency.kind == DependencyKind::Dev {
                dev += 1;
                continue;
            }
            let to = match place(&dependency.dir) {
                Place::Layer(to) if to < from => {
                    down += 1;
                    continue;
                }
                Place::Layer(to) if to == from => {
                    within += 1;
                    continue;
                }
                Place::Layer(to) => Some(to),
                Place::Outside => None,
                // The member's own violation names it.
                Place::Misplaced => continue,
            };
            up += 1;
            match allowed
                .iter()
                .position(|a| a.from == member.name && a.to == dependency.name)
            {
                Some(entry) => {
                    used[entry] = true;
                    accepted += 1;
                }
                None => verdict
                    .violations
                    .push(upward(member, from, dependency, to)),
            }
        }
    }
    for (entry, used) in allowed.iter().zip(used) {
        if !used {
            verdict.violations.push(format!(
                "{RULE} accepts {} -> {} (ADR {}), which is no upward normal or build dependency: delete the entry",
                entry.from, entry.to, entry.adr
            ));
        }
    }
    verdict.lines = stack(members);
    verdict.lines.push(format!(
        "{} normal and build dependencies of crates in a layer: {down} down, {within} within a layer, {up} up \
         ({accepted} accepted by an ADR); {dev} dev-dependencies not judged",
        down + within + up
    ));
    verdict
}

/// `<dir>/Cargo.toml: <name> (<layer>) has a <kind> dependency on <name> (<layer>), a higher layer`, or `…
/// (<dir>), which is in no layer`.
fn upward(member: &Member, from: usize, dependency: &Dependency, to: Option<usize>) -> String {
    let kind = if dependency.kind == DependencyKind::Build {
        "build"
    } else {
        "normal"
    };
    let target = match to {
        Some(to) => format!("{} ({}), a higher layer", dependency.name, LAYERS[to]),
        None => format!(
            "{} ({}), which is in no layer",
            dependency.name, dependency.dir
        ),
    };
    format!(
        "{}/Cargo.toml: {} ({}) has a {kind} dependency on {target}",
        member.dir, member.name, LAYERS[from]
    )
}

/// Each layer with its crates, top first, then the members in no layer.
fn stack(members: &[Member]) -> Vec<String> {
    let names = |wanted: Place| {
        let mut names: Vec<&str> = members
            .iter()
            .filter(|m| place(&m.dir) == wanted)
            .map(|m| m.name.as_str())
            .collect();
        names.sort_unstable();
        names.join(" ")
    };
    let mut lines: Vec<String> = (0..LAYERS.len())
        .rev()
        .map(|layer| format!("{:<8} : {}", LAYERS[layer], names(Place::Layer(layer))))
        .collect();
    lines.push(format!("{:<8} : {}", "no layer", names(Place::Outside)));
    lines
}

#[cfg(test)]
mod tests;
