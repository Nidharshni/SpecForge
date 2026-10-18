// @specforge/typescript extension types -- TypeScript/JavaScript source intelligence
//
// SourceItem is the central type: a single exported symbol extracted from a
// TypeScript or JavaScript source file. The scanner walks the AST (via
// tree-sitter-typescript or ts-morph in Wasm) and emits one SourceItem per
// public export. Agents and the entity-mapping pipeline consume these.

// ── File Extensions & Classification ───────────────────────────

type TsFileExtension "File extensions this extension handles. .d.ts is declaration-only. .mjs/.cjs are ESM/CJS entry points. .tsx/.jsx imply React." = ts
  | tsx
  | js
  | jsx
  | mjs
  | cjs
  | dts

type TsFileRole "Classifies a source file's role in the project." = production
  | test
  | story
  | config
  | script
  | declaration
  | generated

type TsFileRoleSignal "How the file was detected as a particular role." = file_extension
  | path_pattern
  | file_content
  | package_json
  | config_file

type TsSourceFile "Describes a source file with its role classification." {
  path             string @readonly
  extension        TsFileExtension
  role             TsFileRole
  role_signals     TsFileRoleSignal[]
  package_name     string @optional
  // Relative path within monorepo package (e.g., "src/utils/auth.ts")
  package_relative string @optional
  verify unit "TsSourceFile schema is valid"
}

// ── Test File Detection ────────────────────────────────────────

type TsTestFilePattern "Patterns that identify non-production files. The scanner uses these to classify files before extracting SourceItems." {
  pattern     string @readonly
  role        TsFileRole
  description string @optional
  verify unit "TsTestFilePattern schema is valid"
}

// Well-known test file patterns (compiled into the extension):
//   *.test.ts, *.test.tsx, *.spec.ts, *.spec.tsx     -> test
//   __tests__/**                                      -> test
//   *.stories.ts, *.stories.tsx                       -> story
//   *.stories.mdx                                     -> story
//   jest.config.*, vitest.config.*, playwright.config.*-> config
//   *.e2e.ts, *.e2e.tsx                               -> test
//   cypress/**                                        -> test
//   __mocks__/**                                      -> test
//   *.fixture.ts, *.fixture.tsx                       -> test
//   *.bench.ts, *.bench.tsx                           -> test
//   *.d.ts                                            -> declaration
//   **/*.generated.ts, **/*.gen.ts                    -> generated
//   scripts/**, tools/**, bin/**                      -> script

// ── Export Kinds ───────────────────────────────────────────────

type TsExportKind "The syntactic form of a TypeScript/JavaScript export." = named_export
  | default_export
  | re_export
  | barrel_export
  | namespace_export

// ── Source Item Kinds ──────────────────────────────────────────

type TsItemKind "The semantic category of an exported symbol. This is the primary discriminator for entity mapping inference." = function_item
  | class_item
  | interface_item
  | type_alias_item
  | enum_item
  | const_item
  | variable_item
  | react_component
  | react_hook
  | hoc_item
  | decorator_item
  | module_namespace_item

// ── The Core Type: TsSourceItem ────────────────────────────────

