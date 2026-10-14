# RES-28: Language Intelligence Extensions

**Date:** 2026-04-24
**Status:** Research Complete
**Experts consulted:** 10 (vision alignment, manifest/protocol, tree-sitter/Wasm, inference pipeline, Rust extension, TypeScript extension, source anchoring, core refactoring, DX/authoring, market/ecosystem)

---

## 1. Executive Summary

Language intelligence extensions (`@specforge/rust`, `@specforge/typescript`, and future per-language plugins) provide AST-powered source code analysis as a new extension contribution category called **analyzers**. They declare zero entity kinds of their own -- instead they enhance `@specforge/software` entities with source anchoring, gap analysis, test collection, and inference signals. The core compiler gains a `scanners` (or `analyzers`) contribution flag, two new host functions (`host_read_source_file`, `host_list_files`) operating in a sandboxed source-root zone, and a unified `SourceDiscoveryConfig` that replaces four duplicated hardcoded file-walk implementations. The current Rust-specific regex scanner in `crates/kernel/common/src/inference.rs` is extracted to a deprecated fallback module and ultimately replaced by `@specforge/rust`'s Wasm exports. This design fully resolves the Principle 2 violation (zero domain knowledge in core) while maintaining backward compatibility through a four-phase migration path.

---

## 2. Architecture Decision: New Contribution Category

### Consensus: Add `analyzers` to ManifestV2

All experts who addressed naming agreed on a new contribution flag. Two names were proposed:

| Name | Proponents | Rationale |
|------|-----------|-----------|
| `analyzers` | Manifest architect, DX expert | Follows existing noun-plural pattern (validators, collectors, renderers). Describes what the extension does: analyze source code. |
| `scanners` | Inference pipeline architect, Core refactoring planner | More precise: these extensions scan files. Distinguishes from static analysis tools. |

**Decision: `analyzers`** -- the broader term accommodates future capabilities (dependency extraction, complexity analysis) without renaming. Internally, the scan/classify/map operations are the initial analyzer capabilities.

### ManifestV2 Additions

```rust
// In ContributionFlags / ExtensionContributions
#[serde(default)]
pub analyzers: bool,

// New top-level field on ManifestV2
#[serde(default)]
pub analyzer_contributions: Vec<AnalyzerContribution>,
```

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AnalyzerContribution {
    pub name: String,                        // "rust", "typescript"
    pub language: String,                    // tree-sitter language identifier
    pub file_extensions: Vec<String>,        // [".rs"] or [".ts", ".tsx", ".mts", ".cts"]
    pub scan_export: String,                 // "scan__rust"
    pub classify_export: String,             // "classify__rust"
    pub map_export: String,                  // "map__rust"
    #[serde(default)]
    pub deps_export: Option<String>,         // optional dependency extraction
    #[serde(default)]
    pub exclude_patterns: Vec<String>,       // ["**/target/**", "**/node_modules/**"]
    #[serde(default)]
    pub detect_files: Vec<String>,           // ["Cargo.toml"] or ["package.json"]
    #[serde(default)]
    pub test_patterns: Vec<String>,          // ["/tests/", "_test.rs", "/build.rs"]
    #[serde(default)]
    pub naming_convention: NamingConvention, // SnakeCase, CamelCase, AsIs
}
```

### Protocol Descriptor

```rust
// Returned by __describe("analyzers")
pub struct AnalyzerDescriptor {
    pub name: String,
    pub language: String,
    pub file_extensions: Vec<String>,
    pub scan_export: String,
    pub classify_export: String,
    pub map_export: String,
    pub deps_export: Option<String>,
    pub exclude_patterns: Vec<String>,
    pub detect_files: Vec<String>,
}
```

### SUPPORTED_CATEGORIES Update

The `analyzers` category becomes the 10th category:

```
entities, edges, fields, shared_fields, enhancements, validation_rules,
surfaces, grammars, body_parsers, collectors, passes, feature_flags, analyzers
```

---

## 3. Wasm Protocol Extensions

### 3.1 New Wasm Exports (Three Required, One Optional)

| Export | Input | Output | Frequency | Purpose |
|--------|-------|--------|-----------|---------|
| `scan__{lang}` | `ScanFileRequest` | `ScanFileResponse` | Per source file | Extract all discoverable symbols with signatures, doc comments, spans |
| `classify__{lang}` | `ClassifyFileRequest` | `ClassifyFileResponse` | Per file (path only) | Determine file role: source, test, build, generated, config, etc. |
| `map__{lang}` | `MapSymbolsRequest` | `MapSymbolsResponse` | Per gap analysis | Map source symbols to entity IDs using language conventions |
| `deps__{lang}` (optional) | `ScanFileRequest` | `DepsResponse` | Per file | Extract import/dependency information |

**Why three separate exports:** Different call frequencies and cacheability. Classify is path-only (cheap, called during file discovery). Scan reads content (expensive, called once per file). Map is a pure data transform (batchable, called during gap analysis). Separating them enables independent caching and batching.

### 3.2 Language-Agnostic Core Types

```rust
/// Output of scan__{lang}: a single source item
pub struct SourceItem {
    pub name: String,
    pub kind: SourceItemKind,          // function, struct, class, interface, enum, etc.
    pub visibility: SourceVisibility,  // public, private, crate, protected, internal
    pub span: SourceSpan,              // start_line, start_col, end_line, end_col
    pub signature: Option<String>,     // full type signature
    pub doc_comment: Option<String>,
    pub annotations: Vec<String>,      // attributes/decorators
    pub children: Vec<SourceItem>,     // methods in impl blocks, members in classes
    pub body_hash: Option<String>,     // SHA-256 for incremental change detection
}

