use specforge_common::{SourceSpan, Sym};
use specforge_extension_sdk::prelude::{ContributionsBuilder, FieldType};
use specforge_graph::{Graph, Node};
use specforge_lsp::{Document, MOD_DECLARATION, MOD_REFERENCE, SemanticToken};
use specforge_ops::view::ProjectView;
use specforge_parser::{EntityId, EntityKind, FieldMap};
use specforge_project::coverage::RecordedCoverage;
use specforge_registry::RegistryBuild;
use specforge_test_macros::test as spec;
use tree_sitter_specforge::{field, kind};

use crate::registries::registries;
use std::sync::Arc;

/// The semantic tokens of `text` over `registries` and `graph`.
fn tokens_over(text: &str, registries: RegistryBuild, graph: &Graph) -> Vec<SemanticToken> {
    let env = specforge_project::Environment::with_registries(registries, None);
    let recorded = RecordedCoverage::over(graph, &env);
    let view = ProjectView::new(graph, &env, None, &recorded);
    Document::new("file:///test.spec".into(), text.into()).tokens(&view)
}

/// The semantic tokens of `text` over `registries` and an empty graph
/// (every reference names no entity).
fn tokens_of(text: &str, registries: RegistryBuild) -> Vec<SemanticToken> {
    tokens_over(text, registries, &Graph::new())
}

/// What an extension declares for `(kind keyword, declared semantic_token)`
/// pairs.
fn declare_kinds(c: &mut ContributionsBuilder, entries: &[(&str, Option<&str>)]) {
    for (kind, token) in entries {
        c.kind(kind, |k| {
            k.testable(true).supports_verify(true);
            if let Some(token) = token {
                k.semantic_token(token);
            }
        });
    }
}

/// The registries of `(kind keyword, declared semantic_token)` pairs.
fn kinds(entries: &[(&str, Option<&str>)]) -> RegistryBuild {
    registries("@test/ext", |c| declare_kinds(c, entries))
}

/// The token classified for `text`, which must exist.
fn token<'a>(tokens: &'a [SemanticToken], text: &str) -> &'a SemanticToken {
    tokens
        .iter()
        .find(|t| t.text == text)
        .unwrap_or_else(|| panic!("no token {text:?} in {tokens:?}"))
}

// -- provide_semantic_tokens --------------------------------------------------

#[spec(
    behavior = "provide_semantic_tokens",
    verify = "the forms after an unclosed triple-quoted string are classified as the parser reads them"
)]
fn the_forms_after_an_unclosed_triple_quoted_string_are_classified() {
    let text = concat!(
        "behavior login \"Log in\" {\n",
        "  contract \"\"\"\n",
        "    Logs in.\n",
        "}\n",
        "\n",
        "behavior logout \"Log out\" {\n",
        "  contract \"Logs out.\"\n",
        "}\n",
    );
    // The parser keeps the block after the string...
    let parsed = specforge_parser::parse_incremental(text, "test.spec", None).0;
    let ids: Vec<&str> = parsed.entities.iter().map(|e| e.id.raw.as_str()).collect();
    assert_eq!(ids, ["login", "logout"]);
    // ...and the editor reads it as a block too, not as the string's text.
    let tokens = tokens_of(text, kinds(&[("behavior", None)]));
    for line in ["\"\"\"", "    Logs in.", "}"] {
        assert_eq!(token(&tokens, line).token_type, "string", "{line:?}");
    }
    let logout = token(&tokens, "logout");
    assert_eq!(logout.token_type, "function");
    assert_ne!(logout.modifiers & MOD_DECLARATION, 0);
    assert_eq!(token(&tokens, "\"Logs out.\"").token_type, "string");
    let keywords: Vec<&str> = tokens
        .iter()
        .filter(|t| t.text == "behavior")
        .map(|t| t.token_type)
        .collect();
    assert_eq!(keywords, ["type", "type"]);
}

