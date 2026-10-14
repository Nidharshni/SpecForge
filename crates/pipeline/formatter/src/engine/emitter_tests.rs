use super::*;

fn fmt(source: &str) -> String {
    let result = format_source(source, &FormatConfig::default());
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    let again = format_source(&result.formatted, &FormatConfig::default()).formatted;
    assert_eq!(again, result.formatted, "not idempotent");
    result.formatted
}

#[specforge_test_macros::test(
    behavior = "preserve_comments",
    verify = "leading comment attaches to following node"
)]
#[specforge_test_macros::test(
    invariant = "comment_preservation",
    verify = "leading comments remain attached to their following node"
)]
fn a_leading_comment_stays_directly_above_its_block() {
    let out = fmt(
        "behavior a \"A\" {\n  contract \"c\"\n}\n// doc for b\nbehavior b \"B\" {\n  contract \"c\"\n}\n",
    );
    assert!(out.contains("}\n\n// doc for b\nbehavior b"), "{out}");
}

#[specforge_test_macros::test(
    behavior = "preserve_comments",
    verify = "trailing comment attaches to preceding node on same line"
)]
#[specforge_test_macros::test(
    invariant = "comment_preservation",
    verify = "trailing comments remain attached to their preceding node"
)]
fn a_trailing_comment_stays_on_its_line() {
    let out =
        fmt("behavior a \"A\" { // header note\n  contract \"c\" // why\n  status draft\n}\n");
    assert!(out.contains("behavior a \"A\" { // header note\n"), "{out}");
    assert!(out.contains("contract \"c\" // why\n"), "{out}");
}

#[specforge_test_macros::test(
    behavior = "preserve_comments",
    verify = "section header comment attaches to next block group"
)]
#[specforge_test_macros::test(
    behavior = "preserve_comments",
    verify = "standalone comment block between blocks is preserved"
)]
fn standalone_and_section_comments_keep_their_separation() {
    let out = fmt(concat!(
        "behavior a \"A\" {\n  contract \"c\"\n}\n\n\n",
        "// standalone note\n\n\n",
        "// Section: more\nbehavior b \"B\" {\n  contract \"c\"\n}\n",
    ));
    assert_eq!(
        out,
        "behavior a \"A\" {\n  contract \"c\"\n}\n\n// standalone note\n\n// Section: more\nbehavior b \"B\" {\n  contract \"c\"\n}\n"
    );
}

#[specforge_test_macros::test(
    behavior = "preserve_comments",
    verify = "no comments are lost after formatting"
)]
#[specforge_test_macros::test(
    invariant = "comment_preservation",
    verify = "every comment in input appears in formatted output"
)]
fn comments_in_every_block_kind_stay_in_place() {
    let source = concat!(
        "spec \"s\" {\n  version \"1\"\n  // spec note\n}\n\n",
        "define widget {\n  // define note\n  size integer\n}\n\n",
        "ref gh.issue:1 \"Issue\" {\n  // ref note\n  status open\n}\n\n",
        "behavior a \"A\" {\n  contract \"c\"\n  // between\n  status draft\n  // again\n  // again\n  verify unit \"x\"\n}\n",
    );
    let out = fmt(source);
    assert!(out.contains("  version \"1\"\n  // spec note\n}"), "{out}");
    assert!(
        out.contains("define widget {\n  // define note\n  size integer\n}"),
        "{out}"
    );
    assert!(out.contains("  // ref note\n  status open\n}"), "{out}");
    assert!(
            out.contains("  contract \"c\"\n  // between\n  status   draft\n  // again\n  // again\n  verify unit \"x\"\n"),
            "{out}"
        );
}

#[specforge_test_macros::test(
    behavior = "preserve_comments",
    verify = "Preserve Comments During Formatting: comment preservation holds — cst_available, all_comments_attached, no_comments_lost"
)]
fn comment_text_is_kept_as_written() {
    let out = fmt(
        "/// doc comment\n//   - nested bullet\n//no space\nbehavior a \"A\" {\n  contract \"c\"\n}\n",
    );
    assert!(
        out.starts_with("/// doc comment\n//   - nested bullet\n// no space\nbehavior a"),
        "{out}"
    );
}