pub enum SourceItemKind {
    Function, Method, Struct, Class, Interface, Enum, Trait,
    TypeAlias, Constant, Module, ImplBlock, Macro, Test, Other,
}

pub enum SourceVisibility { Public, Private, Crate, Protected, Internal }

pub struct SourceSpan {
    pub start_line: u32,
    pub start_col: Option<u32>,
    pub end_line: u32,
    pub end_col: Option<u32>,
}

/// Output of classify__{lang}
pub enum FileClassification {
    Source, Test, BuildScript, Generated, Config,
    Migration, Benchmark, Example, Declaration, Story, Unknown,
}

/// Output of map__{lang}: entity-to-symbol mapping
pub struct SymbolMapping {
    pub entity_id: String,
    pub symbol: SymbolRef,             // name + file + span
    pub strategy: MappingStrategy,     // Annotation, NamingConvention, Heuristic
    pub confidence: Option<f64>,       // 0.0-1.0
    pub relation: SymbolRelation,      // Tests, Implements, Defines, References
}
```

### 3.3 New Host Functions

| Host Function | Allowed CallSite | Purpose |
|---------------|-----------------|---------|
| `host_read_source_file` | `Analyzer` only | Read files under project source roots |
| `host_list_files` | `Analyzer` only | Enumerate files matching extension/pattern in source roots |

**Two-zone filesystem sandbox:**
- Zone 1 (`host_read_file`): spec_root -- for validators, parsers, providers
- Zone 2 (`host_read_source_file`): source_roots -- for analyzers only

Source roots configured in `specforge.json`:
```json
{
  "source_roots": ["src", "crates", "tests"]
}
```

If absent, defaults to project root. Security invariants: read-only, no `..` traversal, canonicalization for symlink escape prevention, file size limit (10MB default), file count limit (10,000 default).

### 3.4 Updated Permission Matrix

| Host Function | Validator | Renderer | Provider | Parser | Collector | Analyzer |
|---------------|-----------|----------|----------|--------|-----------|----------|
| `host_emit_diagnostic` | Y | Y | Y | Y | Y | Y |
| `host_read_file` (spec) | Y | - | Y | Y | - | - |
| `host_read_source_file` | - | - | - | - | - | **Y** |
| `host_list_files` | - | - | - | - | - | **Y** |
| `host_emit_file` | - | Y | - | - | Y | - |
| `host_http_get` | - | - | Y | - | - | - |
| `host_query_graph` | Y | Y | Y | Y | Y | Y |

---

## 4. @specforge/rust Design

### 4.1 Extension Identity

| Property | Value |
|----------|-------|
| Name | `@specforge/rust` |
| Entity kinds declared | **0** (pure enhancer) |
| Edge types declared | **0** |
| Peer dependencies | `@specforge/software ^1.0` (required), `@specforge/coverage ^1.0` (optional) |
| Contribution flags | `analyzers: true`, `collectors: true`, `validators: true` |
| Binary distribution | External `.wasm` file (NOT embedded builtin) |

### 4.2 Wasm Exports

| Export | Purpose |
|--------|---------|
| `__handshake` / `__describe(*)` | Standard protocol |
| `scan__rust` | Extract pub items from `.rs` files with full signatures |
| `classify__rust` | Determine if file is source/test/build/bench/example |
| `map__rust` | Convert Rust names to entity IDs (CamelCase to snake_case with acronym handling) |
| `collect__rust` | Parse JUnit XML / libtest JSON / cargo text output |
| `validate__source_anchor_not_test_file` | Custom validation rule |
| `validate__uncovered_pub_items` | Gap analysis diagnostic (I100) |
| `cmd__scan_rust` / `cmd__anchor_rust` / `cmd__infer_rust` / `cmd__collect_rust` | CLI commands |
| `mcp__scan` / `mcp__anchor` / `mcp__infer` / `mcp__gaps` | MCP tools |

### 4.3 Rust-Specific Inference Signal Table

| Rust Construct | Suggested Entity Kind | Confidence |
|---------------|----------------------|------------|
| `pub trait Name` | **port** | Very High |
| `pub trait Name: Send + Sync + 'static` | **port** (outbound) | Very High |
| `pub fn` with `&mut self` / `Result` / I/O | **behavior** | High |
| `pub fn` with `&self` returning owned data | **behavior** (query) | High |
| `pub struct Name { fields }` | **type** | High |
| `pub enum Name` with data variants | **type** | High |
| `impl TraitName for StructName` | behavior link to port | High |
| Struct with `Event` suffix | **event** | Very High |
| Struct implementing `std::error::Error` | **type** (error) | Very High |
| `pub const MAX_*` / `MIN_*` / `TIMEOUT_*` | **invariant** | Medium |
| `#[derive(Serialize, Deserialize)]` on struct | **type** (boosted) | High |
| `proptest!` / `quickcheck!` in tests | links to **invariant** | Medium |