#[spec(
    behavior = "provide_semantic_tokens",
    verify = "a union and an inline ref after an unclosed body are classified as top-level forms"
)]
fn a_union_and_an_inline_ref_after_an_unclosed_body_are_classified_as_forms() {
    let tokens = tokens_of(
        concat!(
            "behavior login \"Log in\" {\n",
            "  contract \"Logs in.\"\n",
            "\n",
            "type status = \"active\" | \"inactive\"\n",
            "\n",
            "ref github.issue:org/repo#1 \"Tracking issue\"\n",
        ),
        kinds(&[("behavior", None), ("type", None)]),
    );
    assert_eq!(token(&tokens, "type").token_type, "type");
    assert_eq!(token(&tokens, "ref").token_type, "type");
    let status = token(&tokens, "status");
    assert_eq!(status.token_type, "function");
    assert_ne!(status.modifiers & MOD_DECLARATION, 0);
}

#[spec(
    behavior = "provide_semantic_tokens",
    verify = "entity keywords classified as 'type'"
)]
fn entity_keywords_classified_as_type() {
    // A kind's semantic_token classifies its IDs, never its keyword.
    let tokens = tokens_of(
        "port repo \"Repo\" {\n}\n",
        kinds(&[("port", Some("interface"))]),
    );
    assert_eq!(token(&tokens, "port").token_type, "type");
}

#[spec(
    behavior = "provide_semantic_tokens",
    verify = "entity ID declaration uses its kind's semantic_token from the KindRegistry"
)]
fn entity_id_declaration_uses_kind_semantic_token() {
    let tokens = tokens_of(
        "port repo \"Repo\" {\n}\ninvariant always \"Always\" {\n}\n",
        kinds(&[("port", Some("interface")), ("invariant", Some("property"))]),
    );
    assert_eq!(token(&tokens, "repo").token_type, "interface");
    assert_eq!(token(&tokens, "always").token_type, "property");
}

#[spec(
    behavior = "provide_semantic_tokens",
    verify = "entity ID declaration without a declared semantic_token is 'function'"
)]
fn entity_id_declaration_without_semantic_token_is_function() {
    let tokens = tokens_of("gizmo thing \"Thing\" {\n}\n", kinds(&[("gizmo", None)]));
    assert_eq!(token(&tokens, "thing").token_type, "function");
}

#[spec(
    behavior = "provide_semantic_tokens",
    verify = "entity ID declaration whose semantic_token is not in the legend is 'function'"
)]
fn entity_id_declaration_with_unknown_semantic_token_is_function() {
    // "constant" is no standard LSP token type, so no client could color it.
    let tokens = tokens_of(
        "axiom excluded_middle \"Excluded Middle\" {\n}\n",
        kinds(&[("axiom", Some("constant"))]),
    );
    assert!(!specforge_lsp::TOKEN_TYPES.contains(&"constant"));
    assert_eq!(token(&tokens, "excluded_middle").token_type, "function");
}

#[spec(
    behavior = "provide_semantic_tokens",
    verify = "semantic token legend lists every standard LSP token type"
)]
fn legend_lists_every_standard_token_type() {
    let standard = [
        "namespace",
        "type",
        "class",
        "enum",
        "interface",
        "struct",
        "typeParameter",
        "parameter",
        "variable",
        "property",
        "enumMember",
        "event",
        "function",
        "method",
        "macro",
        "keyword",
        "modifier",
        "comment",
        "string",
        "number",
        "regexp",
        "operator",
        "decorator",
    ];
    let mut legend = specforge_lsp::TOKEN_TYPES.to_vec();
    legend.sort_unstable();
    let mut expected = standard.to_vec();
    expected.sort_unstable();
    assert_eq!(legend, expected);
}

// -- provide_extension_entity_semantic_tokens ---------------------------------

#[spec(
    behavior = "provide_extension_entity_semantic_tokens",
    verify = "extension kind's semantic_token classifies its entity ID declaration"
)]
fn extension_kind_semantic_token_classifies_declaration() {
    let tokens = tokens_of(
        "feature checkout \"Checkout\" {\n}\n",
        kinds(&[("feature", Some("class"))]),
    );
    let id = token(&tokens, "checkout");
    assert_eq!(id.token_type, "class");
    assert_ne!(id.modifiers & MOD_DECLARATION, 0);
}

