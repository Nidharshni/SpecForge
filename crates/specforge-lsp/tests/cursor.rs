//! What the document module reads at a position (`Document::at`): the
//! word, the place, the entity, field and `use` statement around it.

use specforge_lsp::{CompletionSite, Cursor, Document, EntityAt, Place};
use specforge_test_macros::test as spec;
use tower_lsp::lsp_types::Position;
use tree_sitter_specforge::{field, kind};

/// The document of `text`.
fn doc(text: &str) -> Document {
    Document::new(text.into())
}

/// The cursor at (`line`, UTF-16 `character`) of `doc`, which must exist.
fn at(doc: &Document, line: u32, character: u32) -> Cursor<'_> {
    doc.at(Position::new(line, character))
        .unwrap_or_else(|| panic!("no position {line}:{character}"))
}

/// The (line, character) of the `nth` occurrence of `needle` in `text`,
/// plus `offset` characters (ASCII text).
fn find(text: &str, needle: &str, nth: usize, offset: u32) -> (u32, u32) {
    let at = text
        .match_indices(needle)
        .nth(nth)
        .unwrap_or_else(|| panic!("no {needle:?} #{nth}"))
        .0;
    let before = &text[..at];
    let line = before.matches('\n').count() as u32;
    let column = before.rsplit('\n').next().unwrap_or("").len() as u32;
    (line, column + offset)
}

#[spec(
    invariant = "lsp_utf16_positions",
    verify = "the word under a cursor is found by its UTF-16 column"
)]
fn the_word_is_found_by_its_utf16_column() {
    // `contract ` = 9 units/bytes, 🚀 = 2 units/4 bytes, ` ` = 1 unit/byte,
    // `alpha_beta` spans units 12..22 (bytes 14..24).
    let doc = doc("contract 🚀 alpha_beta\n");
    let word = |character| at(&doc, 0, character).word().map(|w| w.text);

    // Column 12 (the word's start in UTF-16 units) is byte 14, not byte
    // 12 (inside the emoji).
    assert_eq!(word(12), Some("alpha_beta"));
    let found = at(&doc, 0, 15).word().unwrap();
    assert_eq!(found.prefix, "alp");
    assert_eq!(
        (found.range.start, found.range.end),
        (Position::new(0, 12), Position::new(0, 22))
    );
    // Just past the word's end still names it.
    assert_eq!(word(22), Some("alpha_beta"));
    // A column inside the emoji's surrogate pair resolves to the emoji's
    // start: a word of its own to the lexer, which never splits one.
    assert_eq!(word(10), Some("🚀"));
    // A column beyond the line's UTF-16 length is no position at all.
    assert!(doc.at(Position::new(0, 23)).is_none());
}

#[test]
fn place_tells_code_from_strings_and_comments() {
    let text = concat!(
        "behavior a \"A b\" { // note\n",
        "  description \"\"\"\n",
        "    long text\n",
        "  \"\"\"\n",
        "  contract \"open\n",
        "}\n",
    );
    let doc = doc(text);
    let place = |(line, character): (u32, u32)| at(&doc, line, character).place();
    assert_eq!(place(find(text, "behavior", 0, 2)), Place::Code);
    assert_eq!(place(find(text, "A b", 0, 1)), Place::String);
    assert_eq!(
        place(find(text, "\"A b\"", 0, 0)),
        Place::Code,
        "before the quote"
    );
    assert_eq!(
        place(find(text, "\"A b\"", 0, 5)),
        Place::Code,
        "after the closing quote"
    );
    assert_eq!(place(find(text, "note", 0, 2)), Place::Comment);
    assert_eq!(
        place(find(text, "note", 0, 4)),
        Place::Comment,
        "a comment's line end"
    );
    assert_eq!(place(find(text, "long", 0, 1)), Place::String);
    assert_eq!(
        place(find(text, "open", 0, 4)),
        Place::String,
        "an unclosed string runs to its line's end"
    );
    assert_eq!(place(find(text, "}", 0, 0)), Place::Code);
}

#[test]
fn a_multi_line_string_is_a_string_on_every_line() {
    let text = concat!(
        "behavior login \"Log in\" {\n",
        "  contract \"first line\n",
        "  second line mentions login and ends\"\n",
        "}\n",
        "\n",
        "behavior logout \"Log out\" {\n",
        "  contract \"x\"\n",
        "}\n",
    );
    let doc = doc(text);
    let (line, character) = find(text, "mentions", 0, 2);
    assert_eq!(at(&doc, line, character).place(), Place::String);
}

