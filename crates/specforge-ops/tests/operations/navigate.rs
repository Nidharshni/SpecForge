//! Navigation (`specforge_ops::navigate`) over compiled projects: every
//! position comes from the parser, as the LSP and MCP see it.

use specforge_common::SourceSpan;
use specforge_extension_sdk::prelude::*;
use specforge_ops::navigate::{Direction, Navigator, Occurrence, Precision, ReferenceQuery, Role};
use specforge_ops::view::ProjectView;
use specforge_project::{CompiledProject, Environment};
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
    let runtime = specforge_component::ComponentRuntime::with_user_cache();
    let project = CompiledProject::compile(dir.path(), Some(Arc::new(runtime)));
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
    /// A navigator reading the project's files from disk.
    pub fn navigator(&self) -> Navigator<'_, impl Fn(&str) -> Option<String> + '_> {
        let spec_root = &self.project.environment().spec_root;
        Navigator::new(ProjectView::of(&self.project), move |file| {
            std::fs::read_to_string(spec_root.join(file)).ok()
        })
    }

    /// A navigator reading `text_of` instead of the files.
    pub fn navigator_over<F: Fn(&str) -> Option<String>>(&self, text_of: F) -> Navigator<'_, F> {
        Navigator::new(ProjectView::of(&self.project), text_of)
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

fn nav() -> Compiled {
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
    assert!(error.is(specforge_common::codes::E003));
    assert_eq!(error.kind, specforge_ops::OpErrorKind::EntityNotFound);
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
    let nav = p.navigator_over(|file| match file {
        "login.spec" => Some(format!("\n{LOGIN}")),
        "limit.spec" => Some(LIMIT.to_string()),
        _ => None,
    });
    let refs = nav.references("session_limit", INCOMING).unwrap();
    assert_eq!(refs.len(), 1);
    assert_eq!(refs[0].precision, Precision::Entity);
    assert_eq!(
        at(&refs[0].span),
        "login.spec 1:1-3:2",
        "the holder's block"
    );

    // Unreadable text: the same fallback, the declaration included.
    let unreadable = p.navigator_over(|_| None);
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
    assert_eq!(declaration.role, Role::Declaration);
    assert_eq!(at(&declaration.span), "limit.spec 1:11-1:24");
    // Just past the token's end still names it.
    assert_eq!(
        nav.occurrence_at("limit.spec", 1, 24).unwrap().target,
        "session_limit"
    );
    let reference = nav.occurrence_at("login.spec", 2, 15).unwrap();
    assert_eq!(reference.role, Role::Reference);
    assert_eq!(reference.holder, "login");
    // The title, a keyword, and an unresolved reference are no occurrence.
    assert_eq!(nav.occurrence_at("limit.spec", 1, 27), None);
    assert_eq!(nav.occurrence_at("limit.spec", 1, 3), None);
    assert_eq!(nav.occurrence_at("login.spec", 6, 16), None);
}

// ── One ranking: completion, workspace symbols and search ───────────────

use specforge_ops::navigate::{EntityQuery, MatchScope, MatchedOn, Tier, find_entities};

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
        .registries
        .fields
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
use specforge_ops::navigate::{is_about, subjects};

fn ids(nodes: &[&specforge_graph::Node]) -> Vec<String> {
    nodes.iter().map(|n| n.id.raw.to_string()).collect()
}

