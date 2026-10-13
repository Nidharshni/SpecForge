//! The inference guide (plan 04, ADR 0015 "Prompt read views"): what an
//! agent is told to look for to infer a project's entities from code.

use serde_json::json;
use specforge_extension_sdk::prelude::*;
use specforge_installed::Installed;
use specforge_ops::infer::{InferencePlanRequest, guide, inference_plan, kind_guide};
use specforge_ops::view::ProjectView;
use specforge_project::Environment;
use specforge_project::coverage::RecordedCoverage;
use specforge_protocol_types::ExtensionDeclaration;
use specforge_registry::build_registries;
use specforge_test::prelude::*;

use crate::view_support::{Project, config_read, loaded};

/// A declaration of `name` that declares each of `kinds` (a keyword and an
/// inference guide).
fn extension(name: &str, kinds: &[(&str, Option<&str>)]) -> ExtensionDeclaration {
    let mut c = ContributionsBuilder::new(ExtensionMeta::new(name, "1.0.0"));
    for (kind, guide) in kinds {
        c.kind(kind, |k| {
            k.description(&format!("A {kind}"));
            if let Some(guide) = guide {
                k.inference_guide(guide);
            }
        });
    }
    c.declaration()
}

/// The project whose source is `source`, built over `declarations`.
fn project_of(source: &str, declarations: Vec<ExtensionDeclaration>) -> Project {
    Project::new(source, build_registries(declarations))
}

#[specforge_test(
    behavior = "compute_inference_guide",
    verify = "a kind's guide is its extension's guide, then the project's guide for the kind under Project-specific"
)]
fn a_guide_is_the_extension_s_then_the_project_s() {
    let project = Project::assembled("", |root| {
        Environment::assemble(
            config_read(
                root,
                json!({"inference": {
                    "global": "This is a Rust project",
                    "behavior": "In our codebase, behaviors are in use_cases/",
                }}),
            ),
            Installed::unread(root),
            loaded(vec![extension(
                "@t/soft",
                &[("behavior", Some("Look for public functions"))],
            )]),
        )
    });
    let view = project.view();

    let guide = guide(&view);
    assert_eq!(
        guide.kinds[0].guide,
        "Look for public functions\n\n**Project-specific:**\nIn our codebase, behaviors are in use_cases/"
    );
    assert_eq!(guide.conventions, Some("This is a Rust project"));
    assert_eq!(
        kind_guide(&view, "behavior").unwrap().guide,
        guide.kinds[0].guide
    );
}

#[specforge_test(
    behavior = "compute_inference_guide",
    verify = "a kind only the project guides has the project's guide alone, and one nobody guides an empty guide"
)]
fn a_kind_without_an_extension_guide_has_the_project_s_alone() {
    let project = Project::assembled("", |root| {
        Environment::assemble(
            config_read(
                root,
                json!({"inference": {"behavior": "Look in use_cases/"}}),
            ),
            Installed::unread(root),
            loaded(vec![extension(
                "@t/soft",
                &[("behavior", None), ("event", None)],
            )]),
        )
    });
    let view = project.view();

    let guide = guide(&view);
    assert_eq!(guide.kind("behavior").unwrap().guide, "Look in use_cases/");
    assert_eq!(guide.kind("event").unwrap().guide, "");
    assert_eq!(guide.conventions, None);
}

#[specforge_test(
    behavior = "compute_inference_guide",
    verify = "a kind guide lists every field registered on the kind, by name, with its type"
)]
fn a_kind_guide_lists_every_registered_field() {
    let mut c = ContributionsBuilder::new(ExtensionMeta::new("@t/soft", "1.0.0"));
    c.kind("behavior", |k| {
        k.field("description", |f| {
            f.field_type(FieldType::String).description("A description");
        });
    });
    // The extension's shared field is registered on the kind too.
    c.shared_field("tags", |f| {
        f.field_type(FieldType::StringList);
    });
    let project = project_of("", vec![c.declaration()]);
    let guide = kind_guide(&project.view(), "behavior").unwrap();

    let fields: Vec<(&str, &str)> = guide
        .fields
        .iter()
        .map(|f| (f.name(), f.field_type().as_str()))
        .collect();
    assert_eq!(fields, [("description", "string"), ("tags", "string_list")]);
    assert_eq!(
        serde_json::to_value(guide.document()).unwrap()["fields"][0]["description"],
        "A description"
    );
}

