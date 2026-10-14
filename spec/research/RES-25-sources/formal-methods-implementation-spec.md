# Formal Methods Plugin: Implementation Specification

> [!CAUTION]
> **SUPERSEDED by RES-27** — This implementation spec for a separate `@specforge/formal-analysis` plugin with `contract`/`refinement`/`process` entities has been superseded. Formal methods syntax is now part of `@specforge/software` as inline blocks (`requires`/`ensures`/`maintains`/`sync`) on existing entities.
>
> **What changed:**
> - Separate `@specforge/formal-analysis` extension manifest → formal methods are part of `@specforge/software` manifest
> - `contract`/`refinement`/`process` entity registrations → rejected; no new entity types needed
> - `specforge gen` codegen hooks → deprecated; AI agents consume entity graph directly
> - Dedicated validation codes for formal entities → validation integrated into `@specforge/software` passes

**Date**: March 4, 2026
**Expert**: Expert 10 — Plugin & Extension System Designer
**Status**: superseded

---

## 1. Extension Metadata

```json
{
  "name": "@specforge/formal-analysis",
  "version": "0.1.0",
  "description": "Formal verification support: DbC, B-Method, CSP",
  "license": "MIT",
  "repository": "github:specforge/formal-analysis",
  "keywords": ["specforge", "plugin", "formal-methods", "design-by-contract", "b-method", "csp"]
}
```

---

## 2. Manifest Schema

**File**: `manifest.json`

```json
{
  "extension": "@specforge/formal-analysis",
  "manifest_version": "2",
  "version": "0.1.0",
  "description": "Formal verification support: DbC, B-Method, CSP",
  "wasm": "formal_analysis.wasm",

  "contributes": {
    "entities": true,
    "validators": true,
    "generators": false,
    "providers": false
  },

  "entity_kinds": [
    {
      "name": "contract",
      "testable": true,
      "fields": [
        {
          "name": "type",
          "type": "enum",
          "values": ["precondition", "postcondition", "invariant"],
          "required": true
        },
        {
          "name": "expression",
          "type": "string",
          "required": true,
          "description": "Contract formula (free-form text)"
        },
        {
          "name": "enforced_by",
          "type": "reference_list",
          "target": "behavior",
          "description": "Behaviors that enforce this contract"
        }
      ]
    },
    {
      "name": "refinement",
      "testable": false,
      "fields": [
        {
          "name": "abstract",
          "type": "reference",
          "target": "behavior",
          "required": true,
          "description": "Abstract behavior being refined"
        },
        {
          "name": "concrete",
          "type": "reference",
          "target": "behavior",
          "required": true,
          "description": "Concrete implementation"
        },
        {
          "name": "proof_obligation",
          "type": "string",
          "description": "What must be proven (free-form text)"
        }
      ]
    },
    {
      "name": "process",
      "testable": true,
      "fields": [
        {
          "name": "alphabet",
          "type": "string_list",
          "required": true,
          "description": "Valid events for this process"
        },
        {
          "name": "definition",
          "type": "string",
          "required": true,
          "description": "CSP expression (free-form text)"
        },
        {
          "name": "traces",
          "type": "reference_list",
          "target": "behavior",
          "description": "Behaviors implementing this process"
        }
      ]
    }
  ],

  "dynamic_edge_types": [
    {
      "name": "guards",
      "source_kind": "contract",
      "target_kind": "behavior",
      "description": "Contract guards a behavior (DbC)"
    },
    {
      "name": "refines",
      "source_kind": "behavior",
      "target_kind": "behavior",
      "description": "Concrete behavior refines abstract (B-Method)"
    },
    {
      "name": "composes_with",
      "source_kind": "process",
      "target_kind": "process",
      "description": "Processes compose in parallel (CSP)"
    }
  ],

  "query_extensions": [
    {
      "name": "event_flow",
      "file": "queries/event_flow.rhai",
      "description": "Trace event flow from a behavior"
    },
    {
      "name": "check_refinement",
      "file": "queries/check_refinement.rhai",
      "description": "Verify refinement correctness (B-Method)"
    },
    {
      "name": "find_deadlocks",
      "file": "queries/find_deadlocks.rhai",
      "description": "Detect deadlocks in process composition (CSP)"
    },
    {
      "name": "get_contracts",
      "file": "queries/get_contracts.rhai",
      "description": "Find contracts for a behavior"
    }
  ],

  "peer_dependencies": {
    "@specforge/product": "^0.1.0"
  },

  "sandbox": {
    "allowed_domains": [],
    "max_fuel": 50000000
  }
}
```

