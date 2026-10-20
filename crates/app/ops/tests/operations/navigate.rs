//! Navigation (`specforge_ops::read::navigate`) over compiled projects: every
//! position comes from the parser, as the LSP and MCP see it.

use specforge_common::SourceSpan;
use specforge_ops::read::navigate::{
    Direction, Navigator, Occurrence, Precision, ReferenceQuery, Role,
};
use specforge_ops::view::ProjectView;
use specforge_project::CompiledProject;
use specforge_project::testing::compile_at;
use specforge_test::prelude::*;
use tempfile::TempDir;

/// A project of `files` with `extensions`, compiled with its extensions
/// loaded.
pub struct Compiled {
    _dir: TempDir,
    pub project: CompiledProject,
}

pub fn compile(extensions: &[&str], files: &[(&str, &str)]) -> Compiled {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("specforge.json"),
        serde_json::json!({"name": "nav", "extensions": extensions}).to_string(),
    )
    .unwrap();
    for (name, text) in files {
        let path = dir.path().join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
    let runtime = specforge_project::testing::component_runtime();
    let project = compile_at(dir.path(), Some(runtime));
    let unloaded: Vec<_> = project
        .diagnostics()
        .into_iter()
        .filter(|d| d.code == "I002")
        .collect();
    assert!(
        unloaded.is_empty(),
        "the extensions did not load: {unloaded:?}"
    );
    Compiled { _dir: dir, project }
}

impl Compiled {
    /// A navigator over the project, reading the text it was compiled from.
    pub fn navigator(&self) -> Navigator<'_> {
        Navigator::new(ProjectView::of(&self.project))
    }
}

/// `"file L:C-L:C"`.
pub fn at(span: &SourceSpan) -> String {
    format!(
        "{} {}:{}-{}:{}",
        span.file, span.start_line, span.start_col, span.end_line, span.end_col
    )
}

/// Each occurrence as `"file L:C-L:C holder->target field"`.
pub fn keys(occurrences: &[Occurrence]) -> Vec<String> {
    occurrences
        .iter()
        .map(|o| {
            format!(
                "{} {}->{} {}",
                at(&o.span),
                o.holder,
                o.target,
                o.field.map_or("-".to_string(), |f| f.to_string())
            )
        })
        .collect()
}

pub const SOFTWARE: &[&str] = &["@specforge/software"];

pub const LIMIT: &str = "invariant session_limit \"Limit\" {\n  guarantee \"x\"\n}\n";
pub const LOGIN: &str = "behavior login \"Login\" {\n  invariants [session_limit]\n}\n\n\
                         behavior logout \"Logout\" {\n  invariants [sesion_limit]\n}\n";

pub fn nav() -> Compiled {
    compile(SOFTWARE, &[("limit.spec", LIMIT), ("login.spec", LOGIN)])
}

const INCOMING: ReferenceQuery = ReferenceQuery {
    direction: Direction::Incoming,
    include_declaration: false,
};

const WITH_DECLARATION: ReferenceQuery = ReferenceQuery {
    direction: Direction::Incoming,
    include_declaration: true,
};

#[specforge_test(
    behavior = "find_all_references",
    verify = "find-refs returns all reference sites"
)]
fn references_are_every_field_that_names_the_entity() {
    let p = compile(
        SOFTWARE,
        &[
            ("limit.spec", LIMIT),
            (
                "login.spec",
                "behavior login \"Login\" {\n  invariants [session_limit]\n}\n\
                 behavior audit \"Audit\" {\n  invariants [session_limit]\n}\n",
            ),
        ],
    );
    let refs = p.navigator().references("session_limit", INCOMING).unwrap();
    assert_eq!(
        keys(&refs),
        [
            "login.spec 2:15-2:28 login->session_limit invariants",
            "login.spec 5:15-5:28 audit->session_limit invariants",
        ]
    );
    assert!(refs.iter().all(|o| o.role == Role::Reference));
}

#[specforge_test(
    behavior = "find_all_references",
    verify = "find-refs includes the declaration site"
)]
fn the_declaration_is_included_on_request() {
    let p = nav();
    let refs = p
        .navigator()
        .references("session_limit", WITH_DECLARATION)
        .unwrap();
    assert_eq!(
        keys(&refs),
        [
            "limit.spec 1:11-1:24 session_limit->session_limit -",
            "login.spec 2:15-2:28 login->session_limit invariants",
        ]
    );
    assert_eq!(refs[0].role, Role::Declaration);
}

