---
id: RES-20
kind: research
title: "LSP Implementation Review — Gap Analysis and Improvement Roadmap"
status: active
date: 2026-03-03
depends_on: RES-11a
---

# RES-20: LSP Implementation Review

> [!NOTE]
> **ID collision:** This document shares the RES-20 ID with the Type System Evolution research. This LSP review should be cited as RES-20-LSP to avoid confusion.

## Problem Statement

The SpecForge LSP (`crates/surfaces/lsp/src/`, 17 files, ~2,200 lines) is functional: it provides go-to-definition, references, hover, completion, rename, code actions, semantic tokens, outline, and workspace symbol search. However, a systematic review against the LSP 3.17 specification and the declared behavioral contracts in `spec/behaviors/lsp.spec` reveals scaling gaps, DX gaps, and spec coverage gaps that will matter as SpecForge projects grow beyond a handful of files.

This review was performed to:
1. Rate each aspect of the implementation on a /10 scale
2. Identify missing behaviors in the spec
3. Prioritize improvements by effort vs. impact
4. Document what is an implementation gap (code not matching existing spec) vs. a spec gap (missing spec)

---

## Methodology

- **Scope**: All 17 files in `crates/surfaces/lsp/src/`
- **Reference**: LSP 3.17 specification, existing `spec/behaviors/lsp.spec`, `spec/features/lsp.spec`, `spec/ports/inbound.spec`
- **Aspects evaluated**: 13 (listed below)
- **Rating scale**: 1 (missing/broken) to 10 (production-grade, no gaps)

---

## Aspect-by-Aspect Findings

### 1. Architecture & State Management — 8/10

**Implementation**: `state.rs` defines `ServerState` holding parsed files, graph, file index, symbol table, diagnostics, and sources. `backend.rs` wraps it in `Mutex<Option<ServerState>>`. Cold build at initialize (`state.rs:24-81`), incremental rebuild on changes (`state.rs:85-168`).

**Strengths**: Single source of truth for all LSP features. Graph-first design. Incremental rebuild uses `file_graph.invalidation_set()` for dependency-aware re-parsing.

**Gaps**: `Mutex<Option<ServerState>>` blocks all handlers serially. Under high-frequency typing, the mutex becomes a bottleneck. No `RwLock` (most handlers only read). No async-aware lock (tower-lsp handlers are async but the lock is `std::sync::Mutex`).

### 2. Document Synchronization — 5/10

**Implementation**: `document_sync.rs` handles `did_open`/`did_change`/`did_close`. `backend.rs:47-49` registers `TextDocumentSyncKind::FULL`.

**Strengths**: Simple, correct. Full-sync means no partial-application bugs.

**Gaps**:
- **Full sync only** (`backend.rs:48`). Every keystroke sends the entire file content. For a 1,000-line spec file, this means ~40KB per keystroke transmitted over JSON-RPC. LSP 3.17 supports incremental sync (`TextDocumentSyncKind::INCREMENTAL`) with `TextDocumentContentChangeEvent` ranges.
- **No debouncing**. `did_change` immediately calls `incremental_rebuild` + `publish_diagnostics_from_state` (`document_sync.rs:27-46`). The `live_diagnostics` behavior contract says "within 100ms of the user **stopping** typing" — implying debouncing. The implementation validates on every keystroke, which is correct but wasteful.

### 3. Go-to-Definition — 7/10

**Implementation**: `goto_definition.rs` (27 lines). Uses `util::entity_at_position()` to identify the entity under cursor, then looks up `state.symbols` for the declaration span.

**Strengths**: Clean, minimal. Correctly resolves cross-file definitions.

**Gaps**:
- **Only handles entity IDs**. Does not support go-to-definition on `use` import paths (e.g., clicking on `use behaviors/core` does not navigate to `spec/behaviors/core.spec`). This is a spec gap — no behavior declares this.

### 4. Find References — 7/10

**Implementation**: `references.rs` (53 lines). Finds the entity at cursor, then uses `util::find_identifier_occurrences()` across all source files.

**Strengths**: Correct cross-file search. Uses string scanning, so no index lag.

**Gaps**: Text-based scanning rather than graph-based reference lookup. Works correctly but will scale linearly with project size.

### 5. Hover — 8/10

**Implementation**: `hover.rs` (65 lines). Shows entity kind, ID, title, contract/guarantee text, reference count, and verify/scenario count in a markdown hover card.

**Strengths**: Rich, well-formatted. Satisfies the `hover_information` contract fully.

**Gaps**: Minor — does not show test coverage status when available.