---

## 3. Host Functions Used

### 3.1 Existing Host Functions

The plugin uses these existing host functions:

| Function | Purpose | Phase |
|----------|---------|-------|
| `specforge.query_graph()` | Get full graph as JSON | validate |
| `specforge.emit_diagnostic()` | Report errors/warnings | validate |
| `specforge.register_entity()` | Add entity kinds | initialize |
| `specforge.register_edge()` | Add edge types | initialize |

### 3.2 New Host Function (Phase 2)

**Function**: `specforge.invoke_query(name: string, args: json) -> json`

**Purpose**: Execute a Rhai query extension registered by the plugin.

**Input schema**:
```typescript
interface InvokeQueryInput {
  name: string;      // Query extension name
  args: JsonValue;   // Query arguments (free-form)
}
```

**Output schema**:
```typescript
type InvokeQueryOutput = JsonValue;  // Query result (free-form)
```

**Implementation location**: `/Users/u1070457/Projects/Perso/specforge/crates/runtime/wasm/src/host_functions.rs`

**Implementation pseudocode**:
```rust
fn build_invoke_query(user_data: extism::UserData<HostContext>) -> extism::Function {
    extism::Function::new(
        "specforge.invoke_query",
        [extism::ValType::I64, extism::ValType::I64],
        [extism::ValType::I64],
        user_data,
        |plugin, inputs, outputs, ud| {
            // 1. Extract query name + args from Wasm memory
            let name_handle = plugin.memory_from_val(&inputs[0])?;
            let name = String::from_utf8(plugin.memory_bytes(name_handle)?.to_vec())?;
            plugin.memory_free(name_handle)?;

            let args_handle = plugin.memory_from_val(&inputs[1])?;
            let args = String::from_utf8(plugin.memory_bytes(args_handle)?.to_vec())?;
            plugin.memory_free(args_handle)?;

            // 2. Find query extension in registry
            let ctx = ud.get()?;
            let ctx_guard = ctx.lock().unwrap();

            let query_ext = ctx_guard.query_registry.get(&name)
                .ok_or_else(|| extism::Error::msg(format!("Unknown query: {}", name)))?;

            // 3. Create Rhai engine with graph helpers
            let mut engine = rhai::Engine::new();
            register_graph_helpers(&mut engine, &ctx_guard.graph_json);

            // 4. Execute Rhai script
            let result: rhai::Dynamic = engine.eval(&query_ext.script)?;
            let result_json = serde_json::to_string(&result)?;

            // 5. Return result as JSON
            let result_handle = plugin.memory_new(&result_json)?;
            outputs[0] = plugin.memory_to_val(result_handle);
            Ok(())
        },
    )
}
```

---

## 4. Entity Schemas

### 4.1 `contract`

**DSL syntax**:
```spec
contract <id> {
  title       <string>                    // Optional (auto-derived from ID)
  type        precondition | postcondition | invariant  // Required
  expression  <string>                    // Required (contract formula)
  enforced_by [<behavior_id>, ...]        // Optional (behaviors enforcing this)

  verify <kind> "<name>" {                // Optional test declarations
    path "<file>::<function>"
  }

  scenario "<title>" {                    // Optional scenarios
    given "<step>"
    when  "<step>"
    then  "<step>"
  }

  description """<multiline>"""           // Optional
  refs [<ref_id>, ...]                    // Optional
  tests [<file>::<function>, ...]         // Optional (test linkage)
}
```

**Graph representation**:
```json
{
  "id": "auth_login_precond",
  "kind": "contract",
  "title": "Login requires credentials",
  "fields": {
    "type": "precondition",
    "expression": "username != null && password != null",
    "enforced_by": ["auth_login"]
  },
  "file": "spec/auth.spec",
  "span": {"start_line": 10, "start_column": 1, "end_line": 15, "end_column": 1}
}
```

**Edges**:
- Outgoing: `guards` → `behavior` (contract guards a behavior)

### 4.2 `refinement`

