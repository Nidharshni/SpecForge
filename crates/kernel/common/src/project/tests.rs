use super::*;
use specforge_test_macros::test as specforge_test;

#[specforge_test(
    behavior = "load_extension_manifests",
    verify = "E069 names why specforge.json can't be used"
)]
fn parsing_a_config_text_is_reading_it_from_its_file() {
    let texts = [
        r#"{"name": "p", "spec_root": "spec", "exclude": ["gen"], "extensions": ["@a/b"],
                "inference": {"global": "g", "density_threshold": 0.5}}"#,
        r#"{"name": 5}"#,
        r#"{ "extensions": ["#,
        "[1]",
    ];
    for text in texts {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("specforge.json");
        std::fs::write(&path, text).unwrap();
        let read = read_project_config(dir.path());
        let parsed = parse_project_config(&path, text);
        assert!(parsed.found, "{text}");
        assert_eq!(format!("{read:?}"), format!("{parsed:?}"), "{text}");
    }
}

#[test]
fn a_config_is_parsed_from_what_its_read_returned() {
    let path = Path::new("/p/specforge.json");
    let missing = project_config_from(path, Err(std::io::ErrorKind::NotFound.into()));
    assert!(!missing.found && missing.problems.is_empty());
    let denied = project_config_from(path, Err(std::io::ErrorKind::PermissionDenied.into()));
    assert!(matches!(
        denied.problems.as_slice(),
        [ConfigProblem::Unreadable { .. }]
    ));
    let binary = project_config_from(path, Ok(vec![0xff]));
    assert!(matches!(
        binary.problems.as_slice(),
        [ConfigProblem::Unreadable { error, .. }] if error == "stream did not contain valid UTF-8"
    ));
    let text = "{\"name\":\"x\"}";
    let ok = project_config_from(path, Ok(text.as_bytes().to_vec()));
    assert_eq!(
        format!("{ok:?}"),
        format!("{:?}", parse_project_config(path, text))
    );
}

#[test]
fn sources_are_at_the_spec_root_or_the_root() {
    let root = Path::new("/p");
    let unset = ProjectConfig::default();
    let set = ProjectConfig {
        spec_root: Some("specs".into()),
        ..ProjectConfig::default()
    };

    assert_eq!(unset.sources(root).dir(), root);
    assert_eq!(set.sources(root).dir(), root.join("specs"));
}

#[test]
fn sources_keep_only_the_sources() {
    let dir = tempfile::TempDir::new().unwrap();
    let root = dir.path();
    for file in [
        "specs/a.spec",
        "specs/sub/b.spec",
        "specs/drafts/d.spec",
        "specs/target/t.spec",
        "specs/notes.md",
        "fixtures/fx.spec",
    ] {
        std::fs::create_dir_all(root.join(file).parent().unwrap()).unwrap();
        std::fs::write(root.join(file), "").unwrap();
    }
    let config = ProjectConfig {
        spec_root: Some("specs".into()),
        exclude: Exclude::parse(["drafts"]).unwrap(),
        ..ProjectConfig::default()
    };
    let sources = config.sources(root);

    assert_eq!(
        sources.files(),
        [root.join("specs/a.spec"), root.join("specs/sub/b.spec")]
    );
    assert!(!sources.holds("drafts/d.spec"));
    assert!(!sources.holds("target/t.spec"));
    assert!(sources.holds("a.spec"));
    assert!(!sources.holds("../fixtures/fx.spec"));
}

#[test]
fn an_entry_names_an_extension_or_a_wasm_file() {
    use ExtensionEntry::{File, Named};
    let cases = [
        ("@specforge/product", Named("@specforge/product")),
        (" @acme/foo@1.2.0 ", Named("@acme/foo")),
        ("greet", Named("greet")),
        (
            "greet.wasm",
            File {
                name: None,
                path: "greet.wasm",
            },
        ),
        (
            "./ext/greet.wasm",
            File {
                name: None,
                path: "./ext/greet.wasm",
            },
        ),
        (
            "/a@b/greet.wasm",
            File {
                name: None,
                path: "/a@b/greet.wasm",
            },
        ),
        (
            "@sdk/greet = ext/greet.wasm",
            File {
                name: Some("@sdk/greet"),
                path: "ext/greet.wasm",
            },
        ),
        // Plan 12 §2.2, as P4 reads them today.
        ("@acme/tool@", Named("@acme/tool")), // I2
        // The requirement is split off at the last inner `@`, whatever it holds.
        ("@acme/tool@1.0.0/x", Named("@acme/tool")), // I7
        ("foo@/bar", Named("foo")),                  // I9
        // A name that is no package name is still a named entry: the load
        // refuses it (E072), the reading does not.
        ("@acme/..", Named("@acme/..")),                   // I12
        ("../../../outside1", Named("../../../outside1")), // I23
    ];
    for (entry, expected) in cases {
        assert_eq!(ExtensionEntry::parse(entry), expected, "{entry}");
    }
}

