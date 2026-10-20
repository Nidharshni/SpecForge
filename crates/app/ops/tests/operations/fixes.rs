//! The fixes a diagnostic's data or the graph names (`specforge_ops::read::navigate`), over compiled projects.

use crate::navigate::{Compiled, LIMIT, LOGIN, SOFTWARE, at, compile, nav, span};
use specforge_common::{Diagnostic, DiagnosticData, SourceSpan, Sym};
use specforge_extension_sdk::prelude::*;
use specforge_ops::authoring::fix::{self, Fix, FixKind, FixQuery, FixSource};
use specforge_ops::read::navigate::{Navigator, TextEdit, apply_edits};
use specforge_ops::view::ProjectView;
use specforge_project::CompiledProject;
use specforge_project::testing::compile_at;
use specforge_test::prelude::*;
use std::sync::Arc;

/// `text` with `edits` (spans of it) applied.
pub(crate) fn apply(text: &str, edits: &[TextEdit]) -> String {
    apply_edits(text, edits).expect("the fix's edits lie in the text")
}

/// The fixes of `p`'s own diagnostics for `query`.
pub(crate) fn fixes_of(p: &Compiled, query: &FixQuery) -> Vec<Fix> {
    fix::fixes(&p.navigator(), &p.project.diagnostics(), query).unwrap()
}

fn titles(fixes: &[Fix]) -> Vec<String> {
    fixes.iter().map(|f| f.title.clone()).collect()
}

const TESTING: &[&str] = &["@specforge/software", "@specforge/testing"];
const UNTESTED: &str = "behavior first \"First\" {\n  contract \"c\"\n}\n\n\
                        behavior second \"Second\" {\n  contract \"c\"\n  verify unit \"s\"\n}\n";

#[specforge_test(
    behavior = "code_actions_for_missing_verify",
    verify = "code action offered on untested testable entity"
)]
fn a_verify_stub_is_offered_for_an_entity_without_obligations() {
    let p = compile(TESTING, &[("flows.spec", UNTESTED)]);
    let fixes = fixes_of(&p, &FixQuery::default());
    let stubs: Vec<&Fix> = fixes
        .iter()
        .filter(|f| f.source == FixSource::AddVerifyStub)
        .collect();
    assert_eq!(
        stubs.len(),
        1,
        "only first lacks verify: {:?}",
        titles(&fixes)
    );
    assert_eq!(stubs[0].title, "Add verify stub for first");
    assert_eq!(stubs[0].subject, Some(Sym::new("first")));
}

/// The verify stubs of `fixes`: each one's subject and the code it fixes.
fn stubs(fixes: Vec<Fix>) -> Vec<(String, Option<String>)> {
    fixes
        .into_iter()
        .filter(|f| f.source == FixSource::AddVerifyStub)
        .map(|f| (f.subject.unwrap().to_string(), f.diagnostic_code))
        .collect()
}

#[specforge_test(
    behavior = "code_actions_for_missing_verify",
    verify = "no verify stub is offered for an entity a union body or an exempting flag exempts"
)]
fn no_verify_stub_for_an_entity_its_structure_exempts() {
    let p = compile(
        TESTING,
        &[(
            "t.spec",
            "type Status = open | done\n\ntype Plain \"Plain\" {\n  id string\n}\n",
        )],
    );
    // W004 exempts the union, so its stub would fix nothing (and, with no
    // block to hold it, break the file).
    assert_eq!(
        stubs(fixes_of(&p, &FixQuery::default())),
        [("Plain".to_string(), Some("W004".to_string()))]
    );
    let w004: Vec<String> = p
        .project
        .diagnostics()
        .into_iter()
        .filter(|d| d.code == "W004")
        .map(|d| d.message)
        .collect();
    assert_eq!(w004.len(), 1, "{w004:?}");
    assert!(w004[0].contains("'Plain'"), "{w004:?}");
}