#[specforge_test(
    behavior = "find_all_references",
    verify = "find-refs omits the declaration when the request excludes it"
)]
fn the_declaration_is_left_out_otherwise() {
    let p = nav();
    let refs = p.navigator().references("session_limit", INCOMING).unwrap();
    assert_eq!(
        keys(&refs),
        ["login.spec 2:15-2:28 login->session_limit invariants"]
    );
}

#[specforge_test(
    behavior = "find_all_references",
    verify = "find-refs across multiple files"
)]
fn references_come_from_every_file() {
    let p = compile(
        SOFTWARE,
        &[
            ("limit.spec", LIMIT),
            (
                "a.spec",
                "behavior a_one \"A\" {\n  invariants [session_limit]\n}\n",
            ),
            (
                "b.spec",
                "behavior b_one \"B\" {\n  invariants [session_limit]\n}\n",
            ),
        ],
    );
    let refs = p
        .navigator()
        .references("session_limit", WITH_DECLARATION)
        .unwrap();
    let files: Vec<&str> = refs.iter().map(|o| o.span.file.as_str()).collect();
    assert_eq!(files, ["a.spec", "b.spec", "limit.spec"]);
}

#[specforge_test(
    behavior = "find_all_references",
    verify = "find-refs excludes what the entity itself references"
)]
fn what_an_entity_refers_to_is_not_a_reference_to_it() {
    let p = nav();
    let nav = p.navigator();
    assert_eq!(nav.references("login", INCOMING).unwrap(), []);
    let outgoing = nav
        .references(
            "login",
            ReferenceQuery {
                direction: Direction::Outgoing,
                include_declaration: false,
            },
        )
        .unwrap();
    assert_eq!(
        keys(&outgoing),
        ["login.spec 2:15-2:28 login->session_limit invariants"]
    );
    let both = nav
        .references(
            "login",
            ReferenceQuery {
                direction: Direction::Both,
                include_declaration: true,
            },
        )
        .unwrap();
    assert_eq!(
        keys(&both),
        [
            "login.spec 1:10-1:15 login->login -",
            "login.spec 2:15-2:28 login->session_limit invariants",
        ]
    );
}

#[specforge_test(
    behavior = "find_all_references",
    verify = "each reference is the identifier token as written"
)]
fn a_reference_is_its_token() {
    let p = compile(
        SOFTWARE,
        &[
            ("limit.spec", LIMIT),
            (
                "login.spec",
                "behavior login \"session_limit\" {\n  invariants [  session_limit ]\n}\n",
            ),
        ],
    );
    let refs = p.navigator().references("session_limit", INCOMING).unwrap();
    assert_eq!(
        keys(&refs),
        ["login.spec 2:17-2:30 login->session_limit invariants"]
    );
    assert_eq!(refs[0].precision, Precision::Token);
}

#[specforge_test(
    behavior = "go_to_definition",
    verify = "go-to-def navigates to entity declaration"
)]
fn the_definition_is_the_declaration() {
    let p = nav();
    let definition = p.navigator().definition("session_limit").unwrap();
    assert_eq!(at(&definition.block), "limit.spec 1:1-3:2");
    assert_eq!(definition.kind, "invariant");
}

#[specforge_test(
    behavior = "go_to_definition",
    verify = "go-to-def on non-existent ID returns no result"
)]
fn an_unknown_id_has_no_definition() {
    let p = nav();
    let error = p.navigator().definition("nope").unwrap_err();
    assert!(error.error.is(specforge_common::codes::E003));
    assert_eq!(error.error.kind, specforge_ops::OpErrorKind::EntityNotFound);
    // A misspelled reference is not an entity either.
    assert!(p.navigator().definition("sesion_limit").is_err());
}

#[specforge_test(behavior = "go_to_definition", verify = "go-to-def works across files")]
fn a_reference_leads_to_its_definition_in_another_file() {
    let p = nav();
    let nav = p.navigator();
    let reference = nav.occurrence_at("login.spec", 2, 20).unwrap();
    assert_eq!(reference.target, "session_limit");
    let definition = nav.definition(reference.target.as_str()).unwrap();
    assert_eq!(at(&definition.name), "limit.spec 1:11-1:24");
}

#[specforge_test(
    behavior = "go_to_definition",
    verify = "the definition's selection is the entity's name token"
)]
fn the_definition_selects_the_name() {
    let p = compile(
        SOFTWARE,
        &[(
            "limit.spec",
            "// a comment naming session_limit\ninvariant   session_limit \"session_limit\" {\n  guarantee \"x\"\n}\n",
        )],
    );
    let definition = p.navigator().definition("session_limit").unwrap();
    assert_eq!(at(&definition.name), "limit.spec 2:13-2:26");
    assert_eq!(definition.precision, Precision::Token);
}