#[test]
fn a_named_entry_names_a_package_or_says_why_not() {
    let name = |entry: &str| ExtensionEntry::parse(entry).package_name();
    assert_eq!(
        name("@acme/tool@1.2.0").unwrap().unwrap().as_str(),
        "@acme/tool"
    );
    assert_eq!(name("greet").unwrap().unwrap().as_str(), "greet");
    for refused in ["../../../outside1", "@acme/..", "Bad Name"] {
        assert!(name(refused).unwrap().is_err(), "{refused}");
    }
    // A file entry names a file, not a package.
    assert!(name("ext/greet.wasm").is_none());
}

#[test]
fn a_file_entry_resolves_against_the_root_and_names_what_it_writes() {
    let root = Path::new("/p");
    let bare = ExtensionEntry::parse("ext/greet.wasm");
    assert_eq!(bare.file(root), Some(PathBuf::from("/p/ext/greet.wasm")));
    assert_eq!(bare.name(), None);

    let named = ExtensionEntry::parse("@sdk/greet=/abs/greet.wasm");
    assert_eq!(named.file(root), Some(PathBuf::from("/abs/greet.wasm")));
    assert_eq!(named.name(), Some("@sdk/greet"));

    assert_eq!(ExtensionEntry::parse("@acme/foo@1.0.0").file(root), None);
}

/// `specforge.json` in a fresh temp directory, written as `text`.
fn config_dir(text: &[u8]) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("specforge.json"), text).unwrap();
    dir
}

#[test]
fn a_missing_config_is_the_default_with_no_problem() {
    let dir = tempfile::tempdir().unwrap();
    let read = read_project_config(dir.path());
    assert!(!read.found);
    assert!(read.problems.is_empty());
    assert!(read.config.raw.is_none());
    assert!(read.config.extensions.is_empty());
}