**DSL syntax**:
```spec
refinement <id> {
  title            <string>       // Optional
  abstract         <behavior_id>  // Required (abstract behavior)
  concrete         <behavior_id>  // Required (concrete implementation)
  proof_obligation <string>       // Optional (what must be proven)

  description """<multiline>"""   // Optional
  refs [<ref_id>, ...]            // Optional
}
```

**Graph representation**:
```json
{
  "id": "payment_flow_refinement",
  "kind": "refinement",
  "title": "Payment flow implementation",
  "fields": {
    "abstract": "payment_process_abstract",
    "concrete": "payment_process_impl",
    "proof_obligation": "preserve_balance_invariant"
  },
  "file": "spec/payment.spec",
  "span": {"start_line": 20, "start_column": 1, "end_line": 25, "end_column": 1}
}
```

**Edges**:
- Implicit: `abstract` field creates `behavior --[refines]--> behavior` edge

### 4.3 `process`

**DSL syntax**:
```spec
process <id> {
  title      <string>             // Optional
  alphabet   [<string>, ...]      // Required (valid event names)
  definition <string>             // Required (CSP expression)
  traces     [<behavior_id>, ...] // Optional (implementing behaviors)

  scenario "<title>" {            // Optional scenarios
    given "<step>"
    when  "<step>"
    then  "<step>"
  }

  description """<multiline>"""   // Optional
  refs [<ref_id>, ...]            // Optional
  tests [<file>::<function>, ...] // Optional
}
```

**Graph representation**:
```json
{
  "id": "auth_service",
  "kind": "process",
  "title": "Authentication service",
  "fields": {
    "alphabet": ["login", "logout", "refresh_token"],
    "definition": "login -> (logout -> STOP | refresh_token -> auth_service)",
    "traces": ["auth_login", "auth_logout", "token_refresh"]
  },
  "file": "spec/auth.spec",
  "span": {"start_line": 30, "start_column": 1, "end_line": 38, "end_column": 1}
}
```

**Edges**:
- Outgoing: `composes_with` → `process` (process composition)

---

## 5. Validation Rules

### 5.1 V001: Contract expression is empty

**Severity**: Error
**Code**: `V001`
**Entity**: `contract`

**Check**: Contract `expression` field is not empty.

**Implementation**:
```rust
for contract in graph.nodes_of_kind("contract") {
    let expr = contract.field("expression").as_str().unwrap_or("");
    if expr.trim().is_empty() {
        emit_diagnostic(json!({
            "severity": "error",
            "code": "V001",
            "message": "Contract expression cannot be empty",
            "file": contract.file,
            "line": contract.span.start_line,
            "column": contract.span.start_column
        }));
    }
}
```

**Example**:
```spec
contract bad_contract {
  type precondition
  expression ""  // ❌ V001: Contract expression cannot be empty
}
```

### 5.2 V002: Refinement cycle detected

**Severity**: Error
**Code**: `V002`
**Entity**: `refinement`

**Check**: Refinement edges (`refines`) form a DAG (no cycles).

**Implementation**:
```rust
let refinement_edges: Vec<(String, String)> = graph.edges()
    .filter(|edge| edge.2.edge_type == EdgeType::Refines)
    .map(|(src, tgt, _)| (src.id.raw().to_string(), tgt.id.raw().to_string()))
    .collect();

if has_cycle(&refinement_edges) {
    emit_diagnostic(json!({
        "severity": "error",
        "code": "V002",
        "message": "Refinement cycle detected",
        "file": "<first cycle node file>",
        "line": 1,
        "column": 1
    }));
}

fn has_cycle(edges: &[(String, String)]) -> bool {
    // Use petgraph topological sort or DFS
    // Return true if cycle exists
}
```

**Example**:
```spec
// ❌ V002: Refinement cycle detected
refinement r1 { abstract a1 concrete a2 }
refinement r2 { abstract a2 concrete a3 }
refinement r3 { abstract a3 concrete a1 }  // Cycle: a1 → a2 → a3 → a1
```

### 5.3 V003: Event not in process alphabet

**Severity**: Warning
**Code**: `V003`
**Entity**: `process`

**Check**: All events produced by traced behaviors are in the process alphabet.