#[test]
fn derived_references_are_the_names_in_types_and_signatures() {
    let p = compile(
        SOFTWARE,
        &[(
            "store.spec",
            "type Status = open | done\n\
             type Task \"T\" {\n  status Status\n  title string\n}\n\
             port store \"Store\" {\n  direction outbound\n  method save(task: Task) -> Status\n}\n",
        )],
    );
    let nav = p.navigator();
    assert_eq!(
        keys(&nav.references("Status", INCOMING).unwrap()),
        [
            "store.spec 3:10-3:16 Task->Status composed_types",
            "store.spec 8:30-8:36 store->Status types",
        ]
    );
    assert_eq!(
        keys(&nav.references("Task", INCOMING).unwrap()),
        ["store.spec 8:21-8:25 store->Task types"]
    );
}

#[test]
fn strings_comments_and_verify_texts_are_not_occurrences() {
    let p = compile(
        &["@specforge/software", "@specforge/testing"],
        &[(
            "a.spec",
            "invariant session_limit \"session_limit cap\" {\n  guarantee \"session_limit is never exceeded\"\n}\n\
             behavior login \"Login\" {\n  invariants [session_limit]\n  // keeps session_limit\n  verify unit \"login respects session_limit\"\n}\n",
        )],
    );
    let refs = p
        .navigator()
        .references("session_limit", WITH_DECLARATION)
        .unwrap();
    assert_eq!(
        keys(&refs),
        [
            "a.spec 1:11-1:24 session_limit->session_limit -",
            "a.spec 5:15-5:28 login->session_limit invariants",
        ]
    );
}

/// A ref and a behavior listing it (plan 06's R1 fixture).
const REFS: &str = concat!(
    "ref gh.issue:42 \"Support Wasm\"\n",
    "\n",
    "behavior issue \"Issue tracking\" {\n",
    "  contract \"tracks issues\"\n",
    "}\n",
    "\n",
    "behavior login \"Login\" {\n",
    "  contract \"see [docs\"\n",
    "  refs [gh.issue:42]\n",
    "}\n",
);

#[specforge_test(
    behavior = "go_to_definition",
    verify = "the definition's selection is the entity's name token"
)]
fn a_refs_definition_selects_its_scheme_id() {
    // A scheme ref ID is one token (the grammar's `scheme_ref_id`).
    let p = compile(SOFTWARE, &[("main.spec", REFS)]);
    let definition = p.navigator().definition("gh.issue:42").unwrap();
    assert_eq!(definition.precision, Precision::Token);
    assert_eq!(at(&definition.name), "main.spec 1:5-1:16");
}

#[test]
fn a_stale_text_gives_entity_precision() {
    let p = nav();
    // The file changed since the compile: the token is not where the
    // graph says.
    let shifted = p.project.with_source_texts([
        ("login.spec", format!("\n{LOGIN}")),
        ("limit.spec", LIMIT.to_string()),
    ]);
    let nav = Navigator::new(ProjectView::of(&shifted));
    let refs = nav.references("session_limit", INCOMING).unwrap();
    assert_eq!(refs.len(), 1);
    assert_eq!(refs[0].precision, Precision::Entity);
    assert_eq!(
        at(&refs[0].span),
        "login.spec 1:1-3:2",
        "the holder's block"
    );

    // Unreadable text: the same fallback, the declaration included.
    let none = p.project.with_source_texts(Vec::<(String, String)>::new());
    let unreadable = Navigator::new(ProjectView::of(&none));
    let refs = unreadable
        .references("session_limit", WITH_DECLARATION)
        .unwrap();
    assert!(refs.iter().all(|o| o.precision == Precision::Entity));
    assert_eq!(
        unreadable.definition("session_limit").unwrap().precision,
        Precision::Entity
    );
}

#[test]
fn the_occurrence_at_a_position_is_the_token_there() {
    let p = nav();
    let nav = p.navigator();
    let declaration = nav.occurrence_at("limit.spec", 1, 11).unwrap();
    assert_eq!(at(&declaration.span), "limit.spec 1:11-1:24");
    // Just past the token's end still names it.
    assert_eq!(
        nav.occurrence_at("limit.spec", 1, 24).unwrap().target,
        "session_limit"
    );
    let reference = nav.occurrence_at("login.spec", 2, 15).unwrap();
    assert_eq!(reference.target, "session_limit");
    // The title, a keyword, and an unresolved reference are no occurrence.
    assert_eq!(nav.occurrence_at("limit.spec", 1, 27), None);
    assert_eq!(nav.occurrence_at("limit.spec", 1, 3), None);
    assert_eq!(nav.occurrence_at("login.spec", 6, 16), None);
}