### 6. Completion — 5/10

**Implementation**: `completion.rs` (113 lines). Completes entity IDs inside reference lists (e.g., `behaviors [...]`). Uses `EdgeType::from_field_name()` to determine the expected entity kind, then filters candidates from the graph.

**Strengths**: Type-aware filtering (only suggests behaviors inside `behaviors [...]`). Includes entity title in detail text.

**Gaps**:
- **No field name completion**. Inside a block `behavior foo { ... }`, typing a field name like `con` does not suggest `contract`, `constraints`, etc. The user gets no help discovering valid fields. (`completion.rs` only enters the entity-ID path via `determine_expected_kind()`.)
- **No keyword completion**. At file top-level, typing `beh` does not suggest `behavior`. No snippet-based completion for block scaffolding.
- **No trigger on typing**. Trigger characters are `[` and ` ` (`backend.rs:56`), which works for reference lists but not for field names or keywords.

### 7. Rename — 9/10

**Implementation**: `rename.rs` (90 lines). `prepare_rename` validates the cursor is on a declared entity. `rename` checks uniqueness of the new name, then finds all occurrences across all files via `util::find_identifier_occurrences()`.

**Strengths**: Atomic (builds full `WorkspaceEdit` before returning). Uniqueness check prevents conflicts. `prepare_rename` provides good UX.

**Gaps**: Rename does not update `use` import paths if an entity ID appears in import statements.

### 8. Code Actions — 4/10

**Implementation**: `code_actions.rs` (130 lines). Offers a single action: "Add tests field" on testable entities that have `verify`/`scenario` but no `tests` field.

**Strengths**: The one action it provides is well-implemented — correct entity detection, proper `WorkspaceEdit`.

**Gaps**:
- **No "Add missing import" action**. When the validator emits E001 (unresolved reference), the LSP does not offer a quick-fix to add the `use` import. This is the highest-impact missing code action.
- **No "Create entity stub" action**. When the validator emits E001 for a reference to a non-existent entity, the LSP does not offer to create a stub definition.
- **No "Fix naming" actions**. No quick-fix for W001 (naming convention violations).
- **No "Add required field" actions**. No quick-fix for missing required fields.

### 9. Semantic Tokens — 9/10

**Implementation**: `semantic_tokens.rs` (280 lines). Classifies keywords, entity IDs (by kind), field names, string literals, triple-quoted strings, numbers, comments, and operators.

**Strengths**: Comprehensive token classification. Correct delta encoding. Entity IDs receive kind-specific token types.

**Gaps**: Minor — no classification of `use` path segments.

### 10. Document Symbols / Outline — 7/10

**Implementation**: `document_symbol.rs` (60 lines). Returns flat list of `SymbolInformation` entries — one per entity in the file.

**Strengths**: Correct symbol kind mapping. Shows entity ID and title.

**Gaps**:
- **Flat, not hierarchical**. LSP supports `DocumentSymbol` (hierarchical) but the implementation uses `SymbolInformation` (flat). Nested entities (e.g., fields inside a behavior) are not shown.

### 11. Workspace Symbol Search — 7/10

**Implementation**: `workspace_symbol.rs` (38 lines). Searches graph nodes by prefix match on raw entity ID.

**Strengths**: Fast, correct.

**Gaps**:
- **No fuzzy matching**. Only prefix match. Typing `live_diag` matches `live_diagnostics`, but `livdiag` does not. LSP 3.17 encourages fuzzy matching for workspace symbols.
- **No title search**. Only matches on entity ID, not title text.

### 12. Diagnostics Pipeline — 7/10

**Implementation**: `diagnostics.rs` (35 lines) converts `Diagnostic` to LSP `Diagnostic`. `document_sync.rs:52-95` collects and publishes diagnostics grouped by file.

**Strengths**: Correctly clears diagnostics for files with no errors. Severity mapping is correct.

**Gaps**: No diagnostic tags (deprecated, unnecessary). No related information (e.g., "also declared at..."). No code description URLs.

### 13. Error Handling & Robustness — 7/10

**Implementation**: All handlers use `Option<T>` return types with `?` propagation. Mutex poisoning is not handled (`.unwrap()`).

**Strengths**: Graceful degradation — missing state returns `None` (no crash). Consistent pattern across all handlers.

**Gaps**: `.unwrap()` on mutex lock means a panic in any handler poisons the lock and crashes the server. Should use `.lock().ok()?` or a recovery mechanism.

---

## Summary Rating Table

