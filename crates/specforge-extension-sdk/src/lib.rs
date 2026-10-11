//! Plugin-side SDK for authoring [SpecForge](https://github.com/leaderiop/SpecForge)
//! extensions against the handshake/describe protocol, at the version
//! [`PROTOCOL_VERSION`](specforge_protocol_types::PROTOCOL_VERSION) names.
//!
//! Wire types come from `specforge-protocol-types` — the same definitions the
//! host (`specforge-wasm`) uses — so the protocol cannot drift between the two
//! sides. The `#[specforge_extension_sdk_macros::extension]` attribute turns a
//! struct plus a [`Contributions`] impl into the `__handshake` / `__describe`
//! exports the host loads.
//!
//! Guests are `wasm32-wasip2` components (`component_guest!`); local-path
//! installs and registry installs are both checked against the lock's hash.

pub use specforge_extension_sdk_macros::extension;

/// The extension vocabulary — field types, validation check kinds and
/// constraint kinds — as the host's registry build reads it. `Custom` hands
/// the decision to the rule's `wasm_function` (see
/// [`RuleBuilder::wasm_function`]).
pub use specforge_protocol_types::{CheckKind, ConstraintKind, FieldType};

pub use specforge_protocol_types::{
    ContributionFlags, DeclaredCategory, EdgeTypeDescriptor, EntityEnhancementDescriptor,
    EntityKindDescriptor, ExtensionDeclaration, FeatureFlagDescriptor, FieldConstraintDescriptor,
    FieldDescriptor, HandshakeResponse, PeerDependency, ProtocolError, SandboxPolicy,
    ValidationRuleDescriptor, ValidationSeverity, ValidatorContext, ValidatorEntity,
    ValidatorField, ValidatorMethod, ValidatorParam, ValidatorRef, ValidatorVerdict,
};

use specforge_protocol_types::{
    AnalyzerDescriptor, AutoDetectConfig, CollectorDescriptor, CompilerPassDescriptor,
    DescribeRequest, DescribeResponse, pass_export,
};

use std::borrow::Cow;
use std::collections::BTreeMap;

/// The categories an extension gave as raw items, by name.
type RawCategories = BTreeMap<&'static str, serde_json::Value>;

/// Runtime-free testing of an extension's contributions.
pub mod testing;

/// Surface contributions (commands, MCP tools, MCP resources) declared with
/// their handlers.
pub mod surface;

/// Operational contributions (passes, collectors, custom rules, scanners,
/// the migration hook) declared with their handlers.
mod operations;