// ── One ranking: completion, workspace symbols and search ───────────────

use specforge_ops::read::navigate::{EntityQuery, MatchScope, MatchedOn, Tier, find_entities};

const USERS: &str = "behavior user_login \"User Login\" {\n  contract \"x\"\n}\n\
                     behavior user_logout \"User Logout\" {\n  contract \"y\"\n}\n\
                     type auth_token \"Auth Token\" {\n}\n";

/// The ids `query` finds, best first.
fn found(p: &Compiled, query: &EntityQuery) -> Vec<String> {
    find_entities(p.project.graph(), query)
        .iter()
        .map(|m| m.node.id.raw.to_string())
        .collect()
}

const FIELD_TEXTS: &str = "invariant fast \"Fast\" {\n  guarantee \"responds quickly\"\n  expression expr {\n    latency < 250ms\n  }\n}\n\n\
                           behavior login \"Log in\" {\n  contract \"Given credentials, returns a token\"\n  \
                           produces [logged_in]\n  requires {\n    ready \"the account exists\"\n  }\n}\n\n\
                           event logged_in \"Logged in\" {\n}\n";

#[specforge_test(
    behavior = "select_entities_over_the_view",
    verify = "the field filter reads a field's text: a list by its items, an expression by its written form, a block by its keys; never the export's JSON"
)]
fn the_field_filter_reads_the_field_text() {
    let p = compile(
        &["@specforge/software", "@specforge/formal"],
        &[("a.spec", FIELD_TEXTS)],
    );
    let holding = |field, value| {
        found(
            &p,
            &EntityQuery {
                field_contains: Some((field, value)),
                ..EntityQuery::new("", MatchScope::NamesAndText)
            },
        )
    };

    // An expression is its written form, not the parser's AST.
    for value in ["latency", "<", "LATENCY < 250", "250ms"] {
        assert_eq!(holding("expression", value), ["fast"], "{value}");
    }
    for value in ["span", "Lt", "start_col", "\""] {
        assert!(holding("expression", value).is_empty(), "{value}");
    }
    // A list is its items, not its JSON.
    for value in ["\"", "["] {
        assert!(holding("produces", value).is_empty(), "{value}");
    }
    assert_eq!(holding("produces", "LOGGED"), ["login"]);
    // A block is its keys.
    assert_eq!(holding("requires", "ready"), ["login"]);
}

#[specforge_test(
    behavior = "workspace_symbol_search",
    verify = "search by ID prefix returns matches"
)]
fn an_id_prefix_finds_its_entities() {
    let p = compile(SOFTWARE, &[("users.spec", USERS)]);
    let names = |text| found(&p, &EntityQuery::new(text, MatchScope::Names));
    assert_eq!(names("user"), ["user_login", "user_logout"]);
    assert_eq!(names("auth_"), ["auth_token"]);
}

#[specforge_test(
    behavior = "workspace_symbol_search",
    verify = "search by title fragment returns matches"
)]
fn a_title_fragment_finds_its_entity() {
    let p = compile(SOFTWARE, &[("users.spec", USERS)]);
    // "user log" is in the titles ("User Login", "User Logout"), not the ids.
    let matches = find_entities(
        p.project.graph(),
        &EntityQuery::new("ser Log", MatchScope::Names),
    );
    let ids: Vec<&str> = matches.iter().map(|m| m.node.id.raw.as_str()).collect();
    assert_eq!(ids, ["user_login", "user_logout"]);
    assert!(
        matches
            .iter()
            .all(|m| m.on == MatchedOn::Title && m.tier == Tier::Substring)
    );
}

#[specforge_test(
    behavior = "workspace_symbol_search",
    verify = "a misspelled query within the fuzzy threshold finds the entity"
)]
fn a_misspelled_id_is_found_within_the_threshold() {
    let p = nav();
    let matches = find_entities(
        p.project.graph(),
        &EntityQuery::new("sesion", MatchScope::Names),
    );
    let ids: Vec<&str> = matches.iter().map(|m| m.node.id.raw.as_str()).collect();
    assert_eq!(
        ids,
        ["session_limit"],
        "only the close match, not every weak one"
    );
    assert_eq!(matches[0].tier, Tier::Fuzzy);
    assert!((matches[0].similarity - 0.874).abs() < 0.001);
    assert!((matches[0].score - 0.6 * matches[0].similarity).abs() < 1e-9);
    let p = compile(SOFTWARE, &[("users.spec", USERS)]);
    assert_eq!(
        found(&p, &EntityQuery::new("user_lgon", MatchScope::Names))[0],
        "user_login"
    );
}

