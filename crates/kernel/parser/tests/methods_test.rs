use specforge_parser::{FieldValue, parse};

#[test]
fn method_signatures_parse_with_params_and_returns() {
    let source = r#"
port FileSystem {
    direction outbound
    category "io/filesystem"

    method readFile(path: string) -> Result<string, EmitterError>
    method listFiles(pattern: string) -> Result<string[], EmitterError>
    method flush()
}
"#;
    let result = parse(source, "ports.spec");
    assert!(result.errors.is_empty(), "errors: {:?}", result.errors);
    let port = result.entities.first().expect("port entity");
    assert_eq!(port.methods.len(), 3);

    let read = &port.methods[0];
    assert_eq!(read.name, "readFile");
    assert_eq!(read.params.len(), 1);
    assert_eq!(read.params[0].name, "path");
    assert_eq!(read.params[0].ty, "string");
    assert_eq!(
        read.returns.as_deref(),
        Some("Result<string, EmitterError>")
    );

    let list = &port.methods[1];
    assert_eq!(list.params[0].ty, "string");
    assert_eq!(
        list.returns.as_deref(),
        Some("Result<string[], EmitterError>")
    );

    let flush = &port.methods[2];
    assert_eq!(flush.name, "flush");
    assert!(flush.params.is_empty());
    assert!(flush.returns.is_none());
}

#[test]
fn method_decl_entities_reach_field_map_untouched() {
    // methods are structural members, not field values: the `metric`/`tests`
    // style FieldMap must stay empty of method noise
    let source = r#"
port P {
    method a() -> T
    verify integration "contract holds"
}
"#;
    let result = parse(source, "p.spec");
    assert!(result.errors.is_empty(), "errors: {:?}", result.errors);
    let port = result.entities.first().unwrap();
    assert_eq!(port.methods.len(), 1);
    assert!(port.fields.get("method").is_none());
    assert!(matches!(
        port.fields.get("verify"),
        Some(FieldValue::VerifyList(_))
    ));
}

#[test]
fn union_typed_field_declarations_parse() {
    let source = r#"
entity_kind EK "Example" {
    query_scope string | string[] @optional
    tags [alpha]
    status planned
}
"#;
    let result = parse(source, "union.spec");
    assert!(result.errors.is_empty(), "errors: {:?}", result.errors);
    let entity = result.entities.first().unwrap();
    match entity.fields.get("query_scope") {
        Some(FieldValue::TypeUnion(types)) => {
            assert_eq!(types, &vec!["string".to_string(), "string[]".to_string()]);
        }
        other => panic!("expected TypeUnion, got {other:?}"),
    }
    // plain values are untouched
    assert!(matches!(
        entity.fields.get("status"),
        Some(FieldValue::Identifier(_))
    ));
}
