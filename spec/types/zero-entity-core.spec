// Zero-entity core architecture types — extension declarations, registries,
// declarative validation

use "types/core"
use "types/surface"
use "types/wasm"

// Everything one extension declares: its handshake and every describe
// category the host reads (specforge_protocol_types::ExtensionDeclaration,
// ADR 0012). The SDK builds it, the guest serves it, the host loads it once
// per environment load, the registry build reads it and a package registry
// stores it as the package's manifest.
type ExtensionDeclaration {
  handshake        HandshakeResponse             @readonly
  entities         EntityKindDescriptor[]        @optional
  edges            EdgeTypeDescriptor[]          @optional
  // Fields every entity kind of the extension gets (a kind's own field of the same name wins)
  shared_fields    FieldDescriptor[]             @optional
  // Fields, edge types and verify kinds added to other extensions' entity kinds
  enhancements     EntityEnhancementDescriptor[] @optional
  validation_rules ValidationRulePattern[]       @optional
  // CLI commands, MCP tools and MCP resources
  surfaces         SurfaceDescriptor             @optional
  // Test result collectors (see register_collector_contributions)
  collectors       CollectorDescriptor[]         @optional
  // Language analyzers (specforge infer)
  analyzers        AnalyzerDescriptor[]          @optional
  passes           CompilerPassDeclaration[]     @optional
  feature_flags    FeatureFlagDeclaration[]      @optional
  verify unit "ExtensionDeclaration schema is valid"
}

// The extension's answer to __handshake: who it is, what it needs, and how
// the host shows it.
type HandshakeResponse {
  protocol_version  string           @readonly
  name              string           @readonly
  version           string           @readonly
  peer_dependencies PeerDependency[] @optional
  // Absent: the host's ceiling holds the extension (30000 ms, 512 MB)
  sandbox_policy    SandboxPolicy    @optional
  // Text of the starter .spec file scaffold_starter_spec_file writes; {project} stands for the project id and {version} for its version
  starter_template  string           @optional
  // Wasm function name to invoke during `specforge migrate` for this extension
  migration_hook    string           @optional
  // The colour diagrams (model, outline) draw the extension in; grey when absent
  theme_color       string           @optional
  // The name its commands are routed by (specforge <ext_short> <command>,
  // specforge.<ext_short>.<id>); lowercase kebab case; absent, the name's last segment
  ext_short         string           @optional
  // What a package registry shows for the extension
  description       string           @optional
  keywords          string[]         @optional
  // It implements configured providers: a specforge.json providers entry may name it (ADR 0050)
  providers         boolean          @optional
  verify unit "HandshakeResponse schema is valid"
}

// What the registry build derives from the loaded declarations, before any
// .spec file is read (specforge_registry::build_registries). It is made only
// by that build and read only through its methods. It is pure.
type RegistryBuild {
  // The declarations it was built from, in load order
  declarations ExtensionDeclaration[] @readonly
  kinds        KindRegistryEntry[]
  fields       FieldRegistryEntry[]
  edges        EdgeRegistryEntry[]
  // The extensions' rules plus the host-generated E006 rules
  rules        ValidationRulePattern[]
  surfaces     SurfaceRegistryEntry[]
  // Every declared pass, extension by extension in load order, each
  // extension's in its after/before order
  passes       CompilerPassDeclaration[]
  // What every graph of the project is built with (registry_build_graph_inputs)
  graph_inputs GraphInputs            @readonly
  // Every diagnostic it reports, in one order: the declarations' own (E030,
  // W021, E073 and E027, W145), the registries' (E026, W018, W019, W021,
  // I004, W017, the rule set's W150, W147 or W112, then W023), the custom
  // rules' probes (W112), then the surfaces' (E055, E039); each group
  // extension by extension in load order
  diagnostics  Diagnostic[]           @readonly
  verify unit "RegistryBuild schema is valid"
}