#[specforge_test(
    behavior = "autocomplete_entity_ids",
    verify = "autocomplete suggests matching IDs"
)]
fn completion_suggests_the_matching_ids() {
    let p = compile(SOFTWARE, &[("users.spec", USERS)]);
    assert_eq!(
        found(&p, &EntityQuery::new("user", MatchScope::Names)),
        ["user_login", "user_logout"]
    );
}

#[specforge_test(
    behavior = "autocomplete_entity_ids",
    verify = "suggestions filtered by target_kind when FieldRegistry has constraint"
)]
fn completion_keeps_the_fields_target_kind() {
    let p = compile(SOFTWARE, &[("limit.spec", LIMIT), ("users.spec", USERS)]);
    // What the LSP reads for a cursor inside `invariants [`.
    let target = p
        .project
        .environment()
        .registries()
        .fields()
        .get("behavior", "invariants")
        .and_then(|f| f.declared().target_kind.clone())
        .expect("@specforge/software's invariants field targets a kind");
    let kinds = [target.as_str()];
    let query = EntityQuery {
        kinds: &kinds,
        ..EntityQuery::new("", MatchScope::Names)
    };
    assert_eq!(found(&p, &query), ["session_limit"]);
}

#[specforge_test(
    behavior = "autocomplete_entity_ids",
    verify = "all IDs suggested when no target_kind constraint exists"
)]
fn completion_without_a_target_kind_suggests_every_id() {
    let p = compile(SOFTWARE, &[("limit.spec", LIMIT), ("users.spec", USERS)]);
    assert_eq!(
        found(&p, &EntityQuery::new("", MatchScope::Names)),
        ["auth_token", "session_limit", "user_login", "user_logout"]
    );
}

#[test]
fn tiers_rank_exact_prefix_substring_field_text_then_fuzzy() {
    let p = compile(
        SOFTWARE,
        &[(
            "a.spec",
            "behavior login \"Sign in\" {\n  contract \"x\"\n}\n\
             behavior login_flow \"Flow\" {\n  contract \"x\"\n}\n\
             behavior user_login \"U\" {\n  contract \"x\"\n}\n\
             behavior audit \"Audit\" {\n  contract \"records each login\"\n}\n\
             behavior logn \"Typo\" {\n  contract \"x\"\n}\n",
        )],
    );
    let all = find_entities(
        p.project.graph(),
        &EntityQuery::new("LOGIN", MatchScope::NamesAndText),
    );
    let ranked: Vec<(&str, Tier, f64)> = all
        .iter()
        .map(|m| (m.node.id.raw.as_str(), m.tier, m.score))
        .collect();
    assert_eq!(ranked[0], ("login", Tier::Exact, 1.0));
    assert_eq!(ranked[1], ("login_flow", Tier::Prefix, 0.9));
    assert_eq!(ranked[2], ("user_login", Tier::Substring, 0.8));
    assert_eq!(ranked[3], ("audit", Tier::FieldText, 0.7));
    assert_eq!((ranked[4].0, ranked[4].1), ("logn", Tier::Fuzzy));
    assert!(ranked[4].2 < 0.6, "a fuzzy score is 0.6 × similarity");
    assert_eq!(all.len(), 5);
    assert_eq!(all[3].on, MatchedOn::Field("contract".into()));

    // Names only: the contract text matches nothing.
    let names = found(&p, &EntityQuery::new("login", MatchScope::Names));
    assert!(!names.contains(&"audit".to_string()), "{names:?}");

    // Filters apply before ranking, the limit after.
    let limited = EntityQuery {
        limit: Some(2),
        ..EntityQuery::new("login", MatchScope::NamesAndText)
    };
    assert_eq!(found(&p, &limited), ["login", "login_flow"]);
    let contract = EntityQuery {
        field_contains: Some(("contract", "RECORDS")),
        ..EntityQuery::new("", MatchScope::NamesAndText)
    };
    assert_eq!(found(&p, &contract), ["audit"]);
}