**Implementation**:
```rust
for process in graph.nodes_of_kind("process") {
    let alphabet: Vec<String> = process.field("alphabet").as_array()
        .unwrap_or(&vec![])
        .iter()
        .filter_map(|v| v.as_str().map(String::from))
        .collect();

    let traces: Vec<String> = process.field("traces").as_array()
        .unwrap_or(&vec![])
        .iter()
        .filter_map(|v| v.as_str().map(String::from))
        .collect();

    for trace_id in traces {
        let produced_events = graph.outgoing_edges(&trace_id)
            .filter(|(_, edge)| edge.edge_type == EdgeType::Produces)
            .map(|(node, _)| node.id.raw().to_string());

        for event_id in produced_events {
            if !alphabet.contains(&event_id) {
                emit_diagnostic(json!({
                    "severity": "warning",
                    "code": "V003",
                    "message": format!("Event '{}' not in process alphabet", event_id),
                    "file": process.file,
                    "line": process.span.start_line,
                    "column": process.span.start_column
                }));
            }
        }
    }
}
```

**Example**:
```spec
process auth_service {
  alphabet ["login", "logout"]
  traces [auth_login]
}

behavior auth_login {
  produces [login_event, refresh_token_event]  // ⚠️ V003: refresh_token_event not in alphabet
}
```

### 5.4 V004: Contract not enforced

**Severity**: Warning
**Code**: `V004`
**Entity**: `contract`

**Check**: Contract has at least one behavior in `enforced_by` field.

**Implementation**:
```rust
for contract in graph.nodes_of_kind("contract") {
    let enforced_by = contract.field("enforced_by").as_array()
        .unwrap_or(&vec![]);

    if enforced_by.is_empty() {
        emit_diagnostic(json!({
            "severity": "warning",
            "code": "V004",
            "message": "Contract not enforced by any behavior",
            "file": contract.file,
            "line": contract.span.start_line,
            "column": contract.span.start_column
        }));
    }
}
```

**Example**:
```spec
contract orphan_contract {
  type precondition
  expression "user != null"
  enforced_by []  // ⚠️ V004: Contract not enforced by any behavior
}
```

### 5.5 V005: Deadlock detected

**Severity**: Error
**Code**: `V005`
**Entity**: `process`

**Check**: Process composition (via `composes_with` edges) does not create deadlocks.

**Implementation** (Phase 2 with query extensions):
```rust
let result = invoke_query("find_deadlocks", json!({}));
let cycles: Vec<Vec<String>> = serde_json::from_str(&result)?;

if !cycles.is_empty() {
    for cycle in cycles {
        emit_diagnostic(json!({
            "severity": "error",
            "code": "V005",
            "message": format!("Deadlock detected: {}", cycle.join(" -> ")),
            "file": "<first process in cycle file>",
            "line": 1,
            "column": 1
        }));
    }
}
```

**Query extension** (`queries/find_deadlocks.rhai`):
```rhai
fn query(graph, args) {
    let edges = graph.edges_of_type("composes_with");
    return graph.find_cycles(edges);
}
```

**Example**:
```spec
// ❌ V005: Deadlock detected
process p1 { composes_with [p2] }
process p2 { composes_with [p3] }
process p3 { composes_with [p1] }  // Cycle: p1 → p2 → p3 → p1
```

### 5.6 V006: Refinement lacks proof obligation

**Severity**: Warning
**Code**: `V006`
**Entity**: `refinement`

**Check**: Refinement has a non-empty `proof_obligation` field.

**Implementation**:
```rust
for refinement in graph.nodes_of_kind("refinement") {
    let proof_obl = refinement.field("proof_obligation").as_str().unwrap_or("");

    if proof_obl.trim().is_empty() {
        emit_diagnostic(json!({
            "severity": "warning",
            "code": "V006",
            "message": "Refinement lacks proof obligation",
            "file": refinement.file,
            "line": refinement.span.start_line,
            "column": refinement.span.start_column
        }));
    }
}
```

**Example**:
```spec
refinement incomplete_refinement {
  abstract payment_abstract
  concrete payment_impl
  // ⚠️ V006: Missing proof_obligation
}
```

### 5.7 V007: Formal analysis skipped

**Severity**: Info
**Code**: `V007`
**Entity**: (any)

**Check**: Plugin detected a missing peer dependency and skipped some analyses.