#[specforge_test_macros::test(
    behavior = "load_extension_manifests",
    verify = "E069 names why specforge.json can't be used"
)]
fn an_unparsable_config_names_the_line_and_column() {
    let dir = config_dir(br#"{ "extensions": ["@specforge/product",  }"#);
    let path = dir.path().join("specforge.json");

    let read = read_project_config(dir.path());

    assert!(read.found);
    assert!(read.config.extensions.is_empty());
    assert!(read.config.raw.is_none());
    let [problem] = read.problems.as_slice() else {
        panic!("{:?}", read.problems);
    };
    assert!(
        matches!(problem, ConfigProblem::NotJson { .. }),
        "{problem:?}"
    );
    assert_eq!(
        problem.to_string(),
        format!(
            "{} is not valid JSON: expected value at line 1 column 41",
            path.display()
        )
    );
    assert!(problem.loads_nothing() && problem.blocks_edits());

    // Not readable (not UTF-8): the file is there, and nothing in it is used.
    let dir = config_dir(&[0xff, 0xfe, 0x7b]);
    let read = read_project_config(dir.path());
    assert!(read.found);
    let [problem] = read.problems.as_slice() else {
        panic!("{:?}", read.problems);
    };
    assert!(
        matches!(problem, ConfigProblem::Unreadable { .. }),
        "{problem:?}"
    );
    assert!(
        problem.to_string().starts_with(&format!(
            "failed to read {}: ",
            dir.path().join("specforge.json").display()
        )),
        "{problem}"
    );
    assert!(problem.loads_nothing());
}

#[test]
fn a_config_that_is_not_an_object_is_a_problem() {
    let dir = config_dir(b"[1,2]");
    let read = read_project_config(dir.path());
    assert_eq!(
        read.problems,
        [ConfigProblem::NotAnObject {
            path: dir.path().join("specforge.json")
        }]
    );
    assert_eq!(
        read.problems[0].to_string(),
        format!(
            "{} must be a JSON object",
            dir.path().join("specforge.json").display()
        )
    );
    assert!(read.config.raw.is_none());
    assert!(read.problems[0].loads_nothing());
}

#[test]
fn a_non_array_extensions_keeps_the_other_keys() {
    let dir = config_dir(br#"{"name":"p","spec_root":"spec","extensions":"x"}"#);
    let read = read_project_config(dir.path());
    assert_eq!(
        read.problems,
        [ConfigProblem::WrongType {
            path: dir.path().join("specforge.json"),
            key: "extensions",
            expected: "an array",
        }]
    );
    assert!(read.problems[0].loads_nothing());
    assert!(read.problems[0].blocks_edits());
    assert_eq!(
        read.problems[0].to_string(),
        format!(
            "\"extensions\" in {} must be an array",
            dir.path().join("specforge.json").display()
        )
    );
    assert_eq!(read.config.spec_root.as_deref(), Some("spec"));
    assert_eq!(read.config.name.as_deref(), Some("p"));
    assert!(read.config.extensions.is_empty());
}

#[specforge_test_macros::test(
    behavior = "load_extension_manifests",
    verify = "E069 names a mistyped key or a non-string item, and the rest of specforge.json is used"
)]
fn a_mistyped_key_is_a_problem_and_its_default_is_used() {
    let dir = config_dir(br#"{"spec_root": 5, "extensions": [], "name": "p", "version": null}"#);
    let path = dir.path().join("specforge.json");

    let read = read_project_config(dir.path());

    // In file order.
    assert_eq!(
        read.problems,
        [
            ConfigProblem::WrongType {
                path: path.clone(),
                key: "spec_root",
                expected: "a string",
            },
            ConfigProblem::WrongType {
                path: path.clone(),
                key: "version",
                expected: "a string",
            },
        ]
    );
    assert!(read.problems.iter().all(|p| !p.loads_nothing()));
    assert!(read.problems.iter().all(|p| !p.blocks_edits()));
    assert_eq!(read.config.spec_root, None);
    assert_eq!(read.config.version, None);
    assert_eq!(read.config.name.as_deref(), Some("p"));
}

#[specforge_test_macros::test(
    behavior = "load_extension_manifests",
    verify = "E069 names a mistyped key or a non-string item, and the rest of specforge.json is used"
)]
fn a_non_string_item_is_a_problem_and_the_others_are_kept() {
    let dir = config_dir(br#"{"extensions": ["@specforge/product", 42], "exclude": ["a", {}]}"#);
    let path = dir.path().join("specforge.json");

    let read = read_project_config(dir.path());

    assert_eq!(
        read.problems,
        [
            ConfigProblem::ItemNotAString {
                path: path.clone(),
                key: "extensions",
                index: 1,
                json: "42".into(),
            },
            ConfigProblem::ItemNotAString {
                path: path.clone(),
                key: "exclude",
                index: 1,
                json: "{}".into(),
            },
        ]
    );
    assert_eq!(
        read.problems[0].to_string(),
        format!(
            "\"extensions\"[1] in {} must be a string, found 42",
            path.display()
        )
    );
    assert!(read.problems.iter().all(|p| !p.blocks_edits()));
    assert_eq!(read.config.extensions, ["@specforge/product"]);
    let written: Vec<&str> = read
        .config
        .exclude
        .entries()
        .iter()
        .map(ExcludeEntry::written)
        .collect();
    assert_eq!(written, ["a"]);
}

#[specforge_test(
    behavior = "resolve_use_imports",
    verify = "an exclude entry that can match no source is E069 and ignored"
)]
fn an_exclude_entry_that_can_match_no_source_is_e069_and_ignored() {
    let dir = tempfile::TempDir::new().unwrap();
    let root = dir.path();
    for file in ["a.spec", "drafts/d.spec"] {
        std::fs::create_dir_all(root.join(file).parent().unwrap()).unwrap();
        std::fs::write(root.join(file), "").unwrap();
    }
    let path = root.join("specforge.json");

    let read = parse_project_config(
        &path,
        r#"{"exclude": ["", "!drafts", "./drafts", "a/../b", "a[b", "drafts"]}"#,
    );

    let shown = |entry: &str, index: usize, why: &str| {
        format!(
            "\"exclude\"[{index}] in {}, \"{entry}\", {why}",
            path.display()
        )
    };
    let dots = "has a \".\" or \"..\" name, which no path under the spec root has";
    let problems: Vec<String> = read.problems.iter().map(ToString::to_string).collect();
    assert_eq!(
        problems,
        [
            shown("", 0, "matches nothing: it has no name"),
            shown(
                "!drafts",
                1,
                "starts with \"!\", and exclude has no negation"
            ),
            shown("./drafts", 2, dots),
            shown("a/../b", 3, dots),
            shown(
                "a[b",
                4,
                "is not a glob pattern (Pattern syntax error near position 1: invalid range pattern)"
            ),
        ]
    );
    assert_eq!(read.config.sources(root).files(), [root.join("a.spec")]);
}

