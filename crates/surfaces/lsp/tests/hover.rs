//! The hover: what the server answers at a position of a project served
//! from disk (`specforge_lsp::answers::hover`, over a project whose
//! extensions are declared in process), and the markdown of its parts
//! (`specforge_lsp::hover`): an entity's facts, read by the inspect read
//! view (`specforge_ops::read::inspect`); a field's help; the diagnostics under
//! the cursor.

use specforge_common::{Severity, SourceSpan, Sym};
use specforge_graph::{Edge, Graph, Node};
use specforge_lsp::LineIndex;
use specforge_ops::view::ProjectView;
use specforge_project::Environment;
use specforge_project::coverage::RecordedCoverage;
use specforge_test_macros::test as spec;
use tower_lsp::lsp_types::Position;

use crate::registries::{kind, obligating, registries};
use crate::served::Served;
use specforge_extension_sdk::prelude::{ContributionsBuilder, FieldType, Vocabulary};

/// A graph node for the tests of the renderer, which hand it facts.
fn node(id: &str, kind: &str, title: Option<&str>) -> Node {
    Node {
        title: title.map(|t| t.to_string()),
        ..Node::new(
            id,
            kind,
            SourceSpan {
                file: Sym::new("test.spec"),
                start_line: 0,
                start_col: 0,
                end_line: 0,
                end_col: 0,
            },
        )
    }
}

fn edge(source: &str, target: &str, label: &str) -> Edge {
    Edge {
        source: source.into(),
        target: target.into(),
        label: label.into(),
    }
}

/// A project of one file, `main.spec`, holding `source`, served with no
/// extension and `main.spec` open.
fn plain(source: &str) -> Served {
    Served::new(&[("main.spec", source)]).open(&["main.spec"])
}

/// A reference-list field `name` of the kind being declared.
fn references(k: &mut specforge_extension_sdk::prelude::KindBuilder, name: &str) {
    k.field(name, |f| {
        f.field_type(FieldType::ReferenceList);
    });
}

/// The kinds the reference tests link: `behavior` (features, types),
/// `feature`, `type`, `invariant` (enforced_by) and `milestone` (features).
fn graph_kinds(c: &mut ContributionsBuilder) {
    c.kind("behavior", |k| {
        references(k, "features");
        references(k, "types");
    });
    c.kind("feature", |_| {});
    c.kind("type", |_| {});
    c.kind("invariant", |k| references(k, "enforced_by"));
    c.kind("milestone", |k| references(k, "features"));
}

#[spec(
    behavior = "hover_information",
    verify = "hover returns markdown-formatted content"
)]
fn hover_renders_markdown() {
    let served = plain("type my_type \"My Type\" {}\n");

    let text = served
        .hover_on("main.spec", "my_type")
        .expect("should produce hover");
    assert!(text.starts_with("**type** `my_type` — My Type"), "{text}");
    assert!(served.hover_on("main.spec", "nonexistent").is_none());
}

#[spec(
    behavior = "provide_extension_entity_hover",
    verify = "hover shows entity kind and source extension"
)]
fn hover_shows_the_kind_and_its_extension() {
    let served = Served::new(&[("main.spec", "behavior login \"User Login\" {}\n")])
        .extension("@specforge/software", |c| {
            c.kind("behavior", |k| {
                k.testable(true)
                    .supports_verify(true)
                    .description("A testable unit of system functionality")
                    // A SymbolKind name, for document symbols: never printed in
                    // the hover.
                    .lsp_icon("Method");
            });
        })
        .open(&["main.spec"]);

    let text = served.hover_on("main.spec", "login").unwrap();
    assert!(text.starts_with("**"), "editor-neutral header:\n{text}");
    let header = text.split("\n\n---\n\n").next().unwrap();
    assert_eq!(
        header,
        "**behavior** `login` — User Login\n\n\
         A testable unit of system functionality\n\
         *@specforge/software* · `testable` · `verify`"
    );
}