#[test]
fn the_entity_and_field_around_a_cursor() {
    let text = concat!(
        "behavior login \"Login\" {\n",
        "  requires {\n",
        "    ready \"x\"\n",
        "  }\n",
        "  invariants [a, b]\n",
        "  contract \"c\"\n",
        "}\n",
        "type t \"T\" { name string kind string }\n",
        "behavior half \"Half\" {\n",
        "  refs [\n",
        "behavior next \"Next\" {\n",
        "  \n",
        "}\n",
        "define widget {\n",
        "  x \"y\"\n",
        "}\n",
        "\n",
    );
    let doc = doc(text);
    let entity = |(line, character): (u32, u32)| at(&doc, line, character).entity();
    let field = |(line, character): (u32, u32)| at(&doc, line, character).field();
    let login = Some(EntityAt {
        kind: "behavior",
        id: Some("login"),
    });

    // The header line.
    assert_eq!(entity(find(text, "behavior", 0, 2)), login);
    assert_eq!(entity(find(text, "login", 0, 1)), login);
    assert_eq!(field(find(text, "login", 0, 1)), None);
    // The body, and a nested block: still the entity; no field of its own
    // body inside the block.
    assert_eq!(entity(find(text, "requires", 0, 1)), login);
    assert_eq!(field(find(text, "requires", 0, 3)), Some("requires"));
    assert_eq!(entity(find(text, "ready", 0, 1)), login);
    assert_eq!(field(find(text, "ready", 0, 1)), None);
    // After the nested block, the fields of the body.
    assert_eq!(field(find(text, "invariants", 0, 3)), Some("invariants"));
    assert_eq!(field(find(text, "[a", 0, 1)), Some("invariants"));
    assert_eq!(field(find(text, "b]", 0, 1)), Some("invariants"));
    assert_eq!(field(find(text, "\"c\"", 0, 1)), Some("contract"));
    // A one-line body.
    let t = Some(EntityAt {
        kind: "type",
        id: Some("t"),
    });
    assert_eq!(entity(find(text, "kind string", 0, 1)), t);
    assert_eq!(field(find(text, "kind string", 0, 1)), Some("kind"));
    assert_eq!(field(find(text, "string }", 0, 1)), Some("kind"));
    // An unclosed list and body, then a new block at column 1: the new
    // block is not part of the half-typed one.
    let next = Some(EntityAt {
        kind: "behavior",
        id: Some("next"),
    });
    assert_eq!(entity(find(text, "next", 0, 1)), next);
    assert_eq!(entity((11, 2)), next);
    assert_eq!(field((11, 2)), None);
    // A define block declares nothing.
    assert_eq!(entity(find(text, "x \"y\"", 0, 0)), None);
    // The top level.
    assert_eq!(entity((16, 0)), None);
}

#[spec(
    behavior = "complete_field_names",
    verify = "a bracket inside a string opens no reference list"
)]
fn a_bracket_in_a_string_opens_no_list() {
    let text = "behavior alpha \"Alpha\" {\n  contract \"see [docs\"\n  \n}\n";
    let doc = doc(text);
    let cursor = at(&doc, 2, 2);
    assert_eq!(cursor.field(), None);
    assert_eq!(cursor.entity().map(|e| e.kind), Some("behavior"));
}

#[test]
fn a_use_statement_is_an_import_anywhere_on_it() {
    let text = concat!(
        "use \"types/core\"\n",
        "pub use { token, other as alias } from \"types/more\"\n",
        "use * as all from \"types/all\"\n",
        "behavior b \"B\" {}\n",
    );
    let doc = doc(text);
    let import = |(line, character): (u32, u32)| at(&doc, line, character).import();
    assert_eq!(import((0, 0)), Some("types/core"));
    assert_eq!(import(find(text, "types/core", 0, 3)), Some("types/core"));
    assert_eq!(import((1, 0)), Some("types/more"), "pub");
    assert_eq!(import(find(text, "token", 0, 2)), Some("types/more"));
    assert_eq!(import(find(text, "alias", 0, 2)), Some("types/more"));
    assert_eq!(import(find(text, "from", 0, 1)), Some("types/more"));
    assert_eq!(import(find(text, "all", 0, 1)), Some("types/all"));
    assert_eq!(import((3, 2)), None);
}

/// A body left unclosed, then a union, an inline ref and a closed block.
const UNCLOSED: &str = concat!(
    "behavior login \"Log in\" {\n",
    "  contract \"Logs in.\"\n",
    "\n",
    "type status = \"active\" | \"inactive\"\n",
    "\n",
    "ref github.issue:org/repo#1 \"Tracking issue\"\n",
    "\n",
    "behavior logout \"Log out\" {\n",
    "  contract \"Logs out.\"\n",
    "}\n",
);