fn span(file: &str, (sl, sc): (usize, usize), (el, ec): (usize, usize)) -> SourceSpan {
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

// ── Fixes: the edits a diagnostic's data or the graph names ─────────────

use specforge_ops::navigate::{Fix, FixKind, FixQuery, FixSource, TextEdit};
use specforge_project::coverage::RecordedCoverage;
use specforge_registry::{RegistryBuild, build_registries};

/// `text` with `edits` (spans of it) applied.
fn apply(text: &str, edits: &[TextEdit]) -> String {
    let offset = |line: usize, col: usize| -> usize {
        let start: usize = text
            .split_inclusive('\n')
            .take(line - 1)
            .map(str::len)
            .sum();
        start + col - 1
    };
    let mut edits: Vec<&TextEdit> = edits.iter().collect();
    edits.sort_by_key(|e| std::cmp::Reverse((e.span.start_line, e.span.start_col)));
    let mut out = text.to_string();
    for edit in edits {
        let start = offset(edit.span.start_line, edit.span.start_col);
        let end = offset(edit.span.end_line, edit.span.end_col);
        out.replace_range(start..end, &edit.new_text);
    }
    out
}

/// The fixes of `p`'s own diagnostics for `query`.
fn fixes_of(p: &Compiled, query: &FixQuery) -> Vec<Fix> {
    p.navigator().fixes(&p.project.diagnostics(), query)
}

fn titles(fixes: &[Fix]) -> Vec<String> {
    fixes.iter().map(|f| f.title.clone()).collect()
}

const TESTING: &[&str] = &["@specforge/software", "@specforge/testing"];
const UNTESTED: &str = "behavior first \"First\" {\n  contract \"c\"\n}\n\n\
                        behavior second \"Second\" {\n  contract \"c\"\n  verify unit \"s\"\n}\n";

#[specforge_test(
    behavior = "code_actions_for_missing_verify",
    verify = "code action offered on untested testable entity"
)]
fn a_verify_stub_is_offered_for_an_entity_without_obligations() {
    let p = compile(TESTING, &[("flows.spec", UNTESTED)]);
    let fixes = fixes_of(&p, &FixQuery::default());
    let stubs: Vec<&Fix> = fixes
        .iter()
        .filter(|f| f.source == FixSource::AddVerifyStub)
        .collect();
    assert_eq!(
        stubs.len(),
        1,
        "only first lacks verify: {:?}",
        titles(&fixes)
    );
    assert_eq!(stubs[0].title, "Add verify stub for first");
    assert_eq!(stubs[0].subject, Some(Sym::new("first")));
}

/// The verify stubs of `fixes`: each one's subject and the code it fixes.
fn stubs(fixes: Vec<Fix>) -> Vec<(String, Option<String>)> {
    fixes
        .into_iter()
        .filter(|f| f.source == FixSource::AddVerifyStub)
        .map(|f| (f.subject.unwrap().to_string(), f.diagnostic_code))
        .collect()
}

#[specforge_test(
    behavior = "code_actions_for_missing_verify",
    verify = "no verify stub is offered for an entity a union body or an exempting flag exempts"
)]
fn no_verify_stub_for_an_entity_its_structure_exempts() {
    let p = compile(
        TESTING,
        &[(
            "t.spec",
            "type Status = open | done\n\ntype Plain \"Plain\" {\n  id string\n}\n",
        )],
    );
    // W004 exempts the union, so its stub would fix nothing (and, with no
    // block to hold it, break the file).
    assert_eq!(
        stubs(fixes_of(&p, &FixQuery::default())),
        [("Plain".to_string(), Some("W004".to_string()))]
    );
    let w004: Vec<String> = p
        .project
        .diagnostics()
        .into_iter()
        .filter(|d| d.code == "W004")
        .map(|d| d.message)
        .collect();
    assert_eq!(w004.len(), 1, "{w004:?}");
    assert!(w004[0].contains("'Plain'"), "{w004:?}");
}

#[specforge_test(
    behavior = "code_actions_for_missing_verify",
    verify = "a verify stub fixes the diagnostic that reports its entity, or none when nothing reports it"
)]
fn a_verify_stub_is_attributed_to_the_rule_that_reports_its_entity() {
    let extensions = [
        "@specforge/software",
        "@specforge/testing",
        "@specforge/governance",
    ];
    let p = compile(
        &extensions,
        &[(
            "a.spec",
            "behavior first \"First\" {\n  contract \"c\"\n}\n\n\
             failure_mode crash \"Crash\" {\n  cause \"c\"\n}\n",
        )],
    );
    // `first` is reported (W004); a failure mode is testable but no rule
    // obliges its kind, so its stub fixes no diagnostic.
    let reported: Vec<String> = p
        .project
        .diagnostics()
        .into_iter()
        .filter(|d| d.code == "W004")
        .map(|d| d.message)
        .collect();
    assert_eq!(reported.len(), 1, "{reported:?}");
    assert_eq!(
        stubs(fixes_of(&p, &FixQuery::default())),
        [
            ("first".to_string(), Some("W004".to_string())),
            ("crash".to_string(), None),
        ]
    );
}