#[spec(
    behavior = "provide_extension_entity_hover",
    verify = "hover shows testability for testable kinds"
)]
fn hover_shows_testability_from_the_standing() {
    let served = Served::new(&[(
        "main.spec",
        "behavior login \"Login\" {}\nfeature auth \"Auth\" {}\n",
    )])
    .extension("@specforge/software", |c| kind(c, "behavior", true))
    .extension("@specforge/product", |c| kind(c, "feature", false))
    .open(&["main.spec"]);

    for (id, testable) in [("login", true), ("auth", false)] {
        let state = served.state();
        let facts = specforge_ops::read::inspect::inspect(&state.view(), id).unwrap();
        assert_eq!(facts.standing.testable, testable, "{id}");
        let text = served.hover_on("main.spec", id).unwrap();
        assert_eq!(text.contains("`testable`"), testable, "{id}:\n{text}");
    }
    assert!(
        served
            .hover_on("main.spec", "auth")
            .unwrap()
            .ends_with("*@specforge/product*")
    );
}

#[spec(
    behavior = "provide_extension_entity_hover",
    verify = "hover shows reference count from graph"
)]
fn hover_shows_reference_counts_and_groups() {
    let served = Served::new(&[(
        "main.spec",
        "behavior create_user \"Create User\" {\n  features [user_management]\n  types [user_type]\n}\n\
         feature user_management \"User Management\" {}\n\
         type user_type \"User\" {}\n\
         invariant data_integrity \"Integrity\" {\n  enforced_by [create_user]\n}\n\
         milestone v1_launch \"V1\" {\n  features [create_user]\n}\n",
    )])
    .extension("@t/graph", graph_kinds)
    .open(&["main.spec"]);

    // The reference sections: the extension and the entity's fields are
    // other sections.
    let text = served.hover_on("main.spec", "create_user").unwrap();
    let references: Vec<&str> = text
        .split("\n\n---\n\n")
        .filter(|section| section.starts_with("**Refer"))
        .collect();
    assert_eq!(
        references,
        [
            "**Refers to** *(2)*\n\
             - `features` → user_management\n\
             - `types` → user_type",
            "**Referenced by** *(2)*\n\
             - invariant via `enforced_by`: data_integrity\n\
             - milestone via `features`: v1_launch"
        ],
        "{text}"
    );

    // Several references of one kind through one field are one group.
    let served = Served::new(&[(
        "main.spec",
        "feature auth_feature \"Auth\" {}\n\
         behavior login \"Login\" {\n  features [auth_feature]\n}\n\
         behavior logout \"Logout\" {\n  features [auth_feature]\n}\n\
         milestone v1_launch \"V1\" {\n  features [auth_feature]\n}\n",
    )])
    .extension("@t/graph", graph_kinds)
    .open(&["main.spec"]);
    let text = served.hover_on("main.spec", "auth_feature").unwrap();
    assert!(
        text.ends_with(
            "**Referenced by** *(3)*\n\
             - behavior via `features`: login, logout\n\
             - milestone via `features`: v1_launch"
        ),
        "{text}"
    );

    // No references, no sections.
    let served = plain("type orphan \"Orphan Type\" {}\n");
    assert_eq!(
        served.hover_on("main.spec", "orphan").unwrap(),
        "**type** `orphan` — Orphan Type"
    );
}

#[spec(
    behavior = "provide_extension_entity_hover",
    verify = "hover content formatted as markdown"
)]
fn hover_without_registries_is_plain_markdown() {
    let served = plain("type g \"G\" {}\n");
    // No extension line, no sections.
    assert_eq!(
        served.hover_on("main.spec", "g").unwrap(),
        "**type** `g` — G"
    );
}