#[test]
fn keys_are_read_in_the_order_the_file_writes_them() {
    assert_eq!(
        keys_in_file_order(r#"{"z": 1, "a": {"nested": 2}, "m": [], "a": 3}"#),
        ["z", "a", "m"]
    );
    assert!(keys_in_file_order("[1]").is_empty());
}

#[specforge_test(
    behavior = "management_operations_over_the_project_view",
    verify = "a config problem that leaves the sources or the extensions unknown blocks a rewrite"
)]
fn which_problems_block_a_rewrite() {
    let path = PathBuf::from("specforge.json");
    let wrong_type = |key| ConfigProblem::WrongType {
        path: path.clone(),
        key,
        expected: "a string",
    };
    let not_a_string = |key| ConfigProblem::ItemNotAString {
        path: path.clone(),
        key,
        index: 0,
        json: "1".into(),
    };
    let table = [
        (
            ConfigProblem::Unreadable {
                path: path.clone(),
                error: "denied".into(),
            },
            true,
        ),
        (
            ConfigProblem::NotJson {
                path: path.clone(),
                error: "eof".into(),
            },
            true,
        ),
        (ConfigProblem::NotAnObject { path: path.clone() }, true),
        (wrong_type("name"), false),
        (wrong_type("version"), false),
        (wrong_type("spec_root"), true),
        (wrong_type("exclude"), true),
        (wrong_type("extensions"), true),
        (not_a_string("extensions"), false),
        (not_a_string("exclude"), false),
        (
            ConfigProblem::ExcludeEntry {
                path: path.clone(),
                index: 0,
                entry: "[".into(),
                error: ExcludeError::Empty,
            },
            false,
        ),
    ];
    for (problem, blocks) in table {
        assert_eq!(problem.blocks_rewrites(), blocks, "{problem:?}");
        // An edit goes around fewer problems than a rewrite does.
        assert!(
            !problem.blocks_edits() || problem.blocks_rewrites(),
            "{problem:?}"
        );
    }
}

#[specforge_test(
    behavior = "load_extension_manifests",
    verify = "E069 names why specforge.json can't be used"
)]
fn e069_text_is_the_problem_s() {
    let path = PathBuf::from("specforge.json");
    let not_json = ConfigProblem::NotJson {
        path: path.clone(),
        error: "eof".into(),
    };
    let name = ConfigProblem::WrongType {
        path,
        key: "name",
        expected: "a string",
    };
    let (cannot, goes_around) = (not_json.diagnostic(), name.diagnostic());
    assert!(cannot.is(codes::E069) && goes_around.is(codes::E069));
    assert_eq!(cannot.severity, crate::Severity::Error);
    assert_eq!(goes_around.severity, crate::Severity::Error);
    assert_eq!(
        cannot.message,
        format!("specforge.json can't be used: {not_json}; no extension is loaded")
    );
    assert_eq!(
        goes_around.message,
        format!("specforge.json: {name}; it is ignored")
    );
    assert_eq!(
        cannot.suggestion.as_deref(),
        Some("fix specforge.json; `specforge explain E069` says what it must be")
    );
}

#[test]
fn a_named_entry_names_its_extension_and_a_file_entry_none() {
    let names = |entry: &str, name: &str| ExtensionEntry::parse(entry).names(name);
    assert!(names("@acme/foo", "@acme/foo"));
    assert!(names("@acme/foo@1.2.0", "@acme/foo"));
    assert!(names(" @specforge/formal@latest ", "@specforge/formal"));
    assert!(names("plain@0.0.0", "plain"));
    assert!(!names("@acme/foobar", "@acme/foo"));
    assert!(
        !names("@sdk/greet=./a.wasm", "@sdk/greet"),
        "a file entry names nothing by its text"
    );
    assert!(!names("./ext/local.wasm", "./ext/local.wasm"));
    assert!(!names("./ext@2/local.wasm", "./ext"));
}