/// The §3 probe's kinds and one rule: `item` (testable, accepts verify,
/// `abstract` exempts), `note` (accepts verify), `memo` (accepts none)
/// and `P300`, an obligation rule with no target kind.
fn untargeted_rule() -> specforge_extension_sdk::prelude::ContributionsBuilder {
    use specforge_extension_sdk::prelude::*;
    let mut c = ContributionsBuilder::new(ExtensionMeta::new("@pin/untargeted", "0.1.0"));
    c.kind("item", |k| {
        k.testable(true).supports_verify(true).open_fields(true);
        k.field("abstract", |f| {
            f.field_type(FieldType::Bool).exempts_obligations();
        });
    });
    c.kind("note", |k| {
        k.supports_verify(true).open_fields(true);
    });
    c.kind("memo", |k| {
        k.open_fields(true);
    });
    c.rule("P300", |r| {
        r.check(CheckKind::NoVerifyStatements)
            .field("verify")
            .message_template("{kind} '{id}' declares no verify obligations");
    });
    c
}

#[specforge_test(
    behavior = "snapshot_entities_once",
    verify = "a rule without a target kind applies to every kind, for the rule, the standing and the verify stub alike"
)]
fn an_untargeted_obligation_rule_stubs_every_kind_that_accepts_verify() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("specforge.json"),
        serde_json::json!({"name": "p", "extensions": ["@pin/untargeted"]}).to_string(),
    )
    .unwrap();
    std::fs::write(
        dir.path().join("a.spec"),
        "item alpha \"Alpha\" {\n  verify unit \"works\"\n}\n\nitem beta \"Beta\" {\n}\n\n\
         note gamma \"Gamma\" {\n}\n\nitem delta \"Delta\" {\n  abstract true\n}\n\n\
         memo epsilon \"Epsilon\" {\n}\n",
    )
    .unwrap();
    specforge_installed::testing::install(dir.path(), &["@pin/untargeted"]);
    let runtime = specforge_wasm::testing::InProcessRuntime::new().with(untargeted_rule);
    let project = CompiledProject::compile(dir.path(), Some(Arc::new(runtime)));
    let reported: Vec<String> = project
        .diagnostics()
        .into_iter()
        .filter(|d| d.code == "P300")
        .map(|d| d.message)
        .collect();
    assert_eq!(
        reported,
        [
            "item 'beta' declares no verify obligations",
            "note 'gamma' declares no verify obligations",
        ]
    );
    // The stubs fix exactly what the rule reports: not delta (its flag
    // exempts it), not epsilon (its kind accepts no verify).
    let navigator = Navigator::new(ProjectView::of(&project), |file| {
        std::fs::read_to_string(dir.path().join(file)).ok()
    });
    assert_eq!(
        stubs(navigator.fixes(&project.diagnostics(), &FixQuery::default())),
        [
            ("beta".to_string(), Some("P300".to_string())),
            ("gamma".to_string(), Some("P300".to_string())),
        ]
    );
}

#[specforge_test(
    behavior = "code_actions_for_missing_verify",
    verify = "generated verify stubs added to entity block in .spec file"
)]
fn the_verify_stub_lands_inside_the_block() {
    let p = compile(TESTING, &[("flows.spec", UNTESTED)]);
    let fixes = fixes_of(&p, &FixQuery::default());
    let stub = fixes
        .iter()
        .find(|f| f.title == "Add verify stub for first")
        .unwrap();
    let edited = apply(UNTESTED, &stub.edits);
    assert_eq!(
        edited,
        UNTESTED.replace(
            "  contract \"c\"\n}\n\nbehavior second",
            "  contract \"c\"\n  verify unit \"first — TODO\"\n}\n\nbehavior second"
        )
    );
    // A one-line block gets its stub on a line of its own, still inside.
    let one_line = "behavior solo \"S\" { contract \"c\" }\n";
    let p = compile(TESTING, &[("solo.spec", one_line)]);
    let stub = fixes_of(&p, &FixQuery::default())
        .into_iter()
        .find(|f| f.source == FixSource::AddVerifyStub)
        .unwrap();
    let edited = apply(one_line, &stub.edits);
    assert_eq!(
        edited,
        "behavior solo \"S\" { contract \"c\" \n  verify unit \"solo — TODO\"\n}\n"
    );
    let parsed = specforge_parser::parse(&edited, "solo.spec");
    assert!(parsed.errors.is_empty(), "{:?}", parsed.errors);
    assert!(parsed.entities[0].fields.get("verify").is_some());
}

