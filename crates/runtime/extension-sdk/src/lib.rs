//! Plugin-side SDK for authoring [SpecForge](https://github.com/leaderiop/SpecForge)
//! extensions against the handshake/describe protocol, at the version
//! [`PROTOCOL_VERSION`](specforge_protocol_types::PROTOCOL_VERSION) names.
//!
//! Wire types come from `specforge-protocol-types` — the same definitions the
//! host (`specforge-wasm`) uses — so the protocol cannot drift between the two
//! sides. The `#[specforge_extension_sdk_macros::extension]` attribute names the
//! extension and generates `specforge_extension_build()`, the
//! [`ContributionsBuilder`] its [`Contributions`] impl fills; [`component_guest!`]
//! serves it as a `wasm32-wasip2` component whose one export, `call`, answers
//! `__handshake`, `__describe` and every declared export by name ([`Served`]).
//! Local-path installs and registry installs are both checked against the lock's
//! hash.
//!
//! A guest depends on this crate alone: `component_guest!` brings its own
//! bindings.

pub use specforge_extension_sdk_macros::extension;

/// The bindings generator [`component_guest!`] expands through.
#[doc(hidden)]
pub use wit_bindgen as __wit_bindgen;

/// The extension vocabulary — field types, validation check kinds and
/// constraint kinds — as the host's registry build reads it. `Custom` hands
/// the decision to the rule's `wasm_function` (see
/// [`RuleBuilder::wasm_function`]).
pub use specforge_protocol_types::{CheckKind, ConstraintKind, FieldType};

pub use specforge_protocol_types::{
    DeclaredCategory, EdgeTypeDescriptor, EntityEnhancementDescriptor, EntityKindDescriptor,
    ExtensionDeclaration, FeatureFlagDescriptor, FieldConstraintDescriptor, FieldDescriptor,
    HandshakeResponse, PeerDependency, ProtocolError, SandboxPolicy, ValidationRuleDescriptor,
    ValidationSeverity, ValidatorContext, ValidatorEntity, ValidatorField, ValidatorMethod,
    ValidatorParam, ValidatorRef, ValidatorVerdict,
};

use specforge_protocol_types::{DescribeResponse, pass_export};

use std::borrow::Cow;
use std::collections::BTreeMap;

/// The categories an extension gave as raw items, by name.
type RawCategories = BTreeMap<&'static str, serde_json::Value>;

/// Runtime-free testing of an extension's contributions (feature `testing`).
#[cfg(any(test, feature = "testing"))]
pub mod testing;

/// Surface contributions (commands, MCP tools, MCP resources) declared with
/// their handlers.
pub mod surface;

/// Operational contributions (passes, collectors, custom rules, scanners,
/// the migration hook) declared with their handlers.
mod operations;

mod vocabulary;
pub use surface::{ArgBuilder, CommandBuilder, CommandCall, McpResourceBuilder, McpToolBuilder};
pub use vocabulary::Vocabulary;

mod builders;
mod command_graph;
mod guest;
pub use builders::{
    AnalyzerBuilder, CollectorBuilder, EdgeBuilder, EnhancementBuilder, FieldBuilder,
    FieldConstraintBuilder, KindBuilder, PassBuilder, RuleBuilder,
};
pub use command_graph::{CommandGraph, CommandInput};
pub use guest::{ExportHandler, Served, answer_export, no_other_exports};