pub use surface::{ArgBuilder, CommandBuilder, CommandCall, McpResourceBuilder, McpToolBuilder};

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
/// surfaces. Contribution flags are **derived** from what was actually
/// contributed, so they cannot drift from the content.
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

    /// The contribution flags the handshake carries: derived from the
    /// declaration's content (raw categories are already applied to it), and
    /// `providers` from [`ExtensionMeta::providers`].
    fn flags(&self, declaration: &ExtensionDeclaration) -> ContributionFlags {
        ContributionFlags {
            providers: self.meta.providers,
            ..declaration.contribution_flags()
        }
    }

    /// Everything this extension declares, as the guest serves it and the
    /// host loads it: the handshake (from [`ExtensionMeta`], flags derived),
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
            contribution_flags: ContributionFlags::default(),
            peer_dependencies: self.meta.peer_dependencies.clone(),
            sandbox_policy: self.meta.sandbox_policy.clone(),
            starter_template: self.meta.starter_template.clone(),
            migration_hook: self.meta.migration_hook.clone(),
            theme_color: self.meta.theme_color.clone(),
            ext_short: self.meta.short.clone(),
            description: self.meta.description.clone(),
            keywords: self.meta.keywords.clone(),
        };
        declaration.handshake.contribution_flags = self.flags(&declaration);
        declaration
    }

    /// The `__handshake` wire payload (pretty JSON, matching the format of the
    /// existing builtin extensions).
    pub fn handshake_json(&self) -> String {
        self.declaration().handshake_json()
    }

    /// The `__describe` wire payload for `category`, or `None` when the
    /// category is not one the protocol defines: a raw category as given,
    /// else the declaration's items.
    ///
    /// For tests: a guest answers through [`Served`], which reads the
    /// declaration once.
    pub fn describe_response_json(&self, category: &str) -> Option<String> {
        describe_answer(&self.raw, || Cow::Owned(self.declaration()), category)
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

/// An extension as its component guest serves the host: its builder, and
/// the declaration and handshake read from it once. [`component_guest!`]
/// keeps one per instance.
pub struct Served {
    builder: ContributionsBuilder,
    declaration: ExtensionDeclaration,
    handshake: Vec<u8>,
}

impl Served {
    /// Serve `builder`: its declaration and handshake are built here, once.
    pub fn new(builder: ContributionsBuilder) -> Self {
        let declaration = builder.declaration();
        let handshake = declaration.handshake_json().into_bytes();
        Self {
            builder,
            declaration,
            handshake,
        }
    }

    /// The answer to the host's `call(name, export, input)`:
    /// - `__handshake` and `__describe` from the declaration read once;
    /// - a declared surface's or operation's export by its handler
    ///   ([`ContributionsBuilder::dispatch_export`]);
    /// - any other export through `handler`, whose `None` is
    ///   `unknown export '<name>'`, as a missing export is.
    pub fn call(
        &self,
        handler: ExportHandler,
        export: &str,
        input: &[u8],
    ) -> Result<Vec<u8>, String> {
        match export {
            "__handshake" => Ok(self.handshake.clone()),
            "__describe" => {
                let request: DescribeRequest = serde_json::from_slice(input)
                    .map_err(|e| format!("invalid describe request: {e}"))?;
                match describe_answer(
                    &self.builder.raw,
                    || Cow::Borrowed(&self.declaration),
                    &request.category,
                ) {
                    Some(body) => Ok(body.into_bytes()),
                    None => Err(format!("unsupported category: {}", request.category)),
                }
            }
            other => match self.builder.dispatch_export(other, input) {
                Some(result) => result,
                None => match handler(other, input) {
                    Some(result) => result,
                    None => Err(format!("unknown export '{other}'")),
                },
            },
        }
    }
}

/// Builder for [`EntityKindDescriptor`].
pub struct KindBuilder(EntityKindDescriptor);
impl KindBuilder {
    fn new(name: &str) -> Self {
        Self(EntityKindDescriptor {
            name: name.to_string(),
            ..Default::default()
        })
    }
    pub fn description(&mut self, d: &str) -> &mut Self {
        self.0.description = Some(d.to_string());
        self
    }
    pub fn keyword(&mut self, k: &str) -> &mut Self {
        self.0.keyword = Some(k.to_string());
        self
    }
    pub fn testable(&mut self, t: bool) -> &mut Self {
        self.0.testable = t;
        self
    }
    pub fn singleton(&mut self, s: bool) -> &mut Self {
        self.0.singleton = s;
        self
    }
    pub fn supports_verify(&mut self, s: bool) -> &mut Self {
        self.0.supports_verify = s;
        self
    }
    pub fn incremental(&mut self, i: bool) -> &mut Self {
        self.0.incremental = Some(i);
        self
    }
    pub fn open_fields(&mut self, o: bool) -> &mut Self {
        self.0.open_fields = o;
        self
    }
    /// The kind's body carries syntax the extension parses, not the core
    /// grammar: the host does not report the core grammar's parse errors
    /// inside its entities.
    pub fn has_body_parser(&mut self) -> &mut Self {
        self.0.has_body_parser = true;
        self
    }
    /// Reference fields that target this kind are contract obligations of
    /// the entity that declares them (the `contracts` analysis, A010).
    pub fn contract_target(&mut self) -> &mut Self {
        self.0.contract_target = true;
        self
    }
    /// The kind's entity ids name types: custom validators receive them as
    /// `ValidatorContext::declared_types`.
    pub fn declares_types(&mut self) -> &mut Self {
        self.0.declares_types = true;
        self
    }
    /// The field holding the kind's lifecycle state, whose previous value
    /// the build cache records for check-phase passes. Must be one of the
    /// kind's fields.
    pub fn lifecycle_field(&mut self, field: &str) -> &mut Self {
        self.0.lifecycle_field = Some(field.to_string());
        self
    }
    pub fn semantic_token(&mut self, t: &str) -> &mut Self {
        self.0.semantic_token = Some(t.to_string());
        self
    }
    pub fn lsp_icon(&mut self, i: &str) -> &mut Self {
        self.0.lsp_icon = Some(i.to_string());
        self
    }
    pub fn dot_shape(&mut self, s: &str) -> &mut Self {
        self.0.dot_shape = Some(s.to_string());
        self
    }
    pub fn dot_color(&mut self, c: &str) -> &mut Self {
        self.0.dot_color = Some(c.to_string());
        self
    }
    pub fn dot_fillcolor(&mut self, c: &str) -> &mut Self {
        self.0.dot_fillcolor = Some(c.to_string());
        self
    }
    pub fn verify_kinds(&mut self, kinds: &[&str]) -> &mut Self {
        self.0.verify_kinds = kinds.iter().map(|s| s.to_string()).collect();
        self
    }
    pub fn inference_guide(&mut self, g: &str) -> &mut Self {
        self.0.inference_guide = Some(g.to_string());
        self
    }
    pub fn field(&mut self, name: &str, f: impl FnOnce(&mut FieldBuilder)) -> &mut Self {
        let mut b = FieldBuilder::new(name);
        f(&mut b);
        self.0.fields.push(b.0);
        self
    }
}

/// Builder for [`FieldDescriptor`].
pub struct FieldBuilder(FieldDescriptor);
impl FieldBuilder {
    fn new(name: &str) -> Self {
        Self(FieldDescriptor {
            name: name.to_string(),
            ..Default::default()
        })
    }
    pub fn field_type(&mut self, t: FieldType) -> &mut Self {
        self.0.field_type = t.as_str().to_string();
        self
    }
    pub fn required(&mut self) -> &mut Self {
        self.0.required = true;
        self
    }
    pub fn description(&mut self, d: &str) -> &mut Self {
        self.0.description = Some(d.to_string());
        self
    }
    pub fn edge(&mut self, label: &str) -> &mut Self {
        self.0.edge = Some(label.to_string());
        self
    }
    pub fn target_kind(&mut self, k: &str) -> &mut Self {
        self.0.target_kind = Some(k.to_string());
        self
    }
    pub fn inverse_of(&mut self, k: &str) -> &mut Self {
        self.0.inverse_of = Some(k.to_string());
        self
    }
    /// The field states what the entity promises (a contract, a
    /// guarantee) rather than prose, so token-optimized exports keep it.
    pub fn normative(&mut self) -> &mut Self {
        self.0.normative = true;
        self
    }
    /// Set on an entity (`true`, or a non-empty value), the entity owes no
    /// obligations of its own (W004, coverage and stats leave it out).
    pub fn exempts_obligations(&mut self) -> &mut Self {
        self.0.exempts_obligations = true;
        self
    }
    /// The context export carries the field at the node's top level.
    pub fn headline(&mut self) -> &mut Self {
        self.0.headline = true;
        self
    }
    /// What the prove pass reads the field as: `"bound"` (a fact the solver
    /// assumes) or `"claim"` (a statement that must follow from the bounds).
    pub fn proof_role(&mut self, role: &str) -> &mut Self {
        self.0.proof_role = Some(role.to_string());
        self
    }
    pub fn default_value(&mut self, v: &str) -> &mut Self {
        self.0.default_value = Some(v.to_string());
        self
    }
    /// The host fills this reference field's edges from type names the
    /// entity writes elsewhere: `"type_expressions"` (its field types) or
    /// `"method_signatures"` (its method parameter and return types).
    pub fn derived_from(&mut self, source: &str) -> &mut Self {
        self.0.derived_from = Some(source.to_string());
        self
    }
    pub fn file_reference(&mut self) -> &mut Self {
        self.0.file_reference = true;
        self
    }
    pub fn enum_values(&mut self, values: &[&str]) -> &mut Self {
        self.0.enum_values = values.iter().map(|s| s.to_string()).collect();
        self
    }
}

/// Builder for [`EdgeTypeDescriptor`].
pub struct EdgeBuilder(EdgeTypeDescriptor);
impl EdgeBuilder {
    fn new(label: &str) -> Self {
        Self(EdgeTypeDescriptor {
            label: label.to_string(),
            description: None,
            source_kind: None,
            target_kind: None,
            edge_style: None,
            edge_color: None,
            edge_arrowhead: None,
        })
    }
    pub fn description(&mut self, d: &str) -> &mut Self {
        self.0.description = Some(d.to_string());
        self
    }
    pub fn source_kind(&mut self, k: &str) -> &mut Self {
        self.0.source_kind = Some(k.to_string());
        self
    }
    pub fn target_kind(&mut self, k: &str) -> &mut Self {
        self.0.target_kind = Some(k.to_string());
        self
    }
    pub fn edge_style(&mut self, s: &str) -> &mut Self {
        self.0.edge_style = Some(s.to_string());
        self
    }
    pub fn edge_color(&mut self, c: &str) -> &mut Self {
        self.0.edge_color = Some(c.to_string());
        self
    }
    pub fn edge_arrowhead(&mut self, a: &str) -> &mut Self {
        self.0.edge_arrowhead = Some(a.to_string());
        self
    }
}

/// Builder for [`EntityEnhancementDescriptor`].
pub struct EnhancementBuilder(EntityEnhancementDescriptor);
impl EnhancementBuilder {
    fn new(target_kind: &str, source_extension: &str) -> Self {
        Self(EntityEnhancementDescriptor {
            target_kind: target_kind.to_string(),
            source_extension: source_extension.to_string(),
            fields: Vec::new(),
            edge_types: Vec::new(),
            verify_kinds: None,
        })
    }
    pub fn field(&mut self, name: &str, f: impl FnOnce(&mut FieldBuilder)) -> &mut Self {
        let mut b = FieldBuilder::new(name);
        f(&mut b);
        self.0.fields.push(b.0);
        self
    }
    /// Make the target kind testable, accepting `verify` obligations of these kinds.
    pub fn verify_kinds(&mut self, kinds: &[&str]) -> &mut Self {
        self.0.verify_kinds = Some(kinds.iter().map(|k| k.to_string()).collect());
        self
    }
    pub fn edge_type(&mut self, label: &str, f: impl FnOnce(&mut EdgeBuilder)) -> &mut Self {
        let mut b = EdgeBuilder::new(label);
        f(&mut b);
        self.0.edge_types.push(b.0);
        self
    }
}

/// Builder for [`ValidationRuleDescriptor`], with a `custom` rule's
/// handler.
pub struct RuleBuilder {
    descriptor: ValidationRuleDescriptor,
    validate: Option<operations::Wire>,
}
impl RuleBuilder {
    fn new(code: &str) -> Self {
        Self {
            descriptor: ValidationRuleDescriptor {
                code: code.to_string(),
                severity: ValidationSeverity::Warning,
                message_template: String::new(),
                check: String::new(),
                target_kind: None,
                edge_type: None,
                field: None,
                constraint: None,
                wasm_function: None,
                target_extension: None,
            },
            validate: None,
        }
    }
    /// Decide a `custom` rule: `handler` receives one entity's
    /// [`ValidatorContext`] and answers its [`ValidatorVerdict`].
    pub fn validate(
        &mut self,
        handler: impl Fn(&ValidatorContext) -> ValidatorVerdict + 'static,
    ) -> &mut Self {
        self.validate = Some(operations::wire(
            "validator",
            move |context: &ValidatorContext| Ok::<_, String>(handler(context)),
        ));
        self
    }
    pub fn check(&mut self, kind: CheckKind) -> &mut Self {
        self.descriptor.check = kind.as_str().to_string();
        self
    }
    pub fn target_kind(&mut self, k: &str) -> &mut Self {
        self.descriptor.target_kind = Some(k.to_string());
        self
    }
    pub fn edge_type(&mut self, e: &str) -> &mut Self {
        self.descriptor.edge_type = Some(e.to_string());
        self
    }
    pub fn field(&mut self, f: &str) -> &mut Self {
        self.descriptor.field = Some(f.to_string());
        self
    }
    pub fn severity(&mut self, s: ValidationSeverity) -> &mut Self {
        self.descriptor.severity = s;
        self
    }
    pub fn message_template(&mut self, m: &str) -> &mut Self {
        self.descriptor.message_template = m.to_string();
        self
    }
    pub fn wasm_function(&mut self, f: &str) -> &mut Self {
        self.descriptor.wasm_function = Some(f.to_string());
        self
    }
    /// The extension whose kind or edge type this rule names, when it is
    /// neither this extension nor one of its declared peers (a rule on a
    /// kind of an extension this one works without, so no peer dependency
    /// is declared). While `extension` is not loaded the rule is inert, and
    /// costs the host no W021.
    pub fn target_extension(&mut self, extension: &str) -> &mut Self {
        self.descriptor.target_extension = Some(extension.to_string());
        self
    }
    pub fn constraint(&mut self, f: impl FnOnce(&mut FieldConstraintBuilder)) -> &mut Self {
        let mut b = FieldConstraintBuilder::default();
        f(&mut b);
        self.descriptor.constraint = Some(b.0);
        self
    }
}

/// Builder for [`FieldConstraintDescriptor`].
pub struct FieldConstraintBuilder(FieldConstraintDescriptor);
impl Default for FieldConstraintBuilder {
    fn default() -> Self {
        Self(FieldConstraintDescriptor {
            kind: String::new(),
            pattern: None,
            values: Vec::new(),
        })
    }
}
impl FieldConstraintBuilder {
    pub fn kind(&mut self, k: ConstraintKind) -> &mut Self {
        self.0.kind = k.as_str().to_string();
        self
    }
    pub fn pattern(&mut self, p: &str) -> &mut Self {
        self.0.pattern = Some(p.to_string());
        self
    }
    pub fn values(&mut self, values: &[&str]) -> &mut Self {
        self.0.values = values.iter().map(|s| s.to_string()).collect();
        self
    }
}

/// Builder for [`CompilerPassDescriptor`], with the pass's handler.
pub struct PassBuilder {
    descriptor: CompilerPassDescriptor,
    run: Option<operations::Wire>,
}
impl PassBuilder {
    fn new(name: &str) -> Self {
        Self {
            descriptor: CompilerPassDescriptor {
                name: name.to_string(),
                after: None,
                before: None,
                phase: None,
            },
            run: None,
        }
    }
    /// What the pass does: `handler` receives the [`PassInput`] snapshot
    /// and answers its diagnostics, bare (`Vec<PassDiagnostic>`) or with a
    /// summary ([`PassOutput`]).
    pub fn run<R: Into<PassAnswer>>(
        &mut self,
        handler: impl Fn(&PassInput) -> R + 'static,
    ) -> &mut Self {
        self.run = Some(operations::wire("pass", move |input: &PassInput| {
            Ok::<PassAnswer, String>(handler(input).into())
        }));
        self
    }
    pub fn after(&mut self, p: &str) -> &mut Self {
        self.descriptor.after = Some(p.to_string());
        self
    }
    pub fn before(&mut self, p: &str) -> &mut Self {
        self.descriptor.before = Some(p.to_string());
        self
    }
    /// The phase the pass runs in. `"check"` runs it with every compile
    /// (`specforge check`, watch, the LSP, MCP), after the graph checks,
    /// its diagnostics joining the compile's; any other phase, or none,
    /// runs it only under `specforge analyze`.
    pub fn phase(&mut self, p: &str) -> &mut Self {
        self.descriptor.phase = Some(p.to_string());
        self
    }
}

/// Builder for [`AnalyzerDescriptor`], with its scanner.
pub struct AnalyzerBuilder {
    descriptor: AnalyzerDescriptor,
    scan: Option<operations::Wire>,
}
impl AnalyzerBuilder {
    fn new(language: &str) -> Self {
        Self {
            descriptor: AnalyzerDescriptor {
                language: language.to_string(),
                file_extensions: Vec::new(),
                excluded_dirs: Vec::new(),
                scan_export: format!("scan__{language}"),
                classify_export: format!("classify__{language}"),
                map_export: format!("map__{language}"),
                description: None,
            },
            scan: None,
        }
    }
    /// The scanner: `handler` receives one source file ([`ScanRequest`])
    /// and answers the public items it found ([`ScanResponse`]).
    pub fn scan(&mut self, handler: impl Fn(&ScanRequest) -> ScanResponse + 'static) -> &mut Self {
        self.scan = Some(operations::wire("scan", move |request: &ScanRequest| {
            Ok::<_, String>(handler(request))
        }));
        self
    }
    /// The file extensions it scans, with their dot (`.rs`).
    pub fn file_extensions(&mut self, extensions: &[&str]) -> &mut Self {
        self.descriptor.file_extensions = extensions.iter().map(|e| e.to_string()).collect();
        self
    }
    /// Directories it never scans (`target`, `node_modules`).
    pub fn excluded_dirs(&mut self, dirs: &[&str]) -> &mut Self {
        self.descriptor.excluded_dirs = dirs.iter().map(|d| d.to_string()).collect();
        self
    }
    pub fn scan_export(&mut self, export: &str) -> &mut Self {
        self.descriptor.scan_export = export.to_string();
        self
    }
    pub fn classify_export(&mut self, export: &str) -> &mut Self {
        self.descriptor.classify_export = export.to_string();
        self
    }
    pub fn map_export(&mut self, export: &str) -> &mut Self {
        self.descriptor.map_export = export.to_string();
        self
    }
    pub fn description(&mut self, d: &str) -> &mut Self {
        self.descriptor.description = Some(d.to_string());
        self
    }
}

/// Builder for [`CollectorDescriptor`], with the collector's handler.
pub struct CollectorBuilder {
    descriptor: CollectorDescriptor,
    collect: Option<operations::Wire>,
}
impl CollectorBuilder {
    fn new(name: &str) -> Self {
        Self {
            descriptor: CollectorDescriptor {
                name: name.to_string(),
                input_formats: Vec::new(),
                export: format!("collect__{}", name.replace('-', "_")),
                auto_detect: None,
                run: Vec::new(),
                report: None,
                capture: None,
            },
            collect: None,
        }
    }
    /// What the collector does: `handler` receives the report files
    /// ([`CollectInput`]) and answers the results ([`CollectOutput`]), or
    /// why it could not read them.
    pub fn collect(
        &mut self,
        handler: impl Fn(&CollectInput) -> Result<CollectOutput, String> + 'static,
    ) -> &mut Self {
        self.collect = Some(operations::wire("collect", handler));
        self
    }
    /// A report format the collector reads (informational).
    pub fn input_format(&mut self, format: &str) -> &mut Self {
        self.descriptor.input_formats.push(format.to_string());
        self
    }
    /// Project-root files whose presence selects this collector.
    pub fn detect_files(&mut self, patterns: &[&str]) -> &mut Self {
        let detect = self
            .descriptor
            .auto_detect
            .get_or_insert_with(|| AutoDetectConfig {
                file_patterns: Vec::new(),
                env_vars: Vec::new(),
            });
        detect
            .file_patterns
            .extend(patterns.iter().map(|p| p.to_string()));
        self
    }
    /// The command the host runs, with consent. `{report}` in any argument
    /// expands to the absolute report path.
    pub fn run(&mut self, argv: &[&str]) -> &mut Self {
        self.descriptor.run = argv.iter().map(|a| a.to_string()).collect();
        self
    }
    /// Report file or directory the runner writes, relative to the project
    /// root. A directory is read as every `*.json` file directly inside it.
    pub fn report(&mut self, path: &str) -> &mut Self {
        self.descriptor.report = Some(path.to_string());
        self
    }
    /// Keep the command's standard output and pass it to the export as
    /// `CollectInput::stdout`, for runners whose results only appear there.
    pub fn capture_stdout(&mut self) -> &mut Self {
        self.descriptor.capture = Some("stdout".to_string());
        self
    }
}

/// Generated by the `extension` attribute: serializes `__handshake`.
pub fn handshake_json(b: &ContributionsBuilder) -> String {
    b.handshake_json()
}

/// The `handler` of a [`component_guest!`]: the answer to an export no
/// declaration answers, `None` for a name it does not implement.
pub type ExportHandler = fn(&str, &[u8]) -> Option<Result<Vec<u8>, String>>;

/// The answer of an export the guest's `handler` serves (one no builder
/// declares with its handler: an analyzer's `classify__`/`map__`, or an
/// export of a category given with [`ContributionsBuilder::raw_category`]):
/// `input` decoded as `I`, `handler`'s answer encoded. An input that is not
/// an `I` is the guest's error, naming the export.
pub fn answer_export<I, O>(
    export: &str,
    input: &[u8],
    handler: impl FnOnce(&I) -> O,
) -> Result<Vec<u8>, String>
where
    I: serde::de::DeserializeOwned,
    O: serde::Serialize,
{
    let input: I = operations::decode(export, input)?;
    operations::encode(export, &handler(&input))
}

/// The `handler` of a [`component_guest!`] that names none: no export
/// beyond the protocol's and the declared surfaces' and operations'.
pub fn no_other_exports(_export: &str, _input: &[u8]) -> Option<Result<Vec<u8>, String>> {
    None
}

/// Everything an extension needs, behind one `use`.
pub mod prelude {
    pub use crate::{
        AnalyzerBuilder, CheckKind, ConstraintKind, Contributions, ContributionsBuilder,
        EdgeBuilder, EnhancementBuilder, ExtensionMeta, FieldBuilder, FieldConstraintBuilder,
        FieldType, KindBuilder, MigrationInput, PassAnswer, PassBuildCache, PassBuilder,
        PassCachedStatus, PassDiagnostic, PassEdge, PassEntity, PassEntityResults, PassInput,
        PassOutput, PassSeverity, PassSpan, PassTestResult, PassTestResults, RuleBuilder,
        ScanRequest, ScanResponse, ScannedItem,
    };
    pub use crate::{
        ArgBuilder, CommandBuilder, CommandCall, CommandError, CommandEvidence, CommandFormat,
        CommandGraph, CommandInput, CommandOutput, EntityEvidence, GraphEdge, GraphNode,
        McpResourceBuilder, McpToolBuilder,
    };
    pub use crate::{
        CollectEntityResult, CollectInput, CollectOutput, CollectReportFile, CollectTestResult,
        CollectUnlinkedTest, CollectorBuilder,
    };
    pub use specforge_extension_sdk_macros::extension;
    pub use specforge_protocol_types::{
        PeerDependency, SandboxPolicy, ValidationSeverity, ValidatorContext, ValidatorVerdict,
    };
}

#[cfg(test)]
mod raw_category_flag_tests {
    use super::*;

    /// Formal's migration exposed this: an extension serving describes via
    /// `raw_category` (static data) must still raise the corresponding
    /// contribution flags, or the host never requests those categories.
    #[test]
    fn raw_categories_raise_their_flags() {
        let mut b = ContributionsBuilder::new(ExtensionMeta::new("@acme/formal", "1.0.0"));
        b.raw_category(
            DeclaredCategory::Entities,
            serde_json::json!([{ "name": "Property" }]),
        );
        b.raw_category(
            DeclaredCategory::ValidationRules,
            serde_json::json!([{
                "code": "F100",
                "severity": "warning",
                "message_template": "m",
                "check": "custom"
            }]),
        );

        let handshake = b.handshake_json();
        let value: serde_json::Value = serde_json::from_str(&handshake).unwrap();
        let flags = &value["contribution_flags"];
        assert_eq!(
            flags["entities"],
            serde_json::json!(true),
            "raw entities must raise entities flag"
        );
        assert_eq!(
            flags["validators"],
            serde_json::json!(true),
            "raw validation_rules must raise validators flag"
        );
        assert_eq!(flags["grammars"], serde_json::json!(false));
    }

    #[test]
    fn a_collector_can_capture_stdout() {
        let mut k = CollectorBuilder::new("cargo-test");
        let plain = serde_json::to_value(&k.descriptor).unwrap();
        assert!(plain.get("capture").is_none());
        k.capture_stdout();
        let captured = serde_json::to_value(&k.descriptor).unwrap();
        assert_eq!(captured["capture"], serde_json::json!("stdout"));
    }

    #[test]
    fn a_starter_template_rides_the_handshake() {
        let mut b = ContributionsBuilder::new(ExtensionMeta::new("@acme/x", "1.0.0"));
        let without: serde_json::Value = serde_json::from_str(&b.handshake_json()).unwrap();
        assert!(without.get("starter_template").is_none(), "{without}");

        b.starter_template("spec \"{project}\" {}\n");
        let with: serde_json::Value = serde_json::from_str(&b.handshake_json()).unwrap();
        assert_eq!(
            with["starter_template"],
            serde_json::json!("spec \"{project}\" {}\n")
        );
    }

    #[test]
    fn a_migration_hook_rides_the_handshake() {
        let mut b = ContributionsBuilder::new(ExtensionMeta::new("@acme/x", "1.0.0"));
        let without: serde_json::Value = serde_json::from_str(&b.handshake_json()).unwrap();
        assert!(without.get("migration_hook").is_none(), "{without}");

        b.migration_hook_handler("migrate_acme", |_| Ok(()));
        let with: serde_json::Value = serde_json::from_str(&b.handshake_json()).unwrap();
        assert_eq!(with["migration_hook"], serde_json::json!("migrate_acme"));
    }
}

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
    CollectEntityResult, CollectInput, CollectOutput, CollectReportFile, CollectTestResult,
    CollectUnlinkedTest, CommandError, CommandEvidence, CommandFormat, CommandOutput,
    EntityEvidence, GraphEdge, GraphNode, GraphWire, McpResourceContent, McpResourceRequest,
    MigrationInput, PassAnswer, PassBuildCache, PassCachedStatus, PassDiagnostic, PassEdge,
    PassEntity, PassEntityResults, PassInput, PassOutput, PassSeverity, PassSpan, PassTestResult,
    PassTestResults, ScanRequest, ScanResponse, ScannedItem,
};