#[specforge_test(
    behavior = "compute_inference_guide",
    verify = "the example entity writes its required fields and three optional ones, each the way its type is written"
)]
fn the_example_writes_each_field_by_its_type() {
    let mut c = ContributionsBuilder::new(ExtensionMeta::new("@t/soft", "1.0.0"));
    c.kind("widget", |k| {
        k.field("owner", |f| {
            f.field_type(FieldType::Reference)
                .target_kind("widget")
                .required();
        });
        k.field("steps", |f| {
            f.field_type(FieldType::StringList).required();
        });
        k.field("active", |f| {
            f.field_type(FieldType::Bool);
        });
        k.field("count", |f| {
            f.field_type(FieldType::Integer);
        });
        k.field("methods", |f| {
            f.field_type(FieldType::Block);
        });
        k.field("refs_to", |f| {
            f.field_type(FieldType::ReferenceList).target_kind("widget");
        });
    });
    let project = project_of("", vec![c.declaration()]);
    let example = kind_guide(&project.view(), "widget").unwrap().example();

    // Required fields first (in name order), then the first three optional
    // ones by name: active, count, methods; refs_to is the fourth.
    assert_eq!(
        example,
        "widget example_widget \"Example Title\" {\n  owner ref_id\n  steps [\"item1\", \"item2\"]\n  active true\n  count 0\n  methods {\n  }\n}"
    );

    // And the remaining types.
    let mut c = ContributionsBuilder::new(ExtensionMeta::new("@t/soft", "1.0.0"));
    c.kind("gadget", |k| {
        k.field("level", |f| {
            f.vocabulary(&Vocabulary::new(&["low", "high"]));
        });
        k.field("note", |f| {
            f.field_type(FieldType::String);
        });
        k.field("refs_to", |f| {
            f.field_type(FieldType::ReferenceList).target_kind("gadget");
        });
    });
    let project = project_of("", vec![c.declaration()]);
    assert_eq!(
        kind_guide(&project.view(), "gadget").unwrap().example(),
        "gadget example_gadget \"Example Title\" {\n  level low\n  note \"...\"\n  refs_to [ref_1, ref_2]\n}"
    );
}

#[specforge_test(
    behavior = "compute_inference_guide",
    verify = "a kind declared by two extensions is guided by the one that registered it"
)]
fn a_kind_declared_twice_is_guided_by_the_first() {
    let project = project_of(
        "",
        vec![
            extension("@t/first", &[("behavior", Some("first"))]),
            extension("@t/second", &[("behavior", Some("second"))]),
        ],
    );
    let view = project.view();

    let kind = kind_guide(&view, "behavior").unwrap();
    assert_eq!(kind.guide, "first");
    assert_eq!(kind.extension, "@t/first");
    let guide = guide(&view);
    assert_eq!(guide.kinds.len(), 1);
    assert_eq!(guide.kinds[0].extension, "@t/first");
}

#[specforge_test(
    behavior = "compute_inference_guide",
    verify = "the guide's spec directory is the project's spec root, relative to its root"
)]
fn the_spec_directory_is_the_spec_root_relative_to_the_root() {
    let rooted = |config: serde_json::Value| {
        Project::assembled("", |root| {
            Environment::assemble(
                config_read(root, config),
                Installed::unread(root),
                loaded(Vec::new()),
            )
        })
    };
    let project = rooted(json!({"spec_root": "model"}));
    assert_eq!(guide(&project.view()).spec_directory, "model/");
    let project = rooted(json!({"spec_root": "docs/spec"}));
    assert_eq!(guide(&project.view()).spec_directory, "docs/spec/");
    let project = rooted(json!({}));
    assert_eq!(guide(&project.view()).spec_directory, "./");

    // A view without a root has no relative directory.
    let project = rooted(json!({"spec_root": "model"}));
    let recorded = RecordedCoverage::over(&project.graph, &project.env);
    let rootless = ProjectView::new(&project.graph, &project.env, None, &recorded);
    assert_eq!(guide(&rootless).spec_directory, "./");
}

