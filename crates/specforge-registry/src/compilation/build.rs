//! One registry build: the loaded declarations in, everything the
//! compiler derives from them out (ADR 0012).
//!
//! [`build_registries`] owns the order: it puts the declarations in load
//! order (ADR 0041), checks them (identity and shape, consistency, peers,
//! pass order), then populates the registries, builds the rule set (the declared rules,
//! checked and resolved, plus the host's E006 rules; ADR 0020) and
//! registers the surfaces. Callers read [`RegistryBuild`].
//! It is pure: no I/O, no runtime.

use specforge_common::Diagnostic;
use specforge_graph::GraphInputs;
use specforge_protocol_types::{CompilerPassDescriptor, ExtensionDeclaration};

use super::declaration::{consistency, order_passes, shape};
use super::graph_inputs;
use super::peers;
use super::populate::populate;
use super::validate::validate_extension_testability;
use crate::rules::{CustomVerdicts, Registries, Rules};
use crate::{
    EdgeRegistry, FieldRegistry, KindRegistry, SurfaceRegistryEntry, refuse_malformed_tool_schemas,
    register_surface_contributions,
};

/// The phase a pass declares to run with every compile instead of under
/// `specforge analyze`: the compiled project runs it after the graph
/// checks, and its diagnostics are the compile's.
pub const CHECK_PHASE: &str = "check";

/// What a diagnostic of the registry build is about (CONTEXT "Setup part").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BuildPart {
    /// One declaration on its own: its identity and shape (E030), its
    /// consistency with the loaded declarations (W021), the order of its
    /// passes (W145).
    Declaration,
    /// A peer requirement among the loaded declarations: E073, E027, a
    /// cycle's E027 (ADR 0041).
    Peers,
    /// What the kind, field and edge registries hold: an unknown field type
    /// (W019), a field declaration that cannot be used as declared (W021), an
    /// enhancement of an unknown kind (I004), a testable kind that cannot
    /// declare obligations (W017).
    Registries,
    /// A declared rule: a code its extension may not use (W150), a property
    /// its check does not read (W147), a rule that cannot work or whose
    /// custom function does not answer its probe (W112).
    Rules,
    /// An extension's surfaces: an MCP tool whose schema is not an object
    /// (E055).
    Surfaces,
    /// Two contributions claim one name and the first registration wins: a
    /// kind keyword (E026), an edge label (W018), a rule code (W023), a
    /// command, MCP tool or resource name (E039).
    Collision,
}

/// One diagnostic of the build and what it is about.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Reported {
    pub(crate) part: BuildPart,
    pub(crate) diagnostic: Diagnostic,
}

impl Reported {
    pub(crate) fn new(part: BuildPart, diagnostic: Diagnostic) -> Self {
        Self { part, diagnostic }
    }

    fn tagged(&self) -> (BuildPart, &Diagnostic) {
        (self.part, &self.diagnostic)
    }
}

/// `diagnostic`s tagged with `part`.
fn about(
    part: BuildPart,
    diagnostics: impl IntoIterator<Item = Diagnostic>,
) -> impl Iterator<Item = Reported> {
    diagnostics.into_iter().map(move |d| Reported::new(part, d))
}

/// One pass an extension declares, with that extension.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeclaredPass {
    pub extension: String,
    pub pass: CompilerPassDescriptor,
}

impl DeclaredPass {
    /// Whether the pass runs with every compile ([`CHECK_PHASE`]) rather
    /// than under `specforge analyze`.
    pub fn is_check_phase(&self) -> bool {
        self.pass.phase.as_deref() == Some(CHECK_PHASE)
    }

    /// `<extension>:<pass>`, the name `specforge analyze` runs it by.
    pub fn full_name(&self) -> String {
        format!("{}:{}", self.extension, self.pass.name)
    }
}

