//! The one lexer of the `.spec` language (`specforge_parser::lex`),
//! checked against tree-sitter's grammar over the repository's spec.

use specforge_parser::lex::{
    Lexeme, LexemeKind, form_starts, is_ref_id_prefix, lex, line_indent, runs_into_text,
    starts_top_level_form,
};
use specforge_parser::parse_incremental;
use specforge_test_macros::test as specforge_test;
use std::path::{Path, PathBuf};
use tree_sitter_specforge::kind;

/// The `.spec` files under `dir`, recursively.
fn spec_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            spec_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "spec") {
            out.push(path);
        }
    }
}

/// The lexeme kind a grammar leaf of kind `node` is, when it is one the
/// lexer reads whole.
fn kind_of(node: &str) -> Option<LexemeKind> {
    Some(match node {
        kind::IDENTIFIER => LexemeKind::Ident,
        kind::SCHEME_REF_ID => LexemeKind::RefId,
        kind::STRING | kind::TRIPLE_QUOTED_STRING => LexemeKind::Str { closed: true },
        kind::COMMENT => LexemeKind::Comment,
        kind::INTEGER => LexemeKind::Number,
        _ => return None,
    })
}

/// Every leaf of `node`'s tree the lexer must read whole, as
/// (start, end, kind).
fn grammar_leaves(node: tree_sitter::Node, out: &mut Vec<(usize, usize, LexemeKind)>) {
    if node.child_count() == 0 {
        if node.is_named()
            && let Some(kind) = kind_of(node.kind())
        {
            out.push((node.start_byte(), node.end_byte(), kind));
        }
        return;
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        grammar_leaves(child, out);
    }
}

#[specforge_test(
    behavior = "lex_spec_text",
    verify = "the lexer agrees with the grammar on every spec file of the repository"
)]
fn lexer_agrees_with_the_grammar() {
    let root = specforge_repo_root::repo_root();
    let mut files = Vec::new();
    spec_files(&root.join("spec"), &mut files);
    assert!(files.len() > 150, "found only {} spec files", files.len());
    for corpus in ["integrations/rust/spec", "examples"] {
        spec_files(&root.join(corpus), &mut files);
    }
    // The forms of the language the corpora do not write; it must parse
    // without an error for the grammar's leaves to be the reference.
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/lexemes.spec");
    files.push(fixture.clone());
    let mut checked = 0;
    for path in &files {
        let text = std::fs::read_to_string(path).unwrap();
        let (file, tree) = parse_incremental(&text, &path.to_string_lossy(), None);
        let tree = tree.expect("a tree");
        if *path == fixture {
            assert!(file.errors.is_empty(), "the fixture: {:?}", file.errors);
            assert!(!tree.root_node().has_error(), "the fixture has a CST error");
        }
        let mut leaves = Vec::new();
        grammar_leaves(tree.root_node(), &mut leaves);
        let lexemes = lex(&text);
        let by_start: std::collections::HashMap<usize, &Lexeme> =
            lexemes.iter().map(|l| (l.start, l)).collect();
        for (start, end, kind) in leaves {
            let lexeme = by_start.get(&start).unwrap_or_else(|| {
                panic!(
                    "{}: no lexeme at byte {start} for {kind:?} {:?}",
                    path.display(),
                    &text[start..end]
                )
            });
            assert_eq!(
                (lexeme.end, lexeme.kind),
                (end, kind),
                "{}: {:?} lexed as {:?}",
                path.display(),
                &text[start..end],
                lexeme.text(&text)
            );
            checked += 1;
        }
    }
    assert!(checked > 10_000, "checked only {checked} leaves");
}

/// The (kind, text) of each lexeme of `text`.
fn lexemes(text: &str) -> Vec<(LexemeKind, &str)> {
    lex(text)
        .into_iter()
        .map(|l| (l.kind, l.text(text)))
        .collect()
}