#[specforge_test(
    behavior = "compute_inference_guide",
    verify = "the guide lists each loaded extension, every declared kind in declaration order, and the entities of every written kind"
)]
fn the_guide_lists_extensions_kinds_and_entities() {
    let project = project_of(
        "behavior zeta \"Z\" {\n}\nbehavior alpha \"A\" {\n}\nwidget w \"W\" {\n}\n",
        vec![
            extension("@t/soft", &[("behavior", None), ("event", None)]),
            extension("@t/more", &[("type", None)]),
        ],
    );
    let view = project.view();
    let guide = guide(&view);

    assert_eq!(guide.extensions, ["@t/soft", "@t/more"]);
    let kinds: Vec<&str> = guide.kinds.iter().map(|k| k.keyword).collect();
    assert_eq!(kinds, ["behavior", "event", "type"]);
    // `widget` is no declared kind (E024's), and still counted.
    assert_eq!(
        guide.entities_by_kind.into_iter().collect::<Vec<_>>(),
        [("behavior", 2), ("widget", 1)]
    );
    assert_eq!(guide.kinds[0].existing, ["alpha", "zeta"]);
    let json = serde_json::to_value(kind_guide(&view, "behavior").unwrap().document()).unwrap();
    assert_eq!(json["existing_entity_ids"], json!(["alpha", "zeta"]));
}

#[specforge_test(
    behavior = "compute_inference_guide",
    verify = "a guide field lists an enum's values and what an absent value counts as"
)]
fn a_guide_field_lists_an_enums_values_and_its_default() {
    let mut c = ContributionsBuilder::new(ExtensionMeta::new("@t/soft", "1.0.0"));
    c.kind("gadget", |k| {
        k.field("level", |f| {
            f.vocabulary(&Vocabulary::new(&["low", "high"]).absent_as("high"));
        });
        k.field("note", |f| {
            f.field_type(FieldType::String);
        });
    });
    let project = project_of("", vec![c.declaration()]);
    let guide = kind_guide(&project.view(), "gadget").unwrap();

    let document = serde_json::to_value(guide.document()).unwrap();
    let fields = document["fields"].as_array().unwrap();
    let field = |name: &str| fields.iter().find(|f| f["name"] == name).unwrap();
    assert_eq!(field("level")["values"], json!(["low", "high"]));
    assert_eq!(field("level")["default"], json!("high"));
    assert!(field("note").get("values").is_none());
    assert!(field("note").get("default").is_none());
    assert!(
        guide.example().contains("level high"),
        "{}",
        guide.example()
    );
}

#[specforge_test(
    behavior = "read_views_over_the_project_view",
    verify = "an argument naming an undeclared kind is refused with unknown_kind naming the closest declared kind"
)]
fn kind_guide_refuses_an_undeclared_kind() {
    let project = project_of("", vec![extension("@t/soft", &[("behavior", None)])]);
    let error = kind_guide(&project.view(), "behaviour").unwrap_err();
    assert_eq!(error.error.code, "unknown_kind");
    assert_eq!(
        error.error.suggestion.as_deref(),
        Some("did you mean 'behavior'?")
    );
}

/// A declaration of `name`: each of `kinds` (a keyword) with reference-list
/// fields to the kinds in its entry.
fn linked(name: &str, kinds: &[(&str, &[&str])]) -> ExtensionDeclaration {
    let mut c = ContributionsBuilder::new(ExtensionMeta::new(name, "1.0.0"));
    for (kind, targets) in kinds {
        c.kind(kind, |k| {
            for target in *targets {
                k.field(&format!("{target}s"), |f| {
                    f.field_type(FieldType::ReferenceList).target_kind(target);
                });
            }
        });
    }
    c.declaration()
}

fn plan_kinds(project: &Project) -> Vec<String> {
    inference_plan(&project.view(), &InferencePlanRequest::default())
        .unwrap()
        .kind_priorities
        .into_iter()
        .map(|priority| priority.kind)
        .collect()
}

#[specforge_test(
    behavior = "provide_infer_plan_scope",
    verify = "plan lists kinds with no entities first, then each kind after the kinds it references"
)]
fn kinds_are_ordered_by_what_they_reference() {
    // behavior -> event, type; event -> type; one behavior is written.
    let project = project_of(
        "behavior b \"B\" {\n}\n",
        vec![linked(
            "@t/soft",
            &[
                ("behavior", &["event", "type"]),
                ("event", &["type"]),
                ("type", &[]),
                ("feature", &[]),
            ],
        )],
    );
    assert_eq!(
        plan_kinds(&project),
        ["type", "event", "feature", "behavior"]
    );

    // A kind referencing itself waits for nothing; the cycle a -> b -> a
    // falls back to declaration order.
    let cycle = project_of(
        "",
        vec![linked(
            "@t/soft",
            &[("a", &["b"]), ("b", &["a"]), ("c", &["c"])],
        )],
    );
    assert_eq!(plan_kinds(&cycle), ["c", "a", "b"]);
}

