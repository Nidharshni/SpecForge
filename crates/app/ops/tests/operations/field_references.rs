//! What a field's value references is decided by the field's declared type (ADR 0093), not by how the value was
//! written. Each pin asserts the target behavior and is ignored until the ticket that flips it; the two live
//! tests at the end keep the behaviors that were already right.

use super::compiled_support::{TestProject, analyzed, check_document, exported};
use serde_json::Value;
use specforge_ops::analysis::analyze::AnalyzeOptions;
use specforge_ops::analysis::check::CheckOptions;
use specforge_project::CompiledProject;
use specforge_test_macros::test as specforge_test;

/// A project enabling `extensions`, `spec` as its one file under `spec/`.
fn compile(extensions: &[&str], spec: &str) -> (TestProject, CompiledProject) {
    let project = TestProject::new()
        .enabling(extensions)
        .config(|c| c["spec_root"] = "spec".into())
        .file("spec/main.spec", spec);
    let compiled = project.compile();
    (project, compiled)
}

fn product(spec: &str) -> (TestProject, CompiledProject) {
    compile(&["@specforge/product", "@specforge/software"], spec)
}

/// The diagnostics of `specforge check`.
fn diagnostics(compiled: &CompiledProject) -> Vec<Value> {
    check_document(compiled, CheckOptions::default())
}

/// The diagnostics of one code.
fn with_code<'d>(diags: &'d [Value], code: &str) -> Vec<&'d Value> {
    diags.iter().filter(|d| d["code"] == code).collect()
}

/// The exported edges as `(source, label, target)`.
fn edges(compiled: &CompiledProject) -> Vec<(String, String, String)> {
    exported(compiled)["edges"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| {
            (
                e["source"].as_str().unwrap().to_string(),
                e["label"].as_str().unwrap().to_string(),
                e["target"].as_str().unwrap().to_string(),
            )
        })
        .collect()
}

fn has_edge(edges: &[(String, String, String)], source: &str, label: &str, target: &str) -> bool {
    edges.contains(&(source.into(), label.into(), target.into()))
}

#[specforge_test(
    behavior = "link_entity_references",
    verify = "a string list's items reference nothing but scheme ref IDs"
)]
fn numbers_on_a_string_list_reference_nothing() {
    let (_p, compiled) = product(
        "term t1 \"T1\" { definition \"one\" see_also [t2] }\n\
         term t2 \"T2\" { definition \"two\" aliases [1, 2] }\n",
    );
    let diags = diagnostics(&compiled);
    assert_eq!(with_code(&diags, "E003"), Vec::<&Value>::new(), "{diags:?}");
}

#[specforge_test(
    behavior = "link_entity_references",
    verify = "a string list's items reference nothing but scheme ref IDs"
)]
fn a_bare_word_on_a_string_list_references_nothing() {
    let (_p, compiled) = product(
        "term t1 \"T1\" { definition \"one\" see_also [t4] }\n\
         term t4 \"T4\" { definition \"four\" aliases alpha }\n",
    );
    let diags = diagnostics(&compiled);
    assert_eq!(with_code(&diags, "E003"), Vec::<&Value>::new(), "{diags:?}");

    let (_p, compiled) = product(
        "term t1 \"T1\" { definition \"one\" }\n\
         term t2 \"T2\" { definition \"two\" aliases [t1] }\n",
    );
    assert!(
        !has_edge(&edges(&compiled), "t2", "aliases", "t1"),
        "a string list item is a string, not a reference"
    );
    let diags = diagnostics(&compiled);
    assert!(
        with_code(&diags, "I010")
            .iter()
            .any(|d| d["message"].as_str().unwrap().contains("'t1'")),
        "t1 has no edges: {diags:?}"
    );
}

#[specforge_test(
    behavior = "link_entity_references",
    verify = "a string list's items reference nothing but scheme ref IDs"
)]
fn booleans_and_bare_words_on_a_string_list_reference_nothing() {
    let (_p, compiled) = product(
        "feature f1 \"F1\" { status proposed contributors [alice, bob] }\n\
         term t1 \"T1\" { definition \"one\" aliases [true, false] }\n",
    );
    let diags = diagnostics(&compiled);
    assert_eq!(with_code(&diags, "E003"), Vec::<&Value>::new(), "{diags:?}");
}

