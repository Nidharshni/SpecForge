use crate::delta::{self, GraphDelta, NodeChange};
use crate::{Graph, Node};
use specforge_common::{Diagnostic, SourceSpan, Sym, codes, structural};
use specforge_parser::{Entity, FieldValue, REF_SCHEME_FIELD, SpecFile};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::sync::Arc;

/// What the graph build reads of the loaded extensions. Only the registry
/// build derives it (`specforge_registry::RegistryBuild::graph_inputs`),
/// from the declarations and the field registry; no caller assembles one: a
/// graph read with a Registry build is made by `RegistryBuild::graph_of`.
/// The default is a project with no extension loaded.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GraphInputs {
    /// For each target kind no loaded extension declares, the extension that
    /// owns it, where the registry build knows one (`kind_owner` of its
    /// candidate vocabulary, ADR 0117): what I004 and E022 tell the user to
    /// enable. Plain data: the graph names no registry type.
    pub target_owners: BTreeMap<String, String>,
    /// Every registered field's form, and the kinds that take any field:
    /// the graph build stores each written value as its form coerces it,
    /// before references resolve, so a single reference on a reference_list
    /// field links like `field [id]`; a declared `reference` field's edges
    /// link after cycle detection (`journey.persona -> persona`).
    pub field_forms: crate::FieldForms,
    /// Registered fields whose edges come from the type names an entity
    /// writes in its field types or method signatures (`derived_from`),
    /// sorted by kind and field.
    pub derived_references: Vec<crate::DerivedReference>,
}

/// What one graph build is configured with: the provider schemes the
/// environment registered, and the inputs the registry build derived.
#[derive(Debug, Clone, Default)]
pub struct GraphConfig {
    /// Provider schemes that are installed (e.g., "gh", "jira").
    /// Refs with schemes not in this set emit I005.
    pub known_provider_schemes: HashSet<String>,
    /// What the graph build reads of the loaded extensions.
    pub inputs: GraphInputs,
}

/// One file's change, as a [`GraphBuild`] applies it.
#[derive(Debug, Clone)]
pub enum FileChange {
    /// The file's current parse, keyed by its `SpecFile::path`: added, or
    /// replacing what the build held for that path.
    Parsed(SpecFile),
    /// The file is gone (its path, as its parse was keyed).
    Removed(String),
}

/// What one [`GraphBuild::apply`] did.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Applied {
    /// The files added, replaced or removed, sorted. Removing a file the
    /// build did not hold changes nothing and is not listed.
    pub files: Vec<String>,
    /// What changed in the graph: only the IDs the changed files held or
    /// declare now can be added, removed or replaced; any other node can
    /// only change through its outgoing edges.
    pub delta: GraphDelta,
    /// The nodes that only changed place: in both graphs, same content or
    /// not, declared at another file or line (each where it is now, sorted
    /// by ID). The delta ignores positions; this does not.
    pub moved: Vec<NodeChange>,
    /// The comparison with a cold build of the same files, when the build
    /// verifies ([`GraphBuild::set_verify`]): `Err` names the first node,
    /// edge or diagnostic that differs, or the delta check that failed.
    pub verification: Option<Result<(), String>>,
}

/// The graph of a set of parsed `.spec` files and what building it
/// reported (E001, E002, W060, W143, I005,
/// then E003/I004 and W061 from linking), kept current one whole file at a
/// time.
///
/// The rule, written once: files are taken in path order; each entity ID is
/// the node of its first declaration that is not a `define` block (W143). A
/// later declaration of the same kind is E002, naming the first of that kind;
/// one of another kind is W060. References are then linked over the whole
/// graph (field coercion first, single-reference fields, derived references).
///
/// Invariant: after any sequence of [`Self::apply`], [`Self::graph`] and
/// [`Self::diagnostics`] are exactly those of [`Self::of`] over the files the
/// build holds. Verification checks that invariant on every apply.
#[derive(Clone)]
pub struct GraphBuild {
    config: GraphConfig,
    /// The parse of every file, by `SpecFile::path`. A copy of the build shares them.
    files: BTreeMap<String, Arc<SpecFile>>,
    graph: Graph,
    /// The graph-build diagnostics in build order.
    diagnostics: Vec<Diagnostic>,
    /// The same, by file (`""`: no span), each in build order.
    by_file: HashMap<String, Vec<Diagnostic>>,
    verify: bool,
}