/// A view of `p`'s graph with `registries` instead of its extensions'.
fn with_registries<'a>(
    p: &'a Compiled,
    env: &'a Environment,
    recorded: &'a RecordedCoverage,
) -> Navigator<'a, impl Fn(&str) -> Option<String> + 'a> {
    let spec_root = &p.project.environment().spec_root;
    Navigator::new(
        ProjectView::new(p.project.graph(), env, None, recorded),
        move |file| std::fs::read_to_string(spec_root.join(file)).ok(),
    )
}

/// The registries of kinds that take verify statements of `verify_kinds`.
fn verifiable(kinds: &[&str], verify_kinds: &[&str]) -> RegistryBuild {
    let mut c = ContributionsBuilder::new(ExtensionMeta::new("@test/ext", "1.0.0"));
    for kind in kinds {
        c.kind(kind, |k| {
            k.keyword(kind)
                .testable(true)
                .supports_verify(true)
                .verify_kinds(verify_kinds);
        });
    }
    let build = build_registries(vec![c.declaration()]);
    assert!(build.diagnostics().next().is_none());
    build
}

#[specforge_test(
    behavior = "code_actions_for_missing_verify",
    verify = "verify stub uses allowed_verify_kinds from KindRegistry"
)]
fn the_verify_stub_uses_the_kinds_first_allowed_verify_kind() {
    let p = compile(
        SOFTWARE,
        &[(
            "a.spec",
            "invariant unique_ids \"U\" {\n  guarantee \"g\"\n}\nfeature untestable \"F\" {\n}\n",
        )],
    );
    let env = Environment::with_registries(verifiable(&["invariant"], &["property", "unit"]));
    let recorded = RecordedCoverage::over(p.project.graph(), &env);
    let fixes = with_registries(&p, &env, &recorded).fixes(&[], &FixQuery::default());
    assert_eq!(
        titles(&fixes),
        ["Add verify stub for unique_ids"],
        "a feature takes none"
    );
    assert_eq!(
        fixes[0].edits[0].new_text,
        "  verify property \"unique_ids — TODO\"\n"
    );
}

#[specforge_test(
    behavior = "code_actions_for_missing_verify",
    verify = "stub format is verify <kind> entity_id TODO"
)]
fn the_verify_stub_names_the_entity_and_todo() {
    let p = compile(TESTING, &[("flows.spec", UNTESTED)]);
    let stub = fixes_of(&p, &FixQuery::default())
        .into_iter()
        .find(|f| f.source == FixSource::AddVerifyStub)
        .unwrap();
    assert_eq!(stub.edits[0].new_text, "  verify unit \"first — TODO\"\n");
}

#[specforge_test(
    behavior = "code_actions_for_missing_verify",
    verify = "code action kind is QuickFix"
)]
fn the_verify_stub_is_a_quick_fix() {
    let p = compile(TESTING, &[("flows.spec", UNTESTED)]);
    let stub = fixes_of(&p, &FixQuery::default())
        .into_iter()
        .find(|f| f.source == FixSource::AddVerifyStub)
        .unwrap();
    assert_eq!(stub.kind, FixKind::QuickFix);
    assert_eq!(serde_json::to_value(stub.kind).unwrap(), "quickfix");
    // Its code is the rule that reports an entity without verify
    // statements, so a code filter finds it.
    let code = stub
        .diagnostic_code
        .clone()
        .expect("the testing rule's code");
    let by_code = fixes_of(
        &p,
        &FixQuery {
            code: Some(&code),
            ..FixQuery::default()
        },
    );
    assert_eq!(titles(&by_code), ["Add verify stub for first"]);
}