| # | Aspect | Rating | Category |
|---|--------|--------|----------|
| 1 | Architecture & State Management | 8/10 | Infrastructure |
| 2 | Document Synchronization | 5/10 | Infrastructure |
| 3 | Go-to-Definition | 7/10 | Navigation |
| 4 | Find References | 7/10 | Navigation |
| 5 | Hover | 8/10 | Intellisense |
| 6 | Completion | 5/10 | Intellisense |
| 7 | Rename | 9/10 | Refactoring |
| 8 | Code Actions | 4/10 | Refactoring |
| 9 | Semantic Tokens | 9/10 | Presentation |
| 10 | Document Symbols / Outline | 7/10 | Navigation |
| 11 | Workspace Symbol Search | 7/10 | Navigation |
| 12 | Diagnostics Pipeline | 7/10 | Infrastructure |
| 13 | Error Handling & Robustness | 7/10 | Infrastructure |
| | **Overall** | **7.4/10** | |

---

## Gap Prioritization Matrix

Top 5 improvements ranked by impact (how many users benefit daily) × feasibility (effort to implement correctly):

| Priority | Gap | Impact | Effort | Aspect |
|----------|-----|--------|--------|--------|
| **P1** | Incremental document sync | High | Medium | #2 Document Sync |
| **P2** | Add missing import code action | High | Low | #8 Code Actions |
| **P3** | Field name + keyword completion | High | Medium | #6 Completion |
| **P4** | Create entity stub code action | Medium | Low | #8 Code Actions |
| **P5** | Go-to-def on import paths | Medium | Low | #3 Go-to-Definition |

Deferred (lower priority, documented for future):
- Hierarchical outline (nice-to-have, flat outline is functional)
- Fuzzy workspace symbol search (prefix matching works for small projects)
- Fix naming code actions (low frequency)
- Required field code actions (low frequency)

---

## Spec Gap Analysis

### Missing Behaviors (now added)

| New Behavior | Rationale | Aspect |
|-------------|-----------|--------|
| `complete_field_names` | Completion only handles entity IDs, not field names inside blocks | #6 |
| `complete_keywords` | No keyword completion at top-level (behavior, feature, use, etc.) | #6 |
| `goto_import_definition` | Go-to-def only works for entity IDs, not `use` import paths | #3 |
| `code_action_add_missing_import` | Highest-value missing code action (auto-import for E001) | #8 |
| `code_action_create_entity_stub` | Second-highest: create stub for referenced-but-undeclared entities | #8 |
| `incremental_document_sync` | Foundation for scaling; currently full-sync only | #2 |

### Deferred Behaviors (future work)

| Deferred Behavior | Rationale for Deferral |
|-------------------|----------------------|
| `code_action_fix_naming` | Low-frequency issue; naming violations are uncommon once learned |
| `code_action_add_required_field` | Helpful but not blocking; error message is already clear |
| `hierarchical_outline` | Flat outline is functional; hierarchical is a DX improvement |
| `fuzzy_workspace_symbol` | Prefix matching suffices for small-medium projects |

### Not a Spec Gap: Debouncing

The existing `live_diagnostics` behavior says "within 100ms of the user **stopping** typing." This implies debouncing. The current implementation validates on **every** keystroke (`document_sync.rs:27-46`), which is an **implementation gap against the existing spec**, not a spec gap. No new behavior is needed — the existing contract already covers this. The implementation should be updated to debounce `did_change` handlers.

### Not Adding Invariants

Two invariants were considered:
1. **Semantic token correctness** — "tokens must match the AST." Already implied by `provide_semantic_tokens` contract.
2. **Completion type safety** — "only suggest entities of the correct type." Already stated in `autocomplete_entity_ids` contract: "Only entities of the correct type for the field MUST be suggested."

No new invariant file is needed.

### Missing Port Method

The `LspProtocol` port in `spec/ports/inbound.spec` omits `prepareRename`, despite the implementation having it (`backend.rs:129-134`, `rename.rs:7-25`). Adding it.

---

## Decision

1. **Add 6 behaviors** to `spec/behaviors/lsp.spec` — the five from the gap analysis plus `incremental_document_sync`
2. **Update 4 features** in `spec/features/lsp.spec` to reference the new behaviors
3. **Add `prepareRename`** to `spec/ports/inbound.spec` (spec-implementation alignment)
4. **Defer 4 lower-priority behaviors** — documented above as future work
5. **No new invariants** — existing contracts cover the considered cases
6. **File implementation issue**: debouncing is an implementation gap against the existing `live_diagnostics` contract, not a spec gap