#[specforge_test_macros::test(
    behavior = "apply_format_rules",
    verify = "wrapping rules break long reference lists to multi-line"
)]
fn wrapping_a_list_never_splits_a_string() {
    let long = "Each kind has correct testable, singleton, and supports_verify flags";
    let source = format!(
        "feature f \"F\" {{\n  criteria [\"{long}\", \"second criterion that is long enough to wrap\"]\n}}\n"
    );
    let out = fmt(&source);
    assert!(out.contains(&format!("    \"{long}\",\n")), "{out}");
    assert!(out.contains("  criteria [\n"), "{out}");
    // A short hand-wrapped list is joined.
    let out = fmt("feature f \"F\" {\n  refs [\n    a,\n    b,\n  ]\n}\n");
    assert!(out.contains("  refs [a, b]\n"), "{out}");
}

#[specforge_test_macros::test(
    behavior = "apply_format_rules",
    verify = "statements keep their source order"
)]
fn statements_keep_their_order() {
    let out = fmt(
        "event e \"E\" {\n  verify unit \"first\"\n  channel bus\n  method m(x: string)\n  payload order\n}\n",
    );
    assert_eq!(
        out,
        "event e \"E\" {\n  verify unit \"first\"\n  channel bus\n  method m(x: string)\n  payload order\n}\n"
    );
}

#[specforge_test_macros::test(
    behavior = "apply_format_rules",
    verify = "alignment rules align field values within blocks"
)]
fn keys_and_annotations_align_but_verify_does_not() {
    let out = fmt(
        "type task \"T\" {\n  id string @readonly @unique\n  title string\n  completedAt timestamp @optional\n  verify   unit    \"x\"\n}\n",
    );
    assert_eq!(
        out,
        "type task \"T\" {\n  id          string    @readonly @unique\n  title       string\n  completedAt timestamp @optional\n  verify unit \"x\"\n}\n"
    );
}

#[specforge_test_macros::test(
    behavior = "apply_format_rules",
    verify = "a union that does not fit wraps one variant per line"
)]
fn a_long_union_wraps() {
    let source = "type verify_kind = unit | contract | integration | property | performance | mutation | load | deadlock_free\n";
    let out = fmt(source);
    assert!(
        out.starts_with("type verify_kind = unit\n  | contract\n"),
        "{out}"
    );
    assert_eq!(fmt("type k = a|b\n"), "type k = a | b\n");
}

/// Comments written between a statement's tokens.
const COMMENTS_IN_STATEMENTS: &str = concat!(
    "use {\n",
    "  x, // the x\n",
    "  y\n",
    "} from \"./x.spec\"\n",
    "\n",
    "ref gh.issue:42 // the issue\n",
    "  \"Support\"\n",
    "\n",
    "type t = // the variants\n",
    "  open | closed\n",
    "\n",
    "port p \"P\" {\n",
    "  contract // the contract\n",
    "    \"c\"\n",
    "  verify // the kind follows\n",
    "    unit \"v\"\n",
    "  tags x @deprecated // why\n",
    "    \"old\"\n",
    "  method f(a: A, // the a\n",
    "    b: B) -> C\n",
    "}\n",
);

#[specforge_test_macros::test(
    behavior = "preserve_comments",
    verify = "a statement holding a comment between its tokens is kept as written"
)]
#[specforge_test_macros::test(
    invariant = "comment_preservation",
    verify = "every comment in input appears in formatted output"
)]
fn a_statement_holding_a_comment_is_kept_as_written() {
    assert_eq!(fmt(COMMENTS_IN_STATEMENTS), COMMENTS_IN_STATEMENTS);
    let result = format_source(COMMENTS_IN_STATEMENTS, &FormatConfig::default());
    assert!(
        specforge_parser::parse(&result.formatted, "c.spec")
            .errors
            .is_empty(),
        "the output must still parse"
    );
}

#[test]
fn one_line_collapses_whitespace_between_lexemes() {
    assert_eq!(
        fmt("use  {  A ,B }  from \"x\"\n"),
        "use { A ,B } from \"x\"\n"
    );
    let out = fmt("port p \"P\" {\n  tags x @since   \"a  b\"\n}\n");
    assert!(out.contains("@since \"a  b\""), "{out}");
}