#[specforge_test(
    behavior = "code_actions_for_missing_verify",
    verify = "no test source files or application code generated"
)]
fn the_verify_stub_edits_only_the_spec_file() {
    let p = compile(TESTING, &[("flows.spec", UNTESTED)]);
    for fix in fixes_of(&p, &FixQuery::default()) {
        for edit in &fix.edits {
            assert_eq!(edit.span.file, "flows.spec", "{fix:?}");
        }
    }
}

#[specforge_test(
    behavior = "code_actions_for_missing_verify",
    verify = "code actions are those whose diagnostic or entity overlaps the requested range"
)]
fn fixes_are_those_overlapping_the_range() {
    let text = "behavior login \"Login\" {\n  contract \"c\"\n  invariants [sesion_limit]\n}\n\n\
                behavior logout \"Logout\" {\n  contract \"c\"\n}\n";
    let p = compile(TESTING, &[("limit.spec", LIMIT), ("login.spec", text)]);
    let at = |start: (usize, usize), end: (usize, usize)| {
        let range = SourceSpan {
            file: Sym::new("login.spec"),
            start_line: start.0,
            start_col: start.1,
            end_line: end.0,
            end_col: end.1,
        };
        let query = FixQuery {
            file: Some("login.spec"),
            within: Some(&range),
            ..FixQuery::default()
        };
        titles(&fixes_of(&p, &query))
    };
    // The reference's line: its fixes and login's verify stub.
    assert_eq!(
        at((3, 1), (3, 30)),
        [
            "Add verify stub for login",
            "Create invariant stub for sesion_limit",
            "Replace with 'session_limit'"
        ]
    );
    // Inside logout: only its verify stub.
    assert_eq!(at((7, 1), (7, 2)), ["Add verify stub for logout"]);
    // Between the blocks: nothing.
    assert!(at((5, 1), (5, 1)).is_empty());
}

// code_action_create_entity_stub

const DANGLING: &str =
    "behavior login \"L\" {\n  invariants [session_limit]\n  verify unit \"y\"\n}\n";

fn stub_of(p: &Compiled) -> Fix {
    fixes_of(p, &FixQuery::default())
        .into_iter()
        .find(|f| f.source == FixSource::CreateStub)
        .unwrap_or_else(|| panic!("no stub: {:?}", titles(&fixes_of(p, &FixQuery::default()))))
}

#[specforge_test(
    behavior = "code_action_create_entity_stub",
    verify = "stub uses correct entity kind from FieldRegistry target_kind"
)]
fn the_stub_kind_is_the_fields_target_kind() {
    let p = compile(TESTING, &[("auth.spec", DANGLING)]);
    let stub = stub_of(&p);
    assert_eq!(stub.title, "Create invariant stub for session_limit");
    assert!(
        stub.edits[0]
            .new_text
            .starts_with("\ninvariant session_limit \"session_limit\" {"),
        "{stub:?}"
    );
}

/// The registries of `behavior.invariants` as a reference list targeting
/// `target_kind` (a kind declared beside `behavior`).
fn invariants_field(target_kind: Option<&str>) -> RegistryBuild {
    let mut c = ContributionsBuilder::new(ExtensionMeta::new("@test/ext", "1.0.0"));
    c.kind("behavior", |k| {
        k.keyword("behavior");
        k.field("invariants", |f| {
            f.field_type(FieldType::ReferenceList);
            if let Some(target_kind) = target_kind {
                f.target_kind(target_kind);
            }
        });
    });
    if let Some(target_kind) = target_kind {
        c.kind(target_kind, |k| {
            k.keyword(target_kind);
        });
    }
    let build = build_registries(vec![c.declaration()]);
    assert!(build.diagnostics().next().is_none());
    build
}

#[specforge_test(
    behavior = "code_action_create_entity_stub",
    verify = "no code action when enclosing field has no target_kind"
)]
fn no_stub_without_a_target_kind() {
    let p = compile(TESTING, &[("auth.spec", DANGLING)]);
    let diagnostics = p.project.diagnostics();
    let env = Environment::with_registries(invariants_field(None));
    let recorded = RecordedCoverage::over(p.project.graph(), &env);
    let fixes = with_registries(&p, &env, &recorded).fixes(&diagnostics, &FixQuery::default());
    assert!(
        fixes.iter().all(|f| f.source != FixSource::CreateStub),
        "{:?}",
        titles(&fixes)
    );
    let targeted = Environment::with_registries(invariants_field(Some("invariant")));
    let fixes = with_registries(&p, &targeted, &recorded).fixes(&diagnostics, &FixQuery::default());
    assert!(fixes.iter().any(|f| f.source == FixSource::CreateStub));
}