impl GraphBuild {
    /// An empty build: no file, no node, no diagnostic.
    pub fn new(config: GraphConfig) -> Self {
        GraphBuild {
            graph: Graph::new(),
            config,
            files: BTreeMap::new(),
            diagnostics: Vec::new(),
            by_file: HashMap::new(),
            verify: false,
        }
    }

    /// This empty build, holding `graph` made elsewhere (a test assembled it
    /// node by node) under its own config, so a snapshot reads the graph with
    /// the Registry build that made the build (`Environment::graph_of([])`).
    /// It holds no file and reports nothing. Its one invariant (the cold
    /// build of its files) does not hold, so only a compiled project that is
    /// never updated holds one (`specforge_project::testing::Assembled`).
    #[cfg(feature = "testing")]
    pub fn holding(self, graph: Graph) -> Self {
        debug_assert!(
            self.files.is_empty(),
            "only an empty build holds a graph made elsewhere"
        );
        GraphBuild { graph, ..self }
    }

    /// The cold build: every one of `files` applied at once. Files are keyed
    /// by `SpecFile::path`; a later file with the same path replaces an
    /// earlier one. No delta is computed.
    pub fn of(files: impl IntoIterator<Item = SpecFile>, config: GraphConfig) -> Self {
        let mut build = GraphBuild::new(config);
        build.files = files
            .into_iter()
            .map(|file| (file.path.to_string(), Arc::new(file)))
            .collect();
        build.rebuild(Ids::All);
        build
    }

    /// Add, replace or remove whole files: strip the changed files' nodes,
    /// place the first declaration of every ID they held or declare now (an
    /// unchanged file's duplicate may win or come back), re-link the whole
    /// graph and recompute the graph-build diagnostics over every file.
    pub fn apply(&mut self, changes: impl IntoIterator<Item = FileChange>) -> Applied {
        let previous = self.verify.then(|| self.graph.clone());
        let mut files: BTreeSet<String> = BTreeSet::new();
        let mut ids: BTreeSet<Sym> = BTreeSet::new();
        for change in changes {
            let path = match &change {
                FileChange::Parsed(spec) => spec.path.to_string(),
                FileChange::Removed(path) => path.clone(),
            };
            // Every ID the file held in the graph.
            ids.extend(self.graph.nodes_in_file(&path).iter().map(|n| n.id.raw));
            match change {
                FileChange::Parsed(spec) => {
                    ids.extend(spec.entities.iter().map(|e| e.id.raw));
                    self.files.insert(path.clone(), Arc::new(spec));
                }
                FileChange::Removed(_) if self.files.remove(&path).is_none() => continue,
                FileChange::Removed(_) => {}
            }
            files.insert(path);
        }

        // What the graph had under those IDs, and all its edges, for the delta.
        let old_nodes: BTreeMap<Sym, Node> = ids
            .iter()
            .filter_map(|id| Some((*id, self.graph.node(id.as_str())?.clone())))
            .collect();
        let old_edges = delta::edge_keys(&self.graph);

        for path in &files {
            self.graph.remove_entities_of_file(Sym::new(path));
        }
        self.rebuild(Ids::Only(&ids));

        let old_nodes: BTreeMap<Sym, &Node> = old_nodes.iter().map(|(id, n)| (*id, n)).collect();
        let delta = delta::diff(&ids, &old_nodes, &old_edges, &self.graph);
        let moved = delta::moved(&ids, &old_nodes, &self.graph);
        Applied {
            files: files.into_iter().collect(),
            verification: previous.map(|previous| self.verify(&previous, &delta)),
            delta,
            moved,
        }
    }

    /// Compare every apply with a cold build of the same files (costly: a
    /// clone of the graph and a full build per apply).
    pub fn set_verify(&mut self, enabled: bool) {
        self.verify = enabled;
    }

    /// Whether every apply is compared with a cold build.
    pub fn verifies(&self) -> bool {
        self.verify
    }

