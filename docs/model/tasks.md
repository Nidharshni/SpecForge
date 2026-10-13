# `specforge model` Implementation Tasks

## Phase 1: Core IR and Builder

### Task 1.1: Define ModelIntermediate types
**Crate:** `specforge-emitter`
**File:** `src/model/mod.rs`

Create the `model` module with the core types:
- `ModelIntermediate`, `ModelEntity`, `ModelField`, `ModelRelationship`, `ModelExtension`
- `Cardinality`, `ModelFormat`, `GroupBy`, `FieldLevel` (a field's type is the protocol's `FieldType`, ADR 0034)
- `ModelOptions` struct
- Derive `Debug`, `Clone`, `Serialize`, `Deserialize` on all types
- `impl Display for Cardinality` producing `"1:1"`, `"1:N"`, `"N:1"`, `"N:M"`

**Tests:**
- Cardinality display strings
- ModelOptions default values

### Task 1.2: Build ModelIntermediate from GraphProtocolSchema
**Crate:** `specforge-emitter`
**File:** `src/model/mod.rs` (or `src/model/build.rs`)

Implement `ModelIntermediate::from_schema(schema: &GraphProtocolSchema) -> ModelIntermediate`:
- Map each `SchemaEntityKind` to a `ModelEntity`
- Prepend synthetic `id` field (string, required, primary_key=true) to each entity
- Map each `SchemaField` to a `ModelField`, deriving `references` from `target_kind` metadata
- Map each `SchemaEdgeType` to a `ModelRelationship`
- Compute `ModelExtension` metadata (entity_count, edge_count per extension)

**Tests:**
- Empty schema -> empty IR
- Single entity kind -> one ModelEntity with id field
- Entity with fields -> correct field mapping
- Edge type -> ModelRelationship with label, source, target
- Extension metadata counts are accurate
- Synthetic id is always first field

### Task 1.3: Cardinality inference
**Crate:** `specforge-emitter`
**File:** `src/model/cardinality.rs`

Implement cardinality inference:
- For each edge type, find the source entity and matching field (where `field.edge == edge_label`)
- `reference` field type -> `ManyToOne`
- `reference_list` field type -> `ManyToMany`
- No matching field -> `ManyToMany` (safe default)

Note: `SchemaField` currently has `field_type: String` and no `edge` field. The cardinality module needs to cross-reference with the `ManifestField` data (which has `edge` and `target_kind`). Two approaches:
1. Enrich `SchemaField` with `edge` and `target_kind` at schema generation time
2. Pass the `FieldRegistry` alongside the schema to the builder

Approach 1 is cleaner — add optional `edge` and `target_kind` fields to `SchemaField`.

**Tests:**
- reference field -> ManyToOne
- reference_list field -> ManyToMany
- No matching field -> ManyToMany default
- Edge with no source_kinds -> ManyToMany default
- Multiple edge types between same entities

### Task 1.4: Filter the ModelIntermediate
**Crate:** `specforge-emitter`
**File:** `src/model/mod.rs` (or `src/model/filter.rs`)

Implement `ModelIntermediate::filter(&self, options: &ModelOptions) -> ModelIntermediate`:
- `extension_filter`: keep only entities where `entity.extension == filter`
- `kind_filter`: keep only entities where `entity.name` is in the list
- `root` + `depth`: build kind-level adjacency graph from relationships, BFS from root to depth N, keep only reachable kinds
- Prune relationships where source or target was filtered out
- Filters compose as intersection

Implement field-level filtering separately:
- `FieldLevel::None` -> empty fields vec
- `FieldLevel::Keys` -> keep `is_primary_key || required || field_type is Reference/ReferenceList`
- `FieldLevel::All` -> keep all

**Tests:**
- Extension filter keeps only matching entities
- Kind filter with known kinds
- Kind filter with unknown kind silently ignored
- Root+depth=0 keeps only root
- Root+depth=1 keeps root + direct neighbors
- Relationship pruning when endpoint filtered out
- Intersection of extension + kind filter
- Field level none/keys/all

