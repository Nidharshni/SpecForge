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
    pub kinds: KindRegistry,
    pub fields: FieldRegistry,
    pub edges: EdgeRegistry,
    /// The rule set: the extensions' rules (checked and resolved against
    /// the registries) plus the host-generated E006 rules for required
    /// fields, each with its origin (ADR 0020).
    pub rules: Rules,
    /// Registered surface contributions (first registration wins).
    pub surfaces: Vec<SurfaceRegistryEntry>,
    /// Every declared pass with its extension: extension by extension in
    /// load order, each extension's in its after/before order.
    pub passes: Vec<DeclaredPass>,
    /// E030 identity and shape, W021 self-consistency, the peers' E073 and
    /// E027 (a cycle's after the declarations'), W145 pass cycles — in that
    /// order, extension by extension within each.
    declaration_diagnostics: Vec<Diagnostic>,
    /// Populate (E026, W018, W019, W021, I004), W017, the rule set's (W150,
    /// W147 or W112 per declared rule), then W023 — in that order.
    registry_diagnostics: Vec<Diagnostic>,
    /// The custom rules' probes (W112), set by [`Self::probe`]; empty until then.
    probe_diagnostics: Vec<Diagnostic>,
    /// Refused MCP tools (E055) and surface registration conflicts (E039).
    surface_diagnostics: Vec<Diagnostic>,
}

impl RegistryBuild {
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
    /// The environment probes when its extensions loaded in a runtime;
    /// `NoVerdicts` answers nothing, so probing with it reports nothing.
    pub fn probe(&mut self, verdicts: &dyn CustomVerdicts) {
        self.probe_diagnostics = self.rules.probe(verdicts);
    }

    /// Every diagnostic the build reports, in its one order: the
    /// declarations' own (E030, W021, E073 and E027, W145), the registries'
    /// (E026, W018, W019, W021, I004; W017; W150, W147 or W112 per rule;
    /// W023), the custom rules' probes (W112), then the surfaces' (E055,
    /// E039); each group extension by extension in load order.
    /// `specforge check` reports them right after the environment's own,
    /// before any source (ADR 0012 D11, amended).
    pub fn diagnostics(&self) -> impl Iterator<Item = &Diagnostic> {
        self.declaration_diagnostics
            .iter()
            .chain(&self.registry_diagnostics)
            .chain(&self.probe_diagnostics)
            .chain(&self.surface_diagnostics)
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
    let mut declaration_diagnostics: Vec<Diagnostic> =
        declarations.iter().flat_map(shape).collect();
    for declaration in &declarations {
        declaration_diagnostics.extend(consistency(declaration, &declarations));
    }
    declaration_diagnostics.extend(peers::check(&declarations));
    let mut passes = Vec::new();
    for declaration in &declarations {
        let (ordered, cycle) = order_passes(declaration.name(), &declaration.passes);
        declaration_diagnostics.extend(cycle);
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
        surface_diagnostics.extend(refuse_malformed_tool_schemas(
            &name,
            &mut declaration.surfaces,
        ));
    }

    let (kinds, fields, edges, mut registry_diagnostics) = populate(&declarations);
    // W017: a testable kind that can't declare obligations.
    registry_diagnostics.extend(validate_extension_testability(&kinds));

    // The rule set: W112 for a rule that cannot work as declared, then
    // W023 for a code two extensions declare; E006 rules for required
    // fields come after the declared rules.
    let (rules, rule_diagnostics) = Rules::build(
        &declarations,
        Registries {
            kinds: &kinds,
            fields: &fields,
            edges: &edges,
        },
    );
    registry_diagnostics.extend(rule_diagnostics);

    let surface_inputs: Vec<(String, _)> = declarations
        .iter()
        .map(|d| (d.name().to_string(), d.surfaces.clone()))
        .collect();
    let (surfaces, duplicates) = register_surface_contributions(&surface_inputs);
    surface_diagnostics.extend(duplicates);

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