#[specforge_test(
    behavior = "link_entity_references",
    verify = "a quoted id on a reference list field is a reference"
)]
fn a_quoted_id_on_a_reference_list_links() {
    let (_p, compiled) = product(
        "term t1 \"T1\" { definition \"one\" }\n\
         term t5 \"T5\" { definition \"five\" see_also \"t1\" }\n",
    );
    assert!(has_edge(&edges(&compiled), "t5", "see_also", "t1"));
    let diags = diagnostics(&compiled);
    assert!(
        !with_code(&diags, "I010")
            .iter()
            .any(|d| d["message"].as_str().unwrap().contains("'t1'")),
        "t1 is referenced: {diags:?}"
    );
}

#[specforge_test(
    behavior = "link_entity_references",
    verify = "a quoted id on a reference list field is a reference"
)]
fn quoted_unknown_ids_on_a_reference_list_are_e003() {
    let (_p, compiled) = product(
        "persona dev \"Dev\" { description \"d\" }\n\
         journey j3 \"J3\" { persona dev channels [\"web\",\"cli\"] flow \"f\" }\n",
    );
    let diags = diagnostics(&compiled);
    let e003 = with_code(&diags, "E003");
    assert_eq!(e003.len(), 2, "{diags:?}");
    for (d, name) in e003.iter().zip(["web", "cli"]) {
        assert!(d["message"].as_str().unwrap().contains(name), "{d}");
        assert_eq!(d["span"]["start_line"], 2, "{d}");
    }
}

#[specforge_test(
    behavior = "link_entity_references",
    verify = "a string list's items reference nothing but scheme ref IDs"
)]
fn a_mixed_list_on_a_string_list_is_no_error() {
    let (_p, compiled) = product("term t1 \"T1\" { definition \"one\" aliases [\"a\", b] }\n");
    let diags = diagnostics(&compiled);
    assert_eq!(with_code(&diags, "E001"), Vec::<&Value>::new(), "{diags:?}");
    assert_eq!(with_code(&diags, "E061"), Vec::<&Value>::new(), "{diags:?}");
    let export = exported(&compiled);
    let node = export["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|n| n["id"] == "t1")
        .unwrap();
    assert_eq!(node["fields"]["aliases"], serde_json::json!(["a", "b"]));
}

#[specforge_test(
    behavior = "link_entity_references",
    verify = "an open-fields kind's undeclared field references nothing"
)]
fn an_open_kinds_undeclared_list_references_nothing() {
    let (_p, compiled) = product(
        "type Level \"Level\" { levels [critical, high] }\n\
         type Severity \"Severity\" { values [critical, high] }\n",
    );
    let diags = diagnostics(&compiled);
    assert_eq!(with_code(&diags, "E003"), Vec::<&Value>::new(), "{diags:?}");
}

#[specforge_test(
    behavior = "link_entity_references",
    verify = "a single reference that resolves to nothing is E003"
)]
fn an_unresolved_single_reference_is_e003() {
    for spec in [
        "channel cli \"CLI\" { description \"c\" }\n\
         journey j1 \"J1\" { persona ghost_persona channels [cli] flow \"f\" }\n",
        "term t1 \"T1\" { definition \"one\" module ghost_mod }\n",
    ] {
        let (_p, compiled) = product(spec);
        let diags = diagnostics(&compiled);
        let e003 = with_code(&diags, "E003");
        assert_eq!(e003.len(), 1, "{diags:?}");
        let name = if spec.contains("ghost_persona") {
            "ghost_persona"
        } else {
            "ghost_mod"
        };
        assert!(e003[0]["message"].as_str().unwrap().contains(name));
        let field = if name == "ghost_persona" {
            "persona"
        } else {
            "module"
        };
        assert_eq!(e003[0]["data"]["field"], field, "{}", e003[0]);
    }
}