---

## Phase 2: Renderers

### Task 2.1: Markdown renderer
**Crate:** `specforge-emitter`
**File:** `src/model/markdown.rs`

Implement `pub fn render_markdown(model: &ModelIntermediate, options: &ModelOptions) -> String`:
- Framing preamble ("How to read this model" section)
- Extension summary table
- Entity sections grouped by extension (or flat if group_by=none)
- Field tables with columns: Field, Type, Required, Description, Enum Values, Default
- Relationship lists per entity with cardinality
- Summary line at bottom

**Tests (snapshot with insta):**
- Full model at keys level with extension grouping
- Full model at all level
- Full model at none level
- Flat grouping (group_by=none)
- Empty model
- Single entity with no relationships

### Task 2.2: Mermaid renderer
**Crate:** `specforge-emitter`
**File:** `src/model/mermaid.rs`

Implement `pub fn render_mermaid(model: &ModelIntermediate, options: &ModelOptions) -> String`:
- `erDiagram` header
- Entity blocks with field definitions (type, PK/FK markers)
- Relationship lines with cardinality notation: `||--||`, `||--o{`, `}o--||`, `}o--o{`
- Extension grouping via `%% @extension-name` comments

**Tests (snapshot with insta):**
- Full model at keys level
- All four cardinality notations
- Fields=none produces entities without blocks
- Extension comment headers
- Empty model

### Task 2.3: DOT renderer
**Crate:** `specforge-emitter`
**File:** `src/model/dot.rs`

Implement `pub fn render_dot(model: &ModelIntermediate, options: &ModelOptions) -> String`:
- `digraph model { ... }` wrapper with `rankdir=LR`
- HTML-like `<table>` labels per entity
- Header row with extension-specific background color
- Field rows: bold for required, `-> target` for references
- `subgraph cluster_*` for extension grouping
- Edge labels with relationship name and cardinality

Extension color palette:
- `@specforge/software` -> `#4a90d9`
- `@specforge/product` -> `#2ecc71`
- `@specforge/governance` -> `#e74c3c`
- `@specforge/formal` -> `#9b59b6`
- Unknown -> deterministic color from name hash, fallback `#95a5a6`

**Tests (snapshot with insta):**
- Full model at keys level with clustering
- Header colors by extension
- Required fields bolded
- Reference markers
- Fields=none header-only nodes
- Empty model

### Task 2.4: JSON renderer
**Crate:** `specforge-emitter`
**File:** `src/model/json.rs`

Implement `pub fn render_json(model: &ModelIntermediate, options: &ModelOptions) -> String`:
- Serialize ModelIntermediate to JSON via serde
- Custom serialization for Cardinality (as "1:1", "1:N", etc.)
- A field's type serializes as its `FieldType` name
- Skip None values with `skip_serializing_if`

**Tests:**
- Output is valid JSON
- schema_version present
- Entities have correct field structure
- Relationships have cardinality strings
- No compiler metadata (testable, singleton, etc.)
- Empty model

### Task 2.5: DBML renderer
**Crate:** `specforge-emitter`
**File:** `src/model/dbml.rs`

Implement `pub fn render_dbml(model: &ModelIntermediate, options: &ModelOptions) -> String`:
- Header comment with model version
- `TableGroup` per extension
- `Enum {entity}_{field}` for each enum field
- `Table {entity} { ... }` with columns
- `id string [pk]` always first
- `[not null]` for required fields
- `[ref: > target.id]` for reference fields
- `[note: '...']` for descriptions
- Named `Ref` declarations for edge types

**Tests (snapshot with insta):**
- Full model with TableGroups
- Enum definitions
- Required fields have [not null]
- Reference fields have inline ref
- Field descriptions as notes
- Empty model

---

## Phase 3: CLI Integration

### Task 3.1: Add model command to CLI
**Crate:** `specforge-cli`
**File:** `src/model.rs` + `src/main.rs`

