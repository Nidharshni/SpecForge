//! The @specforge/formal diagnostics ADR 0040 built, run end to end over the
//! vendored formal component: the declarative rules and the check-phase pass
//! through the check operation, the analysis passes through the analyze
//! operation.

use super::compiled_support::{TestProject, analyzed, check_document};
use serde_json::Value;
use specforge_ops::analyze::{AnalyzeOptions, ReportSource};
use specforge_ops::check::CheckOptions;
use specforge_test_macros::test as specforge_test;

/// A project enabling @specforge/formal and its peer @specforge/software,
/// with `spec` as its one spec file.
fn project(spec: &str) -> TestProject {
    project_with(&["@specforge/formal", "@specforge/software"], spec)
}

/// [`project`] with @specforge/testing too, which makes behaviors
/// testable, so the coverage rule counts them.
fn tested_project(spec: &str) -> TestProject {
    project_with(
        &[
            "@specforge/formal",
            "@specforge/software",
            "@specforge/testing",
        ],
        spec,
    )
}

fn project_with(extensions: &[&str], spec: &str) -> TestProject {
    TestProject::new()
        .without_config()
        .raw_config(format!(
            r#"{{"extensions": {}}}"#,
            serde_json::json!(extensions)
        ))
        .file("main.spec", spec)
}

/// One reported diagnostic: its code, message and suggestion.
#[derive(Debug)]
struct Finding {
    code: String,
    message: String,
    suggestion: String,
}

fn finding(d: &Value) -> Finding {
    Finding {
        code: d["code"].as_str().unwrap().to_string(),
        message: d["message"].as_str().unwrap().to_string(),
        suggestion: d["suggestion"].as_str().unwrap_or_default().to_string(),
    }
}

/// What `specforge check` reports for the project.
fn check(project: &TestProject) -> Vec<Finding> {
    check_document(&project.compile(), CheckOptions::default())
        .iter()
        .map(finding)
        .collect()
}

/// What formal's analyze pass `pass` reports under `specforge analyze`, over the recorded test results.
fn analyze(project: &TestProject, pass: &str) -> Vec<Finding> {
    analyze_over(project, pass, ReportSource::Recorded)
}

/// [`analyze`] over `report`.
fn analyze_over(project: &TestProject, pass: &str, report: ReportSource) -> Vec<Finding> {
    let doc = analyzed(
        &project.compile(),
        &AnalyzeOptions {
            report,
            ..Default::default()
        },
    );
    let name = format!("@specforge/formal:{pass}");
    doc["passes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["pass"] == name.as_str())
        .unwrap_or_else(|| panic!("{name} was not dispatched: {doc}"))["findings"]
        .as_array()
        .unwrap()
        .iter()
        .map(finding)
        .collect()
}

/// The messages of the `code` findings.
fn messages<'a>(findings: &'a [Finding], code: &str) -> Vec<&'a str> {
    findings
        .iter()
        .filter(|f| f.code == code)
        .map(|f| f.message.as_str())
        .collect()
}

// ── Declarative rules (every check) ─────────────────────────────

/// An entity of each formal kind with a blank description, and one with
/// a written one; the behaviors the refinements name.
const DESCRIBED: &str = r#"
behavior spec_side "Spec side" {
  contract "c"
}
behavior impl_side "Impl side" {
  contract "c"
}
property p_blank "P blank" {
  expression    "x"
  property_type safety
  description   ""
}
property p_told "P told" {
  expression    "x"
  property_type safety
  description   "what it asserts"
}
axiom a_blank "A blank" {
  expression  "x"
  description "   "
}
axiom a_told "A told" {
  expression  "x"
  description "why it is assumed"
}
protocol pr_blank "Pr blank" {
  alphabet      ["go"]
  initial_state "idle"
  description   ""
}
protocol pr_told "Pr told" {
  alphabet      ["go"]
  initial_state "idle"
  description   "the handshake"
}
refinement r_blank "R blank" {
  abstract_entity  spec_side
  concrete_entity  impl_side
  invariant_deltas ["adds a bound"]
  description      ""
}
refinement r_told "R told" {
  abstract_entity  spec_side
  concrete_entity  impl_side
  invariant_deltas ["adds a bound"]
  description      "the mapping"
}
process pc_blank "Pc blank" {
  alphabet      ["go"]
  initial_state "idle"
  description   ""
}
process pc_told "Pc told" {
  alphabet      ["go"]
  initial_state "idle"
  description   "the worker"
}
"#;