#[test]
fn the_referencing_filter_keeps_the_entities_that_reference_the_target() {
    let p = nav();
    let query = EntityQuery {
        referencing: Some("session_limit"),
        ..EntityQuery::new("", MatchScope::NamesAndText)
    };
    assert_eq!(found(&p, &query), ["login"]);
    let both = EntityQuery {
        kinds: &["invariant"],
        ..query
    };
    assert!(found(&p, &both).is_empty());
}

// ── Attribution: which entities a diagnostic is about ──────────────────

use specforge_common::{Diagnostic, DiagnosticData, Severity, Sym};
use specforge_ops::read::navigate::{is_about, subjects};

fn ids(nodes: &[&specforge_graph::Node]) -> Vec<String> {
    nodes.iter().map(|n| n.id.raw.to_string()).collect()
}

pub fn span(file: &str, (sl, sc): (usize, usize), (el, ec): (usize, usize)) -> SourceSpan {
    SourceSpan {
        file: Sym::new(file),
        start_line: sl,
        start_col: sc,
        end_line: el,
        end_col: ec,
    }
}

#[test]
fn a_diagnostic_is_about_what_its_data_names() {
    let p = compile(
        SOFTWARE,
        &[(
            "a.spec",
            "behavior alpha \"A\" {\n  contract \"x\"\n}\nbehavior beta \"B\" {\n  contract \"x\"\n}\n",
        )],
    );
    let graph = p.project.graph();
    let cycle = Diagnostic::new(specforge_common::codes::W061, "reference cycle detected")
        .with_data(DiagnosticData::ReferenceCycle {
            path: vec!["beta".into(), "alpha".into(), "beta".into()],
        });
    assert_eq!(ids(&subjects(graph, &cycle)), ["beta", "alpha"]);
    // Data wins over the span.
    let named = Diagnostic::untyped("W900", Severity::Warning, "x")
        .with_span(span("a.spec", (1, 1), (3, 2)))
        .with_data(DiagnosticData::Subject {
            entity: "beta".into(),
        });
    assert_eq!(ids(&subjects(graph, &named)), ["beta"]);
    // A name the graph lacks attributes nothing, unless the span does.
    let ghost =
        Diagnostic::untyped("W900", Severity::Warning, "x").with_data(DiagnosticData::Subject {
            entity: "ghost".into(),
        });
    assert!(subjects(graph, &ghost).is_empty());
    let ghost_inside = ghost.clone().with_span(span("a.spec", (2, 3), (2, 10)));
    assert_eq!(ids(&subjects(graph, &ghost_inside)), ["alpha"]);
}

#[test]
fn a_spanned_diagnostic_is_about_the_innermost_block_holding_it_by_column() {
    let p = compile(
        SOFTWARE,
        &[(
            "a.spec",
            "behavior alpha \"A\" { contract \"x\" } behavior beta \"B\" { contract \"y\" }\n",
        )],
    );
    let graph = p.project.graph();
    let alpha = &graph.node("alpha").unwrap().source_span;
    let beta = &graph.node("beta").unwrap().source_span;
    assert_eq!(alpha.start_line, beta.start_line, "one line, two blocks");
    let at = |col| {
        Diagnostic::untyped("W900", Severity::Warning, "x").with_span(span(
            "a.spec",
            (1, col),
            (1, col + 1),
        ))
    };
    assert_eq!(ids(&subjects(graph, &at(alpha.start_col + 2))), ["alpha"]);
    assert_eq!(ids(&subjects(graph, &at(beta.start_col + 2))), ["beta"]);
    // Between the blocks, and in another file: nobody's.
    assert!(subjects(graph, &at(alpha.end_col)).is_empty());
    let elsewhere = Diagnostic::untyped("W900", Severity::Warning, "x").with_span(span(
        "b.spec",
        (1, 1),
        (1, 2),
    ));
    assert!(subjects(graph, &elsewhere).is_empty());
}

#[test]
fn the_message_is_never_read() {
    let p = nav();
    let graph = p.project.graph();
    let quoting = Diagnostic::untyped(
        "W900",
        Severity::Warning,
        "invariant 'session_limit' is spanless",
    );
    assert!(subjects(graph, &quoting).is_empty());
    assert!(!is_about(graph, &quoting, "session_limit"));
    // E003 is about the entity holding the unresolved reference.
    let e003 = p
        .project
        .diagnostics()
        .into_iter()
        .find(|d| d.code == "E003")
        .unwrap();
    assert!(is_about(graph, &e003, "logout"));
    assert!(!is_about(graph, &e003, "login"));
}