    pub fn graph(&self) -> &Graph {
        &self.graph
    }

    /// The graph-build diagnostics in build order: the order [`Self::of`]
    /// emits them.
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    /// The graph-build diagnostics whose span is in `path`, in build order.
    pub fn file_diagnostics(&self, path: &str) -> &[Diagnostic] {
        self.by_file.get(path).map(Vec::as_slice).unwrap_or(&[])
    }

    /// The graph inputs it is built with: the Registry build's that made it
    /// (`RegistryBuild::graph_of`).
    pub fn inputs(&self) -> &GraphInputs {
        &self.config.inputs
    }

    /// Every file the build holds, by path, in path order.
    pub fn files(&self) -> impl ExactSizeIterator<Item = (&str, &SpecFile)> {
        self.files
            .iter()
            .map(|(path, file)| (path.as_str(), file.as_ref()))
    }

    /// The graph and diagnostics, for a one-shot compile.
    pub fn into_parts(self) -> (Graph, Vec<Diagnostic>) {
        (self.graph, self.diagnostics)
    }

    /// Declare, place, link and report over the files the build holds:
    /// the one sequence [`Self::of`] and [`Self::apply`] share. `ids` says
    /// which declarations are placed; the graph holds the others already.
    fn rebuild(&mut self, ids: Ids<'_>) {
        let declared = declarations(self.files.values().map(Arc::as_ref), &self.config);
        place(&mut self.graph, &declared.winners, ids);
        let mut diagnostics = declared.diagnostics;
        diagnostics.extend(link_and_diagnose(&mut self.graph, &self.config));
        self.by_file = partition_by_file(&diagnostics);
        self.diagnostics = diagnostics;
    }

    /// The build is the cold build of its files: its nodes, edges and
    /// diagnostics equal [`Self::of`]'s, and `delta` is the one a full
    /// comparison with `previous` gives and applies to it.
    fn verify(&self, previous: &Graph, delta: &GraphDelta) -> Result<(), String> {
        let cold = GraphBuild::of(
            self.files.values().map(|f| SpecFile::clone(f)),
            self.config.clone(),
        );
        compare_graph_contents(&self.graph, &cold.graph)?;
        let length = self.diagnostics.len().max(cold.diagnostics.len());
        for i in 0..length {
            let (ours, theirs) = (self.diagnostics.get(i), cold.diagnostics.get(i));
            if ours != theirs {
                return Err(format!(
                    "graph-build diagnostics differ from a cold build at {i}: incremental {ours:?}, cold {theirs:?}"
                ));
            }
        }
        let full = delta::compute_graph_delta(previous, &self.graph);
        if &full != delta {
            return Err(format!(
                "incremental delta differs from the full comparison: incremental {delta:?}, full {full:?}"
            ));
        }
        delta.applies(previous, &self.graph)
    }
}

/// `GraphBuild::of(spec_files, GraphConfig::default()).into_parts()`.
#[must_use = "diagnostics should be checked for errors"]
pub fn build_graph(spec_files: &[SpecFile]) -> (Graph, Vec<Diagnostic>) {
    build_graph_with_config(spec_files, &GraphConfig::default())
}

/// `GraphBuild::of(spec_files, config.clone()).into_parts()`: files in
/// path order, whatever order the slice gives.
#[must_use = "diagnostics should be checked for errors"]
pub fn build_graph_with_config(
    spec_files: &[SpecFile],
    config: &GraphConfig,
) -> (Graph, Vec<Diagnostic>) {
    GraphBuild::of(spec_files.iter().cloned(), config.clone()).into_parts()
}

/// Which declarations [`place`] puts in the graph.
#[derive(Clone, Copy)]
enum Ids<'a> {
    All,
    Only(&'a BTreeSet<Sym>),
}

/// What the sweep over the files decided.
struct Declarations<'a> {
    /// The node's declaration of each raw ID: the first that is not a
    /// `define` block, and the first of its ID's kind.
    winners: HashMap<Sym, &'a Entity>,
    diagnostics: Vec<Diagnostic>,
}