/// Identity of the extension: what `__handshake` reports.
#[derive(Debug, Clone, Default)]
pub struct ExtensionMeta {
    pub name: String,
    pub version: String,
    /// The short name routing the extension's commands and tools
    /// (`specforge <short> <command>`, `specforge.<short>.<id>`), lowercase
    /// kebab case; the name's last segment when absent. On the wire as
    /// `ext_short`.
    pub short: Option<String>,
    /// One line saying what the extension is for (package registries show
    /// it).
    pub description: Option<String>,
    /// Search keywords for package registries.
    pub keywords: Vec<String>,
    pub peer_dependencies: Vec<PeerDependency>,
    pub sandbox_policy: Option<SandboxPolicy>,
    /// The starter `.spec` file `specforge init` writes for a project that
    /// enables this extension; `{project}` stands for the project's id and
    /// `{version}` for its version.
    pub starter_template: Option<String>,
    /// The export `specforge migrate` calls after migrating the project's
    /// files.
    pub migration_hook: Option<String>,
    /// The colour diagrams draw the extension in (`#rrggbb`).
    pub theme_color: Option<String>,
    /// It implements configured providers (the handshake's `providers`).
    pub providers: bool,
}

impl ExtensionMeta {
    pub fn new(name: &str, version: &str) -> Self {
        Self {
            name: name.to_string(),
            version: version.to_string(),
            ..Default::default()
        }
    }
}

/// Trait implemented by the extension author; the attribute macro wires it
/// into the protocol exports.
pub trait Contributions {
    fn contribute(c: &mut ContributionsBuilder);
}

/// Accumulates everything an extension contributes: its
/// [`ExtensionDeclaration`] ([`ContributionsBuilder::declaration`]), which
/// the guest serves on the wire, plus the handlers of its declared
/// surfaces. What it contributes is what it declares
/// ([`ExtensionDeclaration::declares`]); no flag restates it.
#[derive(Default)]
pub struct ContributionsBuilder {
    pub meta: ExtensionMeta,
    /// The declared categories (the handshake is derived from `meta` when
    /// the declaration is built, the surfaces from `surfaces`).
    decl: ExtensionDeclaration,
    surfaces: surface::Surfaces,
    operations: operations::Operations,
    raw: RawCategories,
}

impl ContributionsBuilder {
    pub fn new(meta: ExtensionMeta) -> Self {
        Self {
            meta,
            ..Default::default()
        }
    }

    /// Contribute a new entity kind.
    pub fn kind(&mut self, name: &str, f: impl FnOnce(&mut KindBuilder)) -> &mut Self {
        let mut b = KindBuilder::new(name);
        f(&mut b);
        self.decl.entities.push(b.0);
        self
    }

    /// Contribute an edge type (relationship) between entity kinds.
    pub fn edge(&mut self, label: &str, f: impl FnOnce(&mut EdgeBuilder)) -> &mut Self {
        let mut b = EdgeBuilder::new(label);
        f(&mut b);
        self.decl.edges.push(b.0);
        self
    }

    /// Contribute a field shared by all entity kinds.
    pub fn shared_field(&mut self, name: &str, f: impl FnOnce(&mut FieldBuilder)) -> &mut Self {
        let mut b = FieldBuilder::new(name);
        f(&mut b);
        self.decl.shared_fields.push(b.0);
        self
    }

    /// Add fields and/or edge types to an entity kind contributed by another
    /// extension.
    pub fn enhance(
        &mut self,
        target_kind: &str,
        source_extension: &str,
        f: impl FnOnce(&mut EnhancementBuilder),
    ) -> &mut Self {
        let mut b = EnhancementBuilder::new(target_kind, source_extension);
        f(&mut b);
        self.decl.enhancements.push(b.0);
        self
    }

    /// Contribute a validation rule. A declarative rule names its check; a
    /// `custom` one is decided by its handler ([`RuleBuilder::validate`]),
    /// at the export its `wasm_function` names (`validate__<code>`, the
    /// code lowercased, unless set).
    ///
    /// # Panics
    ///
    /// When a `custom` rule declares no handler, or a rule with a handler
    /// is not `custom`.
    pub fn rule(&mut self, code: &str, f: impl FnOnce(&mut RuleBuilder)) -> &mut Self {
        let mut b = RuleBuilder::new(code);
        f(&mut b);
        let custom = b.descriptor.check == CheckKind::Custom.as_str();
        match (custom, b.validate) {
            (true, Some(validate)) => {
                let export = b
                    .descriptor
                    .wasm_function
                    .get_or_insert_with(|| format!("validate__{}", code.to_lowercase()))
                    .clone();
                self.add_operation(export, format!("custom rule '{code}'"), validate);
            }
            (true, None) => {
                panic!("custom rule '{code}' declares no handler: decide it with `r.validate(...)`")
            }
            (false, Some(_)) => {
                panic!("rule '{code}' has a validate handler, but its check is not custom")
            }
            (false, None) => {}
        }
        self.decl.validation_rules.push(b.descriptor);
        self
    }