#[specforge_test(
    behavior = "code_actions_for_missing_verify",
    verify = "a verify stub fixes the diagnostic that reports its entity, or none when nothing reports it"
)]
fn a_verify_stub_is_attributed_to_the_rule_that_reports_its_entity() {
    let extensions = [
        "@specforge/software",
        "@specforge/testing",
        "@specforge/governance",
    ];
    let p = compile(
        &extensions,
        &[(
            "a.spec",
            "behavior first \"First\" {\n  contract \"c\"\n}\n\n\
             failure_mode crash \"Crash\" {\n  cause \"c\"\n}\n",
        )],
    );
    // `first` is reported (W004); a failure mode is testable but no rule
    // obliges its kind, so its stub fixes no diagnostic.
    let reported: Vec<String> = p
        .project
        .diagnostics()
        .into_iter()
        .filter(|d| d.code == "W004")
        .map(|d| d.message)
        .collect();
    assert_eq!(reported.len(), 1, "{reported:?}");
    assert_eq!(
        stubs(fixes_of(&p, &FixQuery::default())),
        [
            ("first".to_string(), Some("W004".to_string())),
            ("crash".to_string(), None),
        ]
    );
}

/// The §3 probe's kinds and one rule: `item` (testable, accepts verify,
/// `abstract` exempts), `note` (accepts verify), `memo` (accepts none)
/// and `P300`, an obligation rule with no target kind.
fn untargeted_rule() -> specforge_extension_sdk::prelude::ContributionsBuilder {
    use specforge_extension_sdk::prelude::*;
    let mut c = ContributionsBuilder::new(ExtensionMeta::new("@pin/untargeted", "0.1.0"));
    c.kind("item", |k| {
        k.testable(true).supports_verify(true).open_fields(true);
        k.field("abstract", |f| {
            f.field_type(FieldType::Bool).exempts_obligations();
        });
    });
    c.kind("note", |k| {
        k.supports_verify(true).open_fields(true);
    });
    c.kind("memo", |k| {
        k.open_fields(true);
    });
    c.rule("P300", |r| {
        r.check(CheckKind::NoVerifyStatements)
            .field("verify")
            .message_template("{kind} '{id}' declares no verify obligations");
    });
    c
}

#[specforge_test(
    behavior = "snapshot_entities_once",
    verify = "a rule without a target kind applies to every kind, for the rule, the standing and the verify stub alike"
)]
fn an_untargeted_obligation_rule_stubs_every_kind_that_accepts_verify() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("specforge.json"),
        serde_json::json!({"name": "p", "extensions": ["@pin/untargeted"]}).to_string(),
    )
    .unwrap();
    std::fs::write(
        dir.path().join("a.spec"),
        "item alpha \"Alpha\" {\n  verify unit \"works\"\n}\n\nitem beta \"Beta\" {\n}\n\n\
         note gamma \"Gamma\" {\n}\n\nitem delta \"Delta\" {\n  abstract true\n}\n\n\
         memo epsilon \"Epsilon\" {\n}\n",
    )
    .unwrap();
    specforge_installed::testing::install(dir.path(), &["@pin/untargeted"]);
    let runtime = specforge_wasm::testing::InProcessRuntime::new().with(untargeted_rule);
    let project = compile_at(dir.path(), Some(Arc::new(runtime)));
    let reported: Vec<String> = project
        .diagnostics()
        .into_iter()
        .filter(|d| d.code == "P300")
        .map(|d| d.message)
        .collect();
    assert_eq!(
        reported,
        [
            "item 'beta' declares no verify obligations",
            "note 'gamma' declares no verify obligations",
        ]
    );
    // The stubs fix exactly what the rule reports: not delta (its flag
    // exempts it), not epsilon (its kind accepts no verify).
    let navigator = Navigator::new(ProjectView::of(&project));
    assert_eq!(
        stubs(fix::fixes(&navigator, &project.diagnostics(), &FixQuery::default()).unwrap()),
        [
            ("beta".to_string(), Some("P300".to_string())),
            ("gamma".to_string(), Some("P300".to_string())),
        ]
    );
}

