//! Where a block writes its title and its verify statements: where the parse read them (ADR 0187, amended).

use super::*;
use specforge_ops::authoring::fix::{self, FixQuery};

const TESTING: &[&str] = &["@specforge/software", "@specforge/testing"];

const PORT: &str = "port auth_port \"Auth port\" {\n  direction inbound\n  methods {\n    login  \"Logs a user in\"\n    verify \"Checks a session token\"\n  }\n  verify \"the port rejects an expired token\"\n  verify unit \"login returns a session\"\n}\n";

const UNTESTED_PORT: &str = "port auth_port \"Auth port\" {\n  direction inbound\n  methods {\n    login  \"Logs a user in\"\n    verify \"Checks a session token\"\n  }\n}\n";

fn ports(text: &str) -> Compiled {
    compile(TESTING, &[("ports.spec", text)])
}

fn add(id: &str, kind: Option<&str>, text: &str) -> Edit {
    Edit::AddVerify {
        entity_id: id.into(),
        kind: kind.map(String::from),
        text: text.into(),
    }
}

fn remove(id: &str, kind: Option<&str>, text: &str) -> Edit {
    Edit::RemoveVerify {
        entity_id: id.into(),
        kind: kind.map(String::from),
        text: text.into(),
    }
}

#[specforge_test_macros::test(
    behavior = "edit_entity_obligation",
    verify = "remove-verify removes the statement, never a block field's entry named verify"
)]
fn remove_verify_removes_the_statement_never_an_entry_named_verify() {
    let c = ports(PORT);
    let text = after(
        &c,
        "ports.spec",
        &[remove(
            "auth_port",
            None,
            "the port rejects an expired token",
        )],
    );
    assert_eq!(
        text,
        PORT.replace("  verify \"the port rejects an expired token\"\n", "")
    );
    let text = after(
        &c,
        "ports.spec",
        &[remove("auth_port", Some("unit"), "login returns a session")],
    );
    assert_eq!(
        text,
        PORT.replace("  verify unit \"login returns a session\"\n", "")
    );
}

#[specforge_test_macros::test(
    behavior = "edit_entity_obligation",
    verify = "add-verify writes after the last statement, never inside a block field"
)]
fn add_verify_writes_after_the_last_statement_never_inside_a_block_field() {
    let c = ports(UNTESTED_PORT);
    let planned = plan(
        &c,
        &[add("auth_port", Some("unit"), "login returns a session")],
    )
    .map_err(|r| r.error.code.to_string())
    .expect("the edit plans");
    assert_eq!(
        planned.change().after().remove("ports.spec").unwrap(),
        UNTESTED_PORT.replace(
            "  }\n}\n",
            "  }\n  verify unit \"login returns a session\"\n}\n"
        )
    );
}

#[specforge_test_macros::test(
    behavior = "code_actions_for_missing_verify",
    verify = "a verify stub is offered for an entity whose block field holds an entry named verify"
)]
fn a_verify_stub_is_offered_beside_an_entry_named_verify() {
    let c = ports(UNTESTED_PORT);
    let fixes = crate::fixes::fixes_of(&c, &FixQuery::default());
    let stub = fixes
        .iter()
        .find(|f| f.title == "Add verify stub for auth_port")
        .expect("the stub is offered");
    assert_eq!(
        crate::fixes::apply(UNTESTED_PORT, &stub.edits),
        UNTESTED_PORT.replace(
            "  }\n}\n",
            "  }\n  verify integration \"auth_port — TODO\"\n}\n"
        )
    );
}

#[specforge_test_macros::test(
    behavior = "edit_entity_field",
    verify = "retitle of an untitled union writes the title after its name, never over a variant"
)]
fn retitle_of_an_untitled_union_writes_the_title_after_its_name() {
    let retitle = |title: &str, id: &str| Edit::Retitle {
        entity_id: id.into(),
        title: title.into(),
    };
    let c = project(&[("types.spec", "type stage = \"draft\" | \"final\"\n")]);
    assert_eq!(
        after(&c, "types.spec", &[retitle("Stage", "stage")]),
        "type stage \"Stage\" = \"draft\" | \"final\"\n"
    );
    let c = project(&[("types.spec", "type kind_two \"K\" = \"a\" | \"b\"\n")]);
    assert_eq!(
        after(&c, "types.spec", &[retitle("Kinds", "kind_two")]),
        "type kind_two \"Kinds\" = \"a\" | \"b\"\n"
    );
}

#[specforge_test_macros::test(
    behavior = "edit_entity_obligation",
    verify = "add-verify on a union is refused as not_editable, a union has no verify statements"
)]
fn add_verify_on_a_union_is_refused_as_not_editable() {
    let c = project(&[("types.spec", "type stage = \"draft\" | \"final\"\n")]);
    let refusal = plan(&c, &[add("stage", None, "a stage is one of its variants")])
        .map(|_| ())
        .map_err(|r| r.error.code.to_string())
        .unwrap_err();
    assert_eq!(refusal, "not_editable");
}

/// A braced type with a member that happens to be named like the union's variants, beside a real union.
const FEATURE: &str = "type Feature \"Feature\" {\n  name     string\n  variants string[]\n}\n\n\
                       type Status = open | done\n";

#[test]
#[ignore = "flipped by T4"]
fn add_verify_on_a_braced_block_with_a_member_named_variants_writes_the_statement() {
    let c = compile(TESTING, &[("types.spec", FEATURE)]);
    assert_eq!(
        after(
            &c,
            "types.spec",
            &[add("Feature", Some("unit"), "has a name")]
        ),
        FEATURE.replace(
            "  variants string[]\n}\n",
            "  variants string[]\n  verify unit \"has a name\"\n}\n"
        )
    );
}

#[test]
#[ignore = "flipped by T4"]
fn a_verify_stub_is_offered_for_a_braced_block_with_a_member_named_variants() {
    let c = compile(TESTING, &[("types.spec", FEATURE)]);
    let navigator = c.navigator();
    // Every verify stub the standing wants plans: the union is offered none, the braced block one.
    let planned: Vec<String> =
        fix::offers(&navigator, &c.project.diagnostics(), &FixQuery::default())
            .into_iter()
            .filter(|offer| offer.title().starts_with("Add verify stub"))
            .map(|offer| {
                let title = offer.title().to_string();
                offer
                    .fix(&navigator)
                    .map(|_| title.clone())
                    .unwrap_or_else(|refusal| panic!("{title}: {}", refusal.error.message))
            })
            .collect();
    assert_eq!(planned, ["Add verify stub for Feature"]);
}
