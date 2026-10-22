//! A union (`kind name = a | b`) holds only its variants: every edit of a member is refused as one not_editable,
//! read from the parse; retitle and delete take it (ADR 0219).

use super::*;

/// Two unions of two kinds: an open-fields kind and one with declared fields.
const UNIONS: &str =
    "type stage \"Stage\" = \"draft\" | \"final\"\n\nport gate \"Gate\" = open | shut\n";

/// An add-verify (`add`) or remove-verify of `unit "x"` on `id`.
fn statement(add: bool, id: &str) -> Edit {
    let (entity_id, kind, text) = (id.to_string(), Some("unit".to_string()), "x".to_string());
    if add {
        Edit::AddVerify {
            entity_id,
            kind,
            text,
        }
    } else {
        Edit::RemoveVerify {
            entity_id,
            kind,
            text,
        }
    }
}

fn retitle(id: &str, title: &str) -> Edit {
    Edit::Retitle {
        entity_id: id.into(),
        title: title.into(),
    }
}

#[test]
#[ignore = "flipped by T4"]
fn every_member_edit_of_a_union_is_refused_as_one_not_editable() {
    let c = project(&[("types.spec", UNIONS)]);
    for id in ["stage", "gate"] {
        let edits = [
            set(id, "name", one("x")),
            set(id, "direction", one("inbound")),
            set(id, "variants", one("x | y")),
            set(id, "meta.x", one("y")),
            unset(id, "variants"),
            unset(id, "name"),
            link(id, "tags", "x"),
            unlink(id, "tags", "x"),
            statement(true, id),
            statement(false, id),
        ];
        for edit in edits {
            let (code, message) = refused(&c, std::slice::from_ref(&edit));
            assert_eq!(code, "not_editable", "{edit:?}: {message}");
            assert!(
                message.starts_with(&format!("'{id}' is a union")),
                "{edit:?}: {message}"
            );
        }
    }
    // A refusal after an earlier edit of the list reads the planned text.
    let (code, message) = refused(
        &c,
        &[retitle("gate", "Gates"), set("gate", "name", one("x"))],
    );
    assert_eq!(code, "not_editable");
    assert!(message.starts_with("'gate' is a union"), "{message}");
}

#[test]
fn retitle_and_delete_take_a_union() {
    let c = project(&[("types.spec", UNIONS)]);
    assert_eq!(
        after(&c, "types.spec", &[retitle("gate", "Gates")]),
        UNIONS.replace("\"Gate\"", "\"Gates\"")
    );
    let delete = Edit::Delete {
        entity_id: "gate".into(),
        unlink_references: false,
    };
    assert_eq!(
        after(&c, "types.spec", &[delete]),
        "type stage \"Stage\" = \"draft\" | \"final\"\n"
    );
}