#[specforge_test(
    behavior = "code_actions_for_missing_verify",
    verify = "generated verify stubs added to entity block in .spec file"
)]
fn the_verify_stub_lands_inside_the_block() {
    let p = compile(TESTING, &[("flows.spec", UNTESTED)]);
    let fixes = fixes_of(&p, &FixQuery::default());
    let stub = fixes
        .iter()
        .find(|f| f.title == "Add verify stub for first")
        .unwrap();
    let edited = apply(UNTESTED, &stub.edits);
    assert_eq!(
        edited,
        UNTESTED.replace(
            "  contract \"c\"\n}\n\nbehavior second",
            "  contract \"c\"\n  verify unit \"first — TODO\"\n}\n\nbehavior second"
        )
    );
    // A one-line block becomes the canonical multi-line block, as `specforge link --verify` writes it.
    let one_line = "behavior solo \"S\" { contract \"c\" }\n";
    let p = compile(TESTING, &[("solo.spec", one_line)]);
    let stub = fixes_of(&p, &FixQuery::default())
        .into_iter()
        .find(|f| f.source == FixSource::AddVerifyStub)
        .unwrap();
    let edited = apply(one_line, &stub.edits);
    assert_eq!(
        edited,
        "behavior solo \"S\" {\n  contract \"c\"\n  verify unit \"solo — TODO\"\n}\n"
    );
    let parsed = specforge_parser::parse(&edited, "solo.spec");
    assert!(parsed.errors.is_empty(), "{:?}", parsed.errors);
    assert_eq!(parsed.entities[0].obligations.len(), 1);
}

#[specforge_test(
    behavior = "code_actions_for_missing_verify",
    verify = "verify stub uses allowed_verify_kinds from KindRegistry"
)]
fn the_verify_stub_uses_the_kinds_first_allowed_verify_kind() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("specforge.json"),
        serde_json::json!({"name": "p", "extensions": ["@test/verifiable"]}).to_string(),
    )
    .unwrap();
    let text =
        "invariant unique_ids \"U\" {\n  guarantee \"g\"\n}\n\nfeature untestable \"F\" {\n}\n";
    std::fs::write(dir.path().join("a.spec"), text).unwrap();
    specforge_installed::testing::install(dir.path(), &["@test/verifiable"]);
    let runtime = specforge_wasm::testing::InProcessRuntime::new().with(verifiable_kinds);
    let project = compile_at(dir.path(), Some(Arc::new(runtime)));
    let fixes = fix::fixes(
        &Navigator::new(ProjectView::of(&project)),
        &[],
        &FixQuery::default(),
    )
    .unwrap();
    assert_eq!(
        titles(&fixes),
        ["Add verify stub for unique_ids"],
        "a feature takes none"
    );
    assert!(
        apply(text, &fixes[0].edits).contains("verify property \"unique_ids — TODO\""),
        "{:?}",
        fixes[0].edits
    );
}

/// `invariant` (testable, verify kinds `property` and `unit`) and `feature` (no verify).
fn verifiable_kinds() -> specforge_extension_sdk::prelude::ContributionsBuilder {
    let mut c = ContributionsBuilder::new(ExtensionMeta::new("@test/verifiable", "0.1.0"));
    c.kind("invariant", |k| {
        k.testable(true)
            .supports_verify(true)
            .verify_kinds(&["property", "unit"]);
        k.field("guarantee", |f| {
            f.field_type(FieldType::String);
        });
    });
    c.kind("feature", |k| {
        k.keyword("feature");
    });
    c
}

#[specforge_test(
    behavior = "code_actions_for_missing_verify",
    verify = "stub format is verify <kind> entity_id TODO"
)]
fn the_verify_stub_names_the_entity_and_todo() {
    let p = compile(TESTING, &[("flows.spec", UNTESTED)]);
    let stub = fixes_of(&p, &FixQuery::default())
        .into_iter()
        .find(|f| f.source == FixSource::AddVerifyStub)
        .unwrap();
    assert_eq!(stub.edits[0].new_text, "  verify unit \"first — TODO\"\n");
}

#[specforge_test(
    behavior = "code_actions_for_missing_verify",
    verify = "code action kind is QuickFix"
)]
fn the_verify_stub_is_a_quick_fix() {
    let p = compile(TESTING, &[("flows.spec", UNTESTED)]);
    let stub = fixes_of(&p, &FixQuery::default())
        .into_iter()
        .find(|f| f.source == FixSource::AddVerifyStub)
        .unwrap();
    assert_eq!(stub.kind, FixKind::QuickFix);
    assert_eq!(serde_json::to_value(stub.kind).unwrap(), "quickfix");
    // Its code is the rule that reports an entity without verify
    // statements, so a code filter finds it.
    let code = stub
        .diagnostic_code
        .clone()
        .expect("the testing rule's code");
    let by_code = fixes_of(
        &p,
        &FixQuery {
            code: Some(&code),
            ..FixQuery::default()
        },
    );
    assert_eq!(titles(&by_code), ["Add verify stub for first"]);
}