/// The one sweep over the files, in the iteration order they are given:
/// every file's E001s and the I007 / E019 its
/// format version header reports first, then per file and entity W143 / E002 / W060, then I005. It is also first-writer-wins:
/// `seen` is keyed on (kind, id), so a second declaration of a kind is E002
/// naming the first of that kind; `winners` is keyed on the raw ID, so a
/// declaration of another kind is W060. Placement reads `winners`.
fn declarations<'a>(
    files: impl Iterator<Item = &'a SpecFile> + Clone,
    config: &GraphConfig,
) -> Declarations<'a> {
    let mut diagnostics = Vec::new();

    // Surface parse errors as diagnostics so CLI/MCP consumers see them.
    for spec_file in files.clone() {
        diagnostics.extend(spec_file.errors.iter().map(Diagnostic::from));
        // What the format version header reports (I007, E019), where the
        // file declares it.
        diagnostics.extend(spec_file.format_diagnostics.iter().cloned());
    }

    // Where each (kind, ID) was first declared, so a duplicate names both sites.
    let mut seen: HashMap<(Sym, Sym), SourceSpan> = HashMap::new();
    let mut winners: HashMap<Sym, &'a Entity> = HashMap::new();
    for spec_file in files.clone() {
        for entity in &spec_file.entities {
            if is_define_block(entity) {
                diagnostics.push(define_block_warning(entity));
                continue;
            }
            let key = (entity.kind.raw, entity.id.raw);
            if let Some(first) = seen.get(&key) {
                diagnostics.push(
                    Diagnostic::new(
                        codes::E002,
                        format!(
                            "duplicate entity ID '{}' (first declared at {}:{}:{})",
                            entity.id.raw, first.file, first.start_line, first.start_col
                        ),
                    )
                    .with_span(entity.span.clone())
                    .with_suggestion("rename one of the entities to avoid the collision"),
                );
                continue;
            }
            seen.insert(key, entity.span.clone());

            // The same ID used by several kinds is W060; the first
            // declaration is retained.
            if let Some(first) = winners.get(&entity.id.raw) {
                diagnostics.push(
                    Diagnostic::new(
                        codes::W060,
                        format!(
                            "entity ID '{}' is used by kind '{}' and kind '{}'; first declaration (kind '{}') is retained",
                            entity.id.raw, first.kind.raw, entity.kind.raw, first.kind.raw
                        ),
                    )
                    .with_span(entity.span.clone())
                    .with_suggestion("use distinct IDs for entities of different kinds"),
                );
                continue;
            }
            winners.insert(entity.id.raw, entity);
        }
    }

    // Check ref nodes for unknown provider schemes (I005)
    if !config.known_provider_schemes.is_empty() {
        for spec_file in files {
            for entity in &spec_file.entities {
                if entity.kind.raw == structural::REF
                    && let Some(FieldValue::String(scheme)) = entity.fields.get(REF_SCHEME_FIELD)
                    && !config.known_provider_schemes.contains(scheme)
                {
                    diagnostics.push(
                        Diagnostic::new(
                            codes::I005,
                            format!(
                                "unrecognized ref scheme '{}' in '{}' --- no provider installed for this scheme",
                                scheme, entity.id.raw
                            ),
                        )
                        .with_span(entity.span.clone()),
                    );
                }
            }
        }
    }

    Declarations {
        winners,
        diagnostics,
    }
}

/// Put the winning declaration of each ID in scope into the graph.
fn place(graph: &mut Graph, winners: &HashMap<Sym, &Entity>, ids: Ids<'_>) {
    for (id, entity) in winners {
        if match ids {
            Ids::All => true,
            Ids::Only(ids) => ids.contains(id),
        } {
            graph.add_node(node_from_entity(entity));
        }
    }
}

/// The graph node for one parsed entity.
fn node_from_entity(entity: &Entity) -> Node {
    Node {
        id: entity.id,
        kind: entity.kind,
        title: entity.title.clone(),
        fields: entity.fields.clone(),
        source_span: entity.span.clone(),
        methods: entity.methods.clone(),
        obligations: entity.obligations.clone(),
    }
}

