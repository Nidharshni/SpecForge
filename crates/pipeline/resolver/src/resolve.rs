use specforge_common::{Diagnostic, DiagnosticData, codes, find_close_match};
use specforge_cycles::cycles;
use specforge_parser::{ImportBinding, SpecFile};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::{Component, Path, PathBuf};

/// A `pub use` of one file: what it re-exports and from where.
struct ReexportDeclaration {
    /// The target source's key.
    target_path: String,
    /// `None`: everything the target exports.
    bindings: Option<Vec<ImportBinding>>,
    span: specforge_common::SourceSpan,
}

/// What one `use` path names among a compile's sources.
enum Named<'s> {
    Source(&'s str),
    Extension { scope: String, name: String },
    Nothing,
}

/// The import diagnostics of a compile's sources, in this order: per parsed
/// source in key order, each import's E025 (it names no source: no key by
/// that path, or a path that leaves the spec root or is absolute) or I004
/// (`@scope/name`, an extension import); then one W113 per strongly
/// connected set of imports, walking every file from its smallest key, in
/// sorted order; then the W027s of
/// selective re-exports (a file exports what it declares plus what its
/// `pub use` re-exports, transitively; a re-export through an import-cycle
/// participant uses only that file's declared set).
///
/// `parsed` is each parsed source with its key; `unreadable` the keys of the
/// sources the compile holds but could not read (each already E025). A key
/// is a source exactly when one of the two holds it. An import of an
/// unreadable source names it and reports nothing more, and a re-export
/// from one reports no W027: what it exports is unknown.
#[must_use]
pub fn resolve_imports(parsed: &[(&str, &SpecFile)], unreadable: &[&str]) -> Vec<Diagnostic> {
    let mut files: Vec<(&str, &SpecFile)> = parsed.to_vec();
    files.sort_by(|a, b| a.0.cmp(b.0));
    let mut diagnostics = Vec::new();

    let sources: BTreeSet<&str> = files
        .iter()
        .map(|(key, _)| *key)
        .chain(unreadable.iter().copied())
        .collect();

    // Candidates for fuzzy suggestions (source keys without .spec).
    let candidates: Vec<&str> = sources
        .iter()
        .map(|key| key.strip_suffix(".spec").unwrap_or(key))
        .collect();

    // Resolve imports and build the file dependency graph, tracking each
    // file's `pub use` re-exports.
    let mut import_graph: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut reexport_map: BTreeMap<String, Vec<ReexportDeclaration>> = BTreeMap::new();
    for (path, spec_file) in &files {
        let mut deps = Vec::new();
        let mut reexports = Vec::new();
        for import in &spec_file.imports {
            match name(&sources, path, import.path.as_str()) {
                Named::Source(target) => {
                    if import.is_pub {
                        reexports.push(ReexportDeclaration {
                            target_path: target.to_string(),
                            bindings: import.bindings.clone(),
                            span: import.span.clone(),
                        });
                    }
                    deps.push(target.to_string());
                }
                Named::Extension { scope, name } => {
                    diagnostics.push(
                        Diagnostic::new(
                            codes::I004,
                            format!(
                                "extension import @{}/{} — extension not installed",
                                scope, name
                            ),
                        )
                        .with_span(import.span.clone()),
                    );
                }
                Named::Nothing => {
                    let close = find_close_match(import.path.as_str(), candidates.iter().copied());
                    let mut diagnostic = Diagnostic::new(
                        codes::E025,
                        format!(
                            "import target not found among the project's sources: {}",
                            import.path
                        ),
                    )
                    .with_span(import.span.clone())
                    .with_data(DiagnosticData::UnresolvedImport {
                        path: import.path.to_string(),
                        did_you_mean: close.map(str::to_string),
                    });
                    diagnostic.suggestion = Some(match close {
                        Some(m) => format!("did you mean '{}'?", m),
                        None => "a use path names a project source: a .spec file under the spec \
                                 root, not excluded and not under a skipped directory"
                            .to_string(),
                    });
                    diagnostics.push(diagnostic);
                }
            }
        }
        reexport_map.insert(path.to_string(), reexports);
        import_graph.insert(path.to_string(), deps);
    }

    // Import cycles (ADR 0196): one per strongly connected set of files,
    // walked from its smallest path, in sorted order.
    let imports: BTreeMap<String, BTreeSet<String>> = import_graph
        .iter()
        .map(|(file, deps)| (file.clone(), deps.iter().cloned().collect()))
        .collect();
    let cycles = cycles(&imports);
    for cycle in &cycles {
        let walk = &cycle.path[..cycle.path.len() - 1];
        diagnostics.push(
            Diagnostic::new(
                codes::W113,
                format!("circular import detected: {}", walk.join(" -> ")),
            )
            .with_suggestion(
                "break the cycle by removing one of the `use` imports or extracting shared entities into a separate file".to_string(),
            ),
        );
    }

    // File scopes (declared + re-exported entities per file), computed in
    // topological order with the cyclic files last.
    let cyclic: BTreeSet<&str> = cycles
        .iter()
        .flat_map(|cycle| cycle.members.iter().map(String::as_str))
        .collect();
    let mut order = topological_sort(&import_graph, &cyclic);
    order.extend(cyclic.iter().copied());
    let by_path: HashMap<&str, &SpecFile> = files.iter().copied().collect();
    let scope_inputs: Vec<(&str, &SpecFile, &[ReexportDeclaration])> = order
        .iter()
        .filter_map(|path| {
            Some((
                *path,
                *by_path.get(path)?,
                reexport_map.get(*path).map(Vec::as_slice).unwrap_or(&[]),
            ))
        })
        .collect();
    diagnostics.extend(compute_file_scopes(&scope_inputs));
    diagnostics
}