// What the graph build reads of the loaded extensions (specforge_graph::GraphInputs).
// Only the registry build derives it; with the provider schemes the
// environment registers it is every graph's configuration (ADR 0032).
type GraphInputs {
  // Inverse field pairs, each written lesser label first, sorted: a two-hop
  // cycle through both labels of one pair is not a reference cycle
  bidirectional_pairs      string[] @readonly
  // Kinds whose bodies an extension parses: the core grammar's parse
  // errors inside their entities are not reported
  body_parser_kinds        string[] @readonly
  // kind.field of every registered reference field (none while no kind is
  // registered): its one value links as an edge
  single_reference_fields  string[] @readonly
  // kind.field of every reference field whose target kind no loaded
  // extension declares, with that kind: an unresolved value there is I004
  absent_reference_targets string[] @readonly
  // kind.field of every registered field whose value is coerced to its
  // declared type: a list type to a list; integer, bool, string or enum to
  // that type; a single reference or a block is read as written
  field_coercions          string[] @readonly
  // kind.field of every reference field whose edges the host derives
  // (derived_from), with its target kind and source, sorted by kind and field
  derived_references       string[] @readonly
  verify unit "GraphInputs schema is valid"
}

type EntityKindDescriptor {
  name            string            @readonly
  // The keyword entities of the kind are written with; absent, its name
  keyword         string            @optional
  description     string            @optional
  fields          FieldDescriptor[] @optional
  testable        boolean           @optional
  singleton       boolean           @optional
  supports_verify boolean           @optional
  // The verify kinds allowed on this entity kind; empty = all allowed
  verify_kinds    string[]          @optional
  // Whether this entity kind receives GraphDelta (true) or full Graph (false) during incremental validation
  incremental     boolean           @optional
  has_body_parser boolean           @optional
  // Its entities may carry fields the kind does not declare
  open_fields     boolean           @optional
  semantic_token  string            @optional
  lsp_icon        string            @optional
  dot_shape       string            @optional
  dot_color       string            @optional
  dot_fillcolor   string            @optional
  // How specforge infer recognises the kind in code
  inference_guide string            @optional
  // Reference fields that target this kind are contract obligations (A010)
  contract_target boolean           @optional
  // Its entity ids name types: custom validators receive them as declared_types
  declares_types  boolean           @optional
  /// The one field (of those the kind declares) holding its entities'
  /// lifecycle state; the build cache records its value (ADR 0009).
  lifecycle_field string            @optional
  verify unit "EntityKindDescriptor schema is valid"
}

type EdgeTypeDescriptor {
  label          string @readonly
  description    string @optional
  source_kind    string @optional
  target_kind    string @optional
  // Visual style for graph rendering: "solid" | "dashed" | "dotted" (default: "solid")
  edge_style     string @optional
  // Edge color for graph rendering (CSS/X11 color name or hex, default: "black")
  edge_color     string @optional
  // Edge arrowhead for graph rendering: "normal" | "dot" | "diamond" | "none" (default: "normal")
  edge_arrowhead string @optional
  verify unit "EdgeTypeDescriptor schema is valid"
}

// Fields, edge types and verify kinds one extension adds to an entity kind
// another extension declares.
type EntityEnhancementDescriptor {
  target_kind      string               @readonly
  // The extension that owns the target kind: an enhancement of a kind whose
  // owner is not loaded is skipped silently
  source_extension string               @readonly
  fields           FieldDescriptor[]    @optional
  edge_types       EdgeTypeDescriptor[] @optional
  // Makes the target kind testable with exactly these verify kinds (ADR 0002)
  verify_kinds     string[]             @optional
  verify unit "EntityEnhancementDescriptor schema is valid"
}

// A language analyzer specforge infer runs over source files.
type AnalyzerDescriptor {
  language        string   @readonly
  file_extensions string[]
  excluded_dirs   string[] @optional
  scan_export     string
  classify_export string
  map_export      string
  description     string   @optional
  verify unit "AnalyzerDescriptor schema is valid"
}