type TsSourceItem "A single exported symbol extracted from a TypeScript/JavaScript file. This is the primary output of the source scanner and the input to entity ID mapping and inference." {
  // Identity
  name                    string              @readonly
  entity_id               string              @readonly
  item_kind               TsItemKind
  export_kind             TsExportKind
  // Location
  file_path               string              @readonly
  line_start              integer
  line_end                integer
  column_start            integer             @optional
  column_end              integer             @optional
  // Signature
  signature               string              @optional
  // Full TypeScript type signature (e.g., "(input: string, opts?: Options) => Promise<Result>")
  type_signature          string              @optional
  // Return type if extractable
  return_type             string              @optional
  // Generic type parameters (e.g., ["T extends Base", "K keyof T"])
  generics                string[]            @optional
  // Parameters (for functions, methods, React components)
  parameters              TsParameter[]       @optional
  // Documentation
  jsdoc_summary           string              @optional
  jsdoc_tags              TsJsDocTag[]        @optional
  // Raw JSDoc text for agents that want full context
  jsdoc_raw               string              @optional
  // Decorators (Angular, NestJS, TypeORM, etc.)
  decorators              TsDecorator[]       @optional
  // Modifiers
  is_async                boolean             @optional
  is_generator            boolean             @optional
  is_abstract             boolean             @optional
  is_readonly             boolean             @optional
  accessibility           TsAccessibility     @optional
  // Class-specific
  extends_clause          string              @optional
  implements_clause       string[]            @optional
  members                 TsClassMember[]     @optional
  // Interface-specific
  interface_extends       string[]            @optional
  // Enum-specific
  enum_members            TsEnumMember[]      @optional
  is_const_enum           boolean             @optional
  // React-specific
  react_info              TsReactInfo         @optional
  // Re-export tracking
  re_export_source        string              @optional
  re_export_original_name string              @optional
  barrel_file             string              @optional
  // Inference signals (populated by the scanner, consumed by agents)
  inference_signals       TsInferenceSignal[] @optional
  // Module context
  module_system           TsModuleSystem      @optional
  verify unit "TsSourceItem schema is valid"
}

// ── Parameters ─────────────────────────────────────────────────

type TsParameter "Ts Parameter" {
  name            string        @readonly
  type_annotation string        @optional
  is_optional     boolean       @optional
  is_rest         boolean       @optional
  default_value   string        @optional
  // Destructured parameter shape (e.g., "{ name, email }: UserInput")
  destructured    boolean       @optional
  decorators      TsDecorator[] @optional
  verify unit "TsParameter schema is valid"
}

// ── JSDoc ──────────────────────────────────────────────────────

type TsJsDocTag "Ts Js Doc Tag" {
  tag             string @readonly
  name            string @optional
  text            string @optional
  type_expression string @optional
  verify unit "TsJsDocTag schema is valid"
}

// ── Decorators ─────────────────────────────────────────────────

type TsDecorator "Decorators from Angular, NestJS, TypeORM, MobX, etc." {
  name       string   @readonly
  // Full decorator expression (e.g., "@Controller('/users')")
  expression string   @optional
  arguments  string[] @optional
  // Framework that owns this decorator (angular, nestjs, typeorm, mobx, custom)
  framework  string   @optional
  verify unit "TsDecorator schema is valid"
}

// ── Class Members ──────────────────────────────────────────────

type TsClassMember "Ts Class Member" {
  name            string          @readonly
  member_kind     TsClassMemberKind
  type_annotation string          @optional
  accessibility   TsAccessibility @optional
  is_static       boolean         @optional
  is_abstract     boolean         @optional
  is_readonly     boolean         @optional
  is_async        boolean         @optional
  is_override     boolean         @optional
  parameters      TsParameter[]   @optional
  return_type     string          @optional
  decorators      TsDecorator[]   @optional
  jsdoc_summary   string          @optional
  verify unit "TsClassMember schema is valid"
}

type TsClassMemberKind "Ts Class Member Kind" = method
  | property
  | getter
  | setter
  | constructor
  | index_signature
  | static_block

type TsAccessibility "Ts Accessibility" = public | protected | private

// ── Enum Members ───────────────────────────────────────────────

type TsEnumMember "Ts Enum Member" {
  name          string @readonly
  value         string @optional
  jsdoc_summary string @optional
  verify unit "TsEnumMember schema is valid"
}

// ── React-Specific ─────────────────────────────────────────────