Add `Model` variant to `Commands` enum with clap args:
- `path: PathBuf` (default ".")
- `--format` (default "markdown", possible: markdown/mermaid/dot/json/dbml)
- `--group-by` (default "extension", possible: extension/none)
- `--fields` (default "keys", possible: none/keys/all)
- `--extension` (optional)
- `--kinds` (optional, comma-separated)
- `--root` (optional)
- `--depth` (optional, requires root)

Implement `model::run()`:
1. Call `pipeline::compile(path)`
2. Generate schema from registries
3. Build ModelIntermediate from schema
4. Apply filters from options
5. Apply field level filtering
6. Dispatch to renderer
7. Print to stdout

Add `mod model;` and match arm in `main.rs`.

**Tests:**
- E2E: `specforge model` on a test project produces valid Markdown
- E2E: `specforge model --format=mermaid` produces valid erDiagram
- E2E: `--extension` filter works
- E2E: `--kinds` filter works
- E2E: `--fields=all` shows all fields
- E2E: `--root` with `--depth` scopes correctly

### Task 3.2: Update output type spec
**Crate:** spec only
**File:** `spec/types/output.spec`

Add `"model"` to the `OutputFormat` type alias (or keep model formats separate since they're a different command).

---

## Phase 4: MCP Integration

### Task 4.1: Add model MCP tool
**Crate:** `specforge-mcp`
**File:** `src/tools/model.rs` + `src/tools/mod.rs`

Implement `model::call(state: &McpState, arguments: Value, id: Option<Value>) -> JsonRpcResponse`:
- Parse JSON arguments to ModelOptions
- Compile project, generate schema, build IR, filter, render
- Return rendered string as tool result

Register `"specforge.model"` in tool dispatch and tool listing.

**Tests:**
- Tool appears in tool list with correct schema
- Default invocation returns Markdown
- Format parameter selects correct renderer
- Filter parameters pass through
- Invalid format returns error

### Task 4.2: Add model tool descriptor to MCP initialization
**File:** `src/lifecycle.rs` or wherever tool descriptors are registered

Add `specforge.model` to the tool descriptor list with:
- Description: "Render the logical data model (entity kinds, fields, relationships)"
- Input schema with all parameters (format, group_by, fields, extension, kinds, root, depth)

---

## Phase 5: Schema Enrichment (prerequisite for accurate cardinality)

### Task 5.1: Add edge and target_kind to SchemaField
**Crate:** `specforge-emitter`
**File:** `src/schema.rs`

Add to `SchemaField`:
```rust
#[serde(skip_serializing_if = "Option::is_none")]
pub edge: Option<String>,
#[serde(skip_serializing_if = "Option::is_none")]
pub target_kind: Option<String>,
#[serde(skip_serializing_if = "Option::is_none")]
pub description: Option<String>,
#[serde(skip_serializing_if = "Option::is_none")]
pub default_value: Option<String>,
```

Update `generate_schema()` to populate these fields from `ManifestField` data in the `FieldRegistry`.

**Tests:**
- Schema field with edge populated from manifest
- Schema field with target_kind populated
- Schema field with description populated
- Existing schema tests still pass (backward compatible via skip_serializing_if)

---

## Dependency Order

```
Phase 5 (Schema Enrichment)
    |
    v
Phase 1 (Core IR + Builder)
    |
    v
Phase 2 (Renderers) -- all 5 renderers can be done in parallel
    |
    v
Phase 3 (CLI Integration)
    |
    v
Phase 4 (MCP Integration)
```

Phase 5 should be done first because accurate cardinality inference depends on `edge` and `target_kind` being available on `SchemaField`.

## Test Count Estimate

- Phase 1: ~25 unit tests
- Phase 2: ~30 snapshot tests (6 per renderer)
- Phase 3: ~6 E2E tests
- Phase 4: ~5 integration tests
- Phase 5: ~4 unit tests

**Total: ~70 tests**