/// A project with an analyzer for `.rs` files, the sources `files` under
/// `src/`, and `manifest` as `specforge-infer.json` when given.
fn sources(files: &[String], manifest: Option<serde_json::Value>) -> Project {
    let mut c = ContributionsBuilder::new(ExtensionMeta::new("@t/rust", "1.0.0"));
    c.analyzer("rust", |a| {
        a.file_extensions(&[".rs"]).scan(|_| ScanResponse {
            items: Vec::new(),
            language: None,
        });
    });
    let project = project_of("", vec![c.declaration()]);
    std::fs::create_dir_all(project.dir.path().join("src")).unwrap();
    for file in files {
        std::fs::write(project.dir.path().join(file), "fn stub() {}\n").unwrap();
    }
    if let Some(manifest) = manifest {
        std::fs::write(
            project.dir.path().join("specforge-infer.json"),
            manifest.to_string(),
        )
        .unwrap();
    }
    project
}

#[specforge_test(
    behavior = "provide_infer_plan_scope",
    verify = "plan excludes already-analyzed files"
)]
fn the_plan_excludes_analyzed_files() {
    let project = sources(
        &["src/a.rs".to_string(), "src/b.rs".to_string()],
        Some(json!({
            "version": 1,
            "source_roots": ["src"],
            "source_index": [{
                "path": "src/a.rs",
                "content_hash": "h",
                "entities_produced": ["e"],
                "analyzed_at": "2026-10-01T00:00:00Z",
            }],
        })),
    );
    let plan = inference_plan(&project.view(), &InferencePlanRequest::default()).unwrap();
    assert_eq!(plan.unanalyzed.files, ["src/b.rs"]);
    assert_eq!(plan.progress.summary.files_analyzed, 1);
}

#[specforge_test(
    behavior = "provide_infer_plan_scope",
    verify = "plan's target directory defaults to the project's spec root"
)]
fn the_plan_targets_the_spec_root_unless_told() {
    let project = Project::assembled("", |root| {
        Environment::assemble(
            config_read(root, json!({"spec_root": "model"})),
            Installed::unread(root),
            loaded(Vec::new()),
        )
    });

    let default = inference_plan(&project.view(), &InferencePlanRequest::default()).unwrap();
    assert_eq!(default.target_spec_directory, "model/");
    let told = inference_plan(
        &project.view(),
        &InferencePlanRequest {
            target_spec_directory: Some("specs/"),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(told.target_spec_directory, "specs/");
}

#[specforge_test(
    behavior = "provide_infer_plan_scope",
    verify = "plan pages the file lists 50 at a time from the cursor"
)]
fn the_plan_pages_its_file_lists() {
    let files: Vec<String> = (0..60).map(|i| format!("src/mod_{i:02}.rs")).collect();
    let project = sources(&files, None);

    let first = inference_plan(&project.view(), &InferencePlanRequest::default()).unwrap();
    assert_eq!(first.unanalyzed.files.len(), 50);
    assert_eq!(
        (first.unanalyzed.total, first.unanalyzed.remaining),
        (60, 10)
    );
    assert_eq!(first.next_cursor, Some(50));

    let second = inference_plan(
        &project.view(),
        &InferencePlanRequest {
            cursor: 50,
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(second.unanalyzed.files.len(), 10);
    assert_eq!(second.unanalyzed.files[0], "src/mod_50.rs");
    assert_eq!(second.next_cursor, None);
}

#[specforge_test(
    invariant = "mcp_structured_error_responses",
    verify = "an operation states the request field each refusal is about, and a refusal of the project or the environment is about none"
)]
fn an_unknown_kind_guide_is_about_its_kind() {
    use specforge_ops::infer::KindGuideField;
    let project = project_of("", vec![extension("@t/soft", &[("behavior", None)])]);
    let refusal = kind_guide(&project.view(), "behaviour").unwrap_err();
    assert_eq!(refusal.subject, Some(KindGuideField::Kind));
}