#[spec(
    behavior = "provide_extension_entity_semantic_tokens",
    verify = "entity ID declaration falls back to 'function' when semantic_token is not specified"
)]
fn extension_kind_without_semantic_token_falls_back_to_function() {
    let tokens = tokens_of("widget knob \"Knob\" {\n}\n", kinds(&[("widget", None)]));
    assert_eq!(token(&tokens, "knob").token_type, "function");
}

#[spec(
    behavior = "provide_extension_entity_semantic_tokens",
    verify = "semantic_token outside the static legend falls back to 'function'"
)]
fn extension_semantic_token_outside_legend_falls_back_to_function() {
    let tokens = tokens_of(
        "release v1 \"V1\" {\n}\n",
        kinds(&[("release", Some("constant"))]),
    );
    assert_eq!(token(&tokens, "v1").token_type, "function");
}

#[spec(
    behavior = "provide_semantic_tokens",
    verify = "structural keywords are classified as keyword"
)]
fn structural_keywords_classified() {
    let tokens = tokens_of("use \"behaviors/core\"\n", RegistryBuild::default());
    assert!(
        tokens
            .iter()
            .any(|t| t.text == "use" && t.token_type == "keyword")
    );
}

#[spec(
    behavior = "provide_semantic_tokens",
    verify = "triple-quoted strings are classified as strings"
)]
fn triple_quoted_strings_classified() {
    let tokens = tokens_of(
        "behavior foo \"Foo\" {\n  contract \"\"\"\n    hello\n  \"\"\"\n}\nbehavior bar \"Bar\" {\n}\n",
        kinds(&[("behavior", None)]),
    );
    let on_lines = |lines: std::ops::RangeInclusive<u32>| {
        tokens
            .iter()
            .filter(|t| lines.contains(&t.line))
            .map(|t| (t.line, t.col, t.text.as_str(), t.token_type))
            .collect::<Vec<_>>()
    };
    // The opening quotes, the body and the closing quotes are strings.
    assert_eq!(
        on_lines(1..=3),
        [
            (1, 2, "contract", "property"),
            (1, 11, "\"\"\"", "string"),
            (2, 0, "    hello", "string"),
            (3, 0, "  \"\"\"", "string"),
        ]
    );
    // The string ends at its closing quotes: what follows is not a string.
    assert_eq!(token(&tokens, "bar").token_type, "function");
    assert!(
        tokens
            .iter()
            .filter(|t| t.line > 3)
            .all(|t| t.token_type != "string" || t.text == "\"Bar\""),
        "{tokens:?}"
    );
}

#[spec(
    behavior = "provide_semantic_tokens",
    verify = "a string spanning lines is one string, holding no other token"
)]
fn a_multi_line_string_is_one_string_token_per_line() {
    let tokens = tokens_of(
        "behavior login \"Log in\" {\n  contract \"first line\n  second line mentions login and ends\"\n}\n\nbehavior logout \"Log out\" {\n  contract \"x\"\n}\n",
        kinds(&[]),
    );
    assert!(
        tokens.iter().all(|t| t.text != "mentions"),
        "a word inside the string is a token: {tokens:?}"
    );
    let second = token(&tokens, "  second line mentions login and ends\"");
    assert_eq!(
        (second.line, second.col, second.token_type),
        (2, 0, "string")
    );
    let first = token(&tokens, "\"first line");
    assert_eq!((first.line, first.col, first.token_type), (1, 11, "string"));
}

#[spec(
    behavior = "provide_semantic_tokens",
    verify = "entity ID declarations carry the declaration modifier"
)]
fn entity_id_declarations_carry_declaration_modifier() {
    let tokens = tokens_of(
        "behavior foo \"Foo\" {\n}\nport repo \"Repo\" {\n}\n",
        kinds(&[("behavior", None), ("port", Some("interface"))]),
    );
    for id in ["foo", "repo"] {
        assert_ne!(
            token(&tokens, id).modifiers & MOD_DECLARATION,
            0,
            "{id} should have the declaration modifier"
        );
    }
}