/// What a `cmd__<name>` export receives, its graph indexed for lookups
/// ([`CommandGraph`]).
pub type CommandInput = specforge_protocol_types::CommandInput<CommandGraph>;

/// The compiled graph a command reads: its entities sorted by id, its
/// resolved references sorted by (source, target, label).
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
#[serde(from = "GraphWire", into = "GraphWire")]
pub struct CommandGraph {
    nodes: Vec<GraphNode>,
    edges: Vec<GraphEdge>,
    by_id: std::collections::HashMap<String, usize>,
    from: std::collections::HashMap<String, Vec<usize>>,
    to: std::collections::HashMap<String, Vec<usize>>,
}

impl From<GraphWire> for CommandGraph {
    fn from(wire: GraphWire) -> Self {
        CommandGraph::new(wire.nodes, wire.edges)
    }
}

impl From<CommandGraph> for GraphWire {
    fn from(graph: CommandGraph) -> Self {
        GraphWire {
            nodes: graph.nodes,
            edges: graph.edges,
        }
    }
}

impl CommandGraph {
    /// A graph of `nodes` and `edges`, indexed for lookups.
    pub fn new(nodes: Vec<GraphNode>, edges: Vec<GraphEdge>) -> Self {
        let mut graph = CommandGraph {
            nodes,
            edges,
            ..Default::default()
        };
        for (i, node) in graph.nodes.iter().enumerate() {
            graph.by_id.insert(node.id.clone(), i);
        }
        for (i, edge) in graph.edges.iter().enumerate() {
            graph.from.entry(edge.source.clone()).or_default().push(i);
            graph.to.entry(edge.target.clone()).or_default().push(i);
        }
        graph
    }