    /// Contribute a compiler pass, run by its handler ([`PassBuilder::run`])
    /// at the export [`pass_export`] names (`__pass_<name>`).
    ///
    /// # Panics
    ///
    /// When the pass declares no handler.
    pub fn pass(&mut self, name: &str, f: impl FnOnce(&mut PassBuilder)) -> &mut Self {
        let mut b = PassBuilder::new(name);
        f(&mut b);
        let Some(run) = b.run else {
            panic!("pass '{name}' declares no handler: run it with `p.run(...)`");
        };
        self.add_operation(pass_export(name), format!("pass '{name}'"), run);
        self.decl.passes.push(b.descriptor);
        self
    }

    /// Contribute a test-result collector, answered by its handler
    /// ([`CollectorBuilder::collect`]: a [`CollectInput`] in, a
    /// [`CollectOutput`] out) at the export `collect__<name>`, `-` mapped
    /// to `_`.
    ///
    /// # Panics
    ///
    /// When the collector declares no handler.
    pub fn collector(&mut self, name: &str, f: impl FnOnce(&mut CollectorBuilder)) -> &mut Self {
        let mut b = CollectorBuilder::new(name);
        f(&mut b);
        let Some(collect) = b.collect else {
            panic!("collector '{name}' declares no handler: answer it with `k.collect(...)`");
        };
        self.add_operation(
            b.descriptor.export.clone(),
            format!("collector '{name}'"),
            collect,
        );
        self.decl.collectors.push(b.descriptor);
        self
    }

    /// Route the operation export `export` (declared by `what`) to `wire`.
    ///
    /// # Panics
    ///
    /// When a declared surface or operation already answers `export`.
    fn add_operation(&mut self, export: String, what: String, wire: operations::Wire) {
        if let Some(owner) = self.surfaces.owner(&export) {
            panic!("{what}'s export {export} is already '{owner}''s");
        }
        self.operations.add(export, what, wire);
    }

    /// Panics when a declared operation answers the export of a surface
    /// just declared.
    fn assert_surfaces_free(&self) {
        for (export, name) in self.surfaces.exports() {
            if let Some(owner) = self.operations.owner(export) {
                panic!("'{name}''s export {export} is already {owner}'s");
            }
        }
    }

    /// Name the exports of the commands declared after it
    /// `cmd__<prefix>_<id>` (the builtins use their short name:
    /// `cmd__product_features`); without one they are `cmd__<id>`.
    pub fn command_prefix(&mut self, prefix: &str) -> &mut Self {
        self.surfaces.set_prefix(prefix);
        self
    }

    /// Contribute a CLI command, `specforge <ext_short> <id with _ as ->`,
    /// auto-promoted to the MCP tool `specforge.<ext_short>.<id>`. Its
    /// declaration (title, description, args) is the `surfaces` payload;
    /// its handler answers its export, reading the args through the
    /// declaration ([`CommandCall`]). Panics without a handler.
    pub fn command(&mut self, id: &str, f: impl FnOnce(&mut CommandBuilder)) -> &mut Self {
        self.surfaces.add_command(id, f);
        self.assert_surfaces_free();
        self
    }

    /// Contribute an explicit MCP tool, answered by its handler at the
    /// export `mcp__<name>` (`.` and `-` as `_`).
    pub fn mcp_tool(&mut self, name: &str, f: impl FnOnce(&mut McpToolBuilder)) -> &mut Self {
        self.surfaces.add_tool(name, f);
        self.assert_surfaces_free();
        self
    }