#[spec(
    behavior = "provide_semantic_tokens",
    verify = "enhanced fields are classified as property"
)]
fn fields_classified_as_property() {
    let tokens = tokens_of(
        "behavior foo \"Foo\" {\n  contract \"x\"\n}\n",
        kinds(&[("behavior", None)]),
    );
    assert!(
        tokens
            .iter()
            .any(|t| t.text == "contract" && t.token_type == "property")
    );
}

#[spec(
    behavior = "provide_semantic_tokens",
    verify = "reference list items classified as 'variable' with reference modifier"
)]
fn reference_list_items_classified_as_variable() {
    let tokens = tokens_of(
        "behavior foo \"Foo\" {\n  invariants [inv_a, inv_b]\n}\n",
        kinds(&[("behavior", None)]),
    );
    let inv_a = tokens.iter().find(|t| t.text == "inv_a").unwrap();
    assert_eq!(inv_a.token_type, "variable");
    assert_ne!(
        inv_a.modifiers & MOD_REFERENCE,
        0,
        "should have reference modifier"
    );

    let inv_b = tokens.iter().find(|t| t.text == "inv_b").unwrap();
    assert_eq!(inv_b.token_type, "variable");
}

#[spec(
    behavior = "provide_semantic_tokens",
    verify = "structural keywords are classified as keyword"
)]
fn verify_keyword_classified() {
    let tokens = tokens_of(
        "behavior foo \"Foo\" {\n  verify unit \"it works\"\n}\n",
        kinds(&[("behavior", None)]),
    );
    assert!(
        tokens
            .iter()
            .any(|t| t.text == "verify" && t.token_type == "keyword")
    );
}

#[spec(
    behavior = "provide_semantic_tokens",
    verify = "verify kind classified as enumMember"
)]
fn verify_kind_classified_as_enum_member() {
    let tokens = tokens_of(
        "behavior foo \"Foo\" {\n  verify unit \"it works\"\n}\n",
        kinds(&[("behavior", None)]),
    );
    let kind = tokens.iter().find(|t| t.text == "unit").unwrap();
    assert_eq!(kind.token_type, "enumMember");
}

#[spec(
    behavior = "provide_semantic_tokens",
    verify = "comments classified as comment"
)]
fn comments_classified() {
    let tokens = tokens_of("// this is a comment\n", RegistryBuild::default());
    assert!(tokens.iter().any(|t| t.token_type == "comment"));
}

#[spec(
    behavior = "provide_semantic_tokens",
    verify = "structural keywords are classified as keyword"
)]
fn define_block_classified() {
    let tokens = tokens_of("define MyType {\n}\n", RegistryBuild::default());
    assert!(
        tokens
            .iter()
            .any(|t| t.text == "define" && t.token_type == "keyword")
    );
}

#[spec(
    behavior = "provide_semantic_tokens",
    verify = "a define block's name is not a declaration"
)]
fn a_define_blocks_name_is_not_a_declaration() {
    // W143: a define block registers nothing, so its name declares nothing
    // and its body holds no fields.
    let tokens = tokens_of(
        "define MyType {\n  name string\n}\n",
        kinds(&[("behavior", None)]),
    );
    let texts: Vec<(&str, &str)> = tokens
        .iter()
        .map(|t| (t.text.as_str(), t.token_type))
        .collect();
    assert_eq!(texts, [("define", "keyword")]);
}

#[spec(
    behavior = "provide_semantic_tokens",
    verify = "reference list items classified as 'variable' with reference modifier"
)]
fn multiline_reference_list() {
    let tokens = tokens_of(
        "behavior foo \"Foo\" {\n  types [\n    type_a,\n    type_b\n  ]\n}\n",
        kinds(&[("behavior", None)]),
    );
    let type_a = tokens.iter().find(|t| t.text == "type_a").unwrap();
    assert_eq!(type_a.token_type, "variable");
    assert_ne!(type_a.modifiers & MOD_REFERENCE, 0);

    let type_b = tokens.iter().find(|t| t.text == "type_b").unwrap();
    assert_eq!(type_b.token_type, "variable");
}