type TsReactInfo "React component and hook metadata extracted by the scanner." {
  component_type      TsReactComponentType
  // Props type name (e.g., "UserCardProps")
  props_type          string        @optional
  // Props type definition inlined (for anonymous prop types)
  props_inline        TsParameter[] @optional
  // Whether component uses forwardRef
  uses_forward_ref    boolean       @optional
  // Whether component uses React.memo
  uses_memo           boolean       @optional
  // Whether component uses React.lazy
  uses_lazy           boolean       @optional
  // Hook dependencies (for custom hooks: which React hooks they compose)
  hook_dependencies   string[]      @optional
  // State variables (from useState calls)
  state_vars          string[]      @optional
  // Context consumed (from useContext calls)
  contexts_consumed   string[]      @optional
  // Refs used (from useRef calls)
  refs_used           string[]      @optional
  // Whether the component is a page/route component (Next.js, Remix)
  is_route_component  boolean       @optional
  // Whether this is a Server Component (Next.js app directory)
  is_server_component boolean       @optional
  // Whether this is a Client Component ("use client" directive)
  is_client_component boolean       @optional
  verify unit "TsReactInfo schema is valid"
}

type TsReactComponentType "Ts React Component Type" = function_component
  | class_component
  | forward_ref_component
  | memo_component
  | lazy_component
  | higher_order_component
  | custom_hook

// ── Inference Signals ──────────────────────────────────────────

type TsInferenceSignal "Signals detected during scanning that guide entity kind inference. Each signal has a kind (what was detected), a confidence score, and a suggested entity kind for the specforge graph." {
  signal_kind      TsSignalKind
  confidence       number
  suggested_entity TsSuggestedEntity
  reason           string @optional
  verify unit "TsInferenceSignal schema is valid"
}

type TsSignalKind "Ts Signal Kind" = exports_service_class
  | exports_controller_class
  | exports_repository_class
  | exports_react_component
  | exports_react_hook
  | exports_interface
  | exports_abstract_class
  | exports_type_alias
  | exports_zod_schema
  | exports_io_ts_codec
  | exports_enum
  | exports_const_object
  | exports_factory_function
  | exports_handler_function
  | exports_middleware
  | exports_decorator
  | exports_event_class
  | exports_error_class
  | exports_config_schema
  | exports_test_helper
  | has_injectable_decorator
  | has_controller_decorator
  | has_entity_decorator
  | has_subscriber_decorator
  | has_guard_decorator
  | has_pipe_decorator
  | has_interceptor_decorator
  | implements_interface
  | extends_base_class
  | uses_dependency_injection
  | is_barrel_file
  | is_index_file
  | is_route_handler
  | is_graphql_resolver
  | is_grpc_service
  | is_websocket_gateway

type TsSuggestedEntity "Which specforge entity kind a signal suggests." = behavior
  | invariant
  | event
  | type
  | port

// ── Module System ──────────────────────────────────────────────

type TsModuleSystem "Ts Module System" = esm | commonjs | ambient | global_script

// ── Entity ID Mapping ──────────────────────────────────────────

type TsEntityIdMapping "How a TypeScript symbol name is converted to a specforge entity ID." {
  original_name string  @readonly
  entity_id     string  @readonly
  resolution    TsMappingResolution
  source_file   string
  source_line   integer @optional
  verify unit "TsEntityIdMapping schema is valid"
}

type TsMappingResolution "Ts Mapping Resolution" = tests_field
  | jsdoc_tag
  | naming_convention
  | barrel_trace

// Entity ID conversion rules:
//   PascalCase class/interface -> snake_case: UserService -> user_service
//   camelCase function         -> snake_case: validateInput -> validate_input
//   SCREAMING_SNAKE constant   -> lower_snake: MAX_RETRIES -> max_retries
//   React component            -> snake_case: UserCard -> user_card
//   Custom hook                -> snake_case without "use" prefix: useAuth -> auth
//   Barrel re-export           -> traced to original source, uses original's ID
//   @specforge tag in JSDoc    -> explicit override: /** @specforge entity_id */

// ── Monorepo Types ─────────────────────────────────────────────

