# RES-30: Tree-Sitter in Wasm Extensions — Technical Feasibility Study

**Date:** 2026-04-24
**Status:** Research Complete
**Scope:** Can SpecForge Wasm extensions (Extism plugins) use tree-sitter internally for language analysis?

---

## Executive Summary

There are **three viable architectures** for giving SpecForge extensions tree-sitter-powered language analysis. After researching binary sizes, compilation constraints, and the tree-sitter Wasm ecosystem, the **recommended approach is Architecture B: Host-Provided Parsing** — the host embeds tree-sitter with the `wasm` feature, loads grammar `.wasm` files provided by extensions, parses source files, and exposes a query API to extensions. Extensions provide `.scm` query files and grammar `.wasm` binaries, but never embed tree-sitter themselves.

### Key Findings

1. **Embedding tree-sitter inside an Extism plugin is impractical.** The tree-sitter Rust crate has known compilation issues targeting `wasm32-unknown-unknown` (issues #4336, #5205, #5530). The C core can compile to Wasm via Emscripten/wasi-sdk, but not through the Rust `cc` crate targeting `wasm32-unknown-unknown` without significant effort. Even if it worked, binary sizes would be prohibitive (see Q3 below).

2. **Tree-sitter already has a grammar-as-Wasm architecture.** The `wasm` feature (using wasmtime) lets the host load grammar `.wasm` files dynamically via `WasmStore::load_language()`. This is exactly what Zed uses for language extensions.

3. **Grammar `.wasm` files are small.** Measured from the `tree-sitter-wasms` npm package and local builds:
   - SpecForge grammar: **15 KB**
   - Rust grammar: **819 KB**
   - TypeScript grammar: **2.3 MB**
   - TSX grammar: **2.4 MB**
   - JSON grammar: **6 KB**
   - JavaScript grammar: **647 KB**
   - Python grammar: **476 KB**
   - C grammar: **793 KB**
   - C++ grammar: **4.7 MB**

4. **The web-tree-sitter runtime (tree-sitter compiled to Wasm via Emscripten) is ~197 KB.** But this path is for browsers, not for Extism plugins.

---

## Q1: Can a Wasm Extension Embed Tree-Sitter Internally?

### Short Answer: No, not practically.

### Technical Details

**The compilation chain breaks.** SpecForge extensions compile to `wasm32-unknown-unknown` via `cargo build --target wasm32-unknown-unknown` using `extism-pdk`. The tree-sitter Rust crate depends on:

1. A C compilation step via the `cc` crate (for `parser.c`, `alloc.c`, `lexer.c`, etc.)
2. `bindgen` for FFI bindings
3. Optionally `wasmtime-c-api-impl` for the `wasm` feature

**Problem 1: `cc` crate cross-compilation to wasm32.**
The `cc` crate needs a C compiler targeting `wasm32-unknown-unknown`. This requires either Emscripten or `wasi-sdk` — neither is part of the standard Rust toolchain. The tree-sitter CLI uses `wasi-sdk` for this purpose (confirmed in `crates/xtask/src/build_wasm.rs`), but this is a custom build step, not something `cargo build` does automatically.

**Problem 2: Known bugs.**
- Issue #4336: Building tree-sitter for wasm32 fails due to cranelift-codegen dependency (when `wasm` feature is enabled).
- Issue #5205: malloc implementation for wasm32 target was buggy (fixed in #5261, but indicates fragility).
- Issue #5530 (open, April 2026): Runtime panic with "indirect call to null" in the Wasm allocator — `heap_start` initialized to NULL corrupts memory at address 0.

**Problem 3: The `wasm` feature is backwards.**
The `wasm` feature in tree-sitter is NOT "compile tree-sitter to Wasm." It means "tree-sitter can load grammars FROM Wasm" — it brings in `wasmtime-c-api-impl`, which itself cannot run inside Wasm (wasmtime is a native Wasm runtime).

**Even if it compiled**, the binary size would be enormous: tree-sitter runtime (~200 KB) + grammar C code (see Q3) + Rust Wasm overhead + extism-pdk overhead. A single extension could easily exceed 3-5 MB.

### Current extension baseline

The `@specforge/software` extension (JSON metadata only, no tree-sitter) is **214 KB** as a Wasm binary. Adding tree-sitter + a grammar would 5-15x that size.

---

## Q2: Should the Host Provide Tree-Sitter Parsing?

### Short Answer: Yes. This is the recommended architecture.

### Architecture B: Host-Provided Parsing

```
+------------------+     +------------------+     +------------------+
|  Extension .wasm |     |   Grammar .wasm  |     |   .scm Queries   |
|  (Extism plugin) |     |  (tree-sitter    |     |  (text files in  |
|                  |     |   compiled via    |     |   extension pkg) |
|  Calls host API: |     |   wasi-sdk)      |     |                  |
|  host_parse_file |     |                  |     |                  |
|  host_run_query  |     |                  |     |                  |
+--------+---------+     +--------+---------+     +--------+---------+
         |                        |                        |
         v                        v                        v
+------------------------------------------------------------------------+
|                        SpecForge Host                                   |
|                                                                         |
|  tree-sitter (native, with `wasm` feature)                             |
|  WasmStore::load_language("rust", grammar_bytes) -> Language            |
|  Parser::set_language(language)                                         |
|  Parser::parse(source_code) -> Tree                                     |
|  Query::new(language, scm_pattern) -> Query                            |
|  QueryCursor::matches(query, tree.root_node()) -> matches              |
|                                                                         |
|  Host functions exposed to extensions:                                  |
|    host_parse_file(path) -> tree_handle                                |
|    host_run_query(tree_handle, scm_pattern) -> JSON matches            |
|    host_get_node_text(tree_handle, node_id) -> text                    |
+------------------------------------------------------------------------+
```

### How It Works

1. **Extension declares grammar dependency** in its manifest:
   ```json
   {
     "grammar_contributions": [
       { "language": "rust", "grammar_wasm": "grammars/tree-sitter-rust.wasm" }
     ]
   }
   ```

2. **Host loads grammar** using tree-sitter's `wasm` feature:
   ```rust
   // Host code (native, not in Wasm)
   let engine = wasmtime::Engine::default();
   let mut store = WasmStore::new(&engine)?;
   let language = store.load_language("rust", &grammar_wasm_bytes)?;
   ```

3. **Extension calls host functions** to parse and query:
   ```rust
   // Extension code (Extism plugin)
   let tree = host_parse_file("src/main.rs", "rust")?;
   let matches = host_run_query(tree, "(function_item name: (identifier) @name) @def")?;
   ```

4. **Host returns structured JSON** with matched nodes, their text, positions, etc.

### Advantages

- **Extensions stay small** (~200 KB, no tree-sitter embedded)
- **Grammar .wasm files are shared** across extensions (one Rust grammar serves all extensions that need Rust parsing)
- **Host manages memory** — no per-extension tree-sitter instances, efficient tree reuse
- **Grammar updates are independent** — update a grammar .wasm without recompiling extensions
- **Proven architecture** — this is exactly what Zed does (grammar .wasm + .scm queries in extension packages)
- **Tree-sitter's `wasm` feature was designed for this** — `WasmStore::load_language()` is the intended API

### New Host Functions Required

| Function | Signature | Description |
|----------|-----------|-------------|
| `host_parse_file` | `(path: string, language: string) -> tree_handle` | Parse a file using a loaded grammar |
| `host_run_query` | `(tree: handle, scm: string) -> JSON` | Run a tree-sitter query, return matches as JSON |
| `host_get_node_text` | `(tree: handle, start: u32, end: u32) -> string` | Extract text for a matched node range |
| `host_walk_tree` | `(tree: handle, depth: u32) -> JSON` | Walk the syntax tree to a given depth |

### Call Site Restrictions

These should be available from `Collector` and `Parser` call sites (extensions that analyze source code).

---

## Q3: Realistic Binary Sizes

### Grammar .wasm files (standalone, compiled by wasi-sdk via `tree-sitter build --wasm`)

| Grammar | .wasm Size | parser.c Size | Notes |
|---------|-----------|--------------|-------|
| SpecForge | **15 KB** | 69 KB (2,600 lines) | Simple keyword grammar |
| JSON | **6 KB** | ~10 KB | Tiny grammar |
| HTML | **19 KB** | ~50 KB | Simple |
| CSS | **98 KB** | ~300 KB | Moderate |
| Rust | **819 KB** | 6.2 MB | Complex, has scanner.c (12 KB) |
| JavaScript | **647 KB** | ~4 MB | Complex |
| Python | **476 KB** | ~3 MB | Moderate |
| TypeScript | **2.3 MB** | 8.34 MB | Very complex (GitHub can't render it) |
| TSX | **2.4 MB** | ~8.5 MB | TypeScript + JSX |
| Java | **430 KB** | ~3 MB | Moderate |
| Go | **236 KB** | ~1.5 MB | Moderate |
| C | **793 KB** | ~5 MB | Complex |
| C++ | **4.7 MB** | ~30 MB | Extremely complex |
| C# | **4.0 MB** | ~25 MB | Very complex |
| Kotlin | **4.1 MB** | ~25 MB | Very complex |
| Swift | **3.1 MB** | ~20 MB | Complex |

### Hypothetical: Tree-Sitter Embedded in Extension Wasm

If tree-sitter's C core (~200 KB compiled) were embedded alongside a grammar:

| Extension Content | Estimated .wasm Size |
|-------------------|---------------------|
| extism-pdk + serde + metadata only | **~200 KB** (current baseline) |
| + tree-sitter runtime | **~400-500 KB** |
| + tree-sitter-rust grammar | **~1.2-1.5 MB** |
| + tree-sitter-typescript grammar | **~2.5-3.0 MB** |
| + both grammars | **~3.5-4.5 MB** |

### Architecture B (Recommended): Extension + Separate Grammar Files

| Component | Size |
|-----------|------|
| Extension .wasm (logic only) | **~200-300 KB** |
| Grammar .wasm (Rust) | **819 KB** |
| Grammar .wasm (TypeScript) | **2.3 MB** |
| .scm query files | **~2-5 KB each** |
| **Total for Rust extension** | **~1.0-1.1 MB** |
| **Total for TypeScript extension** | **~2.5-2.6 MB** |

---

## Q4: Sharing Tree-Sitter Across Extensions

### The Problem

If three extensions all need to parse Rust files (e.g., `@specforge/rust`, `@specforge/coverage`, `@specforge/refactoring`), should each bundle its own grammar?

### Solution: Host Grammar Registry

```
specforge.json
{
  "extensions": ["@specforge/rust", "@specforge/coverage"],
  "grammars": {
    "rust": "@specforge/rust"  // provided by this extension
  }
}
```

The host maintains a **grammar registry**:

1. Each extension declares what grammars it contributes (via `grammar_contributions` in manifest)
2. The host loads each unique grammar exactly once into `WasmStore`
3. All extensions that request "rust" parsing get the same loaded `Language`
4. Parsed trees are cached by (file_path, content_hash, language) — shared across extensions

### Extension Manifest Integration

This already fits the existing `grammar_contributions` field in ManifestV2:

```json
{
  "grammar_contributions": [
    {
      "language": "rust",
      "file_extensions": [".rs"],
      "grammar_wasm": "grammars/tree-sitter-rust.wasm"
    }
  ]
}
```

Other extensions that need Rust parsing declare a dependency:

```json
{
  "peer_dependencies": { "@specforge/rust": ">=1.0" },
  "uses_grammars": ["rust"]
}
```

### Tree Cache Architecture

```
Host Grammar Registry
  |
  +-- "rust" -> Language (loaded from tree-sitter-rust.wasm)
  +-- "typescript" -> Language (loaded from tree-sitter-typescript.wasm)
  +-- "python" -> Language (loaded from tree-sitter-python.wasm)
  |
  +-- Parse Cache (LRU, keyed by file_path + content_hash + language)
       +-- "src/main.rs" + hash_abc -> Tree (shared across extensions)
       +-- "src/lib.rs" + hash_def -> Tree
```

---

## Q5: Tree-Sitter Queries for Public API Extraction

### Rust: Public API Items (.scm)

```scheme
;; Public functions
(function_item
  (visibility_modifier) @vis
  name: (identifier) @name
  parameters: (parameters) @params
  return_type: (_)? @return_type
) @definition.function

;; Public structs
(struct_item
  (visibility_modifier) @vis
  name: (type_identifier) @name
  body: (field_declaration_list)? @fields
) @definition.struct

;; Public enums
(enum_item
  (visibility_modifier) @vis
  name: (type_identifier) @name
  body: (enum_variant_list) @variants
) @definition.enum

;; Public traits
(trait_item
  (visibility_modifier) @vis
  name: (type_identifier) @name
  body: (declaration_list) @body
) @definition.trait

;; Public type aliases
(type_item
  (visibility_modifier) @vis
  name: (type_identifier) @name
) @definition.type_alias

;; Impl blocks (methods)
(impl_item
  type: (type_identifier) @impl_type
  body: (declaration_list
    (function_item
      (visibility_modifier) @vis
      name: (identifier) @name
      parameters: (parameters) @params
      return_type: (_)? @return_type
    ) @definition.method
  )
)

;; Public constants
(const_item
  (visibility_modifier) @vis
  name: (identifier) @name
  type: (_) @const_type
) @definition.constant

;; Public static items
(static_item
  (visibility_modifier) @vis
  name: (identifier) @name
  type: (_) @static_type
) @definition.static

;; Module declarations
(mod_item
  (visibility_modifier) @vis
  name: (identifier) @name
) @definition.module

;; Use declarations (re-exports)
(use_declaration
  (visibility_modifier) @vis
  argument: (_) @path
) @definition.reexport

;; Macro definitions
(macro_definition
  name: (identifier) @name
) @definition.macro
```

### TypeScript: Public API Items (.scm)

```scheme
;; Exported function declarations
(export_statement
  declaration: (function_declaration
    name: (identifier) @name
    parameters: (formal_parameters) @params
    return_type: (type_annotation)? @return_type
  )
) @definition.function

;; Exported class declarations
(export_statement
  declaration: (class_declaration
    name: (type_identifier) @name
    body: (class_body) @body
  )
) @definition.class

;; Exported interface declarations
(export_statement
  declaration: (interface_declaration
    name: (type_identifier) @name
    body: (interface_body) @body
  )
) @definition.interface

;; Exported type aliases
(export_statement
  declaration: (type_alias_declaration
    name: (type_identifier) @name
    value: (_) @type_value
  )
) @definition.type_alias

;; Exported enum declarations
(export_statement
  declaration: (enum_declaration
    name: (identifier) @name
    body: (enum_body) @variants
  )
) @definition.enum

;; Exported variable declarations (const)
(export_statement
  declaration: (lexical_declaration
    (variable_declarator
      name: (identifier) @name
      type: (type_annotation)? @var_type
      value: (_)? @value
    )
  )
) @definition.variable

;; Named exports (re-exports)
(export_statement
  (export_clause
    (export_specifier
      name: (identifier) @local_name
      alias: (identifier)? @export_name
    )
  )
  source: (string)? @source_module
) @definition.reexport

;; Default exports
(export_statement
  "default" @default_marker
  (_) @exported_value
) @definition.default_export

;; Module declarations (TypeScript namespaces)
(export_statement
  declaration: (module
    name: (identifier) @name
    body: (statement_block) @body
  )
) @definition.namespace

;; Abstract class declarations
(export_statement
  declaration: (abstract_class_declaration
    name: (type_identifier) @name
    body: (class_body) @body
  )
) @definition.abstract_class

;; Method signatures in interfaces/classes (for detailed extraction)
(method_signature
  name: (property_identifier) @name
  parameters: (formal_parameters) @params
  return_type: (type_annotation)? @return_type
) @definition.method

;; Property signatures in interfaces
(property_signature
  name: (property_identifier) @name
  type: (type_annotation)? @prop_type
) @definition.property
```

---

## Architecture Comparison

| Criterion | A: Extension Embeds TS | B: Host Provides TS | C: Extension Provides Queries Only |
|-----------|----------------------|--------------------|------------------------------------|
| Extension .wasm size | 1-5 MB | 200-300 KB | 200-300 KB |
| Grammar sharing | No (duplicated) | Yes (host registry) | Yes (host registry) |
| Tree caching | No (per-extension) | Yes (host cache) | Yes (host cache) |
| Compilation feasibility | Blocked by issues | Native (proven) | Native (proven) |
| Extension complexity | High (C FFI in Wasm) | Low (host API calls) | Lowest (just .scm files) |
| Query flexibility | Full programmatic API | Host API + raw queries | .scm files only |
| Grammar updates | Recompile extension | Replace .wasm file | Replace .wasm file |
| Host complexity | None | Medium (new host fns) | Medium (new host fns) |
| Precedent | None known | Zed editor, tree-sitter `wasm` feature | Zed editor |

**Architecture C** is a simplified variant of B where extensions cannot run queries programmatically — they just declare static `.scm` files that the host evaluates. This is simpler but less flexible. The host would need to define what queries to run and how to interpret results.

**Architecture B is recommended** because it gives extensions programmatic control over what they parse and query, while keeping tree-sitter in the host where it belongs.

---

## Implementation Plan

### Phase 1: Host Grammar Infrastructure

1. Add `tree-sitter` with `wasm` feature to `specforge-extism` Cargo.toml
2. Implement `GrammarRegistry` that maps language names to loaded `Language` instances
3. Implement `ParseCache` (LRU cache keyed by file path + content hash + language)
4. Add `validate_grammar_wasm` call during extension loading (already exists in lifecycle.rs)

### Phase 2: Host Functions

5. Implement `host_parse_file(path, language) -> tree_handle`
6. Implement `host_run_query(tree_handle, scm_pattern) -> JSON`
7. Implement `host_get_node_text(tree_handle, byte_start, byte_end) -> string`
8. Add call-site restrictions (Collector, Parser only)
9. Add these to the permission matrix in `host_functions.rs`

### Phase 3: Extension SDK

10. Add query helpers to `extism-pdk` extension SDK
11. Create `@specforge/rust` extension with:
    - `grammars/tree-sitter-rust.wasm` (819 KB)
    - `queries/public-api.scm`
    - Collector logic that calls `host_parse_file` + `host_run_query`
12. Create `@specforge/typescript` extension similarly

### Dependency Addition

```toml
# In specforge-extism/Cargo.toml or crates/runtime/wasm/Cargo.toml
[dependencies]
tree-sitter = { version = "0.26.8", features = ["wasm"] }
```

Note: This brings in `wasmtime-c-api-impl` (~36.0.7) as a dependency of the HOST binary. The host already uses Extism (which uses wasmtime), so there may be version alignment concerns. Extism 1.x uses wasmtime internally — check compatibility.

### Size Impact on Host Binary

Adding tree-sitter with the `wasm` feature to the host should add roughly 200-400 KB to the `specforge` CLI binary (tree-sitter C core + Rust bindings). The wasmtime dependency is already present via Extism.

---

## Risks and Mitigations

| Risk | Severity | Mitigation |
|------|----------|------------|
| Wasmtime version conflict (Extism vs tree-sitter) | High | Pin compatible versions; both use wasmtime, need same major version |
| Grammar .wasm ABI mismatch | Medium | `validate_grammar_wasm` already checks ABI version (E037 diagnostic) |
| Large grammar files bloating extension packages | Low | Grammar .wasm files are separate from extension .wasm; lazy download |
| Parse cache memory pressure | Low | LRU eviction; configurable cache size |
| tree-sitter `wasm` feature stability | Medium | Feature has been stable since 0.24; used by Zed in production |

---

## Conclusion

**Do not embed tree-sitter inside Wasm extensions.** The compilation path is broken, the sizes are prohibitive, and it duplicates work across extensions.

**Use the host-provided parsing model (Architecture B).** The host natively embeds tree-sitter with its `wasm` feature, loads grammar `.wasm` files from extensions, and exposes `host_parse_file` / `host_run_query` functions. Extensions stay small (~200 KB), grammars are shared, trees are cached, and the architecture matches the proven Zed editor model.

Grammar `.wasm` files are surprisingly small for most languages (Rust = 819 KB, Python = 476 KB, Go = 236 KB). Only C++ (4.7 MB) and TypeScript (2.3 MB) are notably large, and these are compiled once by `tree-sitter build --wasm` using wasi-sdk.