/// `code` reports `blank` once, with `template`, and never `told`.
fn assert_blank_description(code: &str, kind: &str, blank: &str, told: &str) {
    let dir = project(DESCRIBED);
    let findings = check(&dir);
    assert_eq!(
        messages(&findings, code),
        [format!("{kind} '{blank}' has empty description")],
        "{findings:?}"
    );
    assert!(
        messages(&findings, code)
            .iter()
            .all(|m| !m.contains(&format!("'{told}'"))),
        "{findings:?}"
    );
}

#[specforge_test(
    behavior = "fa_validate_empty_property_description",
    verify = "property with empty description produces W124"
)]
#[specforge_test(
    behavior = "fa_validate_empty_property_description",
    verify = "property with non-empty description passes"
)]
fn a_blank_property_description_is_w124() {
    assert_blank_description("W124", "property", "p_blank", "p_told");
}

#[specforge_test(
    behavior = "fa_validate_empty_axiom_description",
    verify = "axiom with empty description produces W127"
)]
#[specforge_test(
    behavior = "fa_validate_empty_axiom_description",
    verify = "axiom with non-empty description passes"
)]
fn a_blank_axiom_description_is_w127() {
    assert_blank_description("W127", "axiom", "a_blank", "a_told");
}

#[specforge_test(
    behavior = "fa_validate_empty_protocol_description",
    verify = "protocol with empty description produces W129"
)]
#[specforge_test(
    behavior = "fa_validate_empty_protocol_description",
    verify = "protocol with non-empty description passes"
)]
fn a_blank_protocol_description_is_w129() {
    assert_blank_description("W129", "protocol", "pr_blank", "pr_told");
}

#[specforge_test(
    behavior = "fa_validate_empty_refinement_description",
    verify = "refinement with empty description produces W132"
)]
#[specforge_test(
    behavior = "fa_validate_empty_refinement_description",
    verify = "refinement with non-empty description passes"
)]
fn a_blank_refinement_description_is_w132() {
    assert_blank_description("W132", "refinement", "r_blank", "r_told");
}

#[specforge_test(
    behavior = "fa_validate_empty_process_description",
    verify = "process with empty description produces W135"
)]
#[specforge_test(
    behavior = "fa_validate_empty_process_description",
    verify = "process with non-empty description passes"
)]
fn a_blank_process_description_is_w135() {
    assert_blank_description("W135", "process", "pc_blank", "pc_told");
}

#[specforge_test(
    behavior = "fa_validate_refinement_without_delta",
    verify = "refinement with no invariant_deltas produces W133"
)]
#[specforge_test(
    behavior = "fa_validate_refinement_without_delta",
    verify = "refinement with invariant_deltas passes"
)]
fn a_refinement_without_invariant_deltas_is_w133() {
    let dir = project(concat!(
        "behavior spec_side \"Spec side\" {\n  contract \"c\"\n}\n",
        "behavior impl_side \"Impl side\" {\n  contract \"c\"\n}\n",
        "refinement unwritten \"Unwritten\" {\n",
        "  abstract_entity spec_side\n",
        "  concrete_entity impl_side\n",
        "}\n",
        "refinement emptied \"Emptied\" {\n",
        "  abstract_entity  spec_side\n",
        "  concrete_entity  impl_side\n",
        "  invariant_deltas []\n",
        "}\n",
        "refinement recorded \"Recorded\" {\n",
        "  abstract_entity  spec_side\n",
        "  concrete_entity  impl_side\n",
        "  invariant_deltas [\"adds a retry bound\"]\n",
        "}\n",
    ));
    let findings = check(&dir);
    let mut w133 = messages(&findings, "W133");
    w133.sort_unstable();
    assert_eq!(
        w133,
        [
            "refinement 'emptied' declares no invariant_deltas",
            "refinement 'unwritten' declares no invariant_deltas",
        ],
        "{findings:?}"
    );
}

#[specforge_test(
    behavior = "fa_validate_process_without_alphabet",
    verify = "process with empty alphabet produces W136"
)]
#[specforge_test(
    behavior = "fa_validate_process_without_alphabet",
    verify = "process with non-empty alphabet passes"
)]
fn a_process_with_an_empty_alphabet_is_w136() {
    let dir = project(concat!(
        "event go \"Go\" {\n  channel \"go\"\n}\n",
        "process silent \"Silent\" {\n",
        "  alphabet      []\n",
        "  initial_state \"idle\"\n",
        "}\n",
        "process talking \"Talking\" {\n",
        "  alphabet      [\"go\"]\n",
        "  initial_state \"idle\"\n",
        "}\n",
    ));
    let findings = check(&dir);
    assert_eq!(
        messages(&findings, "W136"),
        ["process 'silent' has no alphabet (no events declared)"],
        "{findings:?}"
    );
}

