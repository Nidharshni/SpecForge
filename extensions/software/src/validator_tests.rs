use super::*;
use specforge_extension_sdk::{
    ValidatorEntity, ValidatorField, ValidatorMethod, ValidatorParam, ValidatorRef,
};

fn context(entity: ValidatorEntity, referenced: Vec<ValidatorRef>) -> ValidatorContext {
    ValidatorContext {
        entity,
        referenced,
        declared_types: vec!["order".to_string()],
        primitives: vec![],
    }
}

fn field(key: &str, value: serde_json::Value) -> ValidatorField {
    ValidatorField {
        key: key.to_string(),
        value,
        annotations: vec![],
    }
}

fn entity(kind: &str, fields: Vec<ValidatorField>) -> ValidatorEntity {
    ValidatorEntity {
        id: kind.to_string(),
        kind: kind.to_string(),
        fields,
        methods: vec![],
    }
}

fn pass() -> ValidatorVerdict {
    ValidatorVerdict::Pass
}

fn fail(field: &str, value: &str) -> ValidatorVerdict {
    ValidatorVerdict::Fail {
        field: Some(field.to_string()),
        value: Some(value.to_string()),
    }
}

#[test]
fn base_type_names_reach_inside_every_type_form() {
    assert_eq!(
        base_type_names("Result<(), string>"),
        vec!["Result", "string"]
    );
    assert_eq!(base_type_names("float[][]"), vec!["float"]);
    assert_eq!(
        base_type_names("fn(string, i32) -> bool"),
        vec!["string", "i32", "bool"]
    );
    assert!(base_type_names("fn()").is_empty());
}

#[test]
fn milestone_ranges_fail_on_dangling_behavior() {
    let ctx = context(
        entity(
            "milestone",
            vec![field("behaviors", serde_json::json!("gone"))],
        ),
        vec![ValidatorRef {
            id: "gone".into(),
            kind: None,
        }],
    );
    assert_eq!(
        validate_milestone_behavior_ranges(&ctx),
        fail("behaviors", "gone")
    );
}

#[test]
fn milestone_ranges_pass_when_all_exist() {
    let ctx = context(
        entity(
            "milestone",
            vec![field("behaviors", serde_json::json!("b1"))],
        ),
        vec![ValidatorRef {
            id: "b1".into(),
            kind: Some("behavior".into()),
        }],
    );
    assert_eq!(validate_milestone_behavior_ranges(&ctx), pass());
}

#[test]
fn type_annotations_fail_on_unknown_annotation() {
    let mut f = field("total", serde_json::json!("i64"));
    f.annotations = vec!["readonly".to_string(), "indexed".to_string()];
    let ctx = context(entity("type", vec![f]), vec![]);
    assert_eq!(
        validate_type_field_annotations(&ctx),
        fail("total", "@indexed")
    );
}

#[test]
fn type_annotations_pass_on_known_annotations() {
    let mut f = field("total", serde_json::json!("i64"));
    f.annotations = vec![
        "readonly".to_string(),
        "unique".to_string(),
        "optional".to_string(),
        "literal".to_string(),
    ];
    let ctx = context(entity("type", vec![f]), vec![]);
    assert_eq!(validate_type_field_annotations(&ctx), pass());
}

fn port(methods: Vec<ValidatorMethod>) -> ValidatorContext {
    context(
        ValidatorEntity {
            id: "order_port".to_string(),
            kind: "port".to_string(),
            fields: vec![],
            methods,
        },
        vec![],
    )
}

#[test]
fn port_methods_fail_on_undeclared_type() {
    let ctx = port(vec![ValidatorMethod {
        name: "submit".to_string(),
        params: vec![ValidatorParam {
            name: "order".to_string(),
            ty: "Undeclared".to_string(),
        }],
        returns: None,
    }]);
    assert_eq!(validate_port_methods(&ctx), fail("submit", "Undeclared"));
}

#[test]
fn port_methods_accept_generics_arrays_and_declared_types() {
    let ctx = port(vec![ValidatorMethod {
        name: "find".to_string(),
        params: vec![
            ValidatorParam {
                name: "q".to_string(),
                ty: "String".to_string(),
            },
            ValidatorParam {
                name: "ids".to_string(),
                ty: "Vec<string>[]".to_string(),
            },
        ],
        returns: Some("Result<order, string>".to_string()),
    }]);
    assert_eq!(validate_port_methods(&ctx), pass());
}

#[test]
fn port_methods_accept_documented_primitives_without_host_list() {
    let ctx = port(vec![ValidatorMethod {
        name: "stat".to_string(),
        params: vec![ValidatorParam {
            name: "ratio".to_string(),
            ty: "number".to_string(),
        }],
        returns: Some("Result<(integer, boolean, timestamp), never>".to_string()),
    }]);
    assert_eq!(validate_port_methods(&ctx), pass());
}

#[test]
fn port_methods_fail_on_generic_argument_of_known_container() {
    let ctx = port(vec![ValidatorMethod {
        name: "load".to_string(),
        params: vec![],
        returns: Some("Option<Undeclared>".to_string()),
    }]);
    assert_eq!(validate_port_methods(&ctx), fail("load", "Undeclared"));
}

#[test]
fn verdict_wire_shape_is_pinned() {
    assert_eq!(
        serde_json::to_value(pass()).unwrap(),
        serde_json::json!({ "verdict": "pass" })
    );
    assert_eq!(
        serde_json::to_value(fail("triggers", "missing_b")).unwrap(),
        serde_json::json!({ "verdict": "fail", "field": "triggers", "value": "missing_b" })
    );
    let sparse = ValidatorVerdict::Fail {
        field: None,
        value: None,
    };
    assert_eq!(
        serde_json::to_value(sparse).unwrap(),
        serde_json::json!({ "verdict": "fail" })
    );
    let round: ValidatorVerdict =
        serde_json::from_value(serde_json::json!({ "verdict": "fail", "field": "f" })).unwrap();
    assert_eq!(
        round,
        ValidatorVerdict::Fail {
            field: Some("f".into()),
            value: None,
        }
    );
}

#[test]
fn ref_ids_accept_joined_string_and_array_forms() {
    assert_eq!(
        ref_ids(&serde_json::json!("a, b,c")),
        vec!["a".to_string(), "b".to_string(), "c".to_string()]
    );
    assert_eq!(
        ref_ids(&serde_json::json!(["x", "y"])),
        vec!["x".to_string(), "y".to_string()]
    );
    assert!(ref_ids(&serde_json::json!(7)).is_empty());
}