#[specforge_test(
    behavior = "code_actions_for_missing_verify",
    verify = "no test source files or application code generated"
)]
fn the_verify_stub_edits_only_the_spec_file() {
    let p = compile(TESTING, &[("flows.spec", UNTESTED)]);
    for fix in fixes_of(&p, &FixQuery::default()) {
        for edit in &fix.edits {
            assert_eq!(edit.span.file, "flows.spec", "{fix:?}");
        }
    }
}

#[specforge_test(
    behavior = "code_actions_for_missing_verify",
    verify = "code actions are those whose diagnostic or entity overlaps the requested range"
)]
fn fixes_are_those_overlapping_the_range() {
    let text = "behavior login \"Login\" {\n  contract \"c\"\n  invariants [sesion_limit]\n}\n\n\
                behavior logout \"Logout\" {\n  contract \"c\"\n}\n";
    let p = compile(TESTING, &[("limit.spec", LIMIT), ("login.spec", text)]);
    let at = |start: (usize, usize), end: (usize, usize)| {
        let range = SourceSpan {
            file: Sym::new("login.spec"),
            start_line: start.0,
            start_col: start.1,
            end_line: end.0,
            end_col: end.1,
        };
        let query = FixQuery {
            file: Some("login.spec"),
            within: Some(&range),
            ..FixQuery::default()
        };
        titles(&fixes_of(&p, &query))
    };
    // The reference's line: its fixes and login's verify stub.
    assert_eq!(
        at((3, 1), (3, 30)),
        [
            "Add verify stub for login",
            "Create invariant stub for sesion_limit",
            "Replace with 'session_limit'"
        ]
    );
    // Inside logout: only its verify stub.
    assert_eq!(at((7, 1), (7, 2)), ["Add verify stub for logout"]);
    // Between the blocks: nothing.
    assert!(at((5, 1), (5, 1)).is_empty());
}

// code_action_create_entity_stub

const DANGLING: &str =
    "behavior login \"L\" {\n  invariants [session_limit]\n  verify unit \"y\"\n}\n";

fn stub_of(p: &Compiled) -> Fix {
    fixes_of(p, &FixQuery::default())
        .into_iter()
        .find(|f| f.source == FixSource::CreateStub)
        .unwrap_or_else(|| panic!("no stub: {:?}", titles(&fixes_of(p, &FixQuery::default()))))
}

#[specforge_test(
    behavior = "code_action_create_entity_stub",
    verify = "stub uses correct entity kind from FieldRegistry target_kind"
)]
fn the_stub_kind_is_the_fields_target_kind() {
    let p = compile(TESTING, &[("auth.spec", DANGLING)]);
    let stub = stub_of(&p);
    assert_eq!(stub.title, "Create invariant stub for session_limit");
    assert!(
        stub.edits[0]
            .new_text
            .starts_with("\ninvariant session_limit \"session_limit\" {"),
        "{stub:?}"
    );
}

/// `auth.spec` ([`DANGLING`]) on disk, compiled with an extension where `behavior.invariants` is a reference list
/// targeting `target_kind` (a kind declared beside `behavior`, with no required field).
fn invariants_project(target_kind: Option<&'static str>) -> (tempfile::TempDir, CompiledProject) {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("specforge.json"),
        serde_json::json!({"name": "p", "extensions": ["@test/ext"]}).to_string(),
    )
    .unwrap();
    std::fs::write(dir.path().join("auth.spec"), DANGLING).unwrap();
    specforge_installed::testing::install(dir.path(), &["@test/ext"]);
    let runtime = specforge_wasm::testing::InProcessRuntime::new().with(move || {
        let mut c = ContributionsBuilder::new(ExtensionMeta::new("@test/ext", "1.0.0"));
        c.kind("behavior", |k| {
            k.keyword("behavior");
            k.field("invariants", |f| {
                f.field_type(FieldType::ReferenceList);
                if let Some(target_kind) = target_kind {
                    f.target_kind(target_kind);
                }
            });
        });
        if let Some(target_kind) = target_kind {
            c.kind(target_kind, |k| {
                k.keyword(target_kind);
            });
        }
        c
    });
    let project = compile_at(dir.path(), Some(Arc::new(runtime)));
    (dir, project)
}

