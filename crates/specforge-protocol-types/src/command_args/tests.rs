use super::*;
use serde_json::json;

fn arg(name: &str, arg_type: CommandArgType) -> CommandArgDescriptor {
    CommandArgDescriptor {
        name: name.into(),
        arg_type,
        ..Default::default()
    }
}

fn count(name: &str) -> CommandArgDescriptor {
    CommandArgDescriptor {
        minimum: Some(0),
        ..arg(name, CommandArgType::Integer)
    }
}

#[test]
fn a_string_the_caller_gave_is_quoted_and_anything_else_is_json() {
    let limit = count("limit");
    let said = |value: Value| normalize_arg(&limit, &value).unwrap_err().message();
    assert_eq!(
        said(json!("abc")),
        "limit must be a non-negative integer, got 'abc'"
    );
    assert_eq!(
        said(json!(-1)),
        "limit must be a non-negative integer, got -1"
    );
    // A string holding an integer below the minimum is that integer.
    assert_eq!(
        said(json!("-1")),
        "limit must be a non-negative integer, got -1"
    );
    assert_eq!(
        said(json!(1.5)),
        "limit must be a non-negative integer, got 1.5"
    );
    let at_least = CommandArgDescriptor {
        minimum: Some(2),
        ..arg("n", CommandArgType::Integer)
    };
    assert_eq!(
        normalize_arg(&at_least, &json!(1)).unwrap_err().message(),
        "n must be an integer of at least 2, got 1"
    );
    let shift = arg("shift", CommandArgType::Integer);
    assert_eq!(
        normalize_arg(&shift, &json!(true)).unwrap_err().message(),
        "shift must be an integer, got true"
    );
}

#[test]
fn each_variant_has_its_wording() {
    let missing = ArgError::Missing {
        names: vec!["a".into(), "b".into()],
    };
    assert_eq!(missing.message(), "missing required args 'a', 'b'");
    let one = ArgError::Missing {
        names: vec!["milestone".into()],
    };
    assert_eq!(one.message(), "missing required arg 'milestone'");
    let unknown = ArgError::Unknown {
        name: "statsu".into(),
        suggestion: Some("status".into()),
    };
    assert_eq!(unknown.message(), "unknown argument 'statsu'");
    assert_eq!(
        unknown.to_json(),
        json!({"code": "INVALID_INPUT", "message": "unknown argument 'statsu'",
                "suggestion": "status"})
    );
    let style = arg(
        "style",
        CommandArgType::Enum {
            values: vec!["md".into(), "json".into()],
        },
    );
    let refused = normalize_arg(&style, &json!("jsno")).unwrap_err();
    assert_eq!(
        refused.message(),
        "style must be one of md, json, got 'jsno'"
    );
    assert_eq!(refused.suggestion(), Some("json"));
    let far = normalize_arg(&style, &json!("xml")).unwrap_err();
    assert_eq!(far.suggestion(), None);
    assert_eq!(
        far.to_json(),
        json!({"code": "INVALID_INPUT", "message": "style must be one of md, json, got 'xml'"})
    );
    let all = arg("all", CommandArgType::Bool);
    assert_eq!(
        normalize_arg(&all, &json!("yes")).unwrap_err().message(),
        "all must be true or false, got 'yes'"
    );
    let out = arg("out", CommandArgType::Path);
    assert_eq!(
        normalize_arg(&out, &json!(5)).unwrap_err().message(),
        "out must be a string, got 5"
    );
}

#[test]
fn a_refusal_names_the_first_contradiction() {
    let with = |f: fn(&mut CommandArgDescriptor)| {
        let mut a = arg("n", CommandArgType::String);
        f(&mut a);
        refusal(&[a]).map(|r| r.to_string())
    };
    assert_eq!(with(|_| {}), None);
    assert_eq!(
        with(|a| {
            a.arg_type = CommandArgType::Bool;
            a.default_value = Some("true".into());
        }),
        Some("its flag 'n' has a default, but a flag is false unless set".into())
    );
    assert_eq!(
        with(|a| {
            a.required = true;
            a.default_value = Some("x".into());
        }),
        Some("its arg 'n' is both required and with a default".into())
    );
    assert_eq!(
        with(|a| {
            a.arg_type = CommandArgType::Integer;
            a.minimum = Some(0);
            a.default_value = Some("-1".into());
        }),
        Some(
            "its arg 'n' has a default its type refuses: n must be a non-negative integer, got -1"
                .into()
        )
    );
    // A required flag is no contradiction: it is false unless set.
    assert_eq!(
        with(|a| {
            a.arg_type = CommandArgType::Bool;
            a.required = true;
        }),
        None
    );
    for name in HOST_OPTIONS {
        assert_eq!(
            refusal(&[arg(name, CommandArgType::String)]).map(|r| r.to_string()),
            Some(format!("its arg '{name}' takes the host's --{name}"))
        );
    }
    let twice = [
        arg("all_kinds", CommandArgType::Bool),
        arg("all-kinds", CommandArgType::Bool),
    ];
    assert_eq!(
        refusal(&twice),
        Some(ArgRefusal::Twice {
            name: "all-kinds".into()
        })
    );
}

#[test]
fn close_names_are_suggested_and_far_ones_are_not() {
    let names: Vec<String> = ["status", "priority", "limit"]
        .into_iter()
        .map(String::from)
        .collect();
    assert_eq!(close_match("statsu", &names), Some("status"));
    assert_eq!(close_match("limt", &names), Some("limit"));
    assert_eq!(close_match("format", &names), None);
    assert_eq!(close_match("x", &names), None);
    assert_eq!(edit_distance("kitten", "sitting"), 3);
}
