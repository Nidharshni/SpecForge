use specforge_common::SourceSpan;
use specforge_parser::{FieldValue, Written, parse};

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
    assert_eq!(port.obligations.len(), 1);
    assert!(port.fields.get("verify").is_none());
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

/// `(line, start_col, end_col)` of a one-line span.
fn columns(span: &SourceSpan) -> (usize, usize, usize) {
    assert_eq!(span.start_line, span.end_line);
    (span.start_line, span.start_col, span.end_col)
}

#[test]
fn a_parameters_type_and_a_methods_return_keep_their_spans() {
    let source = "port P {\n    method detect(config: Config) -> Result<Project, Error>\n}\n";
    let result = parse(source, "ports.spec");
    assert!(result.errors.is_empty(), "errors: {:?}", result.errors);
    let method = &result.entities[0].methods[0];

    let ty = method.params[0].ty_span.as_ref().expect("the type's span");
    assert_eq!(columns(ty), (2, 27, 33));
    let returns = method.returns_span.as_ref().expect("the return's span");
    assert_eq!(columns(returns), (2, 38, 60));
}

#[test]
fn a_unions_members_keep_their_spans_and_how_they_were_written() {
    let field = "entity_kind EK \"Example\" {\n    kind Open | \"none\" | Closed[]\n}\n";
    let result = parse(field, "union.spec");
    assert!(result.errors.is_empty(), "errors: {:?}", result.errors);
    let Some(FieldValue::TypeUnion(members)) = result.entities[0].fields.get("kind") else {
        panic!("expected a TypeUnion");
    };
    let written: Vec<_> = members.iter().map(|m| m.written).collect();
    assert_eq!(written, [Written::Bare, Written::Quoted, Written::Bare]);
    assert_eq!(members[1].text, "\"none\"");
    let at: Vec<_> = members.iter().map(|m| columns(&m.span)).collect();
    assert_eq!(at, [(2, 10, 14), (2, 17, 23), (2, 26, 34)]);

    let body = "type Level = \"Task\" | Other | -1\n";
    let result = parse(body, "level.spec");
    assert!(result.errors.is_empty(), "errors: {:?}", result.errors);
    let entry = &result.entities[0].fields.entries()[0];
    let FieldValue::VariantList(variants) = &entry.value else {
        panic!("expected a VariantList");
    };
    let written: Vec<_> = variants.iter().map(|m| m.written).collect();
    assert_eq!(written, [Written::Quoted, Written::Bare, Written::Literal]);
    assert_eq!(variants[0].text, "Task");
    let value = entry.value_span.as_ref().expect("the body's value span");
    assert_eq!(columns(value), (1, 14, 33));
}