    /// Contribute an MCP resource, read by its handler at the export
    /// `mcp__<name>` (`.` and `-` as `_`).
    pub fn mcp_resource(
        &mut self,
        name: &str,
        f: impl FnOnce(&mut McpResourceBuilder),
    ) -> &mut Self {
        self.surfaces.add_resource(name, f);
        self.assert_surfaces_free();
        self
    }

    /// Run the declared command whose export is `export` on `input`, as the
    /// host's call would; `None` when no command has that export.
    pub fn call_command(&self, export: &str, input: &CommandInput) -> Option<CommandOutput> {
        self.surfaces.call_command(export, input)
    }

    /// The wire answer of a declared export: a surface's (a command, an
    /// MCP tool or an MCP resource) or an operation's (a pass, a collector,
    /// a custom rule, a scanner, the migration hook); `None` when no
    /// declaration has it. The generated guest routes every export but
    /// `__handshake` and `__describe` here first.
    pub fn dispatch_export(&self, export: &str, input: &[u8]) -> Option<Result<Vec<u8>, String>> {
        self.surfaces
            .dispatch(export, input)
            .or_else(|| self.operations.dispatch(export, input))
    }

    /// Contribute a language analyzer: `specforge infer` scans the files
    /// with these extensions through its scanner ([`AnalyzerBuilder::scan`])
    /// at the export `scan__<language>` unless set otherwise. Its
    /// `classify__<language>` and `map__<language>` exports are declared
    /// too; the host does not call them, so they are the guest's `handler`'s
    /// to answer, if anything.
    ///
    /// # Panics
    ///
    /// When the analyzer declares no scanner.
    pub fn analyzer(&mut self, language: &str, f: impl FnOnce(&mut AnalyzerBuilder)) -> &mut Self {
        let mut b = AnalyzerBuilder::new(language);
        f(&mut b);
        let Some(scan) = b.scan else {
            panic!("analyzer '{language}' declares no scanner: scan with `a.scan(...)`");
        };
        self.add_operation(
            b.descriptor.scan_export.clone(),
            format!("analyzer '{language}'"),
            scan,
        );
        self.decl.analyzers.push(b.descriptor);
        self
    }

    /// Contribute a feature flag.
    pub fn feature_flag(
        &mut self,
        name: &str,
        default_enabled: bool,
        description: &str,
    ) -> &mut Self {
        self.decl.feature_flags.push(FeatureFlagDescriptor {
            name: name.to_string(),
            description: (!description.is_empty()).then(|| description.to_string()),
            default_enabled,
        });
        self
    }

    /// Contribute the starter `.spec` file `specforge init` writes for a
    /// project that enables this extension. `{project}` in `template` is
    /// replaced with the project's entity id and `{version}` with its
    /// version.
    pub fn starter_template(&mut self, template: &str) -> &mut Self {
        self.meta.starter_template = Some(template.to_string());
        self
    }

    /// The colour diagrams draw this extension in (`#rrggbb`): its cluster
    /// in `specforge model --format dot`, its node in `specforge outline`.
    pub fn theme_color(&mut self, color: &str) -> &mut Self {
        self.meta.theme_color = Some(color.to_string());
        self
    }

    /// Contribute the hook `specforge migrate` calls after it migrates the
    /// project's `.spec` files, so the extension can migrate its own data:
    /// `handler` receives a [`MigrationInput`] at the export `export`. Its
    /// answer is not read beyond success or failure.
    ///
    /// # Panics
    ///
    /// When a declared surface or operation already answers `export`.
    pub fn migration_hook_handler(
        &mut self,
        export: &str,
        handler: impl Fn(&MigrationInput) -> Result<(), String> + 'static,
    ) -> &mut Self {
        self.meta.migration_hook = Some(export.to_string());
        self.add_operation(
            export.to_string(),
            "the migration hook".to_string(),
            operations::wire("migration hook", handler),
        );
        self
    }