#[specforge_test(
    behavior = "code_action_create_entity_stub",
    verify = "no code action when enclosing field has no target_kind"
)]
fn no_stub_without_a_target_kind() {
    let stubs = |target_kind| {
        let (_dir, project) = invariants_project(target_kind);
        fix::fixes(
            &Navigator::new(ProjectView::of(&project)),
            &project.diagnostics(),
            &FixQuery::default(),
        )
        .unwrap()
        .into_iter()
        .filter(|f| f.source == FixSource::CreateStub)
        .collect::<Vec<_>>()
    };
    assert_eq!(titles(&stubs(None)), Vec::<String>::new());
    assert_eq!(
        titles(&stubs(Some("invariant"))),
        ["Create invariant stub for session_limit"]
    );
}

#[specforge_test(
    behavior = "code_action_create_entity_stub",
    verify = "stub goes after the last block of its kind in the current file"
)]
fn the_stub_is_appended_to_the_file() {
    let p = compile(TESTING, &[("auth.spec", DANGLING)]);
    let stub = stub_of(&p);
    assert_eq!(stub.edits.len(), 1);
    assert_eq!(
        at(&stub.edits[0].span),
        "auth.spec 5:1-5:1",
        "the end of the file"
    );
    let edited = apply(DANGLING, &stub.edits);
    assert!(edited.starts_with(DANGLING));
    let parsed = specforge_parser::parse(&edited, "auth.spec");
    assert!(parsed.errors.is_empty(), "{:?}", parsed.errors);
    assert_eq!(parsed.entities.len(), 2);
}

#[specforge_test(
    behavior = "code_action_create_entity_stub",
    verify = "code action kind is Refactor"
)]
fn the_stub_is_a_refactoring() {
    let p = compile(TESTING, &[("auth.spec", DANGLING)]);
    assert_eq!(stub_of(&p).kind, FixKind::Refactor);
}

#[specforge_test(
    behavior = "code_action_create_entity_stub",
    verify = "generated stub contains no application code or test files"
)]
fn the_stub_is_a_block_with_its_required_fields() {
    let p = compile(TESTING, &[("auth.spec", DANGLING)]);
    let stub = stub_of(&p);
    assert_eq!(
        stub.edits[0].new_text,
        "\ninvariant session_limit \"session_limit\" {\n  guarantee \"TODO\"\n}\n"
    );
    assert!(stub.edits.iter().all(|e| e.span.file == "auth.spec"));
}

/// An E003 at `span` whose message says nothing a parser could use: only
/// its data names the reference.
fn reworded_e003(span: SourceSpan, data: Option<DiagnosticData>) -> Diagnostic {
    let mut diagnostic = Diagnostic::new(
        specforge_common::codes::E003,
        "this wording is not a contract",
    )
    .with_span(span);
    diagnostic.data = data.map(Box::new);
    diagnostic
}

fn unresolved(target: &str, entity: &str, field: &str, close: Option<&str>) -> DiagnosticData {
    DiagnosticData::UnresolvedReference {
        target: target.into(),
        entity: entity.into(),
        field: field.into(),
        did_you_mean: close.map(String::from),
    }
}

#[specforge_test(
    behavior = "code_action_create_entity_stub",
    verify = "the stub is read from the diagnostic's data, whatever its message says"
)]
fn the_stub_reads_target_entity_and_field_from_the_data() {
    let p = compile(TESTING, &[("auth.spec", DANGLING)]);
    let token = span("auth.spec", (2, 15), (2, 28));
    let data = unresolved("session_limit", "login", "invariants", None);
    // Twice: one stub per target.
    let reworded = [
        reworded_e003(token.clone(), Some(data.clone())),
        reworded_e003(token.clone(), Some(data)),
    ];
    let fixes = fix::fixes(&p.navigator(), &reworded, &FixQuery::default()).unwrap();
    let stubs: Vec<&Fix> = fixes
        .iter()
        .filter(|f| f.source == FixSource::CreateStub)
        .collect();
    assert_eq!(stubs.len(), 1);
    assert_eq!(stubs[0].title, "Create invariant stub for session_limit");

    // The diagnostic as text alone, in the compiler's wording: no stub.
    let mut text_only = reworded_e003(token, None);
    text_only.message = "unresolved reference 'session_limit' in entity 'login'".into();
    let fixes = fix::fixes(&p.navigator(), &[text_only], &FixQuery::default()).unwrap();
    assert!(fixes.iter().all(|f| f.source != FixSource::CreateStub));
}