#[test]
fn hover_shows_field_values() {
    let served = Served::new(&[(
        "main.spec",
        "behavior login \"Login\" {\n  status draft\n  invariants [data_integrity, auth_required]\n  \
         contract \"Given valid credentials, the user is authenticated\"\n}\n\
         invariant data_integrity \"D\" {}\n\
         invariant auth_required \"A\" {}\n\
         behavior bare \"Bare\" {}\n",
    )])
    .extension("@t/fields", |c| {
        c.kind("behavior", |k| {
            k.field("status", |f| {
                f.vocabulary(&Vocabulary::new(&["draft", "final"]));
            });
            references(k, "invariants");
            k.field("contract", |f| {
                f.field_type(FieldType::String);
            });
        });
        c.kind("invariant", |_| {});
    })
    .open(&["main.spec"]);

    let text = served.hover_on("main.spec", "login").unwrap();
    assert!(
        text.contains("**Fields**"),
        "should have Fields section:\n{text}"
    );
    assert!(
        text.contains("`status` = `draft`"),
        "should show status value:\n{text}"
    );
    assert!(
        text.contains("`invariants` = [data_integrity, auth_required]"),
        "should show ref list:\n{text}"
    );
    assert!(
        text.contains("`contract` = \"Given valid credentials"),
        "should show contract string:\n{text}"
    );

    let text = served.hover_on("main.spec", "bare").unwrap();
    assert!(
        !text.contains("Fields"),
        "should not show Fields when entity has none:\n{text}"
    );
}

// -- headline, coverage and the entity's diagnostics ------------------------

/// `behavior` (obligated) with a headline `contract`, `type` (obligated),
/// `constraint` (testable, no rule) and `note` (not testable), declared by
/// three extensions.
pub(crate) fn coverage_project(source: &str) -> Served {
    Served::new(&[("main.spec", source)])
        .extension("@t/soft", |c| {
            c.kind("behavior", |k| {
                k.testable(true).supports_verify(true);
                k.field("contract", |f| {
                    f.field_type(FieldType::String).headline().normative();
                });
            });
            kind(c, "type", true);
            obligating(c, "behavior");
            obligating(c, "type");
        })
        .extension("@t/gov", |c| kind(c, "constraint", true))
        .extension("@t/doc", |c| kind(c, "note", false))
        .open(&["main.spec"])
}

/// The hover's Coverage line of `id`, if it has one.
fn coverage(hover: &str) -> Option<String> {
    hover
        .split("\n\n---\n\n")
        .find(|section| section.starts_with("**Coverage**"))
        .map(str::to_string)
}

const COVERAGE: &str = "\
behavior done \"Done\" {\n  verify unit \"first\"\n}\n\
behavior half \"Half\" {\n  verify unit \"first\"\n  verify unit \"second\"\n}\n\
type U = a | b\n\
constraint free \"Free\" {\n}\n\
note doc \"Doc\" {\n  verify unit \"read\"\n}\n\
note plain \"Plain\" {\n}\n";

#[spec(
    behavior = "provide_extension_entity_hover",
    verify = "hover shows the entity's coverage as specforge.inspect reports it"
)]
fn hover_shows_each_coverage_case() {
    let project = coverage_project(COVERAGE);
    let line = |id: &str| coverage(&project.hover_on("main.spec", id).unwrap());

    // No report recorded.
    assert_eq!(
        line("done").as_deref(),
        Some("**Coverage** · `uncovered` · 0/1 obligations proven · no test report recorded")
    );

    project.write(
        "specforge-report.json",
        r#"{"results": {
            "done": {"tests": [{"name": "t", "status": "passed", "verify": "first"}]},
            "half": {"tests": [
                {"name": "t1", "status": "passed", "verify": "first"},
                {"name": "t2", "status": "failed", "verify": "second"}]},
            "doc": {"tests": []}
        }}"#,
    );
    let rows = [
        (
            "done",
            Some("**Coverage** · `covered` · 1/1 obligations proven · 1 test"),
        ),
        (
            "half",
            Some("**Coverage** · `partial` · 1/2 obligations proven · 2 tests · 1 failing"),
        ),
        (
            "U",
            Some("**Coverage** · exempt: it owes none (a union or an exempting field)"),
        ),
        (
            "free",
            Some("**Coverage** · exempt: its kind need not declare obligations"),
        ),
        (
            "doc",
            Some(
                "**Coverage** · `uncovered` · 0/1 obligations proven · 0 tests · \
                 its kind is not testable, so it does not count",
            ),
        ),
        ("plain", None),
    ];
    for (id, expected) in rows {
        assert_eq!(line(id).as_deref(), expected, "{id}");
        // The line agrees with the read view MCP inspect renders.
        let state = project.state();
        let facts = specforge_ops::read::inspect::inspect(&state.view(), id).unwrap();
        if let Some(expected) = expected
            && !facts.standing.exempt()
        {
            let status = specforge_ops::read::coverage::STATUS.name_of(
                specforge_ops::read::inspect::coverage(&state.view(), id)
                    .unwrap()
                    .status(),
            );
            assert!(expected.contains(&format!("`{status}`")), "{id}");
        }
    }

    // A report that cannot be read is shown, not a failed hover.
    project.write("specforge-report.json", "{not json");
    let unreadable = line("done").unwrap();
    assert!(
        unreadable.starts_with("**Coverage** · the recorded test report cannot be read (E045): "),
        "{unreadable}"
    );
    assert_eq!(
        line("U").as_deref(),
        Some("**Coverage** · exempt: it owes none (a union or an exempting field)"),
        "the standing does not depend on the report"
    );
}