/// The key of the source `use "<import_path>"` in source `importing` names
/// among `sources`, by the cascade [`resolve_imports`] uses: a relative path
/// (`./`, `../`) from `importing`'s directory, any other path from the spec
/// root; `<path>.spec` (`<path>` itself when it ends in `.spec`), else
/// `<path>/index.spec`. Keys compare exactly. `None` for an extension
/// import (`@scope/name`) or a path that names no source.
#[must_use]
pub fn import_target<'s>(
    sources: &BTreeSet<&'s str>,
    importing: &str,
    import_path: &str,
) -> Option<&'s str> {
    match name(sources, importing, import_path) {
        Named::Source(key) => Some(key),
        Named::Extension { .. } | Named::Nothing => None,
    }
}

/// The one cascade (both public functions go through it).
fn name<'s>(sources: &BTreeSet<&'s str>, importing: &str, import_path: &str) -> Named<'s> {
    if let Some(rest) = import_path.strip_prefix('@') {
        return match rest.split_once('/') {
            Some((scope, name)) => Named::Extension {
                scope: scope.into(),
                name: name.into(),
            },
            None => Named::Nothing,
        };
    }
    let from = if import_path.starts_with("./") || import_path.starts_with("../") {
        Path::new(importing).parent().unwrap_or(Path::new(""))
    } else {
        Path::new("")
    };
    let Some(base) = lexical_join(from, Path::new(import_path)) else {
        return Named::Nothing;
    };
    candidates(&base)
        .iter()
        .find_map(|candidate| sources.get(candidate.to_string_lossy().as_ref()).copied())
        .map_or(Named::Nothing, Named::Source)
}

/// `path` under `from` (both relative to the spec root), `.` and `..`
/// resolved lexically; `None` when it leaves the spec root or is absolute.
fn lexical_join(from: &Path, path: &Path) -> Option<PathBuf> {
    let mut joined = PathBuf::new();
    for component in from.components().chain(path.components()) {
        match component {
            Component::Normal(name) => joined.push(name),
            Component::CurDir => {}
            Component::ParentDir => {
                if !joined.pop() {
                    return None;
                }
            }
            Component::RootDir | Component::Prefix(_) => return None,
        }
    }
    Some(joined)
}