#[test]
fn no_stub_for_a_target_that_exists() {
    let p = compile(TESTING, &[("auth.spec", DANGLING), ("limit.spec", LIMIT)]);
    let stale = reworded_e003(
        span("auth.spec", (2, 15), (2, 28)),
        Some(unresolved("session_limit", "login", "invariants", None)),
    );
    let fixes = fix::fixes(&p.navigator(), &[stale], &FixQuery::default()).unwrap();
    assert!(fixes.iter().all(|f| f.source != FixSource::CreateStub));
}

// code_action_replace_unresolved

#[specforge_test(
    behavior = "code_action_replace_unresolved",
    verify = "an unresolved reference with a close match is replaced at its token"
)]
fn a_close_match_replaces_the_unresolved_token() {
    let p = nav();
    let query = FixQuery {
        code: Some("E003"),
        ..FixQuery::default()
    };
    let fix = fixes_of(&p, &query)
        .into_iter()
        .find(|f| f.source == FixSource::ReplaceUnresolved)
        .unwrap();
    assert_eq!(fix.title, "Replace with 'session_limit'");
    assert_eq!(fix.kind, FixKind::QuickFix);
    assert_eq!(fix.edits.len(), 1);
    assert_eq!(at(&fix.edits[0].span), "login.spec 6:15-6:27");
    assert_eq!(fix.edits[0].new_text, "session_limit");
    assert_eq!(
        apply(LOGIN, &fix.edits),
        LOGIN.replace("[sesion_limit]", "[session_limit]")
    );
    // A token like it, x_sesion_limit, is never hit: the span is the token.
}

#[specforge_test(
    behavior = "code_action_replace_unresolved",
    verify = "an unresolved import with a close match is replaced inside its quotes"
)]
fn a_close_match_replaces_the_unresolved_import_inside_its_quotes() {
    let p = compile(
        SOFTWARE,
        &[
            ("auth.spec", "behavior auth \"A\" {\n  contract \"c\"\n}\n"),
            ("main.spec", "use \"autth\"\n"),
        ],
    );
    let fixes = fixes_of(
        &p,
        &FixQuery {
            code: Some("E025"),
            ..FixQuery::default()
        },
    );
    assert_eq!(fixes.len(), 1, "{:?}", titles(&fixes));
    let edit = &fixes[0].edits[0];
    assert_eq!(at(&edit.span), "main.spec 1:6-1:11", "inside the quotes");
    assert_eq!(apply("use \"autth\"\n", &fixes[0].edits), "use \"auth\"\n");
}

#[specforge_test(
    behavior = "code_action_replace_unresolved",
    verify = "the replacement is read from the diagnostic's data, whatever its message says"
)]
fn the_replacement_is_read_from_the_data() {
    let p = nav();
    let token = span("login.spec", (6, 15), (6, 27));
    let reworded = reworded_e003(
        token.clone(),
        Some(unresolved(
            "sesion_limit",
            "logout",
            "invariants",
            Some("session_limit"),
        )),
    );
    let fixes = fix::fixes(&p.navigator(), &[reworded], &FixQuery::default()).unwrap();
    let replace: Vec<&Fix> = fixes
        .iter()
        .filter(|f| f.source == FixSource::ReplaceUnresolved)
        .collect();
    assert_eq!(replace.len(), 1);
    assert_eq!(replace[0].edits[0].new_text, "session_limit");

    // Its message and suggestion alone offer nothing.
    let mut text_only = reworded_e003(token, None);
    text_only.message = "unresolved reference 'sesion_limit' in entity 'logout'".into();
    text_only.suggestion = Some("did you mean 'session_limit'?".into());
    assert!(
        fix::fixes(&p.navigator(), &[text_only], &FixQuery::default())
            .unwrap()
            .is_empty()
    );
}
