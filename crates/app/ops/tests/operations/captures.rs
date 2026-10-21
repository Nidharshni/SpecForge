//! A rename is a substitution of its new ID for its old one and nothing else: a name the project writes for no
//! entity, which the new ID would make name the renamed entity, refuses it (ADR 0198, amended).

use specforge_ops::OpErrorKind;
use specforge_ops::authoring::rename::{self, RenameField};

const OPEN_WORLD: &str = "type Session \"Session\" {\n  token string\n}\n\ntype Login \"Login\" {\n  session Session\n}\n\ntype ApiKey \"Api Key\" {\n  value Token\n}\n";
const PRIMITIVE: &str = "type Session \"Session\" {\n  token string\n}\n\ntype Login \"Login\" {\n  session Session\n}\n\nport auth \"Auth\" {\n  direction inbound\n  method login(user: string) -> Session\n}\n";
const LIMIT: &str = "invariant session_limit \"Limit\" {\n  guarantee \"x\"\n}\n";

fn compile(files: &[(&str, &str)]) -> crate::navigate::Compiled {
    crate::navigate::compile(crate::navigate::SOFTWARE, files)
}

#[test]
#[ignore = "flipped by T2"]
fn a_rename_to_a_name_written_as_an_open_world_type_is_refused_naming_it() {
    let p = compile(&[("a.spec", OPEN_WORLD)]);
    let refused = rename::plan(&p.navigator(), "Session", "Token").unwrap_err();
    assert_eq!(refused.error.code, "name_captured");
    assert_eq!(refused.error.kind, OpErrorKind::Conflict);
    assert_eq!(refused.subject, Some(RenameField::NewId));
    assert!(
        refused
            .error
            .message
            .contains("a.spec:10 (ApiKey, composed_types)"),
        "{}",
        refused.error.message
    );
}

#[test]
#[ignore = "flipped by T2"]
fn a_rename_to_a_primitive_written_in_a_signature_is_refused_though_no_edge_is_added() {
    let p = compile(&[("a.spec", PRIMITIVE)]);
    let refused = rename::plan(&p.navigator(), "Session", "string").unwrap_err();
    assert_eq!(refused.error.code, "name_captured");
    let message = &refused.error.message;
    assert!(message.contains("a.spec:11 (auth, types)"), "{message}");
    assert!(!message.contains("a.spec:2"), "{message}");
}

#[test]
#[ignore = "flipped by T2"]
fn a_rename_onto_a_dangling_reference_is_refused() {
    let p = compile(&[
        ("limit.spec", LIMIT),
        (
            "login.spec",
            "behavior login \"Login\" {\n  invariants [cap_limit]\n}\n",
        ),
    ]);
    let refused = rename::plan(&p.navigator(), "session_limit", "cap_limit").unwrap_err();
    assert_eq!(refused.error.code, "name_captured");
    assert!(
        refused.error.message.contains("login"),
        "{}",
        refused.error.message
    );
}

#[test]
#[ignore = "flipped by T2"]
fn a_rename_is_refused_while_a_source_that_spells_the_new_id_does_not_parse() {
    let p = compile(&[
        ("limit.spec", LIMIT),
        (
            "login.spec",
            "behavior login \"Login\" {\n  invariants [session_cap\n",
        ),
    ]);
    let refused = rename::plan(&p.navigator(), "session_limit", "session_cap").unwrap_err();
    assert_eq!(refused.error.code, rename::UNPARSED);
    assert!(
        refused.error.message.contains("login.spec"),
        "{}",
        refused.error.message
    );
}