// ── The check-phase pass ────────────────────────────────────────

#[specforge_test(
    behavior = "fa_emit_formal_analysis_available",
    verify = "behaviors with requires/ensures trigger I015 info"
)]
fn conditioned_behaviors_are_noted_once_per_check_as_i015() {
    let dir = project(concat!(
        "behavior obligated \"Obligated\" {\n",
        "  contract \"c\"\n",
        "  requires {\n    ready \"it is ready\"\n  }\n",
        "  ensures {\n    done \"it is done\"\n  }\n",
        "}\n",
        "behavior promising \"Promising\" {\n",
        "  contract \"c\"\n",
        "  ensures {\n    done \"it is done\"\n  }\n",
        "}\n",
    ));
    let findings = check(&dir);
    assert_eq!(
        messages(&findings, "I015"),
        ["2 behavior(s) declare requires/ensures; `specforge analyze` checks them"],
        "{findings:?}"
    );

    let plain = project("behavior plain \"Plain\" {\n  contract \"c\"\n}\n");
    assert!(messages(&check(&plain), "I015").is_empty());
}

// ── event_graph_analyze ─────────────────────────────────────────

/// `a` produces `ping`, `b` consumes it and produces `pong`, `a` consumes
/// `pong`; `sync` is written on `pong` when given.
fn ping_pong(sync: Option<&str>) -> TestProject {
    let pong_sync = sync
        .map(|s| format!("  sync {{\n    bound \"{s}\"\n  }}\n"))
        .unwrap_or_default();
    project(&format!(
        concat!(
            "event ping \"Ping\" {{\n  channel \"ping\"\n}}\n",
            "event pong \"Pong\" {{\n  channel \"pong\"\n{}}}\n",
            "behavior a \"A\" {{\n  contract \"c\"\n  produces [ping]\n  consumes [pong]\n}}\n",
            "behavior b \"B\" {{\n  contract \"c\"\n  consumes [ping]\n  produces [pong]\n}}\n",
        ),
        pong_sync
    ))
}

#[specforge_test(
    behavior = "fa_detect_unmitigated_cycles",
    verify = "unmitigated circular event dependency detected as E034"
)]
#[specforge_test(
    behavior = "fa_detect_unmitigated_cycles",
    verify = "E034 names a cycle path and suggests sync on a member"
)]
#[specforge_test(
    behavior = "fa_pattern_e034_unmitigated_cycle",
    verify = "SCC with no mitigations produces E034"
)]
#[specforge_test(
    behavior = "fa_pattern_e034_unmitigated_cycle",
    verify = "E034 lists full cycle path"
)]
#[specforge_test(
    behavior = "fa_pattern_e034_unmitigated_cycle",
    verify = "E034 suggests applicable mitigations"
)]
#[specforge_test(
    behavior = "fa_event_graph_analyze_pass",
    verify = "unmitigated cycle detection runs on SCC"
)]
fn an_unmitigated_event_cycle_is_e034() {
    let findings = analyze(&ping_pong(None), "event_graph_analyze");
    assert_eq!(
        messages(&findings, "E034"),
        [
            "unmitigated event cycle: a -> ping -> b -> pong -> a (no behavior or event in it declares sync)"
        ],
        "{findings:?}"
    );
    let e034 = findings.iter().find(|f| f.code == "E034").unwrap();
    assert_eq!(
        e034.suggestion,
        "declare `sync` (a timeout, barrier or delivery bound) on one of a, b, ping, pong, or break the cycle"
    );
    assert!(messages(&findings, "I009").is_empty(), "{findings:?}");
}

#[specforge_test(
    behavior = "fa_detect_unmitigated_cycles",
    verify = "cycle with a sync on one member passes silently"
)]
#[specforge_test(
    behavior = "fa_pattern_e034_unmitigated_cycle",
    verify = "SCC with a sync on any member passes"
)]
fn a_cycle_with_a_sync_on_a_member_is_not_e034() {
    let findings = analyze(&ping_pong(Some("timeout 5s")), "event_graph_analyze");
    assert!(messages(&findings, "E034").is_empty(), "{findings:?}");
}