    /// Escape hatch for an extension not written with the builders (ADR
    /// 0011): `items` is the raw JSON array the host receives for
    /// `category`, served as given. A declared category's items must parse
    /// as its descriptors: [`ContributionsBuilder::declaration`] panics
    /// naming the category when they do not, so the extension fails when it
    /// is built, not when a host loads it. Keys the descriptors do not
    /// define are ignored by the host and reported (W138).
    pub fn raw_category(
        &mut self,
        category: DeclaredCategory,
        items: serde_json::Value,
    ) -> &mut Self {
        self.raw.insert(category.name(), items);
        self
    }

    /// It implements configured providers: a `specforge.json` `providers`
    /// entry may name it. The handshake says so (`providers`); no describe
    /// category carries it.
    pub fn providers(&mut self) -> &mut Self {
        self.meta.providers = true;
        self
    }

    /// Declare a peer this extension needs (a peer requirement): `name`,
    /// installed at a version `range` accepts. `range` is a SemVer
    /// requirement read as Cargo reads one (`^1.0`); one that does not
    /// parse satisfies nothing (E073). `specforge add` and `init` enable a
    /// required builtin peer along with the extension, and refuse the
    /// install when the peer is missing or out of range (E027).
    pub fn peer(&mut self, name: &str, range: &str) -> &mut Self {
        self.push_peer(name, range, false)
    }

    /// Declare a peer this extension works without: when it is installed,
    /// its version must be one `range` accepts (E027 otherwise).
    pub fn optional_peer(&mut self, name: &str, range: &str) -> &mut Self {
        self.push_peer(name, range, true)
    }

    fn push_peer(&mut self, name: &str, range: &str, optional: bool) -> &mut Self {
        self.meta.peer_dependencies.push(PeerDependency {
            name: name.to_string(),
            version: range.to_string(),
            optional,
        });
        self
    }

    /// Everything this extension declares, as the guest serves it and the
    /// host loads it: the handshake (from [`ExtensionMeta`]),
    /// every category the builders contributed, the declared surfaces, and
    /// each raw category in place of its builders'.
    ///
    /// # Panics
    ///
    /// When a raw category's items do not parse as its descriptors, naming
    /// the category: a build-time error of the extension.
    pub fn declaration(&self) -> ExtensionDeclaration {
        let mut declaration = self.decl.clone();
        declaration.surfaces = self.surfaces.descriptor();
        for category in DeclaredCategory::ALL {
            if let Some(items) = self.raw.get(category.name()) {
                let name = category.name();
                declaration
                    .set_category(category, items)
                    .unwrap_or_else(|error| match error {
                        ProtocolError::DescribeFailed { reason, .. } => {
                            panic!(
                                "raw category '{name}' does not parse as its descriptors: {reason}"
                            )
                        }
                        other => panic!("raw category '{name}': {other}"),
                    });
            }
        }
        declaration.handshake = HandshakeResponse {
            protocol_version: specforge_protocol_types::PROTOCOL_VERSION.to_string(),
            name: self.meta.name.clone(),
            version: self.meta.version.clone(),
            peer_dependencies: self.meta.peer_dependencies.clone(),
            sandbox_policy: self.meta.sandbox_policy.clone(),
            starter_template: self.meta.starter_template.clone(),
            migration_hook: self.meta.migration_hook.clone(),
            theme_color: self.meta.theme_color.clone(),
            ext_short: self.meta.short.clone(),
            description: self.meta.description.clone(),
            keywords: self.meta.keywords.clone(),
            providers: self.meta.providers,
        };
        declaration
    }
}

/// The describe answer: a raw category as given, else `declaration`'s
/// items; `declaration` is read only for a category that is not raw.
fn describe_answer<'d>(
    raw: &RawCategories,
    declaration: impl FnOnce() -> Cow<'d, ExtensionDeclaration>,
    category: &str,
) -> Option<String> {
    let category = DeclaredCategory::from_name(category)?;
    Some(match raw.get(category.name()) {
        Some(items) => DescribeResponse {
            category: category.name().to_string(),
            items: items.clone(),
        }
        .wire_json(),
        None => declaration().describe_json(category),
    })
}