#[specforge_test(behavior = "lex_spec_text", verify = "a scheme ref ID is one lexeme")]
fn a_scheme_ref_id_is_one_lexeme() {
    assert_eq!(lexemes("gh.issue:42"), [(LexemeKind::RefId, "gh.issue:42")]);
    assert_eq!(
        lexemes("refs [jira.story:ABC-123, b]"),
        [
            (LexemeKind::Ident, "refs"),
            (LexemeKind::Punct('['), "["),
            (LexemeKind::RefId, "jira.story:ABC-123"),
            (LexemeKind::Punct(','), ","),
            (LexemeKind::Ident, "b"),
            (LexemeKind::Punct(']'), "]"),
        ]
    );
    assert_eq!(
        lexemes("a.b"),
        [
            (LexemeKind::Ident, "a"),
            (LexemeKind::Punct('.'), "."),
            (LexemeKind::Ident, "b"),
        ]
    );
    assert_eq!(
        lexemes("a.b: 1"),
        [
            (LexemeKind::Ident, "a"),
            (LexemeKind::Punct('.'), "."),
            (LexemeKind::Ident, "b"),
            (LexemeKind::Punct(':'), ":"),
            (LexemeKind::Number, "1"),
        ],
        "an ID needs at least one character"
    );
}

#[specforge_test(
    behavior = "lex_spec_text",
    verify = "strings and comments are lexemes of their own and hold no others"
)]
fn strings_and_comments_are_lexemes_of_their_own() {
    let text = "behavior a \"a b\" { // a c\n  x [a] \"\"\"a\n\"q\" a\"\"\" y \"esc \\\" a\" z\n}";
    let names: Vec<&str> = lex(text)
        .into_iter()
        .filter(Lexeme::is_name)
        .map(|l| l.text(text))
        .collect();
    assert_eq!(names, ["behavior", "a", "x", "a", "y", "z"]);
    let others: Vec<(LexemeKind, &str)> = lexemes(text)
        .into_iter()
        .filter(|(kind, _)| matches!(kind, LexemeKind::Str { .. } | LexemeKind::Comment))
        .collect();
    assert_eq!(
        others,
        [
            (LexemeKind::Str { closed: true }, "\"a b\""),
            (LexemeKind::Comment, "// a c"),
            (LexemeKind::Str { closed: true }, "\"\"\"a\n\"q\" a\"\"\""),
            (LexemeKind::Str { closed: true }, "\"esc \\\" a\""),
        ]
    );
}

#[test]
fn a_word_is_never_split() {
    let text = "x_sesion_limit é→a sesion";
    let names: Vec<&str> = lex(text)
        .into_iter()
        .filter(Lexeme::is_name)
        .map(|l| l.text(text))
        .collect();
    assert_eq!(names, ["x_sesion_limit", "é→a", "sesion"]);
}

#[test]
fn numbers_start_with_a_digit() {
    assert_eq!(
        lexemes("a < 10ms and b > 1.5 x -3"),
        [
            (LexemeKind::Ident, "a"),
            (LexemeKind::Punct('<'), "<"),
            (LexemeKind::Number, "10ms"),
            (LexemeKind::Ident, "and"),
            (LexemeKind::Ident, "b"),
            (LexemeKind::Punct('>'), ">"),
            (LexemeKind::Number, "1.5"),
            (LexemeKind::Ident, "x"),
            (LexemeKind::Punct('-'), "-"),
            (LexemeKind::Number, "3"),
        ]
    );
}

#[specforge_test(
    behavior = "lex_spec_text",
    verify = "a string spanning lines is one lexeme, and one the grammar would not close ends at its line's end"
)]
fn an_unclosed_string_ends_at_its_lines_end() {
    let unclosed = LexemeKind::Str { closed: false };
    assert_eq!(
        lexemes("contract \"see\nrefs [a]"),
        [
            (LexemeKind::Ident, "contract"),
            (unclosed, "\"see"),
            (LexemeKind::Ident, "refs"),
            (LexemeKind::Punct('['), "["),
            (LexemeKind::Ident, "a"),
            (LexemeKind::Punct(']'), "]"),
        ]
    );
    assert_eq!(
        lexemes("d \"\"\"open\nstill"),
        [(LexemeKind::Ident, "d"), (unclosed, "\"\"\"open\nstill")],
        "an unclosed triple-quoted string runs to the end"
    );
    assert_eq!(
        lexemes("c \"a \\\nb"),
        [
            (LexemeKind::Ident, "c"),
            (unclosed, "\"a \\"),
            (LexemeKind::Ident, "b"),
        ],
        "a backslash before a line break is no escape"
    );
    assert_eq!(
        lexemes("c \"x"),
        [(LexemeKind::Ident, "c"), (unclosed, "\"x")],
        "a quote at the text's end"
    );
}