#[spec(
    behavior = "provide_extension_entity_hover",
    verify = "hover never states a coverage fact it cannot read: while the project rebuilds it says so"
)]
fn hover_says_coverage_is_unavailable_while_the_project_rebuilds() {
    let mut project = coverage_project(COVERAGE);
    project.write(
        "specforge-report.json",
        r#"{"results": {"done": {"tests": [{"name": "t", "status": "passed", "verify": "first"}]}}}"#,
    );
    let uri = project.uri("main.spec");
    let hover_while_rebuilding = |project: &mut Served, id: &str| {
        let position = project.position_of("main.spec", id);
        project.rebuilding(|state| crate::served::hover_text(state, &uri, position))
    };
    for id in ["done", "U", "doc"] {
        let hover = hover_while_rebuilding(&mut project, id).unwrap();
        assert_eq!(
            coverage(&hover).as_deref(),
            Some("**Coverage** · unavailable while the project rebuilds"),
            "{id}"
        );
    }
    let plain = hover_while_rebuilding(&mut project, "plain").unwrap();
    assert_eq!(coverage(&plain), None);
    // The standing badges stay.
    let done = hover_while_rebuilding(&mut project, "done").unwrap();
    assert!(done.contains("`testable`"));
}

#[spec(
    behavior = "provide_extension_entity_hover",
    verify = "hover shows the headline statement as its summary"
)]
fn hover_quotes_the_headline_and_fields_skip_it() {
    let source = "\
behavior one \"One\" {\n  contract \"The system MUST do one thing\"\n  verify unit \"it does\"\n}\n\
behavior three \"Three\" {\n  contract \"\"\"\n    The system MUST do a\n      and then b\n    and c\n  \"\"\"\n  verify unit \"it does\"\n}\n\
note doc \"Doc\" {\n  verify unit \"read\"\n}\n";
    let project = coverage_project(source);

    let one = project.hover_on("main.spec", "one").unwrap();
    let header = one.split("\n\n---\n\n").next().unwrap();
    assert!(
        header.ends_with("\n\n> The system MUST do one thing"),
        "{header}"
    );
    assert!(!one.contains("`contract` ="), "Fields skips it:\n{one}");
    assert!(one.contains("- `verify` = unit: it does"), "{one}");

    let three = project.hover_on("main.spec", "three").unwrap();
    let header = three.split("\n\n---\n\n").next().unwrap();
    assert!(
        header.ends_with("\n\n> The system MUST do a\n>   and then b\n> and c"),
        "a multi-line headline is quoted whole, dedented:\n{header}"
    );
    assert!(!three.contains("`contract` ="), "{three}");

    // A kind with no headline has no summary.
    let doc = project.hover_on("main.spec", "doc").unwrap();
    assert!(!doc.contains("\n> "), "{doc}");
}