**Negative signals (skip):** `pub(crate)` items, test/bench/example files, `#[doc(hidden)]`, `impl From/Default/Clone/Debug`, items in `mod internal`/`mod detail`.

### 4.4 Entity Enhancements

Adds `source_anchors: string[]` to all 5 `@specforge/software` testable entity kinds (behavior, type, port, invariant, event) via `entity_enhancements`.

### 4.5 RustSourceItem Type

The extension defines a rich Rust-specific `RustSourceItem` with:
- Identity: `name`, `snake_name` (pre-computed), `item_kind` (RustItemKind enum), `module_path`
- Location: `file`, `line`, `end_line`, `column`
- Signature: `signature`, `parameters` (Vec<RustParameter>), `return_type`, `members`, `trait_methods`, `impl_target`, `generics`, `where_clauses`
- Metadata: `visibility` (RustVisibility), `doc_comment`, `attributes`, `is_async`, `is_unsafe`, `is_const`, `derives`
- Inference hints: `suggested_entity_kind`, `suggestion_confidence`, `suggestion_reason`
- Relationships: `referenced_types`, `implements_traits`, `calls`, recursive `children`

### 4.6 Reusability Assessment

| Component | Status | Location |
|-----------|--------|----------|
| `specforge-test-macros` proc macro | **Keep as-is** | `integrations/rust/specforge-test-macros/` |
| `TestGuard`, `TestRegistry`, `BinaryReport` | **Keep as-is** | `integrations/rust/specforge-test/` |
| `slugify_verify_description` | **Keep as-is** | `integrations/rust/specforge-test/` |
| `scan_rust_pub_items` (regex) | **Becomes fallback** | Move to `inference/fallback_rust.rs` |
| `to_snake_case` | **Needs upgrade** | Acronym handling (`HTTPClient` -> `http_client`) |
| JUnit XML parser | **New** | Part of `collect__rust` Wasm export |
| Source anchoring system | **New** | `anchor__rust` Wasm export |
| Inference signal generator | **New** | `infer__rust` Wasm export |

**Overall: ~65% reusable.** The entire `integrations/rust/` crate pair ships independently to crates.io. The Wasm extension module is the new build target.

### 4.7 File Patterns

```json
{
  "file_extensions": [".rs"],
  "exclude_patterns": ["**/target/**"],
  "detect_files": ["Cargo.toml"],
  "test_patterns": ["/tests/", "/test/", "_test.rs", "/build.rs", "/benches/", "/examples/"]
}
```

---

## 5. @specforge/typescript Design

### 5.1 Extension Identity

| Property | Value |
|----------|-------|
| Name | `@specforge/typescript` |
| Entity kinds declared | **0** |
| Edge types declared | **0** |
| Peer dependencies | `@specforge/software ^1.0` (required) |
| Contribution flags | `analyzers: true`, `collectors: true` |

### 5.2 File Extensions and Classification

Handles 7 file extensions: `.ts`, `.tsx`, `.js`, `.jsx`, `.mjs`, `.cjs`, `.d.ts`

File role classification with rich signals:

| Pattern | Classification |
|---------|---------------|
| `*.test.ts`, `*.spec.ts`, `*.e2e.ts` | test |
| `__tests__/**`, `__mocks__/**`, `cypress/**` | test |
| `*.stories.ts`, `*.stories.tsx`, `*.stories.mdx` | story |
| `jest.config.*`, `vitest.config.*`, `playwright.config.*` | config |
| `*.d.ts` | declaration |
| `*.generated.ts`, `*.gen.ts` | generated |
| `scripts/**`, `tools/**`, `bin/**` | script |
| Everything else | production |

### 5.3 TypeScript-Specific Source Items

Extends the generic `SourceItemKind` with TS-specific variants:

| TS Item Kind | Spec Entity Mapping | Confidence |
|-------------|-------------------|------------|
| `export function` | **behavior** | High |
| `export class` | **type** or **behavior** (depends on methods) | Medium |
| `export interface` | **port** (if methods) or **type** (if data shape) | High |
| `export type` alias | **type** | Medium |
| `export enum` | **type** | High |
| `export const` (object with methods) | **behavior** | Medium |
| React component (`function` returning JSX) | Context-dependent | Medium |
| React hook (`use*` prefix) | **behavior** | High |
| Higher-order component | **behavior** | Medium |
| Decorated class (`@Injectable`, `@Controller`) | **behavior** or **port** | High |

### 5.4 Monorepo Awareness

The TypeScript extension must handle monorepo structures:
- Detect `workspaces` in root `package.json`
- Identify per-package `tsconfig.json` boundaries
- Resolve barrel exports (`index.ts` re-exports)
- Track `module.exports` (CJS) alongside `export` (ESM)

### 5.5 Test Collection

Supports Jest, Vitest, Playwright, Cypress, Mocha output formats (JUnit XML and JSON).

### 5.6 Wasm Exports

Same structure as Rust: `scan__typescript`, `classify__typescript`, `map__typescript`, `collect__typescript`, plus CLI commands and MCP tools.

### 5.7 Spec Files Created

The TypeScript expert created comprehensive spec files at `spec/extensions/typescript/`:
- `types.spec` -- TsSourceItem, TsFileRole, TsExportKind, TsItemKind, 23+ types
- `behaviors.spec` -- scan, classify, extract, map, collect, anchor behaviors
- `invariants.spec` -- export completeness, file role accuracy, React detection
- `ports.spec` -- TsSourceScanner port
- `features.spec`, `decisions.spec`, `constraints.spec`, `failure-modes.spec`

---

## 6. Source Anchoring Protocol

### 6.1 Core Types

```rust
pub struct SourceAnchor {
    pub entity_id: String,
    pub entity_kind: String,
    pub locations: Vec<SourceLocation>,    // 1:N (entity to source locations)
    pub provider: String,                  // "@specforge/rust"
    pub source_hashes: Vec<SourceFileHash>, // for staleness detection
}

pub struct SourceLocation {
    pub path: String,          // relative from project root
    pub line: usize,           // 1-based
    pub col: usize,            // 1-based, 0 if unknown
    pub end_line: Option<usize>,
    pub kind: SourceConstructKind,  // function, struct, trait, class, etc.
    pub confidence: f64,       // 0.0-1.0
}
```

**Confidence levels:**
- 1.0 = explicit annotation (`#[specforge::entity("id")]` or `@specforge id` JSDoc tag)
- 0.7-0.9 = naming convention match
- 0.3-0.5 = heuristic/fuzzy match

### 6.2 Storage: Separate Sidecar (`specforge-anchors.json`)

Anchors stored in a dedicated file, NOT in the graph (would pollute spec-level data) and NOT in `specforge-infer.json` (different query pattern -- entity-centric vs file-centric).

Reverse index (file -> entities) built in-memory at load time from the forward index.

### 6.3 Anchor Creation Paths

| Path | Trigger | Confidence |
|------|---------|------------|
| **A: Inference session** | Agent calls `mark_analyzed` with `anchors` parameter | Varies (0.3-0.9) |
| **B: Explicit annotation** | Scanner detects `#[specforge::entity("id")]` | 1.0 |
| **C: Test collection** | `specforge collect` maps tests to entities | 1.0 (for test locations) |

### 6.4 LSP Integration

Three LSP features consume anchors:

1. **Hover enrichment:** Show "Implementations (2)" section with file:line links and confidence %
2. **Go-to-Implementation:** `implementationProvider` jumps from entity ID in `.spec` to source code (distinct from Go-to-Definition which stays in `.spec`)
3. **CodeLens on source files:** Show `spec: behavior authenticate_user` above functions in `.rs`/`.ts` files

### 6.5 MCP Tools

| Tool | Purpose |
|------|---------|
| `specforge.find_implementation` | Entity ID -> source locations |
| `specforge.find_spec_for_source` | Source file/line -> entity IDs (reverse lookup) |