#[specforge_test(
    behavior = "code_action_create_entity_stub",
    verify = "stub is inserted at end of current file"
)]
fn the_stub_is_appended_to_the_file() {
    let p = compile(TESTING, &[("auth.spec", DANGLING)]);
    let stub = stub_of(&p);
    assert_eq!(stub.edits.len(), 1);
    assert_eq!(
        at(&stub.edits[0].span),
        "auth.spec 5:1-5:1",
        "the end of the file"
    );
    let edited = apply(DANGLING, &stub.edits);
    assert!(edited.starts_with(DANGLING));
    let parsed = specforge_parser::parse(&edited, "auth.spec");
    assert!(parsed.errors.is_empty(), "{:?}", parsed.errors);
    assert_eq!(parsed.entities.len(), 2);
}

#[specforge_test(
    behavior = "code_action_create_entity_stub",
    verify = "code action kind is Refactor"
)]
fn the_stub_is_a_refactoring() {
    let p = compile(TESTING, &[("auth.spec", DANGLING)]);
    assert_eq!(stub_of(&p).kind, FixKind::Refactor);
}

#[specforge_test(
    behavior = "code_action_create_entity_stub",
    verify = "generated stub contains no application code or test files"
)]
fn the_stub_is_a_bare_block() {
    let p = compile(TESTING, &[("auth.spec", DANGLING)]);
    let stub = stub_of(&p);
    assert_eq!(
        stub.edits[0].new_text,
        "\ninvariant session_limit \"session_limit\" {\n  // TODO: fill in fields\n}\n"
    );
    assert!(stub.edits.iter().all(|e| e.span.file == "auth.spec"));
}

/// An E003 at `span` whose message says nothing a parser could use: only
/// its data names the reference.
fn reworded_e003(span: SourceSpan, data: Option<DiagnosticData>) -> Diagnostic {
    let mut diagnostic = Diagnostic::new(
        specforge_common::codes::E003,
        "this wording is not a contract",
    )
    .with_span(span);
    diagnostic.data = data.map(Box::new);
    diagnostic
}

fn unresolved(target: &str, entity: &str, field: &str, close: Option<&str>) -> DiagnosticData {
    DiagnosticData::UnresolvedReference {
        target: target.into(),
        entity: entity.into(),
        field: field.into(),
        did_you_mean: close.map(String::from),
    }
}

#[specforge_test(
    behavior = "code_action_create_entity_stub",
    verify = "the stub is read from the diagnostic's data, whatever its message says"
)]
fn the_stub_reads_target_entity_and_field_from_the_data() {
    let p = compile(TESTING, &[("auth.spec", DANGLING)]);
    let token = span("auth.spec", (2, 15), (2, 28));
    let data = unresolved("session_limit", "login", "invariants", None);
    // Twice: one stub per target.
    let reworded = [
        reworded_e003(token.clone(), Some(data.clone())),
        reworded_e003(token.clone(), Some(data)),
    ];
    let fixes = p.navigator().fixes(&reworded, &FixQuery::default());
    let stubs: Vec<&Fix> = fixes
        .iter()
        .filter(|f| f.source == FixSource::CreateStub)
        .collect();
    assert_eq!(stubs.len(), 1);
    assert_eq!(stubs[0].title, "Create invariant stub for session_limit");

    // The diagnostic as text alone, in the compiler's wording: no stub.
    let mut text_only = reworded_e003(token, None);
    text_only.message = "unresolved reference 'session_limit' in entity 'login'".into();
    let fixes = p.navigator().fixes(&[text_only], &FixQuery::default());
    assert!(fixes.iter().all(|f| f.source != FixSource::CreateStub));
}

