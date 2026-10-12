//! What the registry build checks of the declarations themselves, before
//! it registers anything: their identity and shape (E030), each one's
//! consistency against the loaded peers (W021), and the order of each
//! extension's passes (W145). Peer dependencies (E027) are
//! [`super::validate::peer_dependencies`].

use std::collections::{HashMap, HashSet, VecDeque};

use specforge_common::{Diagnostic, codes};
use specforge_protocol_types::{
    CompilerPassDescriptor, ExtensionDeclaration, FieldDescriptor, FieldType, is_valid_short,
};

use super::populate::keyword;

fn e030(message: String) -> Diagnostic {
    Diagnostic::new(codes::E030, message)
}

/// E030: a declaration the host cannot use as declared: an empty name or
/// version, an `ext_short` that is not lowercase kebab case (it names a CLI
/// subcommand and an MCP tool prefix), an analyzer without a language,
/// file extensions or exports.
pub(crate) fn shape(declaration: &ExtensionDeclaration) -> Vec<Diagnostic> {
    let name = declaration.name();
    let mut diagnostics = Vec::new();
    if name.is_empty() {
        diagnostics.push(e030("extension declaration: its name is empty".to_string()));
    }
    if declaration.version().is_empty() {
        diagnostics.push(e030(format!("extension '{name}': its version is empty")));
    }
    if let Some(short) = &declaration.handshake.ext_short
        && !is_valid_short(short)
    {
        diagnostics.push(
            e030(format!(
                "extension '{name}': ext_short '{short}' is not lowercase kebab case"
            ))
            .with_suggestion(
                "declare a short name like `reports` or `my-tools` ([a-z][a-z0-9-]*)".to_string(),
            ),
        );
    }
    for analyzer in &declaration.analyzers {
        if analyzer.language.is_empty() {
            diagnostics.push(e030(format!(
                "extension '{name}': an analyzer has an empty language"
            )));
        }
        if analyzer.file_extensions.is_empty() {
            diagnostics.push(e030(format!(
                "extension '{name}': the analyzer for '{}' has no file extensions",
                analyzer.language
            )));
        }
        for export in [
            &analyzer.scan_export,
            &analyzer.classify_export,
            &analyzer.map_export,
        ] {
            if export.is_empty() {
                diagnostics.push(e030(format!(
                    "extension '{name}': the analyzer for '{}' has an empty export name",
                    analyzer.language
                )));
            }
        }
    }
    diagnostics
}

/// Why the host can't derive `field`'s edges from `source`, or None when
/// it can: a known source on a reference field with a target kind.
fn derived_from_problem(field: &FieldDescriptor, source: &str) -> Option<&'static str> {
    if !matches!(source, "type_expressions" | "method_signatures") {
        return Some("expected 'type_expressions' or 'method_signatures'");
    }
    let reference = FieldType::parse(&field.field_type).is_some_and(FieldType::is_reference);
    if !reference || field.target_kind.is_none() {
        return Some("only a reference field with a target_kind derives edges");
    }
    None
}