#[specforge_test(
    behavior = "lex_spec_text",
    verify = "a string spanning lines is one lexeme, and one the grammar would not close ends at its line's end"
)]
fn a_string_spanning_lines_is_one_lexeme() {
    assert_eq!(
        lexemes("d \"a\nb\" c"),
        [
            (LexemeKind::Ident, "d"),
            (LexemeKind::Str { closed: true }, "\"a\nb\""),
            (LexemeKind::Ident, "c"),
        ]
    );
    // The closing quote runs straight into text: the pairing an unclosed
    // quote shifts, so the first quote is the unclosed one.
    assert_eq!(
        lexemes("c \"a\n  b\"x"),
        [
            (LexemeKind::Ident, "c"),
            (LexemeKind::Str { closed: false }, "\"a"),
            (LexemeKind::Ident, "b"),
            (LexemeKind::Str { closed: false }, "\"x"),
        ]
    );
}

#[specforge_test(
    behavior = "lex_spec_text",
    verify = "an unclosed triple-quoted string ends before the next line that starts a top-level form"
)]
fn an_unclosed_triple_quoted_string_ends_before_the_next_form() {
    let unclosed = LexemeKind::Str { closed: false };
    let closed = LexemeKind::Str { closed: true };
    let after = [
        (LexemeKind::Ident, "behavior"),
        (LexemeKind::Ident, "b"),
        (closed, "\"B\""),
        (LexemeKind::Punct('{'), "{"),
        (LexemeKind::Punct('}'), "}"),
    ];
    let text = "behavior a \"A\" {\n  contract \"\"\"\n  text\n}\n\nbehavior b \"B\" {\n}\n";
    let read = lexemes(text);
    assert_eq!(
        read[5],
        (unclosed, "\"\"\"\n  text\n}\n"),
        "it ends at the end of the line before the form"
    );
    assert_eq!(read[6..], after);

    // A form indented no deeper than the top-level line the string is under.
    let text =
        "  behavior a \"A\" {\n    c \"\"\"\n    behavior x \"X\" {\n  behavior b \"B\" {\n}";
    let read = lexemes(text);
    assert_eq!(read[5], (unclosed, "\"\"\"\n    behavior x \"X\" {"));
    assert_eq!(read[6..], after);

    // The pairing it shifts: its closing quotes are the next string's
    // opening ones, running straight into text.
    let text = "a x \"X\" {\n  c \"\"\"\n  text\n}\nbehavior b \"B\" {\n  c \"\"\"y\"\"\"\n}";
    let read = lexemes(text);
    assert_eq!(read[5], (unclosed, "\"\"\"\n  text\n}"));
    assert_eq!(read[6..10], after[..4]);
    assert_eq!(
        read[10..],
        [
            (LexemeKind::Ident, "c"),
            (closed, "\"\"\"y\"\"\""),
            (LexemeKind::Punct('}'), "}"),
        ]
    );
}

#[specforge_test(behavior = "lex_spec_text", verify = "a scheme ref ID is one lexeme")]
fn a_ref_id_cut_short_is_a_ref_id_prefix() {
    for text in [
        "gh",
        "gh.",
        "gh.issue",
        "gh.issue:",
        "gh.issue:42",
        "a.b:c.d:e",
    ] {
        assert!(is_ref_id_prefix(text), "{text:?}");
    }
    for text in ["", "9a", "a.b.c", "gh.issue:4 2", "gh.issue:\"x"] {
        assert!(!is_ref_id_prefix(text), "{text:?}");
    }
}