// --- Plan 14: a broken document reads as the parser reads it ---

const UNCLOSED_STRING: &str = "behavior a1 \"A\" {\n  contract \"x\n}\n\nbehavior b1 \"B\" {\n  contract    \"y\"\n}\n\nbehavior c1 \"C\" {\n  contract \"z\"\n}\n";

#[specforge_test_macros::test(
    behavior = "format_with_parse_errors",
    verify = "a form the parser could not read whole is kept as written and the forms after it are formatted"
)]
fn an_unclosed_string_keeps_only_its_form() {
    let result = format_source(UNCLOSED_STRING, &FormatConfig::default());
    assert_eq!(
        result.formatted,
        "behavior a1 \"A\" {\n  contract \"x\n}\n\nbehavior b1 \"B\" {\n  contract \"y\"\n}\n\nbehavior c1 \"C\" {\n  contract \"z\"\n}\n"
    );
    let w142: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| d.is(codes::W142))
        .collect();
    assert_eq!(w142.len(), 1, "{:?}", result.diagnostics);
    assert!(
        w142[0].message.starts_with("Parse error at lines 1-3,"),
        "{}",
        w142[0].message
    );
    let span = w142[0].span.as_ref().expect("a spanned W142");
    assert_eq!(
        (span.start_line, span.start_col, span.end_line, span.end_col),
        (1, 1, 3, 2)
    );
}

const UNCLOSED_BODY: &str =
    "behavior a1 \"A\" {\n  contract    \"x\"\n\nbehavior b1 \"B\" {\n  contract    \"y\"\n}\n";

#[specforge_test_macros::test(
    behavior = "format_with_parse_errors",
    verify = "the diagnostic spans every line of the form it kept, never past the document"
)]
fn a_kept_form_is_reported_at_its_lines() {
    let result = format_source(UNCLOSED_BODY, &FormatConfig::default());
    assert!(
        result
            .formatted
            .starts_with("behavior a1 \"A\" {\n  contract    \"x\"\n\nbehavior b1")
    );
    assert!(result.formatted.contains("  contract \"y\""));
    let span = result
        .diagnostics
        .iter()
        .find(|d| d.is(codes::W142))
        .and_then(|d| d.span.clone())
        .unwrap();
    assert_eq!((span.start_line, span.end_line), (1, 2));
}

#[specforge_test_macros::test(
    behavior = "format_with_parse_errors",
    verify = "a form the parser could not read whole is kept as written and the forms after it are formatted"
)]
fn formatting_a_broken_document_twice_changes_nothing() {
    let texts = [
        // R1: an unclosed body, then two blocks
        "behavior a1 \"A\" {\n  contract    \"x\"\n\nbehavior b1 \"B\" {\n  contract    \"y\"\n}\n\n\nbehavior c1 \"C\" {\n    contract \"z\"\n}\n",
        // R2: a field name typed without its value
        "behavior a1 \"A\" {\n  contract \"x\"\n  st\n\nbehavior b1 \"B\" {\n  contract \"y\"\n}\n\nbehavior c1 \"C\" {\n  contract \"z\"\n}\n",
        // R3: a method header without its parameter list
        "port pay \"Pay\" {\n  direction inbound\n  method charge\n\nbehavior settle \"Settle\" {\n  contract    \"Settles.\"\n}\n",
        // R5: an unclosed string in a port body
        "port pay \"Pay\" {\n  direction inbound\n  description \"Takes payments\n  method charge(amount: String) -> String\n}\n\nbehavior settle \"Settle\" {\n  contract    \"Settles.\"\n}\n",
        // R6: an unclosed string, then two blocks
        UNCLOSED_STRING,
    ];
    let ids = |text: &str| {
        specforge_parser::parse(text, "t.spec")
            .entities
            .iter()
            .map(|e| e.id.raw.to_string())
            .collect::<Vec<_>>()
    };
    for text in texts {
        let once = format_source(text, &FormatConfig::default());
        let twice = format_source(&once.formatted, &FormatConfig::default());
        assert_eq!(twice.formatted, once.formatted, "{text}");
        assert_eq!(ids(&once.formatted), ids(text), "{text}");
    }
}