    /// Every entity, sorted by id.
    pub fn nodes(&self) -> &[GraphNode] {
        &self.nodes
    }

    /// Every resolved reference.
    pub fn edges(&self) -> &[GraphEdge] {
        &self.edges
    }

    /// The entity `id`.
    pub fn node(&self, id: &str) -> Option<&GraphNode> {
        self.by_id.get(id).map(|&i| &self.nodes[i])
    }

    /// The entities of `kind`, sorted by id.
    pub fn nodes_of_kind<'a>(&'a self, kind: &'a str) -> impl Iterator<Item = &'a GraphNode> + 'a {
        self.nodes.iter().filter(move |n| n.kind == kind)
    }

    /// The references out of `id`.
    pub fn edges_from(&self, id: &str) -> Vec<&GraphEdge> {
        self.indexed(&self.from, id)
    }

    /// The references into `id`.
    pub fn edges_to(&self, id: &str) -> Vec<&GraphEdge> {
        self.indexed(&self.to, id)
    }

    fn indexed(
        &self,
        index: &std::collections::HashMap<String, Vec<usize>>,
        id: &str,
    ) -> Vec<&GraphEdge> {
        index
            .get(id)
            .map(|is| is.iter().map(|&i| &self.edges[i]).collect())
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod command_graph_tests {
    use super::*;

    #[test]
    fn a_command_input_carries_its_args_and_graph() {
        let input: CommandInput = serde_json::from_value(serde_json::json!({
            "args": {"status": "done", "limit": 2, "offset": "1", "all": true},
            "cwd": "/p",
            "graph": {
                "format_version": "1.0",
                "nodes": [
                    {"id": "a", "kind": "k", "title": "A", "file": "x.spec", "line": 1,
                     "fields": {"status": "done", "refs": ["b"]}},
                    {"id": "b", "kind": "k", "file": "x.spec", "line": 2, "fields": {}}
                ],
                "edges": [{"source": "a", "target": "b", "label": "refs"}]
            }
        }))
        .unwrap();
        assert_eq!(input.args["status"], "done");
        assert_eq!(input.args["limit"], 2);
        let a = input.graph.node("a").unwrap();
        assert_eq!(a.text("status"), Some("done"));
        assert_eq!(a.text("refs"), None);
        assert_eq!(a.list("refs"), ["b"]);
        assert_eq!(input.graph.nodes_of_kind("k").count(), 2);
        assert_eq!(input.graph.edges_from("a")[0].target, "b");
        assert_eq!(input.graph.edges_to("b")[0].source, "a");
        assert!(input.graph.edges_to("a").is_empty());
    }

    #[test]
    fn a_command_input_is_built_by_its_fields() {
        // The alias of the protocol's generic input builds and defaults as
        // the SDK's own struct did.
        let input = CommandInput {
            args: serde_json::Map::new(),
            cwd: "/p".to_string(),
            graph: CommandGraph::default(),
            format: CommandFormat::Json,
            today: String::new(),
            evidence: Default::default(),
        };
        assert!(input.is_json());
        assert!(!CommandInput::default().is_json());
    }
}

/// Component-mode export glue (wasm32-wasip2 guests).
///
/// Expands to the `wit_bindgen::generate!` bindings for the
/// `specforge:bridge@0.1.0` world plus a `Guest` implementation that
/// dispatches the host's `call(name, export-name, input)` entry point:
///
/// - `__handshake` / `__describe` are served from the [`ContributionsBuilder`]
///   produced by `$build` (the same JSON wire protocol as always);
/// - a declared surface's export (a command, an MCP tool, an MCP resource:
///   [`ContributionsBuilder::command`] and its siblings) is answered by its
///   declared handler ([`ContributionsBuilder::dispatch_export`]);
/// - every other export name is forwarded to `$handler`, which returns
///   `None` for names it does not implement (the guest then errors, exactly
///   like a missing export). An extension whose only exports are its
///   surfaces leaves `handler` out.
///
/// ```ignore
/// fn build() -> specforge_extension_sdk::ContributionsBuilder { /* ... */ }
/// fn dispatch(export: &str, input: &[u8]) -> Option<Result<Vec<u8>, String>> {
///     match export {
///         "scan__rust" => Some(serde_json::to_vec(&scan_rust(req)?)),
///         _ => None,
///     }
/// }
/// specforge_extension_sdk::component_guest!(build = build, handler = dispatch);
/// ```
#[macro_export]
macro_rules! component_guest {
    (build = $build:expr) => {
        ::specforge_extension_sdk::component_guest!(
            build = $build,
            handler = ::specforge_extension_sdk::no_other_exports
        );
    };
    (build = $build:expr, handler = $handler:expr) => {
        ::wit_bindgen::generate!({
            inline: r#"
package specforge:bridge@0.1.0;

world bridge {
    export call: func(name: string, export-name: string, input: list<u8>) ->
        result<list<u8>, string>;
}
"#,
        });

        struct BridgeGuest;

        impl Guest for BridgeGuest {
            fn call(
                _name: String,
                export_name: String,
                input: Vec<u8>,
            ) -> Result<Vec<u8>, String> {
                // A component instance is single-threaded, and the host makes
                // a new one after a trap, so the builder, its declaration and
                // the handshake are built once per instance.
                ::std::thread_local! {
                    static SERVED: ::specforge_extension_sdk::Served =
                        ::specforge_extension_sdk::Served::new($build());
                }
                SERVED.with(|served| served.call($handler, &export_name, &input))
            }
        }

        export!(BridgeGuest);
    };
}