#[test]
fn what_runs_into_a_closing_quote_is_text() {
    let text = "\"a\"x \"b\" \"c\"]\"d\"/ \"e\"|";
    for (needle, runs) in [
        ("x", true),
        (" \"b", false),
        ("]", false),
        ("/", false),
        ("|", false),
    ] {
        let at = text.find(needle).unwrap();
        assert_eq!(runs_into_text(text, at), runs, "{needle:?}");
    }
    assert!(!runs_into_text(text, text.len()), "the end of the text");
}

/// Whether `line`, lexed alone, starts a top-level form at indentation at
/// most `max_indent`: how the parser's recovery asks.
fn starts_alone(line: &str, max_indent: usize) -> bool {
    starts_top_level_form(line, &lex(line), max_indent)
}

/// Whether the lexeme at `needle` in `text` starts a top-level form, read
/// from the document's lexemes: how the LSP's document asks.
fn starts_in(text: &str, needle: &str, max_indent: usize) -> bool {
    let at = text.find(needle).unwrap_or_else(|| panic!("no {needle:?}"));
    let lexemes = lex(text);
    let first = lexemes.partition_point(|l| l.start < at);
    starts_top_level_form(text, &lexemes[first..], max_indent)
}

#[test]
fn form_start_lines() {
    // Every shape of the grammar's top-level forms.
    assert!(starts_alone("behavior b \"B\" {", 0));
    assert!(starts_alone("type t {", 0));
    assert!(starts_alone("spec \"S\" {", 0));
    assert!(starts_alone("ref github.issue:org/repo#1 \"T\"", 0));
    assert!(starts_alone("use \"./a.spec\"", 0));
    assert!(starts_alone("pub use { A } from \"./a.spec\"", 0));
    assert!(!starts_alone("  behavior b \"B\" {", 0));
    assert!(starts_alone("  behavior b \"B\" {", 2));
    assert!(!starts_alone("the quick brown fox", 0));
    assert!(!starts_alone("status done", 0));
    assert!(!starts_alone("behavior b \"B", 0));
    assert!(starts_alone("type status = \"a\" | \"b\"", 0));
    assert!(starts_alone("type t =", 0));
    assert!(starts_alone("use\"./a.spec\"", 0));
    // What the LSP's copy took and the grammar does not.
    assert!(!starts_alone("use", 0));
    assert!(!starts_alone("pub", 0));
    assert!(!starts_alone("behavior b x y {", 0));
    assert!(!starts_alone("behavior gh.issue:1 {", 0));
    assert!(!starts_alone("behavior b \"\"\"B\"\"\" {", 0));
    // Read from a document's lexemes.
    let text = concat!(
        "behavior a \"A\" {\n",
        "  x behavior b \"B\" {\n",
        "}\n",
        "type t\n",
        "  = A\n",
        "behavior c \"C\n",
        "D\" {\n",
        "}\n",
    );
    assert!(starts_in(text, "behavior a", 0));
    assert!(
        !starts_in(text, "behavior b", 4),
        "not the first lexeme of its line"
    );
    assert!(!starts_in(text, "type t", 0), "its `=` is on the next line");
    assert!(!starts_in(text, "behavior c", 0), "a title spanning lines");
    assert!(!starts_top_level_form(text, &[], 0));
    assert_eq!(line_indent(text, text.find("x behavior").unwrap()), Some(2));
    assert_eq!(line_indent(text, text.find("behavior b").unwrap()), None);
}

