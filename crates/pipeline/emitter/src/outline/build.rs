use std::collections::HashMap;

use specforge_protocol_types::{EntityKindDescriptor, ExtensionDeclaration, ValidationSeverity};

use super::*;

/// The keyword a kind is written with: its declared keyword, else its name.
fn keyword(kind: &EntityKindDescriptor) -> &str {
    kind.keyword.as_deref().unwrap_or(&kind.name)
}

fn severity(severity: &ValidationSeverity) -> &'static str {
    match severity {
        ValidationSeverity::Error => "error",
        ValidationSeverity::Warning => "warning",
        ValidationSeverity::Info => "info",
    }
}

impl OutlineIntermediate {
    pub(super) fn of(declarations: &[ExtensionDeclaration]) -> Self {
        // Build kind→extension ownership index
        let mut kind_to_extension: HashMap<String, String> = HashMap::new();
        for m in declarations {
            for ek in &m.entities {
                kind_to_extension.insert(keyword(ek).to_string(), m.name().to_string());
            }
        }

        let mut extensions = Vec::new();
        let mut dependencies = Vec::new();
        let mut enhancements = Vec::new();
        let mut cross_edges = Vec::new();

        for m in declarations {
            // Map entity kinds
            let entity_kinds: Vec<OutlineEntityKind> = m
                .entities
                .iter()
                .map(|ek| {
                    let fields: Vec<OutlineField> = ek
                        .fields
                        .iter()
                        .map(|f| OutlineField {
                            name: f.name.clone(),
                            field_type: f.field_type.clone(),
                            required: f.required,
                            source_extension: m.name().to_string(),
                            edge: f.edge.clone(),
                            target_kind: f.target_kind.clone(),
                        })
                        .collect();

                    // Find enhancements targeting this kind from other extensions
                    let enhanced_by: Vec<OutlineFieldAttribution> = declarations
                        .iter()
                        .filter(|other| other.name() != m.name())
                        .flat_map(|other| {
                            other
                                .enhancements
                                .iter()
                                .filter(|enh| enh.target_kind == keyword(ek))
                                .map(move |enh| OutlineFieldAttribution {
                                    source_extension: other.name().to_string(),
                                    field_count: enh.fields.len(),
                                    field_names: enh
                                        .fields
                                        .iter()
                                        .map(|f| f.name.clone())
                                        .collect(),
                                })
                        })
                        .collect();

                    OutlineEntityKind {
                        name: ek.name.clone(),
                        keyword: keyword(ek).to_string(),
                        testable: ek.testable,
                        field_count: fields.len(),
                        fields,
                        enhanced_by,
                    }
                })
                .collect();

            // Map edge types
            let edge_types: Vec<OutlineEdgeType> = m
                .edges
                .iter()
                .map(|e| OutlineEdgeType {
                    label: e.label.clone(),
                    description: e.description.clone(),
                    source_kind: e.source_kind.clone(),
                    target_kind: e.target_kind.clone(),
                })
                .collect();

            // Map validation rules
            let validation_rules: Vec<OutlineValidationRule> = m
                .validation_rules
                .iter()
                .map(|r| OutlineValidationRule {
                    code: r.code.clone(),
                    severity: severity(&r.severity).to_string(),
                    check: r.check.clone(),
                    target_kind: r.target_kind.clone(),
                })
                .collect();

            // Map contributes
            // The categories it declares, in the host's read order, then providers.
            let contributes: Vec<String> = m
                .declared_categories()
                .map(|category| category.name().to_string())
                .chain(m.handshake.providers.then(|| "providers".to_string()))
                .collect();

            // Map surface counts
            let surface_counts = OutlineSurfaceCounts {
                cli_commands: m.surfaces.commands.len(),
                mcp_tools: m.surfaces.mcp_tools.len(),
                mcp_resources: m.surfaces.mcp_resources.len(),
            };

            // Map shared fields (fields declared once, applied to all entity kinds)
            let shared_fields: Vec<OutlineSharedField> = m
                .shared_fields
                .iter()
                .map(|f| OutlineSharedField {
                    name: f.name.clone(),
                    field_type: f.field_type.clone(),
                    required: f.required,
                })
                .collect();

            extensions.push(OutlineExtension {
                name: m.name().to_string(),
                version: m.version().to_string(),
                entity_kinds,
                edge_types,
                validation_rules,
                contributes,
                verify_kinds: m.verify_kinds().into_iter().map(String::from).collect(),
                surface_counts,
                shared_fields,
                collector_count: m.collectors.len(),
                color: m.handshake.theme_color.clone(),
            });

            // Map peer dependencies (direct)
            for dep in m.peers() {
                dependencies.push(OutlineDependency {
                    from: m.name().to_string(),
                    to: dep.name.clone(),
                    version: dep.version.clone(),
                    optional: dep.optional,
                    kind: DependencyKind::Direct,
                });
            }

            // Map entity enhancements
            for enh in &m.enhancements {
                // The extension that declares the target kind, if a declaration does
                let owner = kind_to_extension.get(&enh.target_kind).cloned();
                enhancements.push(OutlineEnhancement {
                    enhancer: m.name().to_string(),
                    owner,
                    target_kind: enh.target_kind.clone(),
                    field_count: enh.fields.len(),
                    field_names: enh.fields.iter().map(|f| f.name.clone()).collect(),
                });
            }

            // Detect cross-extension edges
            for edge in &m.edges {
                if let (Some(sk), Some(tk)) = (&edge.source_kind, &edge.target_kind)
                    && let Some(te) = kind_to_extension.get(tk.as_str())
                    && te != m.name()
                {
                    cross_edges.push(OutlineCrossEdge {
                        edge_label: edge.label.clone(),
                        owner_extension: m.name().to_string(),
                        source_kind: sk.clone(),
                        target_kind: tk.clone(),
                        target_extension: te.clone(),
                    });
                }
            }
        }

        // Compute transitive closure
        let transitive = compute_transitive_deps(&dependencies, &kind_to_extension, declarations);
        dependencies.extend(transitive);

        OutlineIntermediate {
            extensions,
            dependencies,
            enhancements,
            cross_edges,
        }
    }
}