**Implementation**:
```rust
// Check if @specforge/product is installed
let has_product_plugin = graph.metadata.installed_plugins.contains("@specforge/product");

if !has_product_plugin {
    emit_diagnostic(json!({
        "severity": "info",
        "code": "V007",
        "message": "Formal analysis skipped: @specforge/product not installed",
        "file": "<spec root file>",
        "line": 1,
        "column": 1
    }));
}
```

---

## 6. Query Extensions (Rhai)

### 6.1 `event_flow.rhai`

**Purpose**: Trace event flow from a behavior.

**Input**:
```json
{
  "behavior_id": "auth_login",
  "max_depth": 5
}
```

**Output**:
```json
{
  "events": ["login_event", "session_event", "audit_event"]
}
```

**Implementation**:
```rhai
fn query(graph, args) {
    let behavior_id = args["behavior_id"];
    let max_depth = args.get("max_depth", 10);

    let visited = #{};
    let result = [];

    fn traverse(node_id, depth) {
        if depth > max_depth || visited.contains(node_id) {
            return;
        }
        visited[node_id] = true;

        let events = graph.outgoing_edges(node_id, "produces");
        for event in events {
            result.push(event.id);

            let consumers = graph.incoming_edges(event.id, "consumes");
            for consumer in consumers {
                traverse(consumer.id, depth + 1);
            }
        }
    }

    traverse(behavior_id, 0);

    return #{
        "events": result
    };
}
```

### 6.2 `find_deadlocks.rhai`

**Purpose**: Detect deadlocks in process composition.

**Input**: (none)

**Output**:
```json
{
  "cycles": [
    ["p1", "p2", "p3", "p1"]
  ]
}
```

**Implementation**:
```rhai
fn query(graph, args) {
    let edges = graph.edges_of_type("composes_with");
    let cycles = graph.find_cycles(edges);

    return #{
        "cycles": cycles
    };
}
```

### 6.3 `get_contracts.rhai`

**Purpose**: Find all contracts for a behavior.

**Input**:
```json
{
  "behavior_id": "auth_login"
}
```

**Output**:
```json
{
  "contracts": [
    {"id": "auth_login_precond", "type": "precondition"},
    {"id": "auth_login_postcond", "type": "postcondition"}
  ]
}
```

**Implementation**:
```rhai
fn query(graph, args) {
    let behavior_id = args["behavior_id"];

    let contracts = graph.incoming_edges(behavior_id, "guards");

    return #{
        "contracts": contracts.map(|c| #{
            "id": c.id,
            "type": c.field("type")
        })
    };
}
```

### 6.4 `check_refinement.rhai`

**Purpose**: Verify refinement correctness (placeholder for future proof checking).

**Input**:
```json
{
  "refinement_id": "payment_refinement"
}
```

**Output**:
```json
{
  "valid": true,
  "obligations": ["preserve_balance_invariant"]
}
```

**Implementation**:
```rhai
fn query(graph, args) {
    let refinement_id = args["refinement_id"];
    let refinement = graph.get_node(refinement_id);

    let abstract_id = refinement.field("abstract");
    let concrete_id = refinement.field("concrete");
    let proof_obl = refinement.field("proof_obligation");

    // Placeholder: always return valid
    // Future: integrate with proof checker
    return #{
        "valid": true,
        "obligations": [proof_obl]
    };
}
```

---

## 7. Code Generation Integration

### 7.1 TypeScript Generator

**File**: `@specforge/gen-typescript`

**Contract emission**:
```typescript
// For each behavior, find its contracts
const behavior = graph.findNode("auth_login");
const contracts = graph.incomingEdges(behavior.id, "guards");

const preconditions = contracts.filter(c => c.type === "precondition");
const postconditions = contracts.filter(c => c.type === "postcondition");

// Emit function with runtime checks
emit(`
export function ${behavior.id}(${params}): ${returnType} {
  ${preconditions.map(c => `
  // @specforge-contract: ${c.id}
  if (!(${translateExpression(c.expression)})) {
    throw new ContractViolation("${c.expression}");
  }
  `).join("\n")}

  const result = ${implementation};

  ${postconditions.map(c => `
  // @specforge-contract: ${c.id}
  if (!(${translateExpression(c.expression)})) {
    throw new ContractViolation("${c.expression}");
  }
  `).join("\n")}

  return result;
}
`);
```