#[test]
fn no_stub_for_a_target_that_exists() {
    let p = compile(TESTING, &[("auth.spec", DANGLING), ("limit.spec", LIMIT)]);
    let stale = reworded_e003(
        span("auth.spec", (2, 15), (2, 28)),
        Some(unresolved("session_limit", "login", "invariants", None)),
    );
    let fixes = p.navigator().fixes(&[stale], &FixQuery::default());
    assert!(fixes.iter().all(|f| f.source != FixSource::CreateStub));
}

// code_action_replace_unresolved

#[specforge_test(
    behavior = "code_action_replace_unresolved",
    verify = "an unresolved reference with a close match is replaced at its token"
)]
fn a_close_match_replaces_the_unresolved_token() {
    let p = nav();
    let query = FixQuery {
        code: Some("E003"),
        ..FixQuery::default()
    };
    let fix = fixes_of(&p, &query)
        .into_iter()
        .find(|f| f.source == FixSource::ReplaceUnresolved)
        .unwrap();
    assert_eq!(fix.title, "Replace with 'session_limit'");
    assert_eq!(fix.kind, FixKind::QuickFix);
    assert_eq!(fix.edits.len(), 1);
    assert_eq!(at(&fix.edits[0].span), "login.spec 6:15-6:27");
    assert_eq!(fix.edits[0].new_text, "session_limit");
    assert_eq!(
        apply(LOGIN, &fix.edits),
        LOGIN.replace("[sesion_limit]", "[session_limit]")
    );
    // A token like it, x_sesion_limit, is never hit: the span is the token.
}

#[specforge_test(
    behavior = "code_action_replace_unresolved",
    verify = "an unresolved import with a close match is replaced inside its quotes"
)]
fn a_close_match_replaces_the_unresolved_import_inside_its_quotes() {
    let p = compile(
        SOFTWARE,
        &[
            ("auth.spec", "behavior auth \"A\" {\n  contract \"c\"\n}\n"),
            ("main.spec", "use \"autth\"\n"),
        ],
    );
    let fixes = fixes_of(
        &p,
        &FixQuery {
            code: Some("E025"),
            ..FixQuery::default()
        },
    );
    assert_eq!(fixes.len(), 1, "{:?}", titles(&fixes));
    let edit = &fixes[0].edits[0];
    assert_eq!(at(&edit.span), "main.spec 1:6-1:11", "inside the quotes");
    assert_eq!(apply("use \"autth\"\n", &fixes[0].edits), "use \"auth\"\n");
}

#[specforge_test(
    behavior = "code_action_replace_unresolved",
    verify = "the replacement is read from the diagnostic's data, whatever its message says"
)]
fn the_replacement_is_read_from_the_data() {
    let p = nav();
    let token = span("login.spec", (6, 15), (6, 27));
    let reworded = reworded_e003(
        token.clone(),
        Some(unresolved(
            "sesion_limit",
            "logout",
            "invariants",
            Some("session_limit"),
        )),
    );
    let fixes = p.navigator().fixes(&[reworded], &FixQuery::default());
    let replace: Vec<&Fix> = fixes
        .iter()
        .filter(|f| f.source == FixSource::ReplaceUnresolved)
        .collect();
    assert_eq!(replace.len(), 1);
    assert_eq!(replace[0].edits[0].new_text, "session_limit");

    // Its message and suggestion alone offer nothing.
    let mut text_only = reworded_e003(token, None);
    text_only.message = "unresolved reference 'sesion_limit' in entity 'logout'".into();
    text_only.suggestion = Some("did you mean 'session_limit'?".into());
    assert!(
        p.navigator()
            .fixes(&[text_only], &FixQuery::default())
            .is_empty()
    );
}

// ── Files: the file rule and the outline ────────────────────────────────

use specforge_ops::navigate::{FileMatch, anchors_of_file, match_file, outline};