Existing tools enriched: `specforge.find_definition` gains `include_implementation` flag; `specforge.inspect` and `specforge.export` include anchor data when available.

### 6.6 Staleness Detection

Per-anchor content hashes enable targeted staleness checking:
- **I201**: Source anchor points to changed file (info)
- **I203**: Source anchor points to deleted file (warning)

Lifecycle: anchors become stale when source changes, refresh when re-analyzed, orphan when entity removed from spec.

---

## 7. Core Refactoring Plan

### 7.1 Inventory of Hardcoded Language Knowledge

**18 functions/sites across 4 files** with three violation categories:

| Category | Functions | Files |
|----------|-----------|-------|
| **(A) Rust pub-item scanning** | `scan_rust_pub_items`, `parse_rust_pub_item`, `to_snake_case`, `is_test_or_build_file` | `crates/kernel/common/src/inference.rs` |
| **(B) Source extension lists** | `is_source_file` / `is_source_extension` (13 hardcoded extensions) | 4 files (3 duplicate copies) |
| **(C) Excluded directory lists** | `is_excluded_dir` (8 hardcoded dirs) | 4 files (3 duplicate copies) |

### 7.2 Unified Source Discovery

The four duplicate implementations of `discover_source_files` / `walk_source_dir` / `is_source_file` / `is_excluded_dir` collapse into one:

```rust
// crates/kernel/common/src/inference.rs
pub struct SourceDiscoveryConfig {
    pub source_extensions: Vec<String>,  // from analyzer_contributions
    pub excluded_dirs: HashSet<String>,  // from analyzer_contributions + specforge.json
}

pub fn discover_source_files(
    project_root: &Path,
    source_roots: &[String],
    config: &SourceDiscoveryConfig,
) -> Vec<String>;
```

Locations replaced:
- `crates/surfaces/cli/src/infer_status.rs` (lines 129-196)
- `crates/surfaces/mcp/src/tools/infer_progress.rs` (lines 64-131)
- `crates/surfaces/mcp/src/prompts/infer.rs` (lines 349-397)
- `crates/kernel/common/src/inference.rs:compute_gap_report` (inline filtering)

### 7.3 Type Renames

| Old | New | Reason |
|-----|-----|--------|
| `InferenceGap` | `SourceItem` | Represents what scanner found, not the gap itself |
| `InferenceGapReport` | `GapReport` | Shorter, less redundant |
| `total_pub_items` | `total_source_items` | "pub" is Rust-specific |
| New field: `scanner` | on `SourceItem` | Attribution to extension |
| New field: `scanners_used` | on `GapReport` | Which languages analyzed |

### 7.4 Files Modified

| Crate | File | Changes |
|-------|------|---------|
| `specforge-registry` | `manifest/types.rs` | Add `AnalyzerContribution`, `analyzer_contributions`, sandbox policy fields |
| `specforge-wasm` | `protocol/types.rs` | Add `AnalyzerDescriptor`, `analyzers` flag |
| `specforge-wasm` | `protocol/mod.rs` | Add `"analyzers"` to `SUPPORTED_CATEGORIES` |
| `specforge-wasm` | `host_functions.rs` | Add `CallSite::Analyzer`, permission matrix, new host functions |
| `specforge-wasm` | `contributions.rs` | Add `register_analyzer_contributions`, `dispatch_analyzer` |
| `specforge-extism` | `host_context.rs` | Add `project_root`, `source_roots` to `HostContext` |
| `specforge-common` | `inference.rs` | Unified `SourceDiscoveryConfig`, renamed types |
| `specforge-common` | NEW `anchor.rs` | `SourceAnchor`, `AnchorIndex`, `ReverseAnchorIndex` |
| `specforge-mcp` | `state.rs` | Add `anchor_index` field |
| `specforge-mcp` | `tools/` | New `find_implementation`, `find_spec_for_source` tools |
| `specforge-lsp` | `hover.rs` | Implementations section |
| `specforge-lsp` | `backend.rs` | `implementationProvider`, CodeLens on source files |

---

## 8. Inference Pipeline Changes

### 8.1 How Inference Becomes Extension-Driven

**Before:** Core hardcodes `scan_rust_pub_items()` and `is_test_or_build_file()`.

**After:** Core collects `AnalyzerContribution` declarations from installed extensions, dispatches Wasm exports, and merges results.

```
File Discovery (walk source dirs)
  -> file_extensions come from installed analyzers
  -> excluded_dirs come from analyzers + specforge.json
  -> For each file: call classify__{lang} to determine role

Source Scanning (per source file)
  -> call scan__{lang} with file path + content
  -> receive Vec<SourceItem> with full signatures

Gap Analysis (per scan result)
  -> call map__{lang} with source items + known entity IDs
  -> receive SymbolMapping[] with matched/unmatched items
  -> unmatched public items = gaps

Prompt Generation
  -> Gap report fed to MCP infer_gaps tool
  -> plan/workflow prompts adapt based on scanner availability
```