### 7.2 Rust Generator

**File**: `@specforge/gen-rust`

**Contract emission**:
```rust
// For each behavior, find its contracts
let behavior = graph.find_node("auth_login");
let contracts = graph.incoming_edges(&behavior.id, &EdgeType::Guards);

let preconditions: Vec<_> = contracts.iter()
    .filter(|c| c.field("type") == "precondition")
    .collect();

// Emit function with doc comments + debug assertions
emit(format!(r#"
impl AuthService {{
    /// # Preconditions
    {}
    pub fn {}(&self, {}) -> Result<{}> {{
        {}

        // ... implementation

        Ok(result)
    }}
}}
"#,
    preconditions.iter()
        .map(|c| format!("    /// - `{}`", c.field("expression")))
        .join("\n"),
    behavior.id,
    params,
    return_type,
    preconditions.iter()
        .map(|c| format!(r#"debug_assert!({}, "Contract violated: {}");"#,
            translate_expression(&c.field("expression")),
            c.id
        ))
        .join("\n        ")
));
```

---

## 8. Test Scaffolding

### 8.1 Contract Tests

**TypeScript** (`@specforge/vitest`):
```typescript
// For testable entities with verify blocks
const contract = graph.findNode("auth_login_precond");
const verifyBlocks = contract.verify;

for (const verify of verifyBlocks) {
  emit(`
describe("${contract.id}", () => {
  it("${verify.name}", () => {
    // TODO: Implement contract test
  });
});
  `);
}
```

**Rust** (`@specforge/gen-rust` + `specforge-test` crate):
```rust
// For testable entities with verify blocks
let contract = graph.find_node("auth_login_precond");
let verify_blocks = &contract.verify;

for verify in verify_blocks {
    emit(format!(r#"
#[cfg(test)]
mod {}__{} {{
    use super::*;

    #[test]
    #[specforge::test("{}")]
    fn {}() {{
        // TODO: Implement contract test
    }}
}}
"#,
        contract.id,
        slugify(&verify.name),
        contract.id,
        slugify(&verify.name)
    ));
}
```

### 8.2 Process Tests

Same scaffolding as contracts (processes are testable).

---

## 9. Performance Budgets

| Analysis | LSP Budget | CLI Budget |
|----------|-----------|------------|
| V001: Empty expression | 1ms | 10ms |
| V002: Refinement cycle | 50ms | 500ms |
| V003: Alphabet consistency | 10ms | 100ms |
| V004: Unenforced contract | 5ms | 50ms |
| V005: Deadlock (simple) | 100ms | 1s |
| V005: Deadlock (advanced) | ❌ Skip | 10s (timeout) |
| V006: Missing proof | 1ms | 10ms |
| V007: Skipped analysis | 1ms | 10ms |

**Implementation**: Plugin checks execution context and skips expensive analyses in LSP mode.

```rust
fn validate(graph_json: &str, context: &ExecutionContext) -> Result<Vec<Diagnostic>> {
    let is_lsp = context.mode == "lsp";
    let mut diagnostics = vec![];

    // Always run fast checks (< 50ms)
    diagnostics.extend(validate_contracts(&graph_json)?);
    diagnostics.extend(validate_refinement_dag(&graph_json)?);
    diagnostics.extend(validate_alphabet_consistency(&graph_json)?);

    // Only run expensive checks in CLI mode
    if !is_lsp {
        diagnostics.extend(detect_deadlocks_advanced(&graph_json)?);
    }

    Ok(diagnostics)
}
```

---

## 10. Implementation Checklist

### Phase 1: MVP (2 weeks)

- [ ] Set up Rust Wasm project with `cargo-wasm`
- [ ] Implement manifest.json with entity_kinds + dynamic_edge_types
- [ ] Implement `initialize()` export: register entities + edges
- [ ] Implement `validate()` export: V001-V004 (fast checks)
- [ ] Write unit tests for validators
- [ ] Create example `.spec` files using formal entities
- [ ] Test plugin loading in SpecForge CLI
- [ ] Document in `docs/plugins/formal-analysis.md`

### Phase 2: Query Extensions (2 weeks)