/// W021: `declaration`'s internal consistency, against the other `loaded`
/// declarations (`declaration` itself may be among them).
///
/// A kind it references resolves when it or one of its loaded peer
/// dependencies declares it. A kind only a non-peer extension declares is
/// W021: the kind exists, but the dependency is undeclared. While a named
/// peer is not loaded its kinds are unknown, so any kind is let through.
/// Every edge label a field maps to must be one of its own edges, and a
/// `derived_from` must derive something. A validation rule's target kind
/// and edge type resolve against the extension's own, then its loaded
/// peers', then its `target_extension`'s: that extension not loaded makes
/// the rule inert (no W021); loaded without the kind or edge type, W021.
/// With no `target_extension` a rule resolves as a field does (anything
/// goes while a named peer is not loaded), and a kind only a non-peer
/// declares is W021 suggesting `target_extension`.
pub(crate) fn consistency(
    declaration: &ExtensionDeclaration,
    loaded: &[ExtensionDeclaration],
) -> Vec<Diagnostic> {
    let name = declaration.name();
    let mut diagnostics = Vec::new();

    let own_kinds: HashSet<&str> = declaration.entities.iter().map(keyword).collect();
    let peer_deps: HashSet<&str> = declaration
        .peers()
        .iter()
        .map(|p| p.name.as_str())
        .collect();
    let peers_known = peer_deps
        .iter()
        .all(|peer| loaded.iter().any(|d| d.name() == *peer));
    let peer_kinds: HashSet<&str> = loaded
        .iter()
        .filter(|d| peer_deps.contains(d.name()))
        .flat_map(|d| d.entities.iter().map(keyword))
        .collect();
    let own_edge_labels: HashSet<&str> =
        declaration.edges.iter().map(|e| e.label.as_str()).collect();
    let peer_edge_labels: HashSet<&str> = loaded
        .iter()
        .filter(|d| peer_deps.contains(d.name()))
        .flat_map(|d| d.edges.iter().map(|e| e.label.as_str()))
        .collect();

    // Why `kind` does not resolve, or None when it does.
    let unresolved = |kind: &str| -> Option<String> {
        if own_kinds.contains(kind) || peer_kinds.contains(kind) || !peers_known {
            return None;
        }
        let owner = loaded
            .iter()
            .filter(|d| d.name() != name)
            .find(|d| d.entities.iter().any(|k| keyword(k) == kind));
        Some(match owner {
            Some(owner) => format!(
                "declared by '{}', which is not a peer dependency",
                owner.name()
            ),
            None => "not declared by this extension".to_string(),
        })
    };
    let warn = |message: String| Diagnostic::new(codes::W021, message);

    for kind in &declaration.entities {
        let kind_keyword = keyword(kind);
        for field in &kind.fields {
            if let Some(target) = &field.target_kind
                && let Some(why) = unresolved(target)
            {
                diagnostics.push(warn(format!(
                    "extension '{}': field '{}' on kind '{}' references target_kind '{}' {}",
                    name, field.name, kind_keyword, target, why
                )));
            }
            if let Some(edge) = &field.edge
                && !own_edge_labels.contains(edge.as_str())
            {
                diagnostics.push(warn(format!(
                    "extension '{}': field '{}' on kind '{}' references edge label '{}' not declared among its edges",
                    name, field.name, kind_keyword, edge
                )));
            }
            if let Some(source) = &field.derived_from
                && let Some(why) = derived_from_problem(field, source)
            {
                diagnostics.push(warn(format!(
                    "extension '{}': field '{}' on kind '{}' declares derived_from '{}', which derives nothing: {}",
                    name, field.name, kind_keyword, source, why
                )));
            }
        }
    }

    // The fields every kind shares and the enhancements' fields write edge
    // types too (ADR 0060): their labels are declared like a kind's own.
    for field in &declaration.shared_fields {
        if let Some(edge) = field
            .edge
            .as_deref()
            .filter(|edge| !own_edge_labels.contains(edge))
        {
            diagnostics.push(warn(format!(
                "extension '{}': shared field '{}' references edge label '{}' not declared among its edges",
                name, field.name, edge
            )));
        }
    }
    for enhancement in &declaration.enhancements {
        for field in &enhancement.fields {
            if let Some(edge) = field
                .edge
                .as_deref()
                .filter(|edge| !own_edge_labels.contains(edge))
            {
                diagnostics.push(warn(format!(
                    "extension '{}': enhancement field '{}' on kind '{}' references edge label '{}' not declared among its edges",
                    name, field.name, enhancement.target_kind, edge
                )));
            }
        }
    }

    for edge in &declaration.edges {
        for (role, kind) in [
            ("source_kind", &edge.source_kind),
            ("target_kind", &edge.target_kind),
        ] {
            if let Some(kind) = kind
                && let Some(why) = unresolved(kind)
            {
                diagnostics.push(warn(format!(
                    "extension '{}': edge type '{}' references {} '{}' {}",
                    name, edge.label, role, kind, why
                )));
            }
        }
    }

    for rule in &declaration.validation_rules {
        // The extension the rule says its kind or edge type belongs to,
        // when it is loaded.
        let target_extension = rule.target_extension.as_deref();
        let loaded_target = target_extension.and_then(|n| loaded.iter().find(|d| d.name() == n));
        if let Some(target) = &rule.target_kind {
            let problem = match (target_extension, loaded_target) {
                _ if own_kinds.contains(target.as_str())
                    || peer_kinds.contains(target.as_str()) =>
                {
                    None
                }
                // Not loaded: the rule is inert, and nothing is wrong.
                (Some(_), None) => None,
                (Some(extension), Some(declared)) => {
                    (!declared.entities.iter().any(|k| keyword(k) == target))
                        .then(|| format!("not declared by '{extension}', its target_extension"))
                }
                (None, _) => unresolved(target).map(|why| {
                    if why.starts_with("declared by") {
                        format!("{why}; name it as the rule's target_extension")
                    } else {
                        why
                    }
                }),
            };
            if let Some(why) = problem {
                diagnostics.push(warn(format!(
                    "extension '{}': rule '{}' references target_kind '{}' {}",
                    name, rule.code, target, why
                )));
            }
        }
        if let Some(edge_type) = &rule.edge_type
            && !own_edge_labels.contains(edge_type.as_str())
            && !peer_edge_labels.contains(edge_type.as_str())
        {
            let why = match (target_extension, loaded_target) {
                (Some(_), None) => None,
                (Some(extension), Some(declared)) => {
                    (!declared.edges.iter().any(|e| e.label == *edge_type))
                        .then(|| format!("not declared by '{extension}', its target_extension"))
                }
                (None, _) => peers_known
                    .then(|| "not declared among its edges or its peers' edges".to_string()),
            };
            if let Some(why) = why {
                diagnostics.push(warn(format!(
                    "extension '{}': rule '{}' references edge type '{}' {}",
                    name, rule.code, edge_type, why
                )));
            }
        }
    }

    diagnostics
}