#[spec(
    behavior = "provide_semantic_tokens",
    verify = "enhanced fields are classified as property"
)]
fn field_name_before_list_classified() {
    let tokens = tokens_of(
        "behavior foo \"Foo\" {\n  invariants [inv_a]\n}\n",
        kinds(&[("behavior", None)]),
    );
    assert!(
        tokens
            .iter()
            .any(|t| t.text == "invariants" && t.token_type == "property")
    );
}

#[spec(
    behavior = "provide_semantic_tokens",
    verify = "number values classified as number"
)]
fn number_values_classified() {
    let tokens = tokens_of(
        "behavior foo \"Foo\" {\n  risk 5\n}\n",
        kinds(&[("behavior", None)]),
    );
    let num = tokens.iter().find(|t| t.text == "5").unwrap();
    assert_eq!(num.token_type, "number");
}

#[spec(
    behavior = "provide_semantic_tokens",
    verify = "entity title strings classified as string"
)]
fn entity_title_classified_as_string() {
    let tokens = tokens_of(
        "behavior foo \"My Title\" {\n}\n",
        kinds(&[("behavior", None)]),
    );
    assert!(
        tokens
            .iter()
            .any(|t| t.text.contains("My Title") && t.token_type == "string")
    );
}

#[spec(
    behavior = "provide_semantic_tokens",
    verify = "use path classified as string"
)]
fn use_path_classified_as_string() {
    let tokens = tokens_of("use \"core/types\"\n", RegistryBuild::default());
    assert!(
        tokens
            .iter()
            .any(|t| t.text.contains("core/types") && t.token_type == "string")
    );
}

#[spec(
    behavior = "lsp_initialize",
    verify = "initialize response includes semantic token legend"
)]
fn token_types_constant_complete() {
    let types = specforge_lsp::TOKEN_TYPES;
    assert!(types.contains(&"keyword"));
    assert!(types.contains(&"type"));
    assert!(types.contains(&"function"));
    assert!(types.contains(&"variable"));
    assert!(types.contains(&"property"));
    assert!(types.contains(&"string"));
    assert!(types.contains(&"comment"));
    assert!(types.contains(&"number"));
    assert!(types.contains(&"enumMember"));
}

#[spec(
    behavior = "lsp_initialize",
    verify = "initialize response includes semantic token legend"
)]
fn token_modifiers_constant_complete() {
    let mods = specforge_lsp::TOKEN_MODIFIERS;
    assert!(mods.contains(&"declaration"));
    assert!(mods.contains(&"reference"));
}

/// An entity body with a nested block before two more fields, the last
/// one followed by a comment.
const AFTER_A_NESTED_BLOCK: &str = concat!(
    "behavior gamma \"Gamma\" {\n",
    "  requires {\n",
    "    ready \"it is ready\"\n",
    "  }\n",
    "  contract \"after the block\"\n",
    "  features [] // trailing comment\n",
    "}\n",
);

#[spec(
    behavior = "provide_semantic_tokens",
    verify = "every field of an entity body is classified, after a nested block too"
)]
fn fields_after_a_nested_block_are_classified() {
    let tokens = tokens_of(AFTER_A_NESTED_BLOCK, kinds(&[("behavior", None)]));
    let on = |line: u32| {
        tokens
            .iter()
            .filter(|t| t.line == line)
            .map(|t| (t.col, t.text.as_str(), t.token_type))
            .collect::<Vec<_>>()
    };
    assert_eq!(
        on(4),
        [
            (2, "contract", "property"),
            (11, "\"after the block\"", "string")
        ]
    );
    assert_eq!(on(5)[0], (2, "features", "property"));
}

#[spec(
    behavior = "provide_semantic_tokens",
    verify = "a comment after code on its line is classified as comment"
)]
fn a_trailing_comment_is_a_comment() {
    let tokens = tokens_of(AFTER_A_NESTED_BLOCK, kinds(&[("behavior", None)]));
    let comment = tokens
        .iter()
        .find(|t| t.line == 5 && t.token_type == "comment")
        .unwrap_or_else(|| panic!("{tokens:?}"));
    assert_eq!(
        (comment.col, comment.text.as_str()),
        (14, "// trailing comment")
    );
}