- [ ] Implement `invoke_query` host function in specforge-wasm crate
- [ ] Add Rhai engine integration to host function
- [ ] Implement graph helper functions for Rhai:
  - [ ] `graph.outgoing_edges(node_id, edge_type)`
  - [ ] `graph.incoming_edges(node_id, edge_type)`
  - [ ] `graph.filter_nodes(predicate)`
  - [ ] `graph.has_cycle(edges)`
  - [ ] `graph.find_cycles(edges)`
- [ ] Write Rhai query extensions:
  - [ ] `event_flow.rhai`
  - [ ] `find_deadlocks.rhai`
  - [ ] `get_contracts.rhai`
  - [ ] `check_refinement.rhai`
- [ ] Update `validate()` to use query extensions (V005)
- [ ] Benchmark query performance (target: < 100ms)
- [ ] Add query result caching

### Phase 3: Advanced Validation (3 weeks)

- [ ] Implement V005 deadlock detection with timeout
- [ ] Add fuel limit for expensive analyses
- [ ] Implement execution context detection (LSP vs CLI)
- [ ] Skip expensive checks in LSP mode
- [ ] Add custom rule support:
  - [ ] Load user-provided Rhai scripts from `.specforge/queries/`
  - [ ] Register custom rules from config
  - [ ] Execute custom rules during validation
- [ ] Implement V007 (peer dependency check)
- [ ] LSP integration:
  - [ ] Fast checks on keystroke
  - [ ] Medium checks on save
  - [ ] Document performance strategy

### Phase 4: Codegen Integration (2 weeks)

- [ ] Extend `@specforge/gen-typescript`:
  - [ ] Query contracts via graph edges
  - [ ] Emit runtime checks (preconditions/postconditions)
  - [ ] Emit test scaffolding for contracts
- [ ] Extend `@specforge/gen-rust`:
  - [ ] Query contracts via graph edges
  - [ ] Emit doc comments for contracts
  - [ ] Emit debug assertions
  - [ ] Emit test scaffolding
- [ ] Document codegen integration in language guides
- [ ] Add examples to `examples/` directory

### Phase 5: Proof Tooling (4 weeks, optional)

- [ ] Research proof checker integration (Coq, Isabelle, Z3)
- [ ] Design proof obligation format
- [ ] Implement proof generation from refinements
- [ ] Add `specforge prove` CLI command
- [ ] Integrate proof validation in CI pipeline
- [ ] Document proof workflow in `docs/formal-verification.md`

---

## 11. Testing Strategy

### Unit Tests

- [ ] Entity registration (contract, refinement, process)
- [ ] Edge type registration (guards, refines, composes_with)
- [ ] V001-V007 validators (with positive/negative cases)
- [ ] Query extension execution
- [ ] Graph helper functions

### Integration Tests

- [ ] Plugin loading in CLI
- [ ] Graph construction with formal entities
- [ ] Cross-plugin references (contract → behavior)
- [ ] Code generation with contracts
- [ ] Custom rule loading

### Performance Tests

- [ ] Benchmark validators on large graphs (1000+ nodes)
- [ ] Measure query extension overhead
- [ ] Test LSP responsiveness with formal entities
- [ ] Verify timeout handling for expensive analyses

---

## 12. Documentation Requirements

### User Documentation

- [ ] `docs/plugins/formal-analysis.md` — Plugin overview
- [ ] `docs/entities/contract.md` — Contract entity reference
- [ ] `docs/entities/refinement.md` — Refinement entity reference
- [ ] `docs/entities/process.md` — Process entity reference
- [ ] `docs/guides/formal-methods.md` — User guide for formal verification
- [ ] `docs/guides/custom-rules.md` — Writing custom validation rules

### Developer Documentation

- [ ] `docs/dev/query-extensions.md` — Query extension API
- [ ] `docs/dev/formal-plugin-architecture.md` — Plugin architecture
- [ ] Contributing guide for formal methods features

### Examples

- [ ] `examples/formal-methods/dbc-example.spec` — DbC usage
- [ ] `examples/formal-methods/b-method-example.spec` — B-Method refinement
- [ ] `examples/formal-methods/csp-example.spec` — CSP processes
- [ ] `examples/formal-methods/custom-rule.rhai` — Custom validation rule

---

**End of Implementation Specification**