### 8.2 `specforge-infer.json` v2

Gains `scanners` section and per-file scanner provenance:

```json
{
  "version": 2,
  "scanners": {
    "rust": { "extension": "@specforge/rust", "version": "1.0.0", "file_extensions": ["rs"] }
  },
  "source_index": [{
    "path": "src/pipeline.rs",
    "content_hash": "a1b2c3...",
    "scanned_by": "rust",
    "scanned_items": 12,
    "classification": "source"
  }]
}
```

### 8.3 Behavior Without Language Extensions

| Feature | Without analyzer | With analyzer |
|---------|-----------------|---------------|
| `infer_progress` (file counting) | Works with builtin extension list | Uses extension-declared extensions |
| `infer_gaps` (gap analysis) | Returns `unavailable_reason` message | Full AST-powered gap report |
| Prompt workflow | "Read every file" | "Check gaps first, read selectively" |

**No regex fallback when extensions exist.** The hardcoded Rust scanner is removed in Phase 4. Gap analysis without a scanner returns an explicit "install a scanner extension" message rather than misleading approximate data.

---

## 9. DX and Authoring Experience

### 9.1 Extension Author Effort

| Approach | Lines of Code | Compile Time (first) | Binary Size |
|----------|---------------|---------------------|-------------|
| Rust PDK + tree-sitter | 600-800 | 2-5 min | 800KB-1.2MB |
| Rust PDK + regex only | 300-400 | 30-60s | ~250KB |
| TypeScript PDK (QuickJS) | 200-300 | 5-10s | 1-2MB |

### 9.2 SDK Recommendation: `specforge-lang-sdk`

A dedicated SDK crate (`specforge-lang-sdk`) providing:

1. **Shared types**: `SourceItem`, `SourceItemKind`, `SourceVisibility`, `SourceSpan`, `FileClassification`, `SymbolMapping`
2. **`LanguageScanner` trait**: `file_extensions()`, `scan_file()`, `classify_file()` -- testable without Wasm
3. **Wasm boilerplate**: Macro to wire `LanguageScanner` impl to Extism exports
4. **Test harness**: Test scanning logic natively, no Wasm compilation needed

```rust
// Extension author writes only:
impl LanguageScanner for RustScanner {
    fn file_extensions(&self) -> &[&str] { &[".rs"] }
    fn scan_file(&self, content: &str, path: &str) -> Vec<SourceItem> { /* ... */ }
    fn classify_file(&self, path: &str, content: &str) -> FileClassification { /* ... */ }
}
```

### 9.3 The Tree-Sitter DX Problem

**The single largest DX barrier:** Compiling tree-sitter grammars (C code) to `wasm32-unknown-unknown` requires `wasi-sdk` or `emscripten`. This is not `cargo build` out of the box.

**Three mitigation strategies:**

| Strategy | Pros | Cons |
|----------|------|------|
| **A: Extension embeds tree-sitter** | Self-contained, no host dependency | Requires C cross-compilation, large binary |
| **B: Host provides tree-sitter, extension sends queries** | Small extensions, shared parser | Complex host API, version coupling |
| **C: Regex-only scanning** | Zero C toolchain, fast builds, ~250KB | Misses edge cases (macros, nested impls) |

**Recommendation:** Start with **Strategy C** (regex) for v1, plan for **Strategy A** (embedded tree-sitter) for v2 once the protocol stabilizes. Strategy B has too much coupling risk.

### 9.4 Testing Strategy

| Level | What | How |
|-------|------|-----|
| **Unit** | `LanguageScanner` trait methods | Native `#[test]`, no Wasm, no C toolchain |
| **Integration** | Wasm binary directly | `extism call plugin.wasm scan__rust --input '{...}'` |
| **E2E** | Full `specforge check` with extension | Similar to `crates/surfaces/cli/tests/extensions.rs` |

**Critical principle:** Scan logic MUST be testable without Wasm compilation. The SDK ensures this by making the scanner a regular Rust trait impl.

### 9.5 Binary Size Budget

| Extension | Current Size |
|-----------|-------------|
| `@specforge/software` (builtin) | 209 KB |
| `@specforge/product` (builtin) | 237 KB |
| `@specforge/rust` (estimated, regex) | ~250 KB |
| `@specforge/rust` (estimated, tree-sitter) | 800KB-1.2MB |
| `@specforge/typescript` (estimated, regex) | ~250 KB |