#[specforge_test(
    behavior = "lex_spec_text",
    verify = "a line starts a top-level form exactly where the grammar reads one, on every spec file of the repository"
)]
fn form_starts_are_the_grammars_top_level_nodes() {
    let root = specforge_repo_root::repo_root();
    let mut files = Vec::new();
    for corpus in ["spec", "integrations/rust/spec", "examples"] {
        spec_files(&root.join(corpus), &mut files);
    }
    files.push(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/lexemes.spec"));
    assert!(files.len() > 150, "found only {} spec files", files.len());
    let mut missed = std::collections::BTreeSet::new();
    let mut spurious = std::collections::BTreeSet::new();
    for file in files {
        let src = std::fs::read_to_string(&file).unwrap();
        let lexemes = lex(&src);
        let (_, tree) = parse_incremental(&src, &file.to_string_lossy(), None);
        let tree = tree.unwrap();
        let root_node = tree.root_node();
        assert!(!root_node.has_error(), "{}: does not parse", file.display());
        let mut cursor = root_node.walk();
        for node in root_node.named_children(&mut cursor) {
            if node.kind() == kind::COMMENT {
                continue;
            }
            let row = node.start_position().row;
            let column = node.start_position().column;
            let line = src.lines().nth(row).unwrap();
            assert!(
                line[..column].trim().is_empty(),
                "{}: two forms on one line",
                file.display()
            );
            let first = lexemes.partition_point(|l| l.start < node.start_byte());
            // As recovery reads it: the line alone.
            if !starts_top_level_form(line, &lex(line), column) {
                missed.insert(format!(
                    "{}: {} (alone): {line}",
                    file.display(),
                    node.kind()
                ));
            }
            // As the LSP's document reads it: the document's lexemes.
            if !starts_top_level_form(&src, &lexemes[first..], column) {
                missed.insert(format!(
                    "{}: {} (document): {line}",
                    file.display(),
                    node.kind()
                ));
            }
            // No other line of the form starts one at its indentation.
            let end = lexemes.partition_point(|l| l.start < node.end_byte());
            for j in first + 1..end {
                if starts_top_level_form(&src, &lexemes[j..], column) {
                    let at = src[..lexemes[j].start].matches('\n').count();
                    spurious.insert(format!(
                        "{}:{}: {}",
                        file.display(),
                        at + 1,
                        src.lines().nth(at).unwrap()
                    ));
                }
            }
        }
    }
    assert!(missed.is_empty(), "not form starts: {missed:#?}");
    assert!(
        spurious.is_empty(),
        "form starts inside a form: {spurious:#?}"
    );
}

/// Whether tree-sitter's grammar ends what is left open before `line` (one
/// line, not indented): its scanner (`crates/kernel/tree-sitter/src/scanner.c`)
/// reads the line as a form start. The probe leaves a list open inside a
/// body, which the scanner ends with a zero-width token, no error, before
/// such a line (the body then misses its `}`); a list the grammar does not
/// end there runs on, or misses its `]`. Read off the list, the answer is
/// the scanner's, whatever the error recovery makes of the line after it.
fn grammar_ends_a_body_at(parser: &mut tree_sitter::Parser, line: &str) -> bool {
    fn ended_list(node: tree_sitter::Node, end: usize) -> bool {
        let mut cursor = node.walk();
        let children: Vec<_> = node.children(&mut cursor).collect();
        (node.kind() == kind::LIST
            && node.end_byte() == end
            && !children.iter().any(|c| c.is_missing()))
            || children.into_iter().any(|c| ended_list(c, end))
    }
    let probe = "probe p {\n  k [v\n";
    let source = format!("{probe}{line}\n");
    let tree = parser.parse(&source, None).unwrap();
    ended_list(tree.root_node(), probe.len() - 1)
}

#[specforge_test(
    behavior = "lex_spec_text",
    verify = "the grammar ends an unclosed body at exactly the lines the form-start rule reads as form starts"
)]
fn the_grammars_scanner_reads_form_starts_as_the_lexer_does() {
    let root = specforge_repo_root::repo_root();
    let mut files = Vec::new();
    for corpus in ["spec", "integrations/rust/spec", "examples"] {
        spec_files(&root.join(corpus), &mut files);
    }
    files.push(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/lexemes.spec"));
    let mut lines = std::collections::BTreeSet::new();
    for file in files {
        let src = std::fs::read_to_string(&file).unwrap();
        lines.extend(src.lines().map(|l| l.trim().to_string()));
    }
    // The shapes the corpora do not write.
    for line in [
        "use",
        "use x",
        "use *",
        "use {",
        "use \"unclosed",
        "pub",
        "pub use",
        "pub use \"a\"",
        "pub x \"a\"",
        "spec x {",
        "spec \"T\"",
        "spec \"T\" {",
        "spec \"T\"{",
        "spec\"T\"{",
        "ref x \"T\"",
        "ref a.b:c",
        "ref a.b: \"T\"",
        "ref a.b:c\"T\"",
        "ref a.b:c \"T\" {",
        "ref é.b:c \"T\"",
        "kind name {",
        "kind name{",
        "kind name =",
        "kind name=x",
        "kind name \"T\" {",
        "kind name \"T\"",
        "kind name \"\" {",
        "kind name \"\"\"T\"\"\" {",
        "kind name \"T\\\" {",
        "kind name \"T\\\\\" {",
        "kind name \"unclosed {",
        "kind name \"T\" x {",
        "kind name // {",
        "kind name.x {",
        "kind a.b:c {",
        "a.b:c name {",
        "kind 1 {",
        "1kind name {",
        "kïnd nämé {",
        "kind name\t{",
        "kind_2 name_3 {",
        "verify unit \"x\" {",
        "{",
        "}",
        "= x",
        "// comment",
    ] {
        lines.insert(line.to_string());
    }
    lines.remove("");
    assert!(lines.len() > 5000, "found only {} lines", lines.len());
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&tree_sitter_specforge::LANGUAGE.into())
        .unwrap();
    let disagree: Vec<String> = lines
        .iter()
        .filter(|line| starts_alone(line, 0) != grammar_ends_a_body_at(&mut parser, line))
        .map(|line| format!("{line:?}: lexer {}", starts_alone(line, 0)))
        .collect();
    assert!(disagree.is_empty(), "{disagree:#?}");
}