/// Whether a parsed entity is a `define` block. Define blocks are not
/// supported (ADR 0005): every entity kind comes from an extension. The
/// grammar still parses them so they can be reported (W143); they never
/// become graph nodes.
fn is_define_block(entity: &Entity) -> bool {
    entity.kind.raw == structural::DEFINE
}

/// W143: a define block, which declares nothing.
fn define_block_warning(entity: &Entity) -> Diagnostic {
    Diagnostic::new(
        codes::W143,
        format!(
            "define blocks are not supported: '{}' is not registered as an entity kind",
            entity.id.raw
        ),
    )
    .with_span(entity.span.clone())
    .with_suggestion(
        "declare custom entity kinds in an extension (`specforge new --extension`), then enable it",
    )
}

/// Link reference edges, resolve E003s (single-reference aware) and emit
/// W061 cycle warnings, over the whole graph.
fn link_and_diagnose(graph: &mut Graph, config: &GraphConfig) -> Vec<Diagnostic> {
    // Values take their declared types before anything reads them.
    graph.coerce_field_values(&config.inputs.field_forms);

    // Link references -> edges (E003, I004) and warn on reference cycles
    // (W061), which leave a declared single reference out.
    let diagnostics = graph.link(&config.inputs.field_forms, &config.inputs.target_owners);

    // Derived reference fields, after cycle detection: a recursive type is
    // not a reference cycle.
    graph.link_derived_references(
        &config.inputs.derived_references,
        &config.inputs.field_forms,
    );

    // The linker's postcondition (E060, retired): every reference to an
    // existing entity has its edge.
    #[cfg(debug_assertions)]
    graph.assert_linked(&config.inputs.field_forms);

    diagnostics
}

/// `diagnostics` by the file of their span (`""`: no span), each file's in
/// build order.
fn partition_by_file(diagnostics: &[Diagnostic]) -> HashMap<String, Vec<Diagnostic>> {
    let mut map: HashMap<String, Vec<Diagnostic>> = HashMap::new();
    for diag in diagnostics {
        let file = diag
            .span
            .as_ref()
            .map(|s| s.file.to_string())
            .unwrap_or_default();
        map.entry(file).or_default().push(diag.clone());
    }
    map
}