#[specforge_test(
    behavior = "link_entity_references",
    verify = "a quoted single reference is a reference"
)]
fn a_quoted_single_reference_links() {
    let (_p, compiled) = product(
        "persona dev \"Dev\" { description \"d\" }\n\
         channel cli \"CLI\" { description \"c\" }\n\
         journey j1 \"J1\" { persona \"dev\" channels [cli] flow \"f\" }\n",
    );
    assert!(has_edge(&edges(&compiled), "j1", "persona", "dev"));
    let diags = diagnostics(&compiled);
    assert!(
        !with_code(&diags, "I046")
            .iter()
            .any(|d| d["message"].as_str().unwrap().contains("'dev'")),
        "dev is referenced: {diags:?}"
    );
}

#[specforge_test(
    behavior = "link_entity_references",
    verify = "a single reference to an existing entity of another kind than its field's target kind is E022 at the reference"
)]
fn a_single_reference_to_the_wrong_kind_is_e022() {
    let (_p, compiled) = product(
        "channel cli \"CLI\" { description \"c\" }\n\
         term glossary_term \"G\" { definition \"g\" }\n\
         journey j1 \"J1\" { persona glossary_term channels [cli] flow \"f\" }\n",
    );
    let diags = diagnostics(&compiled);
    let e022 = with_code(&diags, "E022");
    assert_eq!(e022.len(), 1, "{diags:?}");
    let message = e022[0]["message"].as_str().unwrap();
    for word in ["glossary_term", "persona", "term"] {
        assert!(message.contains(word), "{message}");
    }
}

#[specforge_test(
    behavior = "report_entities_without_contract_obligations",
    verify = "a single contract reference is a contract obligation"
)]
fn a_single_contract_reference_is_a_contract_obligation() {
    let (_p, compiled) = compile(
        &["@specforge/governance", "@specforge/software"],
        "invariant inv_one \"Inv\" { guarantee \"holds\" }\n\
         failure_mode fm_one \"FM\" { invariant inv_one }\n",
    );
    let doc = analyzed(
        &compiled,
        &AnalyzeOptions {
            pass: "contracts".into(),
            ..Default::default()
        },
    );
    let findings = doc["passes"][0]["findings"].as_array().unwrap();
    assert!(
        !findings
            .iter()
            .any(|f| f["code"] == "A010" && f["message"].as_str().unwrap().contains("fm_one")),
        "{doc}"
    );
}

// -- Behaviors that were already right, pinned live ------------------------------------------------------

#[test]
fn a_scheme_ref_id_on_a_string_list_links_its_ref() {
    let (_p, compiled) = product(
        "ref gh.issue:42 \"Issue\"\n\
         feature f1 \"F1\" { status proposed refs [gh.issue:42] }\n",
    );
    let diags = diagnostics(&compiled);
    assert_eq!(with_code(&diags, "E003"), Vec::<&Value>::new(), "{diags:?}");
    assert!(has_edge(&edges(&compiled), "f1", "refs", "gh.issue:42"));
}

#[test]
fn an_undeclared_fields_bare_list_links() {
    let (_p, compiled) = product(
        "term t1 \"T1\" { definition \"one\" foo [t2] }\n\
         term t2 \"T2\" { definition \"two\" }\n",
    );
    let diags = diagnostics(&compiled);
    assert_eq!(with_code(&diags, "W020").len(), 1, "{diags:?}");
    assert!(has_edge(&edges(&compiled), "t1", "foo", "t2"));
}

#[specforge_test(
    behavior = "link_entity_references",
    verify = "a single reference to an existing entity of another kind than its field's target kind is E022 at the reference"
)]
fn a_wrong_kind_reference_is_e022_at_the_reference_with_help() {
    let (_p, compiled) = product(
        "channel cli \"CLI\" { description \"c\" }\n\
         term glossary_term \"G\" { definition \"g\" }\n\
         journey j1 \"J1\" { persona glossary_term channels [cli] flow \"f\" }\n",
    );
    let diags = diagnostics(&compiled);
    let e022 = with_code(&diags, "E022");
    assert_eq!(e022.len(), 1, "{diags:?}");
    let span = &e022[0]["span"];
    let line = "journey j1 \"J1\" { persona ";
    assert_eq!(span["start_line"], 3, "{span}");
    assert_eq!(span["start_col"], line.len() + 1, "{span}");
    assert_eq!(
        e022[0]["suggestion"], "name a persona here, or drop the reference",
        "{span}"
    );
}