#[specforge_test(
    behavior = "lex_spec_text",
    verify = "a document's form starts are the grammar's top-level nodes, and stay so with every body left unclosed"
)]
fn a_documents_form_starts_are_its_top_level_nodes() {
    let root = specforge_repo_root::repo_root();
    let mut files = Vec::new();
    for corpus in ["spec", "integrations/rust/spec", "examples"] {
        spec_files(&root.join(corpus), &mut files);
    }
    assert!(files.len() > 150);
    for file in files {
        let src = std::fs::read_to_string(&file).unwrap();
        let (_, tree) = parse_incremental(&src, "c.spec", None);
        let tree = tree.unwrap();
        let root_node = tree.root_node();
        let mut cursor = root_node.walk();
        let nodes: Vec<tree_sitter::Node> = root_node
            .named_children(&mut cursor)
            .filter(|n| n.kind() != kind::COMMENT)
            .collect();
        let expected: Vec<usize> = nodes.iter().map(|n| n.start_byte()).collect();
        let starts = |text: &str| {
            let lexemes = lex(text);
            form_starts(text, &lexemes)
                .into_iter()
                .map(|i| lexemes[i].start)
                .collect::<Vec<_>>()
        };
        assert_eq!(starts(&src), expected, "{}", file.display());
        // Every top-level `}` blanked: each body is left unclosed.
        let mut bytes = src.clone().into_bytes();
        for node in &nodes {
            if bytes[node.end_byte() - 1] == b'}' {
                bytes[node.end_byte() - 1] = b' ';
            }
        }
        let unclosed = String::from_utf8(bytes).unwrap();
        assert_eq!(starts(&unclosed), expected, "{} (unclosed)", file.display());
    }
}

#[test]
fn form_starts_close_every_bracket() {
    let text = concat!(
        "behavior a \"A\" {\n",   // a form
        "  tags [x,\n",           //   a list left open
        "  method m(p: T\n",      //   a parameter list left open
        "  behavior b \"B\" {\n", // deeper than the last line begun with no bracket open: not a form
        "type t = A | B\n",       // a form: closes all three
        "  behavior c \"C\" {\n", // a form: no bracket is open
        "}\n",
        "// behavior d \"D\" {\n", // a comment: not a form
        "use \"./x\"\n",           // a form
    );
    let lexemes = lex(text);
    let at: Vec<&str> = form_starts(text, &lexemes)
        .into_iter()
        .map(|i| text[lexemes[i].start..].lines().next().unwrap())
        .collect();
    assert_eq!(
        at,
        [
            "behavior a \"A\" {",
            "type t = A | B",
            "behavior c \"C\" {",
            "use \"./x\""
        ]
    );
}