type TsMonorepoTool "Monorepo tool detection and package resolution." = npm_workspaces
  | yarn_workspaces
  | pnpm_workspaces
  | nx
  | turborepo
  | lerna
  | rush

type TsMonorepoInfo "Ts Monorepo Info" {
  tool            TsMonorepoTool
  root_path       string   @readonly
  packages        TsPackageInfo[]
  // Workspace glob patterns from root package.json or tool config
  workspace_globs string[] @optional
  verify unit "TsMonorepoInfo schema is valid"
}

type TsPackageInfo "Ts Package Info" {
  name          string   @readonly
  path          string   @readonly
  // Relative path from monorepo root
  relative_path string
  // package.json "main" or "exports" entry point
  entry_points  string[] @optional
  // Dependencies within the monorepo (local packages)
  internal_deps string[] @optional
  // Whether this is a publishable package or internal-only
  is_private    boolean  @optional
  verify unit "TsPackageInfo schema is valid"
}

// ── Framework Detection ────────────────────────────────────────

type TsFramework "Ts Framework" = react
  | next
  | remix
  | angular
  | nestjs
  | express
  | fastify
  | hono
  | elysia
  | svelte
  | vue
  | solid
  | astro
  | nuxt
  | gatsby

type TsFrameworkDetection "Ts Framework Detection" {
  framework  TsFramework
  version    string @optional
  confidence number
  signals    string[]
  verify unit "TsFrameworkDetection schema is valid"
}

// ── Scan Result ────────────────────────────────────────────────

type TsScanResult "Top-level output from scanning a TypeScript/JavaScript project." {
  files_scanned    integer
  files_skipped    integer
  items_extracted  integer
  source_items     TsSourceItem[]
  source_files     TsSourceFile[]
  monorepo         TsMonorepoInfo         @optional
  frameworks       TsFrameworkDetection[] @optional
  scan_duration_ms number
  verify unit "TsScanResult schema is valid"
}

// ── Source Anchor ──────────────────────────────────────────────

type TsSourceAnchor "Given an entity ID, the source anchor locates the defining symbol across potentially many files (barrel re-exports, type merging)." {
  entity_id           string                @readonly
  primary_file        string
  primary_line        integer
  primary_column      integer               @optional
  // Additional locations for merged declarations (interface merging,
  // augmented modules, re-exports)
  secondary_locations TsSecondaryLocation[] @optional
  verify unit "TsSourceAnchor schema is valid"
}

type TsSecondaryLocation "Ts Secondary Location" {
  file_path string
  line      integer
  column    integer @optional
  role      TsSecondaryRole
  verify unit "TsSecondaryLocation schema is valid"
}

type TsSecondaryRole "Ts Secondary Role" = re_export
  | interface_merge
  | module_augmentation
  | type_override
  | barrel_re_export

// ── Config ─────────────────────────────────────────────────────

type TsExtensionConfig "Extension-specific configuration in specforge.json." {
  // Additional file patterns to exclude from scanning
  exclude_patterns   string[]             @optional
  // Additional file patterns to include (overrides default exclusions)
  include_patterns   string[]             @optional
  // Whether to follow barrel re-exports to find canonical source
  resolve_barrels    boolean              @optional
  // Whether to extract class members (methods, properties)
  extract_members    boolean              @optional
  // Whether to extract React-specific metadata
  extract_react_info boolean              @optional
  // Whether to extract JSDoc tags
  extract_jsdoc      boolean              @optional
  // Custom entity ID mapping overrides
  id_overrides       TsEntityIdOverride[] @optional
  // tsconfig.json path for type resolution (default: auto-detect)
  tsconfig_path      string               @optional
  verify unit "TsExtensionConfig schema is valid"
}

type TsEntityIdOverride "Ts Entity Id Override" {
  symbol_name  string @readonly
  file_pattern string @optional
  entity_id    string @readonly
  verify unit "TsEntityIdOverride schema is valid"
}