// ── Files: the file rule and the outline ────────────────────────────────

use specforge_ops::read::navigate::{FileMatch, match_file, outline};

#[test]
fn match_file_is_component_wise() {
    // Moved from prompts/infer.rs (C9-09).
    assert_eq!(match_file("e.rs", "src/cache.rs"), FileMatch::None);
    assert_eq!(match_file("todo_list.rs", "todo_list.rs"), FileMatch::Exact);
    assert_eq!(
        match_file("todo_list.rs", "src/todo_list.rs"),
        FileMatch::SuffixPath
    );
    assert_eq!(
        match_file("src/auth", "src/auth/login.rs"),
        FileMatch::Directory
    );
    assert_eq!(match_file("src\\auth.rs", "src/auth.rs"), FileMatch::Exact);
    // `.` components are dropped: ./src/login.rs is src/login.rs.
    assert_eq!(
        match_file("./src/login.rs", "src/login.rs"),
        FileMatch::Exact
    );
    assert_eq!(
        match_file("src/./login.rs", "src/login.rs"),
        FileMatch::Exact
    );
    assert_eq!(match_file("", "src/login.rs"), FileMatch::None);
    assert_eq!(match_file("login.rs", "src/xlogin.rs"), FileMatch::None);
}

#[specforge_test(
    behavior = "outline_view",
    verify = "outline lists all entities in file"
)]
fn the_outline_lists_the_files_entities() {
    let p = compile(
        SOFTWARE,
        &[
            ("test.spec", "type b \"B\" {\n}\n\nbehavior a \"A\" {\n}\n"),
            ("other.spec", "event c \"C\" {\n}\n"),
        ],
    );
    let entries = outline(&p.navigator(), "test.spec").expect("a source");
    let ids: Vec<&str> = entries.iter().map(|e| e.id.as_str()).collect();
    assert_eq!(ids, ["b", "a"], "in line order, other files left out");
    assert_eq!(outline(&p.navigator(), "./test.spec").unwrap().len(), 2);
    assert!(
        outline(&p.navigator(), "est.spec").is_none(),
        "a file is a file"
    );
}

#[specforge_test(
    behavior = "outline_view",
    verify = "outline shows entity kind, ID, and title"
)]
fn the_outline_shows_kind_id_title_and_name() {
    let p = compile(
        SOFTWARE,
        &[(
            "store.spec",
            "type Item \"Item\" {\n  name string\n}\n\nport store \"Store\" {\n  direction outbound\n  method save(item: Item, note?: string) -> Item\n}\n",
        )],
    );
    let entries = outline(&p.navigator(), "store.spec").expect("a source");
    assert_eq!(entries.len(), 2);
    let store = &entries[1];
    assert_eq!((store.id.as_str(), store.kind.as_str()), ("store", "port"));
    assert_eq!(store.title.as_deref(), Some("Store"));
    assert_eq!(at(&store.block), "store.spec 5:1-8:2");
    assert_eq!(at(&store.name), "store.spec 5:6-5:11");
    assert_eq!(store.children.len(), 1);
    let save = &store.children[0];
    assert_eq!(save.name, "save");
    assert_eq!(save.signature, "save(item: Item, note?: string) -> Item");
    assert_eq!(at(&save.name_span), "store.spec 7:10-7:14");
}

// ── The reference list without tokens ───────────────────────────────────

use crate::view_support::{Assembled, Viewed};
use specforge_ops::read::navigate::{Reference, References};

const CYCLE: &str = "behavior alpha \"A\" {\n  depends_on [beta]\n}\n\
                     behavior beta \"B\" {\n  depends_on [alpha]\n}\n";

fn cyc() -> Compiled {
    compile(SOFTWARE, &[("a.spec", CYCLE)])
}

/// The distinct ids of the occurrences' holders (incoming) or targets
/// (outgoing), sorted.
fn ends(occurrences: &[Occurrence], direction: Direction) -> Vec<String> {
    let ids: std::collections::BTreeSet<String> = occurrences
        .iter()
        .map(|o| match direction {
            Direction::Outgoing => o.target.to_string(),
            _ => o.holder.to_string(),
        })
        .collect();
    ids.into_iter().collect()
}