/// Everything an extension needs, behind one `use`.
pub mod prelude {
    pub use crate::{
        AnalyzerBuilder, CheckKind, ConstraintKind, Contributions, ContributionsBuilder,
        EdgeBuilder, EnhancementBuilder, ExtensionMeta, FieldBuilder, FieldConstraintBuilder,
        FieldType, KindBuilder, MigrationInput, PassAnswer, PassBuildCache, PassBuilder,
        PassCachedStatus, PassDiagnostic, PassEdge, PassEntity, PassInput, PassOutput,
        PassSeverity, PassSpan, RuleBuilder, ScanRequest, ScanResponse, ScannedItem, TestResult,
        TestResults, TestStatus, Vocabulary,
    };
    pub use crate::{
        ArgBuilder, CommandBuilder, CommandCall, CommandError, CommandEvidence, CommandFormat,
        CommandGraph, CommandInput, CommandOutput, EntityEvidence, GraphEdge, GraphNode,
        McpResourceBuilder, McpToolBuilder,
    };
    pub use crate::{
        CollectEntityResult, CollectInput, CollectOutput, CollectReportFile, CollectUnlinkedTest,
        CollectorBuilder,
    };
    pub use specforge_extension_sdk_macros::extension;
    pub use specforge_protocol_types::{
        SandboxPolicy, ValidationSeverity, ValidatorContext, ValidatorVerdict,
    };
}

#[cfg(test)]
mod raw_category_tests;

// ── Operational payloads ───────────────────────────────────────────────────
// What the host sends each export it calls and what the export answers are
// the protocol's types (`specforge_protocol_types::calls`, ADR 0013), the
// same definitions the host decodes and encodes, re-exported here under the
// names extension authors use.
//
// A compiler pass is a `__pass_<name>` export that receives a snapshot of
// the compiled project's entities ([`PassInput`]) and answers diagnostics,
// bare or with a summary ([`PassAnswer`]). A collector is a
// `collect__<name>` export ([`CollectInput`] in, [`CollectOutput`] out).
//
// A CLI command an extension contributes ([`ContributionsBuilder::command`],
// declared with its handler; see [`surface`]) is a `cmd__<name>` export. The
// host parses the command line against the command's declared args,
// compiles the project and calls the export with a [`CommandInput`]: the
// args, the project root, the compiled graph in the graph export's shape
// (`specforge export --format graph` without the schema), the format the
// caller asked for and the host's date. The export answers with a
// [`CommandOutput`]. The same export serves the MCP tool the command is
// auto-promoted to (`specforge.<ext_short>.<id>`), so it never reads the
// file system: the graph is all it knows.
//
// The host owns `--format` (ADR 0011): every command has it, `human` (the
// CLI default) or `json` (always, over MCP), and no command declares an arg
// of that name. The extension renders both: under `json` one root object on
// stdout, under `human` its own layout. A command that cannot answer writes
// one [`CommandError`] to stderr and nothing to stdout
// ([`CommandOutput::error`]).

pub use specforge_protocol_types::{
    CollectEntityResult, CollectInput, CollectOutput, CollectReportFile, CollectUnlinkedTest,
    CommandError, CommandEvidence, CommandFormat, CommandOutput, EntityEvidence, EntityTests,
    GraphEdge, GraphNode, GraphWire, McpResourceContent, McpResourceRequest, MigrationInput,
    PassAnswer, PassBuildCache, PassCachedStatus, PassDiagnostic, PassEdge, PassEntity, PassInput,
    PassOutput, PassSeverity, PassSpan, ScanRequest, ScanResponse, ScannedItem, TestResult,
    TestResults, TestStatus,
};

#[cfg(test)]
mod command_graph_tests;