#[specforge_test(
    behavior = "fa_detect_unmitigated_cycles",
    verify = "non-circular event dependency passes"
)]
#[specforge_test(
    behavior = "fa_emit_no_structural_cycles_info",
    verify = "cycle-free event graph produces I009"
)]
fn a_cycle_free_event_graph_is_noted_as_i009() {
    let dir = project(concat!(
        "event job \"Job\" {\n  channel \"job\"\n  sync [\"timeout 5s\"]\n}\n",
        "behavior enqueue \"Enqueue\" {\n  contract \"c\"\n  produces [job]\n}\n",
        "behavior work \"Work\" {\n  contract \"c\"\n  consumes [job]\n}\n",
    ));
    let findings = analyze(&dir, "event_graph_analyze");
    assert!(messages(&findings, "E034").is_empty(), "{findings:?}");
    assert_eq!(
        messages(&findings, "I009"),
        [
            "no unmitigated cycle in the event graph (2 behavior(s), 1 event(s)); structural only, runtime conditions are not analyzed"
        ],
        "{findings:?}"
    );
}

#[specforge_test(
    behavior = "fa_detect_unmitigated_retry_cycle",
    verify = "re-triggering without backoff detected as W032"
)]
#[specforge_test(
    behavior = "fa_detect_unmitigated_retry_cycle",
    verify = "re-triggering with timeout/backoff passes"
)]
fn a_behavior_re_producing_what_it_consumes_is_w032() {
    let retry = |sync: &str| {
        project(&format!(
            concat!(
                "event attempt \"Attempt\" {{\n  channel \"attempt\"\n}}\n",
                "behavior retry \"Retry\" {{\n  contract \"c\"\n  consumes [attempt]\n  produces [attempt]\n{}}}\n",
            ),
            sync
        ))
    };
    let findings = analyze(&retry(""), "event_graph_analyze");
    assert_eq!(
        messages(&findings, "W032"),
        [
            "behavior 'retry' consumes event 'attempt' and produces it again, with no sync on either (an unmitigated retry cycle)"
        ],
        "{findings:?}"
    );
    let bounded = analyze(&retry("  sync [\"backoff 2s\"]\n"), "event_graph_analyze");
    assert!(messages(&bounded, "W032").is_empty(), "{bounded:?}");
}

#[specforge_test(
    behavior = "fa_detect_unbounded_channel",
    verify = "event channel with no sync produces W034"
)]
#[specforge_test(
    behavior = "fa_detect_unbounded_channel",
    verify = "event channel with a sync passes"
)]
fn a_produced_event_without_sync_is_w034() {
    let dir = project(concat!(
        "event open \"Open\" {\n  channel \"open\"\n}\n",
        "event bounded \"Bounded\" {\n  channel \"bounded\"\n  sync [\"timeout 5s\"]\n}\n",
        "behavior emit \"Emit\" {\n  contract \"c\"\n  produces [open, bounded]\n}\n",
    ));
    let findings = analyze(&dir, "event_graph_analyze");
    assert_eq!(
        messages(&findings, "W034"),
        ["event 'open' is produced but declares no sync, so nothing bounds its channel"],
        "{findings:?}"
    );
}

#[specforge_test(
    behavior = "fa_detect_asymmetric_connectivity",
    verify = "port with unbalanced access pattern produces W033"
)]
#[specforge_test(
    behavior = "fa_detect_asymmetric_connectivity",
    verify = "port with single consumer passes"
)]
fn a_port_with_unbalanced_users_is_w033() {
    // `hot` is refined by four behaviors (4 incoming edges); `cold` by
    // none. `solo` is the only user of its port.
    let mut spec = String::from(concat!(
        "port store \"Store\" {\n  direction outbound\n}\n",
        "port quiet \"Quiet\" {\n  direction outbound\n}\n",
        "behavior hot \"Hot\" {\n  contract \"c\"\n  abstract true\n  ports [store]\n}\n",
        "behavior cold \"Cold\" {\n  contract \"c\"\n  ports [store]\n}\n",
        "behavior solo \"Solo\" {\n  contract \"c\"\n  ports [quiet]\n}\n",
    ));
    for i in 0..4 {
        spec.push_str(&format!(
            "behavior impl{i} \"Impl {i}\" {{\n  contract \"c\"\n  refines hot\n}}\n"
        ));
    }
    let findings = analyze(&project(&spec), "event_graph_analyze");
    assert_eq!(
        messages(&findings, "W033"),
        [
            "port 'store' is used by 2 behaviors with unbalanced connectivity: 'hot' has 4 incoming edge(s), 'cold' 0 (more than 3:1)"
        ],
        "{findings:?}"
    );
}