#[specforge_test(
    behavior = "find_all_references",
    verify = "find-refs excludes what the entity itself references"
)]
fn references_list_the_same_entities_the_navigator_finds() {
    for p in [nav(), cyc()] {
        let navigator = p.navigator();
        let view = ProjectView::of(&p.project);
        let mut checked = 0;
        for node in p.project.graph().nodes() {
            let id = node.id.raw.as_str();
            let references = References::of(&view, id);
            for direction in [Direction::Incoming, Direction::Outgoing] {
                let query = ReferenceQuery {
                    direction,
                    include_declaration: false,
                };
                let found = ends(&navigator.references(id, query).unwrap(), direction);
                let listed = match direction {
                    Direction::Outgoing => references.refers_to(),
                    _ => references.referenced_by(),
                };
                assert_eq!(listed, found, "{id} {direction:?}");
                checked += found.len();
            }
        }
        assert!(checked > 0, "the fixture has references");
    }
}

#[test]
fn references_keep_edge_order_peer_kind_and_field() {
    for p in [nav(), cyc()] {
        let view = ProjectView::of(&p.project);
        let graph = p.project.graph();
        let kind_of = |id: &str| graph.node(id).map(|n| n.kind.raw);
        for node in graph.nodes() {
            let id = node.id.raw.as_str();
            let references = References::of(&view, id);
            let incoming: Vec<Reference> = graph
                .edges_to(id)
                .iter()
                .map(|e| Reference {
                    peer: e.source,
                    peer_kind: kind_of(e.source.as_str()),
                    field: e.label,
                })
                .collect();
            let outgoing: Vec<Reference> = graph
                .edges_from(id)
                .iter()
                .map(|e| Reference {
                    peer: e.target,
                    peer_kind: kind_of(e.target.as_str()),
                    field: e.label,
                })
                .collect();
            assert_eq!(references.incoming, incoming, "{id}");
            assert_eq!(references.outgoing, outgoing, "{id}");
        }
    }
    let cyc = cyc();
    let alpha = References::of(&ProjectView::of(&cyc.project), "alpha");
    assert_eq!(alpha.referenced_by(), ["beta"]);
    assert_eq!(alpha.refers_to(), ["beta"]);
    assert_eq!(alpha.incoming[0].peer_kind, Some(Sym::new("behavior")));
    assert_eq!(alpha.incoming[0].field, Sym::new("depends_on"));
}

#[test]
fn an_outgoing_reference_to_a_missing_entity_has_no_peer_kind() {
    let (mut graph, _) = specforge_graph::build_graph(&[specforge_parser::parse(
        "behavior a \"A\" {\n}\n",
        "a.spec",
    )]);
    graph.add_edge(specforge_graph::Edge {
        source: "a".into(),
        target: "ghost".into(),
        label: "uses".into(),
    });
    let project = Assembled::new().graph(graph);
    let view = project.rootless_view();
    let references = References::of(&view, "a");
    assert_eq!(
        references.outgoing,
        [Reference {
            peer: Sym::new("ghost"),
            peer_kind: None,
            field: Sym::new("uses"),
        }]
    );
    assert!(references.incoming.is_empty());
    assert_eq!(References::of(&view, "ghost"), References::default());
}

#[specforge_test(
    invariant = "mcp_structured_error_responses",
    verify = "an operation states the request field each refusal is about, and a refusal of the project or the environment is about none"
)]
fn a_read_view_about_one_entity_names_it() {
    use specforge_ops::view::EntityField;
    let p = nav();
    let navigator = p.navigator();
    let about = Some(EntityField::EntityId);
    assert_eq!(navigator.definition("ghost").unwrap_err().subject, about);
    let references = navigator.references("ghost", ReferenceQuery::default());
    assert_eq!(references.unwrap_err().subject, about);
    let view = ProjectView::of(&p.project);
    assert_eq!(
        view.neighbourhood("ghost", None).unwrap_err().subject,
        about
    );
    assert_eq!(
        specforge_ops::read::inspect::inspect(&view, "ghost")
            .unwrap_err()
            .subject,
        about
    );
    let explored = specforge_ops::read::explore::explore(
        &view,
        &specforge_ops::read::explore::ExplorationRequest {
            entity_id: Some("ghost"),
            ..Default::default()
        },
    );
    assert_eq!(explored.unwrap_err().subject, about);
    let reviewed = specforge_ops::read::review::review(
        &view,
        &specforge_ops::read::review::ReviewRequest {
            entity_id: Some("ghost"),
            ..Default::default()
        },
    );
    assert_eq!(reviewed.unwrap_err().subject, about);
    let traced = specforge_ops::read::trace::trace(
        &view,
        specforge_ops::read::trace::Target::Entity("ghost"),
    );
    assert_eq!(traced.unwrap_err().subject, about);
}