**Decision:** Language extensions are NOT embedded as builtins. They are external `.wasm` files loaded via `ExtismRuntime`, installed via `specforge add @specforge/rust`.

---

## 10. Market Context

### 10.1 Competitive Landscape

| Tool | What It Does | How SpecForge Differs |
|------|-------------|----------------------|
| **LSIF** (Microsoft) | Language Server Index Format -- offline code intelligence index | LSIF indexes code for navigation; SpecForge indexes code-to-spec traceability. Complementary, not competing. |
| **SCIP** (Sourcegraph) | Source Code Intelligence Protocol -- precise cross-repo code navigation | SCIP provides symbol resolution; SpecForge maps symbols to spec entities. Could consume SCIP data. |
| **Stack Graphs** (GitHub) | Language-agnostic name binding for code navigation | Similar tree-sitter-based approach. SpecForge's extension model parallels stack-graphs' per-language rules. |
| **rust-analyzer** | Rust IDE intelligence (semantic analysis) | Deep Rust understanding but no spec traceability. SpecForge could leverage its APIs. |
| **cargo-public-api** | Extract Rust public API surface | Similar to `scan__rust` but without spec mapping. Validates our approach. |
| **API Extractor** (Microsoft) | TypeScript public API extraction + reporting | Similar to `scan__typescript`. Prior art for our TS extension design. |
| **TypeDoc** | TypeScript documentation generation from source | Extracts the same data SpecForge needs. Potential data source. |
| **ast-grep** | AST-based code search using tree-sitter | Pattern language could inform SpecForge's query model. |
| **Semgrep** | AST-based static analysis with patterns | Different goal (security/bugs vs spec traceability) but shared technique. |
| **Biome/oxc** | Fast JS/TS toolchain (linting, formatting) | Performance benchmark for TS parsing. |

### 10.2 Tree-Sitter Ecosystem

- **tree-sitter-rust**: Grammar is ~800KB (parser.c), ~300 node types, well-maintained
- **tree-sitter-typescript**: Two grammars (TypeScript + TSX), ~1.2MB combined parser.c
- **Wasm feasibility**: tree-sitter has native Wasm support (`tree-sitter-cli` generates `.wasm` grammar files). Binary size overhead: ~200-400KB for the runtime

### 10.3 Unique Positioning

SpecForge's language intelligence extensions occupy a unique niche: **bidirectional spec-to-code traceability** powered by structured entity mapping. No existing tool provides:
1. Spec-entity-aware public API extraction
2. Three-tier entity mapping (annotation > naming convention > heuristic)
3. Graph-integrated source anchoring with confidence scores
4. Agent-optimized gap reports that guide spec authoring

---

## 11. Migration Path

### Phase 1: Deduplicate and Isolate (No Behavior Change)

| Step | Action | Files |
|------|--------|-------|
| 1a | Add `SourceDiscoveryConfig` struct | `crates/kernel/common/src/inference.rs` |
| 1b | Create `SourceDiscoveryConfig::hardcoded_defaults()` | Same |
| 1c | Move shared `discover_source_files` to specforge-common | Same |
| 1d | Delete 3 duplicate copies | `infer_status.rs`, `infer_progress.rs`, `prompts/infer.rs` |
| 1e | Move Rust scanner to `inference/fallback_rust.rs` with `#[deprecated]` | New submodule |
| 1f | Stop re-exporting `scan_rust_pub_items` | `crates/kernel/common/src/lib.rs` |

### Phase 2: Extension Scanner Protocol

| Step | Action |
|------|--------|
| 2a | Add `AnalyzerContribution` to `ManifestV2` |
| 2b | Add `analyzers` to `ContributionFlags` |
| 2c | Implement `SourceDiscoveryConfig::from_extensions()` |
| 2d | Rename `InferenceGap` -> `SourceItem`, `InferenceGapReport` -> `GapReport` |
| 2e | Refactor `compute_gap_report` to accept scanner contributions |
| 2f | Add `host_read_source_file` and `host_list_files` host functions |
| 2g | Add `CallSite::Analyzer` with permission matrix |
| 2h | Log deprecation diagnostic (I203) when using Rust fallback |
| 2i | Remove `.ends_with(".rs")` from `compute_inference_diagnostics` |

### Phase 3: Ship @specforge/rust

| Step | Action |
|------|--------|
| 3a | Create `@specforge/rust` extension with analyzer manifest |
| 3b | Implement `scan__rust`, `classify__rust`, `map__rust` exports |
| 3c | Implement `collect__rust` with JUnit XML + libtest JSON parsing |
| 3d | Add source anchoring (`anchor__rust`) |
| 3e | Integration test: Wasm scanner vs fallback produces same results |