#[spec(
    behavior = "hover_diagnostic",
    verify = "the diagnostic under the cursor comes before the entity's hover, which does not list it again"
)]
fn hover_puts_the_diagnostic_under_the_cursor_first() {
    let served = plain(
        "behavior alpha \"A\" {\n  types [beta]\n}\nbehavior beta \"B\" {\n  types [alpha]\n}\n",
    );
    let text = served.hover("main.spec", 0, 10).expect("a hover");
    assert!(
        text.starts_with("**W061** · Reference cycle detected"),
        "{text}"
    );
    assert!(
        text.contains("\n\n---\n\n**behavior** `alpha` — A"),
        "{text}"
    );
    assert!(!text.contains("**Diagnostics**"), "{text}");
}

#[spec(
    behavior = "provide_extension_entity_hover",
    verify = "hover lists the entity's diagnostics the cursor's do not already show"
)]
fn hover_lists_the_diagnostics_not_already_shown() {
    use specforge_common::{Diagnostic, DiagnosticData};
    let mut g = Graph::new();
    g.add_node(node("alpha", "behavior", Some("A")));
    g.add_node(node("beta", "behavior", Some("B")));
    g.add_edge(edge("alpha", "beta", "depends_on"));
    g.add_edge(edge("beta", "alpha", "depends_on"));
    // Spanless: about the entities its data names.
    let cycle = Diagnostic::new(
        specforge_common::codes::W061,
        "reference cycle detected: alpha -> beta -> alpha",
    )
    .with_data(DiagnosticData::ReferenceCycle {
        path: vec!["alpha".into(), "beta".into(), "alpha".into()],
    });
    let third_party = Diagnostic::untyped("X900", Severity::Warning, "acme says no").with_data(
        DiagnosticData::Subject {
            entity: "beta".into(),
        },
    );
    let reported = vec![cycle.clone(), third_party];
    let env = Environment::empty();
    let recorded = RecordedCoverage::over(&g, &env);
    let view = ProjectView::new(&g, &env, None, &recorded).reporting(&reported);
    let facts = specforge_ops::read::inspect::inspect(&view, "beta").unwrap();

    let coverage = specforge_ops::read::inspect::coverage(&view, "beta");
    let text = specforge_lsp::hover::entity(&facts, &coverage, &[], false);
    assert!(
        text.ends_with(
            "**Diagnostics** *(2)*\n\
             - [**W061**](https://github.com/leaderiop/SpecForge/blob/main/docs/diagnostics.md#w061) \
             reference cycle detected: alpha -> beta -> alpha\n\
             - **X900** acme says no"
        ),
        "{text}"
    );

    // What the cursor already shows is not listed again.
    let text = specforge_lsp::hover::entity(&facts, &coverage, &[&cycle], false);
    assert!(
        text.ends_with("**Diagnostics** *(1)*\n- **X900** acme says no"),
        "{text}"
    );
    let all = [&reported[0], &reported[1]];
    let text = specforge_lsp::hover::entity(&facts, &coverage, &all, false);
    assert!(!text.contains("**Diagnostics**"), "{text}");
}

// -- hover_field_info (description) -------------------------------------------

#[test]
fn hover_shows_field_description() {
    let build = registries("@specforge/software", |c| {
        c.kind("behavior", |k| {
            k.field("contract", |f| {
                f.field_type(FieldType::String)
                    .description("The behavioral contract this entity fulfills");
            });
        });
    });
    let reg = build.fields();

    let text = specforge_lsp::hover_field_info("contract", "behavior", reg).unwrap();
    assert!(
        text.contains("The behavioral contract this entity fulfills"),
        "should show field description:\n{text}"
    );
}

// -- hover_field_info --------------------------------------------------------

fn make_field_registry() -> specforge_registry::RegistryBuild {
    registries("@specforge/software", |c| {
        c.kind("behavior", |k| {
            k.field("contract", |f| {
                f.field_type(FieldType::String);
            });
            k.field("features", |f| {
                f.field_type(FieldType::ReferenceList)
                    .edge("BehaviorImplementsFeature")
                    .target_kind("feature")
                    .required();
            });
        });
    })
}

#[test]
fn field_hover_shows_type_and_extension() {
    let build = make_field_registry();
    let reg = build.fields();
    let text = specforge_lsp::hover_field_info("contract", "behavior", reg).unwrap();
    assert!(
        text.contains("`contract`"),
        "should show field name:\n{text}"
    );
    assert!(text.contains("string"), "should show type:\n{text}");
    assert!(
        text.contains("*@specforge/software*"),
        "should show extension:\n{text}"
    );
}