/// Compute transitive dependencies from the direct dependency graph.
///
/// For each pair A→B (direct) and B→C (direct/transitive), adds A→C.
/// Optional propagation: if any link in the chain is optional, transitive dep is optional.
/// Effective detection: if A references any kind owned by C, the dep is Effective; else Transitive.
fn compute_transitive_deps(
    direct_deps: &[OutlineDependency],
    kind_to_extension: &HashMap<String, String>,
    declarations: &[ExtensionDeclaration],
) -> Vec<OutlineDependency> {
    use std::collections::{BTreeMap, BTreeSet, HashSet};

    // Everything below iterates in name order, so which chain decides a
    // transitive dependency's optionality and version, and the order the
    // dependencies come out in, are the same on every run.

    // Build adjacency: extension → [(target, optional, version)]
    let mut adj: BTreeMap<String, Vec<(String, bool, String)>> = BTreeMap::new();
    for dep in direct_deps {
        adj.entry(dep.from.clone()).or_default().push((
            dep.to.clone(),
            dep.optional,
            dep.version.clone(),
        ));
    }

    // Existing direct pairs (to avoid duplicates)
    let direct_pairs: BTreeSet<(String, String)> = direct_deps
        .iter()
        .map(|d| (d.from.clone(), d.to.clone()))
        .collect();

    // BFS/fixed-point transitive closure
    let mut all_deps: BTreeMap<(String, String), (bool, String)> = BTreeMap::new();
    for dep in direct_deps {
        all_deps.insert(
            (dep.from.clone(), dep.to.clone()),
            (dep.optional, dep.version.clone()),
        );
    }

    let mut changed = true;
    while changed {
        changed = false;
        let snapshot: Vec<_> = all_deps
            .iter()
            .map(|((f, t), (o, v))| (f.clone(), t.clone(), *o, v.clone()))
            .collect();
        for (a, b, a_b_optional, _) in &snapshot {
            if let Some(b_targets) = adj.get(b) {
                for (c, b_c_optional, version) in b_targets {
                    if a == c {
                        continue; // no self-loops
                    }
                    let key = (a.clone(), c.clone());
                    let transitive_optional = *a_b_optional || *b_c_optional;
                    if let std::collections::btree_map::Entry::Vacant(e) = all_deps.entry(key) {
                        e.insert((transitive_optional, version.clone()));
                        changed = true;
                    }
                }
            }
        }
    }

    // Build index: extension name → set of kinds it owns
    let mut ext_to_kinds: HashMap<String, HashSet<String>> = HashMap::new();
    for (kind, ext) in kind_to_extension {
        ext_to_kinds
            .entry(ext.clone())
            .or_default()
            .insert(kind.clone());
    }

    // Build index: extension name → set of kinds it references
    let mut ext_references: HashMap<String, HashSet<String>> = HashMap::new();
    for m in declarations {
        let refs = ext_references.entry(m.name().to_string()).or_default();
        // From edge types
        for edge in &m.edges {
            if let Some(sk) = &edge.source_kind {
                refs.insert(sk.clone());
            }
            if let Some(tk) = &edge.target_kind {
                refs.insert(tk.clone());
            }
        }
        // From entity enhancement targets
        for enh in &m.enhancements {
            refs.insert(enh.target_kind.clone());
            for f in &enh.fields {
                if let Some(tk) = &f.target_kind {
                    refs.insert(tk.clone());
                }
            }
        }
        // From entity kind field targetKinds
        for ek in &m.entities {
            for f in &ek.fields {
                if let Some(tk) = &f.target_kind {
                    refs.insert(tk.clone());
                }
            }
        }
    }

    // Emit transitive deps (skip pairs that are already direct)
    let mut transitive = Vec::new();
    for ((from, to), (optional, version)) in &all_deps {
        if direct_pairs.contains(&(from.clone(), to.clone())) {
            continue;
        }
        // Determine if effective: does `from` reference any kind owned by `to`?
        let is_effective = if let (Some(from_refs), Some(to_kinds)) =
            (ext_references.get(from), ext_to_kinds.get(to))
        {
            from_refs.iter().any(|r| to_kinds.contains(r))
        } else {
            false
        };

        transitive.push(OutlineDependency {
            from: from.clone(),
            to: to.clone(),
            version: version.clone(),
            optional: *optional,
            kind: if is_effective {
                DependencyKind::Effective
            } else {
                DependencyKind::Transitive
            },
        });
    }

    transitive
}
