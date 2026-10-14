use super::*;

#[test]
fn method_statements_survive_formatting_idempotently() {
    let source = concat!(
        "port FileSystem {\n",
        "  direction outbound\n",
        "  method readFile(path: string) -> Result<string, EmitterError>\n",
        "  method flush()\n",
        "}\n",
    );
    let config = FormatConfig::default();
    let once = format_source(source, &config);
    assert!(
        once.diagnostics.is_empty(),
        "format diagnostics: {:?}",
        once.diagnostics
    );
    assert_eq!(
        once.formatted.matches("method readFile").count(),
        1,
        "method must be emitted exactly once: {:?}",
        once.formatted
    );
    assert!(
        once.formatted
            .contains("method readFile(path: string) -> Result<string, EmitterError>"),
        "method line lost or mangled: {:?}",
        once.formatted
    );
    let twice = format_source(&once.formatted, &config);
    assert_eq!(
        twice.formatted, once.formatted,
        "formatting must be idempotent"
    );
}