#[test]
fn field_hover_shows_target_kind_and_edge() {
    let build = make_field_registry();
    let reg = build.fields();
    let text = specforge_lsp::hover_field_info("features", "behavior", reg).unwrap();
    assert!(
        text.contains("→ **feature**"),
        "should show target kind:\n{text}"
    );
    assert!(
        text.contains("Edge `BehaviorImplementsFeature`"),
        "should show edge type:\n{text}"
    );
    assert!(text.contains("*required*"), "should show required:\n{text}");
}

#[test]
fn field_hover_unknown_field_returns_none() {
    let build = make_field_registry();
    let reg = build.fields();
    assert!(specforge_lsp::hover_field_info("nonexistent", "behavior", reg).is_none());
}

#[test]
fn field_hover_unknown_kind_returns_none() {
    let build = make_field_registry();
    let reg = build.fields();
    assert!(specforge_lsp::hover_field_info("contract", "unknown_kind", reg).is_none());
}

#[spec(
    behavior = "hover_information",
    verify = "field help says what an absent value counts as"
)]
fn field_hover_says_what_an_absent_value_counts_as() {
    let declaration = crate::registries::declaration("@t/x", |c| {
        c.kind("ticket", |k| {
            k.field("status", |f| {
                f.vocabulary(&Vocabulary::new(&["draft", "final"]).absent_as("draft"))
                    .description("Where it stands");
            });
            k.field("owner", |f| {
                f.field_type(FieldType::String).description("Who has it");
            });
        });
    });
    let build = crate::registries::registries_of(vec![declaration]);
    let reg = build.fields();

    let status = specforge_lsp::hover_field_info("status", "ticket", reg).unwrap();
    assert!(
        status.contains("Where it stands  \nAbsent, it counts as `draft`."),
        "{status}"
    );
    let owner = specforge_lsp::hover_field_info("owner", "ticket", reg).unwrap();
    assert!(!owner.contains("Absent"), "{owner}");
}

#[spec(
    behavior = "hover_information",
    verify = "field help names a field's type as E061 does, an enum's declared values included"
)]
fn field_hover_names_a_type_as_e061_does() {
    let build = registries("@t/x", |c| {
        c.kind("ticket", |k| {
            k.field("urgent", |f| {
                f.field_type(FieldType::Bool);
            });
            k.field("priority", |f| {
                f.vocabulary(&Vocabulary::new(&["low", "high"]));
            });
        });
    });
    let reg = build.fields();

    let urgent = specforge_lsp::hover_field_info("urgent", "ticket", reg).unwrap();
    assert!(urgent.starts_with("**`urgent`** : bool"), "{urgent}");
    let priority = specforge_lsp::hover_field_info("priority", "ticket", reg).unwrap();
    assert!(
        priority.starts_with("**`priority`** : enum (low, high)  \n"),
        "{priority}"
    );
}

fn diagnostic_at(
    code: &str,
    message: &str,
    line: usize,
    start: usize,
    end: usize,
) -> specforge_common::Diagnostic {
    specforge_common::Diagnostic::untyped(code, specforge_common::Severity::Error, message)
        .with_span(SourceSpan {
            file: Sym::new("test.spec"),
            start_line: line,
            start_col: start,
            end_line: line,
            end_col: end,
        })
}

/// The cursor section of the hover at `position`: the published
/// diagnostics there, rendered.
fn diagnostic_hover(
    published: &[specforge_common::Diagnostic],
    index: &LineIndex,
    position: Position,
) -> Option<String> {
    specforge_lsp::hover::diagnostics(&specforge_lsp::hover::diagnostics_at(
        published, index, position,
    ))
}