type FieldDescriptor {
  name                string                 @readonly
  /// A FieldType name or an accepted older spelling; any other is W019.
  field_type          string                 @readonly
  required            boolean                @optional
  description         string                 @optional
  edge                string                 @optional
  target_kind         string                 @optional
  file_reference      boolean                @optional
  default_value       string                 @optional
  // The values an enum field accepts
  enum_values         string[]               @optional
  // The field on the target kind that holds the inverse reference
  inverse_of          string                 @optional
  /// The field states what the entity promises (a behavior's contract, an
  /// invariant's guarantee), as opposed to prose; token-optimized exports keep it.
  normative           boolean                @optional
  /// Set on an entity (true, or a non-empty value), the entity owes no
  /// obligations of its own: W004, coverage and stats leave it out.
  exempts_obligations boolean                @optional
  /// The context export carries the field at the node's top level (a
  /// behavior's contract, a feature's status).
  headline            boolean                @optional
  /// The host fills the field's edges from type names the entity writes
  /// elsewhere (behavior link_derived_references).
  derived_from        DerivedReferenceSource @optional
  /// What the prove pass reads the field as: a bound it assumes or a claim
  /// that must follow from the bounds (ADR 0009). No role: not read.
  proof_role          ProofRole              @optional
  verify unit "FieldDescriptor schema is valid"
}

// A field's role in the prove pass: a bound is assumed (bounds must be
// consistent, E046); a claim must follow from the bounds (W139 when not).
type ProofRole = bound | claim

// Where a derived reference field takes its targets from: the type names in
// the entity's type-syntax field values, or in its method signatures.
type DerivedReferenceSource = type_expressions | method_signatures

// FieldType: how the host reads a field's value (FieldType in
// specforge-protocol-types). Every output names a field type by these
// names: the declaration, the Graph Protocol schema and exports, the
// published JSON Schema, the model, the hover and E061 (ADR 0034). The
// host also reads the older spellings — each name with a `_type` suffix,
// and "boolean" — and never writes them.
//
// verify is NOT a field type — it is a grammar-level construct parsed by a
// dedicated rule (parse_verify_statements). Whether an entity kind supports
// verify is declared via the supports_verify flag on EntityKindDescriptor, not
// via field type registration.
type FieldType = "string"
  | "integer"
  | "bool"
  | "enum"
  | "string_list"
  | "reference"
  | "reference_list"
  | "block"

// A validation rule as an extension declares it (the protocol's
// ValidationRuleDescriptor). The registry build turns it into a typed rule
// (specforge_registry::rules::Rule) carrying only what its check reads;
// constraint.pattern is a regex for field_value_constraint/matches and the
// condition field's name for conditional_field_required.
type ValidationRulePattern {
  code             string          @readonly
  severity         string          @readonly
  message_template string
  check            ValidationPatternKind
  target_kind      string          @optional
  edge_type        string          @optional
  field            string          @optional
  constraint       FieldConstraint @optional
  wasm_function    string          @optional
  target_extension string          @optional
  verify unit "ValidationRulePattern schema is valid"
}

type FieldConstraint {
  kind    ConstraintKind @readonly
  pattern string         @optional
  values  string[]       @optional
  verify unit "FieldConstraint schema is valid"
}

// How a constraint reads its pattern and values (ConstraintKind in
// specforge-protocol-types): non_empty, one_of and matches for
// field_value_constraint, when_field_equals for conditional_field_required,
// one_of for verify_kind_allowlist.
type ConstraintKind = non_empty | one_of | matches | when_field_equals

// The check kinds of the extension vocabulary (CheckKind in
// specforge-protocol-types — the SDK writes these names, the registry build
// reads them; it also reads the older SDK spellings missing_field,
// field_constraint, cycle and conditional_required).
type ValidationPatternKind = no_incoming_edges
  | no_outgoing_edges
  | no_edges
  | missing_field_when_flag_set
  | missing_required_field
  | conditional_field_required
  | field_value_constraint
  | cycle_detection
  | file_exists
  | verify_kind_allowlist
  | no_verify_statements
  | custom