#[spec(
    behavior = "complete_keywords",
    verify = "keywords are suggested after a top-level form that follows an unclosed body"
)]
fn a_union_and_an_inline_ref_end_an_unclosed_body() {
    let doc = doc(UNCLOSED);
    let fields = CompletionSite::Fields {
        kind: "behavior",
        prefix: "",
    };
    assert_eq!(
        at(&doc, 3, 7).entity(),
        Some(EntityAt {
            kind: "type",
            id: Some("status")
        }),
        "status"
    );
    assert_eq!(
        at(&doc, 5, 7).entity(),
        Some(EntityAt {
            kind: "ref",
            id: Some("github.issue:org/repo#1")
        }),
        "the ref's ID"
    );
    assert_eq!(
        at(&doc, 4, 0).completion(),
        CompletionSite::Keywords { prefix: "" },
        "after the union"
    );
    // Stable: the body is still open before the union, and a block at
    // column 1 with a `{` resumes.
    assert_eq!(at(&doc, 2, 0).completion(), fields);
    assert_eq!(
        at(&doc, 7, 10).entity(),
        Some(EntityAt {
            kind: "behavior",
            id: Some("logout")
        })
    );
}

#[test]
fn a_bare_use_or_pub_does_not_end_an_unclosed_body() {
    let head = "behavior login \"Log in\" {\n  contract \"x\"\n\n";
    let login = Some(EntityAt {
        kind: "behavior",
        id: Some("login"),
    });
    for word in ["use", "pub"] {
        let doc = doc(&format!("{head}{word}\n"));
        assert_eq!(at(&doc, 3, 1).entity(), login, "{word}");
    }
    // Stable: a `use` with its path ends the body.
    let import_doc = doc(&format!("{head}use \"./a.spec\"\n"));
    assert_eq!(at(&import_doc, 3, 1).import(), Some("./a.spec"));
    assert_eq!(at(&import_doc, 3, 1).entity(), None);
}

#[test]
fn a_form_indented_like_the_open_one_ends_it() {
    let indented = doc("  behavior a \"A\" {\n    contract \"x\"\n\n  behavior b \"B\" {\n  }\n");
    assert_eq!(
        at(&indented, 3, 12).entity(),
        Some(EntityAt {
            kind: "behavior",
            id: Some("b")
        })
    );
    // Stable: a union indented deeper than the open form's line is no form.
    let deeper = doc("behavior a \"A\" {\n  contract \"x\"\n  type t = A | B\n");
    assert_eq!(
        at(&deeper, 2, 8).entity(),
        Some(EntityAt {
            kind: "behavior",
            id: Some("a")
        })
    );
}

#[spec(
    behavior = "provide_semantic_tokens",
    verify = "the document resumes at every top-level form of the repository's spec after an unclosed body"
)]
fn every_top_level_form_of_the_corpora_ends_an_unclosed_body() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut files = Vec::new();
    for corpus in ["spec", "integrations/rust/spec", "examples"] {
        crate::spec_files(&root.join(corpus), &mut files);
    }
    assert!(files.len() > 150, "found only {} spec files", files.len());
    for path in &files {
        let source = std::fs::read_to_string(path).unwrap();
        let (_, tree) = specforge_parser::parse_incremental(&source, "corpus.spec", None);
        let tree = tree.unwrap();
        let top = tree.root_node();
        assert!(!top.has_error(), "{}: does not parse", path.display());
        let mut walker = top.walk();
        let forms: Vec<tree_sitter::Node> = top
            .named_children(&mut walker)
            .filter(|n| n.kind() != kind::COMMENT)
            .collect();
        // Every block's closing brace blanked: each body is left unclosed.
        let mut bytes = source.clone().into_bytes();
        for form in &forms {
            if bytes[form.end_byte() - 1] == b'}' {
                bytes[form.end_byte() - 1] = b' ';
            }
        }
        let text = String::from_utf8(bytes).unwrap();
        let doc = doc(&text);
        let named = |name: &str, node: tree_sitter::Node| {
            node.child_by_field_name(name)
                .map(|n| &text[n.byte_range()])
        };
        for form in &forms {
            let start = form.start_position();
            let where_ = format!("{}:{} {}", path.display(), start.row + 1, form.kind());
            // Inside the form's first word (ASCII; only whitespace before it).
            let cursor = at(&doc, start.row as u32, start.column as u32 + 1);
            match form.kind() {
                kind::USE_IMPORT | kind::PUB_USE_IMPORT => {
                    assert!(cursor.import().is_some(), "{where_}");
                }
                kind::DEFINE_BLOCK => assert_eq!(cursor.entity(), None, "{where_}"),
                kind::SPEC_BLOCK => assert_eq!(
                    cursor.entity(),
                    Some(EntityAt {
                        kind: "spec",
                        id: None
                    }),
                    "{where_}"
                ),
                kind::REF_BLOCK => {
                    let inner = form.named_child(0).unwrap();
                    assert_eq!(
                        cursor.entity(),
                        Some(EntityAt {
                            kind: "ref",
                            id: named(field::ID, inner)
                        }),
                        "{where_}"
                    );
                }
                _ => assert_eq!(
                    cursor.entity(),
                    Some(EntityAt {
                        kind: named(field::KIND, *form).unwrap(),
                        id: named(field::NAME, *form),
                    }),
                    "{where_}"
                ),
            }
        }
    }
}