/// `passes` in the order their `after`/`before` constraints give, ties in
/// declaration order (stable Kahn). Constraints naming an unknown pass (a
/// host phase like `resolve`, another extension's pass) or the pass itself
/// are ignored. Constraints that form a cycle keep declaration order and
/// cost `extension` a W145.
pub(crate) fn order_passes(
    extension: &str,
    passes: &[CompilerPassDescriptor],
) -> (Vec<CompilerPassDescriptor>, Option<Diagnostic>) {
    let index: HashMap<&str, usize> = passes
        .iter()
        .enumerate()
        .map(|(i, p)| (p.name.as_str(), i))
        .collect();
    let mut successors: Vec<Vec<usize>> = vec![Vec::new(); passes.len()];
    let mut indegree = vec![0usize; passes.len()];

    for (i, pass) in passes.iter().enumerate() {
        // (dependency name, dependency_runs_first): `after: X` means X runs
        // first; `before: X` means this pass runs first.
        let mut deps: Vec<(&str, bool)> = Vec::new();
        if let Some(after) = &pass.after {
            deps.push((after, true));
        }
        if let Some(before) = &pass.before {
            deps.push((before, false));
        }
        for (dep, dep_first) in deps {
            let Some(&dep_idx) = index.get(dep) else {
                continue; // unknown name: host phase or cross-extension
            };
            if dep == pass.name.as_str() {
                continue; // self-referential constraint: ignore
            }
            let (from, to) = if dep_first {
                (dep_idx, i)
            } else {
                (i, dep_idx)
            };
            if successors[from].contains(&to) {
                continue;
            }
            successors[from].push(to);
            indegree[to] += 1;
        }
    }

    let mut ready: VecDeque<usize> = (0..passes.len()).filter(|&i| indegree[i] == 0).collect();
    let mut order = Vec::with_capacity(passes.len());
    while let Some(i) = ready.pop_front() {
        order.push(i);
        for &to in &successors[i] {
            indegree[to] -= 1;
            if indegree[to] == 0 {
                ready.push_back(to);
            }
        }
    }
    if order.len() == passes.len() {
        return (order.into_iter().map(|i| passes[i].clone()).collect(), None);
    }
    let cyclic: Vec<&str> = (0..passes.len())
        .filter(|i| !order.contains(i))
        .map(|i| passes[i].name.as_str())
        .collect();
    let warning = Diagnostic::new(
        codes::W145,
        format!(
            "extension '{extension}': the order constraints of passes {} form a cycle; its passes run in declaration order",
            cyclic
                .iter()
                .map(|n| format!("'{n}'"))
                .collect::<Vec<_>>()
                .join(", ")
        ),
    )
    .with_suggestion("remove the `after`/`before` constraint that closes the cycle".to_string());
    (passes.to_vec(), Some(warning))
}