// ── condition_check ─────────────────────────────────────────────

#[specforge_test(
    behavior = "fa_validate_condition_consistency",
    verify = "ensures without requires produces I011 info"
)]
#[specforge_test(
    behavior = "fa_validate_condition_consistency",
    verify = "consistent requires and ensures passes"
)]
#[specforge_test(
    behavior = "fa_parse_ensures_block",
    verify = "ensures without requires produces info diagnostic"
)]
#[specforge_test(
    behavior = "fa_condition_check_pass",
    verify = "behavior with requires and no ensures produces W096"
)]
fn one_sided_conditions_are_i011_or_w096() {
    let dir = project(concat!(
        "behavior promising \"Promising\" {\n",
        "  contract \"c\"\n",
        "  ensures {\n    done \"it is done\"\n  }\n",
        "}\n",
        "behavior demanding \"Demanding\" {\n",
        "  contract \"c\"\n",
        "  requires {\n    ready \"it is ready\"\n  }\n",
        "}\n",
        "behavior contracted \"Contracted\" {\n",
        "  contract \"c\"\n",
        "  requires {\n    ready \"it is ready\"\n  }\n",
        "  ensures {\n    done \"it is done\"\n  }\n",
        "}\n",
    ));
    let findings = analyze(&dir, "condition_check");
    assert_eq!(
        messages(&findings, "I011"),
        ["behavior 'promising' declares ensures but no requires"],
        "{findings:?}"
    );
    assert_eq!(
        messages(&findings, "W096"),
        ["behavior 'demanding' declares requires but no ensures"],
        "{findings:?}"
    );
}

#[specforge_test(
    behavior = "fa_detect_redundant_precondition",
    verify = "precondition repeated in its requires block produces W039"
)]
#[specforge_test(
    behavior = "fa_detect_redundant_precondition",
    verify = "independent precondition passes"
)]
fn a_requires_naming_a_condition_twice_is_w039() {
    let dir = project(concat!(
        "behavior twice \"Twice\" {\n",
        "  contract \"c\"\n",
        "  requires {\n    ready \"it is ready\"\n    open \"it is open\"\n    ready \"still ready\"\n  }\n",
        "  ensures {\n    done \"it is done\"\n  }\n",
        "}\n",
        "behavior once \"Once\" {\n",
        "  contract \"c\"\n",
        "  requires {\n    ready \"it is ready\"\n    open \"it is open\"\n  }\n",
        "  ensures {\n    done \"it is done\"\n  }\n",
        "}\n",
    ));
    let findings = analyze(&dir, "condition_check");
    assert_eq!(
        messages(&findings, "W039"),
        ["behavior 'twice' requires 'ready' more than once (the repeat is redundant)"],
        "{findings:?}"
    );
}

#[specforge_test(
    behavior = "fa_detect_invariant_without_property",
    verify = "invariant with prose-only guarantee produces W040"
)]
#[specforge_test(
    behavior = "fa_detect_invariant_without_property",
    verify = "invariant with an expression passes"
)]
fn a_prose_only_invariant_is_w040() {
    let dir = project(concat!(
        "invariant prose \"Prose\" {\n  guarantee \"ids are unique\"\n}\n",
        "invariant formal \"Formal\" {\n",
        "  guarantee  \"ids are unique\"\n",
        "  expression \"count >= 0\"\n",
        "}\n",
    ));
    let findings = analyze(&dir, "condition_check");
    assert_eq!(
        messages(&findings, "W040"),
        ["invariant 'prose' states its guarantee in prose only (no expression)"],
        "{findings:?}"
    );
}

// ── coverage_tracking ───────────────────────────────────────────

/// `analyze` with `report` as the recorded test results.
fn analyze_with_results(dir: &TestProject, report: Value) -> Vec<Finding> {
    dir.write("report.json", report.to_string());
    analyze_over(
        dir,
        "coverage_tracking",
        ReportSource::File(dir.root().join("report.json")),
    )
}