// One call of a custom rule's function: the extension that exports it, the
// function, and what it is asked about (an entity, or the load-time probe).
type CustomCall {
  extension string @readonly
  function  string @readonly
  subject   string @readonly
  verify unit "CustomCall schema is valid"
}

type FieldRegistryEntry {
  kind_name           string                 @readonly
  field_name          string                 @readonly
  /// Built only from a descriptor whose type the host reads, named
  /// canonically; its enum values and proof role are the descriptor's.
  field_type          FieldType              @readonly
  enum_values         string[]               @optional
  source_extension    string                 @readonly
  edge                string                 @optional
  target_kind         string                 @optional
  file_reference      boolean                @optional
  required            boolean                @optional
  normative           boolean                @optional
  exempts_obligations boolean                @optional
  headline            boolean                @optional
  derived_from        DerivedReferenceSource @optional
  /// The prove-pass role its manifest declares; any other value is refused.
  proof_role          ProofRole              @optional
  verify unit "FieldRegistryEntry schema is valid"
}

type KindRegistryEntry {
  kind_name            string   @readonly
  source_extension     string   @readonly
  testable             boolean
  singleton            boolean
  supports_verify      boolean
  // Subset of extension verify_kinds allowed on this entity kind; empty = all allowed
  allowed_verify_kinds string[] @optional
  // Unreferenced checking handled by extension validation_rules (e.g. no_incoming_edges pattern)
  semantic_token       string   @optional
  lsp_icon             string   @optional
  dot_shape            string   @optional
  dot_color            string   @optional
  dot_fillcolor        string   @optional
  contract_target      boolean  @optional
  declares_types       boolean  @optional
  /// The kind's lifecycle field; a name the kind does not declare is refused.
  lifecycle_field      string   @optional
  verify unit "KindRegistryEntry schema is valid"
}

// SchemeRegistryEntry maps a ref scheme to the provider extension that
// handles validation for that scheme. Populated by register_provider_schemes
// from provider extension manifests. The scheme is the prefix portion of a
// ref identifier (e.g., the "gh" in "gh.issue:42").
type SchemeRegistryEntry {
  scheme          string   @readonly @unique
  provider_alias  string   @readonly
  extension_name  string   @readonly
  supported_kinds string[] @optional
  verify unit "SchemeRegistryEntry schema is valid"
}

type KeywordExtensionMapping {
  keyword     string
  extension   string
  entity_kind string
  verify unit "KeywordExtensionMapping schema is valid"
}

type EdgeRegistryEntry {
  label            string @readonly
  source_kind      string @optional
  target_kind      string @optional
  source_extension string @readonly
  edge_style       string @optional
  edge_color       string @optional
  edge_arrowhead   string @optional
  writers          EdgeWriter[]
  verify unit "EdgeRegistryEntry schema is valid"
}

// EdgeWriter is a field that writes an edge label no loaded extension
// declares; an undeclared edge type's entry lists every writer in load
// order, and its ends are theirs (registry_build_edges).
type EdgeWriter {
  extension   string @readonly
  kind        string @readonly
  field       string @readonly
  target_kind string @optional
  verify unit "EdgeWriter schema is valid"
}

type KeywordExtensionIndex {
  _tag    "KeywordExtensionIndex" @literal
  entries KeywordExtensionMapping[]
  verify unit "KeywordExtensionIndex schema is valid"
}

type CompilerPassDeclaration {
  name          string @readonly
  run_after     string @optional
  wasm_function string @readonly
  description   string @optional
  verify unit "CompilerPassDeclaration schema is valid"
}

type FeatureFlagDeclaration {
  name          string @readonly
  default_value boolean
  description   string @optional
  verify unit "FeatureFlagDeclaration schema is valid"
}