/// The incremental graph is the cold one: every node's kind, file, position, title, fields and methods, and every
/// edge. It is the graph delta's comparison plus positions (each node's file, line and column, and the spans in its
/// expressions and methods): the delta ignores them by contract, and an incremental build that misplaces a node is
/// wrong even when nothing changed (ADR 0032, round 7 amendment).
fn compare_graph_contents(incremental: &Graph, cold: &Graph) -> Result<(), String> {
    let node_sigs = |g: &Graph| -> BTreeMap<String, String> {
        g.nodes()
            .iter()
            .map(|n| {
                let sig = format!(
                    "{} in {}:{}:{} title={:?} fields={:?} methods={:?} obligations={:?}",
                    n.kind.raw,
                    n.source_span.file,
                    n.source_span.start_line,
                    n.source_span.start_col,
                    n.title,
                    n.fields,
                    n.methods,
                    n.obligations,
                );
                (n.id.raw.to_string(), sig)
            })
            .collect()
    };
    let (inc, cold_nodes) = (node_sigs(incremental), node_sigs(cold));
    if let Some(id) = inc
        .keys()
        .chain(cold_nodes.keys())
        .find(|id| inc.get(*id) != cold_nodes.get(*id))
    {
        return Err(format!(
            "incremental/cold mismatch on node '{}': incremental {:?}, cold {:?}",
            id,
            inc.get(id),
            cold_nodes.get(id)
        ));
    }
    let written = |g: &Graph| -> BTreeMap<delta::EdgeKey, Vec<SourceSpan>> {
        g.edges()
            .iter()
            .map(|e| ((e.source, e.target, e.label), g.written(e).to_vec()))
            .collect()
    };
    let (inc_at, cold_at) = (written(incremental), written(cold));
    let (inc, cold) = (delta::edge_keys(incremental), delta::edge_keys(cold));
    if let Some((s, t, l)) = inc.symmetric_difference(&cold).next() {
        let side = if inc.contains(&(*s, *t, *l)) {
            "only in incremental"
        } else {
            "only in cold"
        };
        return Err(format!(
            "incremental/cold mismatch on edge {s} -{l}-> {t} ({side})"
        ));
    }
    // Where each edge is written is part of the graph too (ADR 0127).
    if let Some((s, t, l)) = inc_at
        .keys()
        .find(|key| inc_at.get(*key) != cold_at.get(*key))
    {
        return Err(format!(
            "incremental/cold mismatch on where edge {s} -{l}-> {t} is written: incremental {:?}, cold {:?}",
            inc_at.get(&(*s, *t, *l)),
            cold_at.get(&(*s, *t, *l))
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use specforge_parser::parse;
    use specforge_test_macros::test as specforge_test;

    fn build(files: &[(&str, &str)]) -> GraphBuild {
        GraphBuild::of(
            files.iter().map(|(path, text)| parse(text, path)),
            GraphConfig::default(),
        )
    }

    fn change(path: &str, text: &str) -> FileChange {
        FileChange::Parsed(parse(text, path))
    }

    /// A graph that drifted from its sources outside the changed file is
    /// caught: the comparison names the node that differs.
    #[specforge_test(
        behavior = "rebuild_affected_subgraph",
        verify = "debug --verify-incremental performs cold rebuild comparison"
    )]
    fn a_divergence_from_the_cold_build_is_reported() {
        let mut build = build(&[
            ("a.spec", "behavior foo \"Foo\" { contract \"x\" }"),
            ("b.spec", "behavior other \"Other\" { contract \"o\" }"),
        ]);
        build.set_verify(true);
        let mut stale = build.graph.node("other").cloned().unwrap();
        stale.title = Some("Stale Title".to_string());
        build.graph.add_node(stale);

        let applied = build.apply([change(
            "a.spec",
            "behavior bar \"Bar\" { contract \"new\" }",
        )]);

        assert_eq!(applied.files, ["a.spec"], "b.spec not re-applied");
        let err = applied.verification.unwrap().unwrap_err();
        assert!(
            err.contains("'other'") && err.contains("Stale Title"),
            "{err}"
        );
    }

    /// A node out of place is no change to the delta, but the verification places every node.
    #[specforge_test(
        behavior = "rebuild_affected_subgraph",
        verify = "debug --verify-incremental performs cold rebuild comparison"
    )]
    fn a_node_out_of_place_is_a_divergence_the_delta_ignores() {
        let mut build = build(&[
            ("a.spec", "behavior foo \"Foo\" { contract \"x\" }"),
            ("b.spec", "behavior other \"Other\" { contract \"o\" }"),
        ]);
        build.set_verify(true);
        let placed = build.graph.clone();
        let mut moved = build.graph.node("other").cloned().unwrap();
        moved.source_span.start_line += 3;
        build.graph.add_node(moved);
        assert!(
            crate::compute_graph_delta(&placed, &build.graph).is_empty(),
            "a move is not a change"
        );

        let applied = build.apply([change(
            "a.spec",
            "behavior bar \"Bar\" { contract \"new\" }",
        )]);

        let err = applied.verification.unwrap().unwrap_err();
        assert!(err.contains("'other'") && err.contains("b.spec:"), "{err}");
    }

    /// The diagnostics are compared too, in order.
    #[specforge_test(
        behavior = "rebuild_affected_subgraph",
        verify = "a rebuild whose diagnostics differ from a cold build is reported"
    )]
    fn a_diagnostic_divergence_from_the_cold_build_is_reported() {
        let mut build = build(&[
            ("a.spec", "behavior foo \"Foo\" { contract \"x\" }"),
            ("b.spec", "behavior other \"Other\" { contract \"o\" }"),
        ]);
        assert!(build.diagnostics().is_empty());
        // The build reports something a cold build of its files does not.
        build
            .diagnostics
            .push(Diagnostic::new(codes::W143, "stale"));

        let err = build
            .verify(&build.graph.clone(), &GraphDelta::default())
            .unwrap_err();
        assert!(
            err.contains("graph-build diagnostics differ from a cold build at 0"),
            "{err}"
        );
    }
}