/// `<base>.spec` (`<base>` when its name ends in `.spec`), then
/// `<base>/index.spec`; an empty base (the spec root) names only
/// `index.spec`.
fn candidates(base: &Path) -> Vec<PathBuf> {
    let mut out = Vec::with_capacity(2);
    if let Some(name) = base.file_name().map(|n| n.to_string_lossy()) {
        out.push(if name.ends_with(".spec") {
            base.to_path_buf()
        } else {
            base.with_file_name(format!("{name}.spec"))
        });
    }
    out.push(base.join("index.spec"));
    out
}

/// The non-cyclic files, each after the files it imports (Kahn's
/// algorithm, smallest path first among the ready ones).
fn topological_sort<'a>(
    graph: &'a BTreeMap<String, Vec<String>>,
    cyclic: &BTreeSet<&str>,
) -> Vec<&'a str> {
    let mut in_deg: BTreeMap<&str, usize> = BTreeMap::new();
    let mut importers: HashMap<&str, BTreeSet<&str>> = HashMap::new();
    for (node, deps) in graph {
        if cyclic.contains(node.as_str()) {
            continue;
        }
        let deps: BTreeSet<&str> = deps
            .iter()
            .map(String::as_str)
            .filter(|d| !cyclic.contains(d) && graph.contains_key(*d))
            .collect();
        in_deg.insert(node, deps.len());
        for dep in deps {
            importers.entry(dep).or_default().insert(node);
        }
    }

    let mut ready: BTreeSet<&str> = in_deg
        .iter()
        .filter(|(_, deg)| **deg == 0)
        .map(|(node, _)| *node)
        .collect();
    let mut result = Vec::new();
    while let Some(node) = ready.pop_first() {
        result.push(node);
        for importer in importers.get(node).into_iter().flatten() {
            if let Some(deg) = in_deg.get_mut(importer) {
                *deg -= 1;
                if *deg == 0 {
                    ready.insert(importer);
                }
            }
        }
    }
    result
}

/// Compute file scopes: for each file, determine which entity IDs are declared
/// and which are exported (declared + re-exported via `pub use`), and report
/// the selective re-exports of a name the target does not export (W027).
///
/// Files are processed in the order given (topologically sorted for
/// non-cyclic files, cyclic files appended at the end). For cycle participants
/// whose scope hasn't been computed yet, only their `declared` set is used
/// (no transitive resolution).
fn compute_file_scopes(files: &[(&str, &SpecFile, &[ReexportDeclaration])]) -> Vec<Diagnostic> {
    let mut scopes: HashMap<String, HashSet<String>> = HashMap::new();
    let mut diagnostics = Vec::new();

    // First pass: build declared sets for all files
    for (path, spec_file, _) in files {
        let declared: HashSet<String> = spec_file
            .entities
            .iter()
            .map(|e| e.id.raw.to_string())
            .collect();
        scopes.insert(path.to_string(), declared);
    }

    // Second pass: process re-exports (files are in topological order,
    // so targets should already have their scopes computed)
    for (path, _, reexports) in files {
        if reexports.is_empty() {
            continue;
        }

        let mut additional_exports = HashSet::new();

        for reexport in reexports.iter() {
            // A target with no scope is an unreadable source: what it
            // exports is unknown, so nothing is reported against it.
            let Some(target_exported) = scopes.get(&reexport.target_path).cloned() else {
                continue;
            };

            match &reexport.bindings {
                None => {
                    // pub use "target" — re-export all
                    additional_exports.extend(target_exported);
                }
                Some(bindings) => {
                    // pub use { A, B } from "target" — re-export selected
                    for binding in bindings {
                        if target_exported.contains(&binding.name) {
                            // Use alias if present, otherwise use the original name
                            let export_name = binding.alias.as_ref().unwrap_or(&binding.name);
                            additional_exports.insert(export_name.clone());
                        } else {
                            diagnostics.push(
                                Diagnostic::new(
                                    codes::W027,
                                    format!(
                                        "selective re-export '{}' not found in target '{}'",
                                        binding.name, reexport.target_path
                                    ),
                                )
                                .with_span(reexport.span.clone()),
                            );
                        }
                    }
                }
            }
        }

        if let Some(exported) = scopes.get_mut(*path) {
            exported.extend(additional_exports);
        }
    }

    diagnostics
}