### Phase 4: Remove Fallback

| Step | Action |
|------|--------|
| 4a | Delete `crates/kernel/common/src/inference/fallback_rust.rs` |
| 4b | Remove `hardcoded_defaults()` from `SourceDiscoveryConfig` |
| 4c | Remove I203 deprecation diagnostic |
| 4d | Grep codebase for `.rs"`, `"rust"`, `"Rust"` in inference paths -- zero hits required |
| 4e | Tests requiring Rust source scanning now need `@specforge/rust` fixture |

---

## 12. Open Questions

### 12.1 Naming: `analyzers` vs `scanners`

Two experts preferred `scanners` (more precise), two preferred `analyzers` (more extensible). The synthesis recommends `analyzers` but this should be confirmed before implementation. The contribution flag name propagates to `SUPPORTED_CATEGORIES`, `__describe` categories, `CallSite` enum, and host function permissions.

### 12.2 Tree-Sitter Strategy: Embedded vs Host-Provided

No consensus reached due to tree-sitter expert being interrupted. Three options remain:

| Option | Status |
|--------|--------|
| Extension embeds tree-sitter (Strategy A) | Feasible but DX-heavy (C cross-compilation) |
| Host provides tree-sitter runtime, extension sends `.scm` queries (Strategy B) | Architecturally cleaner but creates coupling |
| Regex-only for v1 (Strategy C) | Recommended for initial release |

**Recommended approach:** Ship v1 with regex scanning (Strategy C). Design the protocol to allow Strategy A as a non-breaking enhancement. Do NOT implement Strategy B (host-provided tree-sitter) as it creates tight coupling between host versions and extension queries.

### 12.3 Source Anchoring: Sidecar File vs Graph Metadata

The source anchoring expert strongly advocated for a separate `specforge-anchors.json` sidecar. Counter-arguments exist:
- Pro sidecar: Different update cadence from graph, avoids polluting spec-level data
- Pro graph metadata: Single source of truth, simpler mental model, already queryable

**Recommendation:** Sidecar file, per the expert's detailed analysis. The graph is the spec-level typed graph; source anchors are implementation-level metadata with different lifecycle.

### 12.4 Analyzer Interaction with Collectors

Clear consensus: analyzers and collectors are complementary, not overlapping.
- **Analyzers** read source code, produce symbol tables and entity mappings
- **Collectors** read test artifacts, produce entity-to-test-result mappings
- Analyzer output can enrich collector accuracy (symbol mappings inform test-to-entity attribution)

The pipeline is: Analyze -> Collect -> Trace.

### 12.5 What Happens to `providers: true` for Language Extensions?

The DX expert's manifest used `providers: true` for the scanner capability (pre-dating the `analyzers` flag). Once `analyzers` is added, language extensions should use `analyzers: true` exclusively. The `providers` flag remains for data-sourcing extensions (e.g., fetching external APIs).

### 12.6 `specforge analyze` CLI Command

Several experts referenced a new top-level command. Design not finalized:
- `specforge analyze` -- discover analyzers, walk source roots, dispatch scan/classify/map
- Should it write to `specforge-infer.json` directly, or to a separate `specforge-analysis.json`?
- Should it run automatically as part of `specforge check`, or be a separate manual step?

**Recommendation:** Separate command (`specforge analyze`), writes to `specforge-infer.json` (extends it with scanner provenance), does NOT auto-run in `specforge check` (would slow down the fast feedback loop).

### 12.7 Multi-Language Projects

Projects with both Rust and TypeScript (e.g., Tauri apps) will have multiple analyzers installed. Questions:
- How are file conflicts resolved (none expected -- extensions claim disjoint extensions)?
- How does gap analysis aggregate across languages?
- Should `specforge analyze` report per-language or unified?

**Recommendation:** Unified gap report with `scanner` field per item. Extensions claim disjoint file extensions. If two extensions claim the same extension, first-installed wins (with a warning).

---

## References

- **RES-17:** `spec/research/RES-17-specforge-rust-plugin-design.md` -- Original Rust plugin design (test collection focus)
- **RES-26:** `spec/research/RES-26-zero-entity-core-architecture.md` -- Zero-entity core architecture
- **RES-27:** `spec/research/RES-27-software-eng-entity-redesign.md` -- Software entity validation
- **Vision:** `docs/vision/README.md`, `docs/vision/principles.md`, `docs/vision/north-star.md`
- **Manifest v2:** `spec/types/zero-entity-core.spec`
- **TypeScript extension specs:** `spec/extensions/typescript/*.spec` (created by TS expert)