/// A node of `kind` named `id`.
fn node(id: &str, kind: &str) -> Node {
    Node {
        id: EntityId { raw: Sym::new(id) },
        kind: EntityKind {
            raw: Sym::new(kind),
        },
        title: None,
        fields: FieldMap::new(),
        source_span: SourceSpan {
            file: Sym::new("other.spec"),
            start_line: 1,
            start_col: 1,
            end_line: 1,
            end_col: 1,
        },
        methods: Vec::new(),
    }
}

#[spec(
    behavior = "provide_semantic_tokens",
    verify = "a reference is classified as the kind of the entity it names"
)]
fn a_reference_takes_its_targets_kind_token() {
    let registries = registries("@test/ext", |c| {
        c.kind("feature", |k| {
            k.testable(true)
                .supports_verify(true)
                .semantic_token("class");
        });
        c.kind("behavior", |k| {
            k.testable(true).supports_verify(true);
            k.field("features", |f| {
                f.field_type(FieldType::ReferenceList);
            });
            k.field("extends", |f| {
                f.field_type(FieldType::Reference);
            });
        });
    });
    let mut graph = Graph::new();
    graph.add_node(node("signin", "feature"));
    let tokens = tokens_over(
        "behavior b \"B\" {\n  features [signin, ghost]\n  extends signin\n}\n",
        registries,
        &graph,
    );
    let at = |line: u32, text: &str| {
        let t = tokens
            .iter()
            .find(|t| t.line == line && t.text == text)
            .unwrap_or_else(|| panic!("no {text} on {line}: {tokens:?}"));
        (t.token_type, t.modifiers)
    };
    assert_eq!(at(1, "signin"), ("class", MOD_REFERENCE));
    assert_eq!(at(1, "ghost"), ("variable", MOD_REFERENCE));
    assert_eq!(
        at(2, "signin"),
        ("class", MOD_REFERENCE),
        "a single reference too"
    );
}

#[spec(
    behavior = "provide_semantic_tokens",
    verify = "an enum field's value is an enumMember and a boolean field's value a keyword"
)]
fn enum_and_bool_values_are_classified() {
    let registries = registries("@test/ext", |c| {
        c.kind("task", |k| {
            k.testable(true).supports_verify(true);
            k.field("state", |f| {
                f.field_type(FieldType::Enum)
                    .enum_values(&["draft", "done"]);
            });
            k.field("done", |f| {
                f.field_type(FieldType::Bool);
            });
            k.field("kind", |f| {
                f.field_type(FieldType::String);
            });
        });
    });
    let mut graph = Graph::new();
    // A value that spells an entity's ID names nothing in an enum field.
    graph.add_node(node("draft", "task"));
    let tokens = tokens_over(
        "task t \"T\" {\n  state draft\n  done true\n  kind struct\n}\n",
        registries,
        &graph,
    );
    let of = |text: &str| tokens.iter().find(|t| t.text == text).map(|t| t.token_type);
    assert_eq!(of("draft"), Some("enumMember"));
    assert_eq!(of("true"), Some("keyword"));
    assert_eq!(
        of("struct"),
        None,
        "other identifier values stay unclassified"
    );
}

/// Whether `node` has an ancestor of kind `kind`.
fn within(node: tree_sitter::Node, kinds: &[&str]) -> bool {
    let mut parent = node.parent();
    while let Some(p) = parent {
        if kinds.contains(&p.kind()) {
            return true;
        }
        parent = p.parent();
    }
    false
}

/// Every node of `tree` under `node`, depth first.
fn walk<'t>(node: tree_sitter::Node<'t>, out: &mut Vec<tree_sitter::Node<'t>>) {
    out.push(node);
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        walk(child, out);
    }
}