#[test]
fn match_file_is_component_wise() {
    // Moved from prompts/infer.rs (C9-09).
    assert_eq!(match_file("e.rs", "src/cache.rs"), FileMatch::None);
    assert_eq!(match_file("todo_list.rs", "todo_list.rs"), FileMatch::Exact);
    assert_eq!(
        match_file("todo_list.rs", "src/todo_list.rs"),
        FileMatch::Suffix
    );
    assert_eq!(
        match_file("src/auth", "src/auth/login.rs"),
        FileMatch::Under
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

/// An anchors manifest anchoring each `(entity, file)`.
fn anchored(anchors: &[(&str, &str)]) -> specforge_ops::navigate::AnchorManifest {
    specforge_ops::navigate::AnchorManifest {
        version: 1,
        anchors: anchors
            .iter()
            .map(|(entity, file)| specforge_ops::navigate::SourceAnchor {
                entity_id: entity.to_string(),
                file: file.to_string(),
                line: 1,
                symbol_name: entity.to_string(),
                item_kind: "fn".into(),
                scanner: "manual".into(),
                mapping_strategy: None,
                confidence: None,
            })
            .collect(),
    }
}

/// An entity's anchors are the manifest's anchors of it, in manifest
/// order; an entity with none, or a project with no manifest, has none.
#[specforge_test(
    behavior = "provide_mcp_find_implementation_tool",
    verify = "an entity with no anchor has no implementations, and no anchors manifest is none"
)]
fn anchors_of_an_entity_are_in_manifest_order() {
    use specforge_ops::navigate::{anchors_of_entity, source_anchors};

    let manifest = anchored(&[
        ("alpha", "src/lib.rs"),
        ("beta", "src/lib.rs"),
        ("alpha", "src/net.rs"),
    ]);
    let files = |entity: &str| -> Vec<String> {
        anchors_of_entity(&manifest, entity)
            .iter()
            .map(|a| a.file.clone())
            .collect()
    };
    assert_eq!(files("alpha"), ["src/lib.rs", "src/net.rs"]);
    assert!(files("nope").is_empty());

    let project = crate::view_support::Project::new(
        "behavior a \"A\" {\n}\n",
        specforge_registry::RegistryBuild::default(),
    );
    assert!(source_anchors(&project.view()).unwrap().anchors.is_empty());
}

#[test]
fn the_tightest_match_wins() {
    let manifest = anchored(&[
        ("top", "a.rs"),
        ("nested", "sub/a.rs"),
        ("other", "sub/b.rs"),
        ("cache", "src/cache.rs"),
    ]);
    let ids = |found: &specforge_ops::navigate::FileAnchors| -> Vec<String> {
        found.anchors.iter().map(|a| a.entity_id.clone()).collect()
    };
    let exact = anchors_of_file(&manifest, "a.rs");
    assert_eq!(
        (exact.mode, ids(&exact)),
        (FileMatch::Exact, vec!["top".to_string()])
    );
    // Every spelling of a path is the path.
    for spelling in ["./a.rs", "a.rs", ".\\a.rs"] {
        assert_eq!(
            ids(&anchors_of_file(&manifest, spelling)),
            ["top"],
            "{spelling}"
        );
    }
    let under = anchors_of_file(&manifest, "./sub");
    assert_eq!(under.mode, FileMatch::Under);
    assert_eq!(ids(&under), ["nested", "other"]);
    let suffix = anchors_of_file(&manifest, "b.rs");
    assert_eq!(
        (suffix.mode, ids(&suffix)),
        (FileMatch::Suffix, vec!["other".to_string()])
    );
    // A substring is no match (C9-09).
    assert_eq!(anchors_of_file(&manifest, "e.rs").mode, FileMatch::None);
    assert_eq!(anchors_of_file(&manifest, "c.rs").mode, FileMatch::None);
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
    let entries = outline(&p.navigator(), "test.spec");
    let ids: Vec<&str> = entries.iter().map(|e| e.id.as_str()).collect();
    assert_eq!(ids, ["b", "a"], "in line order, other files left out");
    assert_eq!(outline(&p.navigator(), "./test.spec").len(), 2);
    assert!(
        outline(&p.navigator(), "est.spec").is_empty(),
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
    let entries = outline(&p.navigator(), "store.spec");
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

use specforge_ops::navigate::{Reference, References};
use std::sync::Arc;

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
    let env = Environment::empty();
    let recorded = RecordedCoverage::over(&graph, &env);
    let view = ProjectView::new(&graph, &env, None, &recorded);
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