/// Everything the compiler derives from the loaded declarations, before
/// any `.spec` file is read. Callers read it through its methods: the
/// registries, the rule set and passes, the graph inputs, and its
/// diagnostics in one order.
#[derive(Debug, Default)]
pub struct RegistryBuild {
    /// The declarations the build was made from, in load order; MCP tools
    /// refused for a malformed schema (E055) are left out of their surfaces.
    declarations: Vec<ExtensionDeclaration>,
    pub(crate) kinds: KindRegistry,
    pub(crate) fields: FieldRegistry,
    pub(crate) edges: EdgeRegistry,
    /// The rule set: the extensions' rules (checked and resolved against
    /// the registries) plus the host-generated E006 rules for required
    /// fields, each with its origin (ADR 0020).
    pub(crate) rules: Rules,
    /// Registered surface contributions (first registration wins).
    pub(crate) surfaces: Vec<SurfaceRegistryEntry>,
    /// Every declared pass with its extension: extension by extension in
    /// load order, each extension's in its after/before order.
    pub(crate) passes: Vec<DeclaredPass>,
    /// E030 identity and shape, W021 self-consistency, the peers' E073 and
    /// E027 (a cycle's after the declarations'), W145 pass cycles — in that
    /// order, extension by extension within each.
    declaration_diagnostics: Vec<Reported>,
    /// Populate (E026, W018, W019, W021, I004, then the edge types' W021),
    /// W017, the rule set's (W150, W147 or W112 per declared rule), then
    /// W023 — in that order.
    registry_diagnostics: Vec<Reported>,
    /// The custom rules' probes (W112), set by [`Self::probe`]; empty until then.
    probe_diagnostics: Vec<Diagnostic>,
    /// Refused MCP tools (E055) and surface registration conflicts (E039).
    surface_diagnostics: Vec<Reported>,
}

impl RegistryBuild {
    /// The kinds the declarations register, each under its keyword (first
    /// in load order wins, E026).
    pub fn kinds(&self) -> &KindRegistry {
        &self.kinds
    }

    /// The fields registered on each kind: its own, its extension's shared
    /// fields and enhancements.
    pub fn fields(&self) -> &FieldRegistry {
        &self.fields
    }

    /// The edge types, each resolved once (ADR 0060): declared, and every
    /// label a field maps to without declaring it, with every field that
    /// writes it, its ends, and the edge type each field writes.
    pub fn edges(&self) -> &EdgeRegistry {
        &self.edges
    }

    /// The rule set (ADR 0020).
    pub fn rules(&self) -> &Rules {
        &self.rules
    }

    /// Every declared pass with its extension, in order (both phases).
    pub fn passes(&self) -> &[DeclaredPass] {
        &self.passes
    }

    /// The surface contributions registered (first registration wins; a
    /// later duplicate is E039).
    pub fn surfaces(&self) -> &[SurfaceRegistryEntry] {
        &self.surfaces
    }

    /// What the graph build reads of these declarations: body-parser kinds,
    /// single and absent-target reference fields, inverse pairs, value
    /// coercions and derived references (ADR 0032, amended). The one place
    /// they are derived; `Environment::graph_config` adds the provider
    /// schemes.
    pub fn graph_inputs(&self) -> GraphInputs {
        graph_inputs::derive(&self.declarations, &self.kinds, &self.fields)
    }

    /// Ask `verdicts` once for each `custom` rule's function, on an entity
    /// of its target kind that declares nothing (ADR 0020): a function that
    /// cannot answer is W112, reported after the registries' diagnostics; the
    /// rule stays registered. Probing again replaces the earlier result.
    /// `Environment::assemble` probes when its extensions loaded in a runtime;
    /// `NoVerdicts` answers nothing, so probing with it reports nothing.
    pub fn probe(&mut self, verdicts: &dyn CustomVerdicts) {
        self.probe_diagnostics = self.rules.probe(verdicts);
    }

    /// Every diagnostic the build reports with what it is about, in its one
    /// order: the declarations' own (E030, W021, E073 and E027, W145), the
    /// registries' (E026, W018, W019, W021, I004, the edge types' W021; W017;
    /// W150, W147 or W112 per rule; W023), the custom rules' probes (W112), then the surfaces'
    /// (E055, E039); each group extension by extension in load order.
    /// `specforge check` reports them right after the environment's own,
    /// before any source (ADR 0012 D11, amended).
    pub fn reported(&self) -> impl Iterator<Item = (BuildPart, &Diagnostic)> {
        self.declaration_diagnostics
            .iter()
            .map(Reported::tagged)
            .chain(self.registry_diagnostics.iter().map(Reported::tagged))
            .chain(self.probe_diagnostics.iter().map(|d| (BuildPart::Rules, d)))
            .chain(self.surface_diagnostics.iter().map(Reported::tagged))
    }

    /// [`Self::reported`] without the parts.
    pub fn diagnostics(&self) -> impl Iterator<Item = &Diagnostic> {
        self.reported().map(|(_, diagnostic)| diagnostic)
    }