/// An extension's finding with a code it may not use (here E001, core's)
/// is not described as that code: the hover names the extension instead of
/// quoting the parse-error explanation.
#[spec(
    behavior = "call_extension_exports",
    verify = "a diagnostic an extension reported names its extension, and a code it may not use is not described as its owner's"
)]
fn hover_names_the_extension_of_a_squatted_code() {
    let content = "behavior login \"Login\" {\n  types [ghost]\n}\n";
    let mut squatted = diagnostic_at("E001", "an extension's finding", 2, 10, 15);
    squatted.origin = Some("@acme/squat".to_string());
    // The same extension's own owner would be described: W004 is testing's.
    let mut owned = diagnostic_at("W004", "untested", 2, 10, 15);
    owned.origin = Some("@specforge/testing".to_string());

    let index = LineIndex::new(content);
    let md = diagnostic_hover(&[squatted], &index, Position::new(1, 11)).unwrap();
    assert_eq!(
        md,
        "**E001** \u{b7} reported by '@acme/squat'\n\nan extension's finding"
    );
    assert!(!md.contains("Parse error"), "{md}");
    assert!(!md.contains("Documentation"), "{md}");

    let md = diagnostic_hover(&[owned], &index, Position::new(1, 11)).unwrap();
    assert!(
        md.starts_with("**W004** \u{b7} Untested testable entity"),
        "{md}"
    );
    assert!(md.contains("[Documentation]"), "{md}");
}

#[spec(
    behavior = "hover_diagnostic",
    verify = "hovering a diagnostic shows its catalogued title and explanation"
)]
fn hovering_a_diagnostic_shows_the_catalogue_entry() {
    let content = "behavior login \"Login\" {\n  types [ghost]\n}\n";
    // `ghost`: line 2, columns 10..15 (1-based).
    let diagnostics = [diagnostic_at(
        "E003",
        "unresolved reference 'ghost'",
        2,
        10,
        15,
    )];

    let md =
        diagnostic_hover(&diagnostics, &LineIndex::new(content), Position::new(1, 11)).unwrap();
    assert_eq!(
        md,
        "**E003** · Unresolved reference\n\nunresolved reference 'ghost'\n\n\
         A reference field names an entity ID that doesn't resolve to any declared entity. \
         Fix the typo or add the missing entity; a `did you mean` suggestion is included when \
         a close match exists.\n\n\
         [Documentation](https://github.com/leaderiop/SpecForge/blob/main/docs/diagnostics.md#e003)"
    );
    // Outside the range, nothing.
    assert_eq!(
        diagnostic_hover(&diagnostics, &LineIndex::new(content), Position::new(0, 3)),
        None
    );
}

#[spec(
    behavior = "hover_diagnostic",
    verify = "an uncatalogued diagnostic's hover shows its code and message only"
)]
fn an_uncatalogued_diagnostic_hover_shows_code_and_message() {
    let content = "behavior login \"Login\" {\n  types [ghost]\n}\n";
    // E901 is a third-party extension's code.
    let diagnostics = [diagnostic_at("E901", "acme says no", 2, 10, 15)];
    assert_eq!(
        diagnostic_hover(&diagnostics, &LineIndex::new(content), Position::new(1, 12)).as_deref(),
        Some("**E901**\n\nacme says no")
    );
}

// -- field values -------------------------------------------------------------

/// The `contract` line of the hover of a behavior whose contract is `value`.
fn contract_line(value: &str) -> String {
    let served = plain(&format!(
        "behavior long_one \"Long\" {{\n  contract \"{value}\"\n}}\n"
    ));
    let text = served.hover_on("main.spec", "long_one").unwrap();
    text.lines()
        .find(|l| l.starts_with("- `contract`"))
        .unwrap_or_else(|| panic!("no contract line:\n{text}"))
        .to_string()
}

#[spec(
    behavior = "provide_extension_entity_hover",
    verify = "a long field value is cut at a character boundary"
)]
fn a_long_field_is_cut_at_a_character_boundary() {
    let value = format!("{}é{}", "a".repeat(119), "b".repeat(20));
    assert_eq!(
        contract_line(&value),
        format!("- `contract` = \"{}…\"", "a".repeat(119))
    );
    // ASCII is cut at 120 bytes, as before.
    let ascii = "x".repeat(200);
    assert_eq!(
        contract_line(&ascii),
        format!("- `contract` = \"{}…\"", "x".repeat(120))
    );
}