#[specforge_test(
    behavior = "fa_emit_coverage_item_covered_info",
    verify = "coverage item covered by test produces I008"
)]
fn an_item_every_test_proves_is_i008_naming_the_tests() {
    let dir = tested_project(concat!(
        "behavior proven \"Proven\" {\n",
        "  contract \"c\"\n",
        "  verify unit \"it works\"\n",
        "  verify unit \"it fails safely\"\n",
        "}\n",
        "behavior halfway \"Halfway\" {\n",
        "  contract \"c\"\n",
        "  verify unit \"it works\"\n",
        "  verify unit \"it fails safely\"\n",
        "}\n",
    ));
    let findings = analyze_with_results(
        &dir,
        serde_json::json!({"runner": "manual", "results": {
            "proven": {"tests": [
                {"name": "works", "status": "passed", "verify": "it works"},
                {"name": "fails_safely", "status": "passed", "verify": "it fails safely"}
            ]},
            "halfway": {"tests": [
                {"name": "works", "status": "passed", "verify": "it works"}
            ]}
        }}),
    );
    assert_eq!(
        messages(&findings, "I008"),
        ["behavior 'proven': all 2 obligation(s) proven by recorded test(s): fails_safely, works"],
        "{findings:?}"
    );
}

/// Behaviors at each depth: `bare` (prose), `linked` (entity_graph),
/// `conditioned` (conditions), `kept` (invariants), `proven` (proofs, with
/// the results [`depth_results`] records).
const DEPTHS: &str = r#"
invariant unique_ids "Unique ids" {
  guarantee "ids are unique"
}
behavior bare "Bare" {
  contract "c"
}
behavior linked "Linked" {
  contract   "c"
  invariants [unique_ids]
}
behavior conditioned "Conditioned" {
  contract "c"
  ensures {
    done "it is done"
  }
}
behavior kept "Kept" {
  contract  "c"
  ensures {
    done "it is done"
  }
  maintains {
    unique_ids "ids stay unique"
  }
}
behavior proven "Proven" {
  contract  "c"
  ensures {
    done "it is done"
  }
  maintains {
    unique_ids "ids stay unique"
  }
  verify unit "it works"
}
"#;

fn depth_results() -> serde_json::Value {
    serde_json::json!({"runner": "manual", "results": {
        "proven": {"tests": [{"name": "works", "status": "passed", "verify": "it works"}]}
    }})
}

#[specforge_test(
    behavior = "fa_detect_specification_depth",
    verify = "entity with requires/ensures computes as Level 2 (conditions)"
)]
#[specforge_test(
    behavior = "fa_detect_specification_depth",
    verify = "entity with maintains and invariant refs computes as Level 3 (invariants)"
)]
#[specforge_test(
    behavior = "fa_detect_specification_depth",
    verify = "entity with all obligations proven computes as Level 4 (proofs)"
)]
#[specforge_test(
    behavior = "fa_detect_specification_depth",
    verify = "entity at Level 2+ emits I014"
)]
#[specforge_test(
    behavior = "fa_detect_specification_depth",
    verify = "entity at Level 0 emits no depth diagnostic"
)]
fn behaviors_from_level_two_report_their_specification_depth() {
    let dir = tested_project(DEPTHS);
    let findings = analyze_with_results(&dir, depth_results());
    let mut i014 = messages(&findings, "I014");
    i014.sort_unstable();
    assert_eq!(
        i014,
        [
            "behavior 'conditioned' is at specification depth 'conditions' (level 2 of 4)",
            "behavior 'kept' is at specification depth 'invariants' (level 3 of 4)",
            "behavior 'proven' is at specification depth 'proofs' (level 4 of 4)",
        ],
        "bare and linked are below level 2: {findings:?}"
    );
    let kept = findings
        .iter()
        .find(|f| f.code == "I014" && f.message.contains("'kept'"))
        .unwrap();
    assert!(
        kept.suggestion.contains("level 4 (proofs)"),
        "the step to the next level: {kept:?}"
    );
}

#[specforge_test(
    behavior = "fa_detect_specification_depth",
    verify = ">5 prose-only behaviors triggers adoption nudge in I014"
)]
fn many_shallow_behaviors_earn_the_adoption_note() {
    let mut spec = String::from(DEPTHS);
    for i in 0..4 {
        spec.push_str(&format!(
            "behavior prose{i} \"Prose {i}\" {{\n  contract \"c\"\n}}\n"
        ));
    }
    let findings = analyze_with_results(&tested_project(&spec), depth_results());
    let note = "6 behavior(s) are at specification depth 'prose' or 'entity_graph' (no requires or ensures)";
    assert!(messages(&findings, "I014").contains(&note), "{findings:?}");

    let few = analyze_with_results(&tested_project(DEPTHS), depth_results());
    assert!(
        messages(&few, "I014")
            .iter()
            .all(|m| !m.contains("'prose' or 'entity_graph'")),
        "two shallow behaviors earn no note: {few:?}"
    );
}