    /// The declarations the build was made from, in load order.
    pub fn declarations(&self) -> &[ExtensionDeclaration] {
        &self.declarations
    }

    /// The declaration of the extension named `name`, when it is loaded.
    pub fn declaration(&self, name: &str) -> Option<&ExtensionDeclaration> {
        self.declarations.iter().find(|d| d.name() == name)
    }

    /// The short name of the loaded extension `extension` (its declared
    /// `ext_short`, else its name's last segment), which names its commands
    /// on the CLI and its tools over MCP.
    pub fn short(&self, extension: &str) -> Option<std::borrow::Cow<'_, str>> {
        self.declaration(extension).map(ExtensionDeclaration::short)
    }

    /// The passes every compile runs (`phase: "check"`), in order.
    pub fn check_passes(&self) -> impl Iterator<Item = &DeclaredPass> {
        self.passes.iter().filter(|p| p.is_check_phase())
    }

    /// The passes `specforge analyze` runs, in order.
    pub fn analyze_passes(&self) -> impl Iterator<Item = &DeclaredPass> {
        self.passes.iter().filter(|p| !p.is_check_phase())
    }

    /// (name, version) of each loaded extension, in load order.
    pub fn extension_info(&self) -> impl Iterator<Item = (&str, &str)> {
        self.declarations.iter().map(|d| (d.name(), d.version()))
    }
}

/// Build every registry and derived input from the loaded declarations, given in entry order:
/// the build puts them in load order first (ADR 0041), and everything after reads that order.
pub fn build_registries(declarations: Vec<ExtensionDeclaration>) -> RegistryBuild {
    let mut declarations = peers::in_load_order(declarations);
    // The declarations themselves: E030, W021, the peers' E073 and E027, then W145.
    let mut declaration_diagnostics: Vec<Reported> =
        about(BuildPart::Declaration, declarations.iter().flat_map(shape)).collect();
    for declaration in &declarations {
        declaration_diagnostics.extend(about(
            BuildPart::Declaration,
            consistency(declaration, &declarations),
        ));
    }
    declaration_diagnostics.extend(about(BuildPart::Peers, peers::check(&declarations)));
    let mut passes = Vec::new();
    for declaration in &declarations {
        let (ordered, cycle) = order_passes(declaration.name(), &declaration.passes);
        declaration_diagnostics.extend(about(BuildPart::Declaration, cycle));
        passes.extend(ordered.into_iter().map(|pass| DeclaredPass {
            extension: declaration.name().to_string(),
            pass,
        }));
    }

    // A tool whose schema is not an object is refused before anything
    // registers or lists it (E055).
    let mut surface_diagnostics = Vec::new();
    for declaration in &mut declarations {
        let name = declaration.name().to_string();
        surface_diagnostics.extend(about(
            BuildPart::Surfaces,
            refuse_malformed_tool_schemas(&name, &mut declaration.surfaces),
        ));
    }

    let (kinds, fields, edges, mut registry_diagnostics) = populate(&declarations);
    // W017: a testable kind that can't declare obligations.
    registry_diagnostics.extend(about(
        BuildPart::Registries,
        validate_extension_testability(&kinds),
    ));

    // The rule set: W150, W147 or W112 per declared rule, then W023 for a
    // code two extensions declare; E006 rules for required
    // fields come after the declared rules.
    let (rules, rule_diagnostics) = Rules::build(
        &declarations,
        Registries {
            kinds: &kinds,
            fields: &fields,
            edges: &edges,
        },
    );
    registry_diagnostics.extend(about(BuildPart::Rules, rule_diagnostics));
    registry_diagnostics.extend(about(
        BuildPart::Collision,
        Rules::duplicate_codes(&declarations),
    ));

    let surface_inputs: Vec<(String, _)> = declarations
        .iter()
        .map(|d| (d.name().to_string(), d.surfaces.clone()))
        .collect();
    let (surfaces, duplicates) = register_surface_contributions(&surface_inputs);
    surface_diagnostics.extend(about(BuildPart::Collision, duplicates));

    RegistryBuild {
        declarations,
        kinds,
        fields,
        edges,
        rules,
        surfaces,
        passes,
        declaration_diagnostics,
        registry_diagnostics,
        probe_diagnostics: Vec::new(),
        surface_diagnostics,
    }
}