#[spec(
    behavior = "provide_semantic_tokens",
    verify = "classification agrees with the grammar on every spec file of the repository"
)]
fn tokens_agree_with_the_grammar() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let runtime = specforge_component::ComponentRuntime::with_user_cache();
    let project = specforge_project::CompiledProject::compile(
        &specforge_common::ProjectRoot::of(&root).unwrap(),
        Some(Arc::new(runtime)),
    );
    let view = ProjectView::of(&project);
    let declared: Vec<&str> = view
        .registries()
        .kinds()
        .iter()
        .filter_map(|(_, entry)| entry.declared.semantic_token.as_deref())
        .collect();
    let mut files = Vec::new();
    crate::spec_files(&root.join("spec"), &mut files);
    assert!(files.len() > 150, "found only {} spec files", files.len());
    for path in &files {
        let text = std::fs::read_to_string(path).unwrap();
        let doc = Document::new("file:///spec.spec".into(), text.clone());
        let index = doc.index();
        let tokens = doc.tokens(&view);
        // Each token as (start byte, end byte) -> (type, modifiers).
        let mut spans = std::collections::HashMap::new();
        let mut previous_end = 0;
        for t in &tokens {
            let at = |character| {
                index
                    .offset(tower_lsp::lsp_types::Position::new(t.line, character))
                    .unwrap_or_else(|| panic!("{}: {t:?} leaves its line", path.display()))
            };
            let (start, end) = (at(t.col), at(t.col + t.length));
            assert!(start >= previous_end, "{}: {t:?} overlaps", path.display());
            previous_end = end;
            spans.insert((start, end), (t.token_type, t.modifiers));
        }
        let (_, tree) = specforge_parser::parse_incremental(&text, "spec.spec", None);
        let tree = tree.unwrap();
        let mut nodes = Vec::new();
        walk(tree.root_node(), &mut nodes);
        let token_at = |node: tree_sitter::Node| spans.get(&(node.start_byte(), node.end_byte()));
        let where_ = |node: tree_sitter::Node| {
            format!(
                "{}:{} {:?}",
                path.display(),
                node.start_position().row + 1,
                &text[node.byte_range()]
            )
        };
        for node in nodes {
            if within(node, &[kind::DEFINE_BLOCK]) {
                continue;
            }
            match node.kind() {
                kind::ENTITY_BLOCK => {
                    let kind = node.child_by_field_name(field::KIND).unwrap();
                    assert_eq!(
                        token_at(kind).map(|t| t.0),
                        Some("type"),
                        "{}",
                        where_(kind)
                    );
                    let name = node.child_by_field_name(field::NAME).unwrap();
                    let token = token_at(name).unwrap_or_else(|| panic!("{}", where_(name)));
                    assert_ne!(token.1 & MOD_DECLARATION, 0, "{}", where_(name));
                }
                kind::FIELD => {
                    let key = node.child_by_field_name(field::KEY).unwrap();
                    assert_eq!(
                        token_at(key).map(|t| t.0),
                        Some("property"),
                        "{}",
                        where_(key)
                    );
                }
                kind::STRING | kind::TRIPLE_QUOTED_STRING | kind::COMMENT => {
                    let expected = if node.kind() == kind::COMMENT {
                        "comment"
                    } else {
                        "string"
                    };
                    // Each line of it is one token.
                    let (mut from, end) = (node.start_byte(), node.end_byte());
                    while from < end {
                        let line_end = text[from..end].find('\n').map_or(end, |at| from + at);
                        if from < line_end {
                            assert_eq!(
                                spans.get(&(from, line_end)).map(|t| t.0),
                                Some(expected),
                                "{}",
                                where_(node)
                            );
                        }
                        from = line_end + 1;
                    }
                }
                kind::IDENTIFIER | kind::SCHEME_REF_ID
                    if node.parent().is_some_and(|p| p.kind() == kind::LIST)
                        && !within(node, &[kind::NESTED_BLOCK]) =>
                {
                    let token = token_at(node).unwrap_or_else(|| panic!("{}", where_(node)));
                    assert_ne!(token.1 & MOD_REFERENCE, 0, "{}", where_(node));
                    assert!(
                        token.0 == "variable"
                            || token.0 == "function"
                            || declared.contains(&token.0),
                        "{}: {token:?}",
                        where_(node)
                    );
                }
                _ => {}
            }
        }
    }
}
