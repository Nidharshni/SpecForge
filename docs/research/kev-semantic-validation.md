# Kev and System One for semantic SpecForge review

Research and design sketch, 2026-10-02. This is a proposal, not an implemented feature.

## Sources and verified facts

- [Kev README](https://github.com/jaredpalmer/kev): Kev is a local family of decision models with a TypeSafe System One compatible `POST /v1/systemone` endpoint. The request contains one `state` and named independent `noul` (yes/no), `choice`, or `score` questions. The response supplies probabilities. The model does not generate explanations or edits.
- [Kev API and model notes](https://github.com/jaredpalmer/kev#api): `state` accepts a string or structured JSON, choice options are caller supplied, questions share the state but cannot read one another, and the local server defaults to `127.0.0.1`. It rejects overlong inputs with 422 unless truncation is explicitly enabled. A local server can require `KEV_API_KEY`.
- [Kev models and limitations](https://github.com/jaredpalmer/kev#models): the 0.8B, 4B, and 9B models have 8,192 token *validated* context; 27B has 65,536. The README recommends 4B to start. Reported calibration and benchmarks are for Kev's evaluated data, not SpecForge specs. The README explicitly recommends measuring thresholds on one's own data.
- [TypeSafe API reference](https://docs.typesafe.ai/api): System One question and answer shapes. Choice and score `confidence` are functions of the answer distribution, not measured accuracy rates.
- [Kev license](https://github.com/jaredpalmer/kev/blob/main/LICENSE): Apache-2.0.
- SpecForge's own [extension tutorial](../guides/extending-specforge.md), [protocol](../extension-protocol.md), [formal guide](../guides/formal-verification.md), and source files named below establish the local architecture.

## Current SpecForge boundary

SpecForge already supplies deterministic answers to several requested checks:

| Need | Current mechanism | Remaining semantic question |
| --- | --- | --- |
| Referenced node exists | Graph builder emits `E003` for unresolved references; `I004` identifies references to kinds provided by an absent extension (`crates/specforge-graph/src/graph.rs`). | Does this reference **mean** the relationship its field claims? Is an unlinked relationship implied by the prose? |
| Resolved edge exists | The linker asserts, in debug builds, that a resolved reference became a graph edge (`Graph::assert_linked`, `crates/specforge-graph/src/graph.rs`). | Is the edge appropriate and useful? |
| Node shape | Extension field/rule registries and custom validators run in `check` (`docs/guides/extending-specforge.md`). | Is its description coherent, specific, and falsifiable? |
| Verify declaration and proof | `@specforge/testing` has `W004`/`W009`; coverage verdicts require passing tests naming exact obligations or formal proof (`crates/specforge-coverage/src/lib.rs`). | Is each `verify` statement actually a good test of its claim? Do the obligations cover the failure cases? |
| Formal contradiction | `analyze --prove` uses Z3 to detect unsatisfiable numeric bounds and unentailed expression claims (`docs/guides/formal-verification.md`). | Do two natural-language claims conflict, even when no expression captures them? |

Do not let a probability replace `E003`, Z3, or the coverage verdict. A high Kev score is a review signal, not proof that a node or spec is valid.

The current WASI component passes receive an entity/edge snapshot and return diagnostics. They are pure computation and cannot call the documented host HTTP function today (`docs/extension-protocol.md`, `docs/extension-sdk.md`). A pass declared `phase: "check"` runs during `check`, watch, LSP, and MCP; an ordinary pass runs during `analyze`. Calling a local Kev server inside either pass is therefore **not** available under the present ABI. The `host_http_get` permission matrix is only a future design and is limited to providers, so enabling a network flag in a manifest does not solve this.

## Proposed split

1. **Deterministic extension, `@specforge/semantic-rules`:** optional structural profile rules (required fields, edge patterns, coverage paths) through existing rules and passes. No model call in the Wasm guest. The present pass ABI returns diagnostics plus a summary; it does not declare a reusable candidate-generation contract for a separate host operation. Add that contract only if multiple domain extensions need to contribute candidates.
2. **Host-side System One adapter:** an opt-in Rust operation that generates candidate pairs and bounded evidence packages from the resolved graph, calls `POST /v1/systemone` over loopback HTTP, parses probabilities, and produces *review findings* with node IDs and source spans. Provider URL, pinned model revision, timeout and thresholds are explicit settings. Keep a provider trait so Jev or another compatible endpoint can later be used.
3. **Policy and result separation:** deterministic diagnostics keep their existing meaning. Model findings are `suggestion` or `needs-review` initially; they never discharge `verify`, synthesize graph edges, or make `check` fail. A later CI gate can be based on a measured, task-specific false-positive rate.

An initial prototype can live outside the compiler: `specforge export --format=graph` plus a small local CLI/sidecar calling Kev. It should store candidate/evidence/result JSON and print review suggestions. Once the question set and evaluation data are credible, integrate the adapter as an operation, for example `specforge review --provider systemone`, shared by CLI and MCP. This preserves the current pure compile path and avoids model latency on every keystroke.

## Subgraph review and missing relationships

Large specs need two separate scaling decisions: how SpecForge stores the graph, and how much evidence Kev sees. The current compiler can keep its indexed project graph for now, but **no model request should contain the entire graph**. Review overlapping windows selected by an anchor and a purpose: a channel, feature, journey, module, invariant, or an explicit pair of anchors. Traversal follows typed relations, reverses them when needed, and stops by node/token budget. Preserve IDs and source spans. Include connector nodes at the boundary so the model can judge paths that cross windows; a disjoint file partition would hide the missing relationships of interest. If the graph itself later exceeds one project's memory budget, a slice provider can load module/project shards while a global ID, edge, and boundary index preserves cross-shard links. That storage change does not require changing the review question contract.

The self-hosting SpecForge spec supplies a useful test case. `spec/product/channels.spec` defines `cli` and `mcp`. `JourneyUsesChannel` goes from journey to channel; `JourneyExercisesFeature` goes from journey to feature (`extensions/product/src/describe_edges.json`). At present `spec/product/journeys.spec` has 35 journeys naming `cli` and 6 naming `mcp` (counted from the current source). **This count is not a parity target:** the MCP journeys are often broad and may cover many operations. Yet `check_test_coverage` and `trace_test_coverage` name the CLI and `te_coverage_analysis`, while `provide_mcp_coverage_tool` in `spec/behaviors/mcp-tools.spec` explicitly exposes coverage over MCP. Another candidate is validation: `consume_graph_via_mcp` says the agent calls `specforge.validate`, but its feature list names only `mcp_resource_exposure` and `mcp_core_tools`; the CLI's `validate_spec_files` journey links `structural_validation`. These are plausible missing *journey or feature relationships* to review, not automatically missing direct channel-to-behavior edges: that edge kind does not exist in the current schema. The outcome might be a new MCP journey, an added feature reference on an existing journey, or an intentional difference.

For each anchor pair such as CLI/MCP, compare **capability motifs**, not raw edge counts:

1. Build one neighborhood per channel: `channel <- JourneyUsesChannel <- journey -> JourneyExercisesFeature -> feature <- BehaviorImplementsFeature <- behavior`, plus linked modules, invariants, events, and `verify` obligations when relevant.
2. Align small capability clusters using explicit IDs, tool/command names, shared `specforge-ops` operation, field text, and optionally embedding similarity. This retrieval index must work **independently of existing graph edges**: traversing only current edges cannot discover the edge that is absent. Candidate retrieval is not a decision. Prefer structural evidence such as `provide_mcp_coverage_tool` over name similarity alone.
3. For an aligned CLI/MCP capability, compute the relation/path difference. Check whether an edge is schema legal and absent, whether an equivalent edge is represented by another journey, or whether a *node* is needed first.
4. Ask Kev a narrow question with both neighborhoods in `state`: "MCP exposes coverage through this behavior. Is the lack of a journey-to-coverage-feature link an omission, an intentional surface difference, or unclear?" Include the edge type definition and the relevant source excerpts. Never ask it to infer the whole project from the channel descriptions alone.
5. Produce a review candidate with a proposed patch target (`channels`, `features`, or a new journey), both supporting spans, and the exact missing path. Require human acceptance before writing an edge. Suppress accepted intentional differences with a scoped reason.

Use overlapping windows and a review manifest so every in-scope anchor and connector path is eventually examined. A deterministic boundary audit verifies that declared cross-window references resolve and that the corresponding edges exist; the semantic audit looks for *undeclared* cross-window relationships using the independent candidate index. Track `anchors_reviewed`, `candidate_pairs_examined`, `boundary_pairs_examined`, and `findings_resolved`; do not call these percentages a proof of absolute spec completeness. In incremental mode, invalidate only windows touching changed nodes, their typed neighbors, and aligned counterpart clusters. This lets the model work under Kev-4B's validated 8,192-token context while the underlying SpecForge graph can be much larger ([Kev models](https://github.com/jaredpalmer/kev#models)).

## Review tasks and questions

For every model task, generate candidates deterministically, put **all evidence the question needs** in `state`, and ask bounded questions. The model cannot discover every possible missing entity or produce a reasoned explanation from its API. Any displayed reason must be a template derived from the question, evidence, and score, with the supporting source locations from SpecForge.

| Task | Candidate state | Example question | Treatment |
| --- | --- | --- | --- |
| Node quality | One entity with kind, title, fields, `verify` statements and close neighbors | Choice: `actionable`, `ambiguous`, `inconsistent`, `insufficient evidence` under a fixed rubric | Suggest precise missing information; ask human to edit. |
| Verify quality | Claim text plus each obligation and relevant test name/result | Noul: "Would this exact obligation, if satisfied, provide evidence for the claim?" | Never mark proven. Flag a weak or tautological obligation for review. |
| Edge semantics | Source and target excerpts, edge label definition, nearby alternatives | Choice: `supports relation`, `contradicts relation`, `unrelated`, `unclear` | Question a present edge; retain deterministic existence check. |
| Missing edge | Two aligned, bounded subgraphs and a schema-legal candidate relation absent on one side | Choice: `missing relation`, `represented elsewhere`, `intentional difference`, `unclear` | Propose the specific edge or missing node; never silently create one. |
| Traceability completeness | A feature/journey and reachable behaviors, invariants, tests and uncovered paths | Choice: `complete`, `missing behavior`, `missing failure case`, `missing verification`, `unclear` | Review a declared scope and policy, not the universe of possible requirements. |
| Natural-language contradiction | Two potentially conflicting claims, each with identity, scope, conditions, and source text | Choice: `contradiction`, `compatible`, `different scope`, `unclear` | Show both source locations; human decides. Formal bounds still go to Z3. |

Use two directional questions for sensitive pairs (A conflicts with B, B conflicts with A), require agreement or abstain, and send the same candidate with swapped choice order on a small evaluation sample. This reduces order and framing surprises; it does not turn the classifier into a proof engine. Kev supplies a `/v1/systemone/permute` endpoint for testing order effects ([Kev API](https://github.com/jaredpalmer/kev#api)).

## Evidence and output contract

Suggested request for one edge candidate (illustrative, not yet an API):

```json
{
  "model": "kev-latest",
  "state": {
    "source": {"kind": "behavior", "id": "sync_reconnect", "text": "Upload queued changes on reconnect."},
    "target": {"kind": "feature", "id": "offline_sync", "text": "Changes queue while offline and upload after reconnect."},
    "relation": {"label": "BehaviorImplementsFeature", "meaning": "The behavior implements the feature."}
  },
  "questions": {
    "edge_support": {
      "type": "choice",
      "instructions": "Does the source behavior implement the target feature, based only on the supplied text?",
      "criteria": {"supported": "Direct support", "unsupported": "No support", "unclear": "Insufficient detail"}
    }
  }
}
```

For actual findings, capture `task`, entity IDs, relation, input hash, source spans, question/rubric version, model revision, raw distribution, threshold policy, and timestamp outside the content hash. Cache by graph neighborhood hash plus model and rubric versions. Refuse truncated inputs, invalid answers, stale graph hashes, missing local service, and out-of-range probabilities as `review unavailable` rather than as spec errors. Keep the payload local by default; a remote System One endpoint should be an explicit separate configuration.

Do not interpret Choice `confidence` as a validation probability. For a finding such as contradiction, use the probability of the named option and calibrate it on labeled SpecForge examples. Include an abstain/unclear option. Review output should include exact evidence snippets and locations, but the text "why" remains a reviewer-authored explanation until a generative component or a formal derivation supplies one.

## Feasible delivery sequence

1. **Benchmark first:** label 100–300 real and synthetic SpecForge cases across node quality, relation support, missing relation, weak `verify`, and contradiction. Include hard negatives: shared vocabulary without conflict, changed scope/time, intentionally optional nodes, absent extensions, and equivalent paraphrases. Keep entire projects apart between training and test to avoid leakage. Measure precision/recall, calibration, abstention, latency, and performance by task and input length for Kev-0.8B and Kev-4B. Use `kev-4b@v1.0` or a pinned digest for reproducibility.
2. **Local prototype:** export graph, extract overlapping channel neighborhoods, align CLI/MCP capability motifs, call local Kev on a small candidate set, and emit a review JSON and terminal report. Start with the coverage, trace, validate, and export capabilities in SpecForge's own spec; label both real omissions and intentional asymmetries. Then add edge semantic support and `verify` quality. Compare against human judgments before adding a CI gate.
3. **Host integration:** add an explicit `Review` operation and System One provider port in `specforge-ops` or a dedicated crate. Reuse `ProjectView` and source span conventions. CLI first; MCP tool next. A background, debounced LSP review can follow once latency and caching are acceptable.
4. **Complete-spec and contradiction review:** construct traceability paths and candidate claim pairs deterministically; use Kev to rank only candidates. Add review resolution/suppression with a reason and input fingerprint, so accepted exceptions do not return after every run.
5. **Fine-tune only if benchmark warrants it:** train on labeled SpecForge question/state/answer examples using Kev's supported JSONL format and `--init_from` a released checkpoint ([Kev fine-tuning guide](https://github.com/jaredpalmer/kev#fine-tune-on-your-own-data)). Recalibrate thresholds on a held-out project set. Evaluate the tuned model against the untouched release and retain rollback.

## Decision to make after the prototype

Define the exact meaning of **semantic review finding** (probabilistic concern requiring judgment) separately from **diagnostic** (compiler rule failure) and **proven obligation** (test/formal evidence). This distinction protects the existing `Verdict` contract and gives users an honest account of what Kev established.
