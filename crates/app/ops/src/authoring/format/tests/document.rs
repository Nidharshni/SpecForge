use super::*;

#[specforge_test(
    behavior = "load_format_config",
    verify = "a file's configuration does not depend on where format runs"
)]
fn each_file_uses_its_nearest_config() {
    let two = "behavior login \"Login\" {\n  contract \"The system MUST log in\"\n}\n";
    let dir = project_with(
        "{}",
        &[
            ("spec/sub/.specforgefmt.toml", "indent_width = 4\n"),
            ("spec/sub/a.spec", FOUR),
            ("spec/b.spec", two),
        ],
    );

    // From the root, each file gets its nearest configuration: both are
    // canonical.
    assert_eq!(changed(dir.path()), Vec::<String>::new());
    // Named from the subdirectory, the same.
    let named = check_paths(dir.path(), &["spec/sub"]);
    assert_eq!((named.checked, named.changes.len()), (1, 0));
}

#[specforge_test(
    behavior = "load_format_config",
    verify = "invalid indent_width produces diagnostic and uses default"
)]
fn an_invalid_config_is_reported_once_per_run() {
    let dir = project_with(
        "{}",
        &[
            (".specforgefmt.toml", "indent_width = 99\n"),
            ("spec/a.spec", MESSY),
            ("spec/b.spec", MESSY),
        ],
    );

    let outcome = run(&request(
        &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
        Mode::Check,
    ));

    assert_eq!(outcome.checked, 2);
    let w141: Vec<_> = outcome
        .diagnostics
        .iter()
        .filter(|d| d.is(codes::W141))
        .collect();
    assert_eq!(w141.len(), 1, "{w141:?}");
}

#[specforge_test(
    behavior = "load_format_config",
    verify = "a file is formatted with the configuration of its own project"
)]
fn a_nested_projects_file_uses_its_own_projects_config() {
    let two = "behavior login \"Login\" {\n  contract \"The system MUST log in\"\n}\n";
    let dir = project_with(
        "{}",
        &[
            (".specforgefmt.toml", "indent_width = 4\n"),
            ("inner/specforge.json", "{}"),
            ("inner/spec/a.spec", two),
        ],
    );
    let file = dir.path().join("inner/spec/a.spec");

    // From the outer project, the inner file keeps its own project's
    // defaults; the outer configuration stops at the inner root.
    let outcome = check_paths(dir.path(), &["inner"]);
    assert_eq!((outcome.checked, outcome.changes.len()), (1, 0));
    // As the editor formats it.
    let doc = document(
        Place::File(&file),
        two,
        None,
        None,
        &crate::authoring::format::Forms::none(),
    );
    assert_eq!(doc.config_source, ConfigSource::Defaults);
    assert!(!doc.changed());
}

/// Two blocks: `foo` (whose fields full formatting aligns) and `bar`,
/// misindented on line 6.
const TWO_BLOCKS: &str = "behavior foo \"Foo\" {\n  contract \"a\"\n  types [x]\n}\n\nbehavior bar \"Bar\" {\n      contract \"b\"\n}\n";
/// A canonical block, then a stray `}}}` on line 5.
const BROKEN: &str = "behavior login \"Login\" {\n  contract \"ok\"\n}\n\n}}}\n";

/// Whether `edits` are in order and none overlaps or touches the next.
fn in_order_without_overlap(edits: &[Edit]) -> bool {
    edits.windows(2).all(|w| w[0].range.end < w[1].range.start)
}

/// The W142 diagnostics of `doc`.
fn kept_regions<'d>(doc: &'d FormattedDocument) -> Vec<&'d Diagnostic> {
    doc.diagnostics
        .iter()
        .filter(|d| d.is(codes::W142))
        .collect()
}

#[specforge_test(
    behavior = "lsp_format_document",
    verify = "formatting request returns TextEdit list"
)]
fn a_misformatted_document_gets_edits() {
    let doc = document(
        Place::Detached,
        MISINDENTED,
        None,
        None,
        &crate::authoring::format::Forms::none(),
    );

    assert!(doc.changed());
    // Only the four extra spaces of line 1, which starts at byte 21.
    assert_eq!(
        doc.edits(),
        [Edit {
            range: 23..27,
            new_text: String::new()
        }]
    );
}

#[specforge_test(
    behavior = "lsp_format_document",
    verify = "applying the formatting edits to the document gives the formatted text byte for byte"
)]
fn edits_rebuild_the_formatted_text() {
    let blank = |range: Range<usize>| Edit {
        range,
        new_text: String::new(),
    };
    let expected: [(&str, Vec<Edit>); 6] = [
        (
            "behavior a \"A\" {\n\n  contract \"x\"\n}\n",
            vec![blank(17..18)],
        ),
        (
            "\n\n\nbehavior a \"A\" {\n  contract \"x\"\n}\n",
            vec![blank(0..3)],
        ),
        (
            "behavior a \"A\" {\n  contract \"x\"\n}\n\n\n",
            vec![blank(34..36)],
        ),
        (
            "behavior a \"A\" {\n  contract \"x\"\n}",
            vec![Edit {
                range: 33..33,
                new_text: "\n".into(),
            }],
        ),
        (
            "behavior a \"A\" {\r\n  contract \"x\"\r\n}\r\n",
            vec![Edit {
                range: 16..37,
                new_text: "\n  contract \"x\"\n}\n".into(),
            }],
        ),
        (
            "behavior a \"A\" {\n      contract \"x\"\n}\n",
            vec![blank(19..23)],
        ),
    ];
    for (text, edits) in expected {
        let doc = document(
            Place::Detached,
            text,
            None,
            None,
            &crate::authoring::format::Forms::none(),
        );
        assert_eq!(doc.edits(), edits, "{text:?}");
        assert_eq!(
            edits::tests::apply(text, &doc.edits()),
            doc.formatted,
            "{text:?}"
        );
    }
    // Two blank lines between blocks: which of the two goes is the diff's choice; the text is not.
    let between =
        "behavior a \"A\" {\n  contract \"x\"\n}\n\n\nbehavior b \"B\" {\n  contract \"y\"\n}\n";
    let doc = document(
        Place::Detached,
        between,
        None,
        None,
        &crate::authoring::format::Forms::none(),
    );
    assert_eq!(edits::tests::apply(between, &doc.edits()), doc.formatted);
    // A multibyte line: offsets count bytes, on char boundaries.
    let text = "behavior foo \"Foo\" {\n      contract \"é\"\n}\n";
    assert_eq!(
        document(
            Place::Detached,
            text,
            None,
            None,
            &crate::authoring::format::Forms::none()
        )
        .edits(),
        [blank(23..27)]
    );
}

#[specforge_test(
    behavior = "lsp_format_document",
    verify = "TextEdit operations in a response do not overlap"
)]
fn edits_do_not_overlap() {
    let text = "behavior foo \"Foo\" {\n      contract \"a\"\n      types [x]\n}\n";
    let edits = document(
        Place::Detached,
        text,
        None,
        None,
        &crate::authoring::format::Forms::none(),
    )
    .edits();

    assert!(!edits.is_empty());
    assert!(in_order_without_overlap(&edits), "{edits:?}");
}

#[specforge_test(
    behavior = "lsp_format_document",
    verify = "parse errors in document trigger format_with_parse_errors delegation"
)]
fn a_parse_error_is_reported_and_kept() {
    let dir = on_disk(&[("spec/x.spec", BROKEN)]);
    let file = dir.path().join("spec/x.spec");

    let doc = document(
        Place::File(&file),
        BROKEN,
        None,
        None,
        &crate::authoring::format::Forms::none(),
    );

    let kept = kept_regions(&doc);
    assert_eq!(kept.len(), 1, "{:?}", doc.diagnostics);
    let span = kept[0].span.as_ref().unwrap();
    assert_eq!(span.file.as_str(), file.display().to_string());
    assert_eq!(span.start_line, 5);
    assert_eq!(doc.formatted.lines().nth(4), Some("}}}"));
    assert!(!doc.complete());
}

#[specforge_test(
    behavior = "lsp_format_document",
    verify = "formats document within 50ms for files under 1000 lines"
)]
fn formats_a_document_within_50ms() {
    let mut source = String::from("use types/core\n\n");
    for i in 0..50 {
        source.push_str(&format!(
            "behavior b{i} \"Behavior {i}\" {{\n  invariants [a, b]\n  types [x]\n  contract \"thing {i}\"\n  verify unit \"test {i}\"\n}}\n\n"
        ));
    }

    let start = std::time::Instant::now();
    document(
        Place::Detached,
        &source,
        None,
        None,
        &crate::authoring::format::Forms::none(),
    )
    .edits();
    let elapsed = start.elapsed();
    assert!(
        elapsed.as_millis() < 50,
        "formatting took {}ms",
        elapsed.as_millis()
    );

    // The worst case for the line diff: every line differs.
    let crlf = source.replace('\n', "\r\n");
    let start = std::time::Instant::now();
    document(
        Place::Detached,
        &crlf,
        None,
        None,
        &crate::authoring::format::Forms::none(),
    )
    .edits();
    let elapsed = start.elapsed();
    assert!(
        elapsed.as_millis() < 50,
        "formatting a CRLF document took {}ms",
        elapsed.as_millis()
    );
}

#[specforge_test(
    behavior = "lsp_format_document",
    verify = "LSP Format Document: LSP document formatting holds — document_open, format_config_loaded, textedit_list_returned, cli_parity_enforced, format_complete_emitted"
)]
fn document_contract() {
    // document_open: the text is the open document's.
    let dir = on_disk(&[("spec/a.spec", MISINDENTED)]);
    let file = dir.path().join("spec/a.spec");

    let doc = document(
        Place::File(&file),
        MISINDENTED,
        None,
        Some(EDITOR_4),
        &crate::authoring::format::Forms::none(),
    );

    // format_config_loaded: the project's (no file: the defaults), not
    // the editor's.
    assert_eq!(doc.config_source, ConfigSource::Defaults);
    assert_eq!(doc.config, FormatConfig::default());
    // textedit_list_returned: non-overlapping edits.
    let edits = doc.edits();
    assert!(!edits.is_empty());
    assert!(in_order_without_overlap(&edits), "{edits:?}");
    // cli_parity_enforced: what `specforge format` writes for the file.
    let outcome = run(&request(
        &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
        Mode::Check,
    ));
    assert_eq!(outcome.changes[0].after, doc.formatted);
    assert_eq!(
        edits::tests::apply(MISINDENTED, &edits),
        outcome.changes[0].after
    );
    assert!(doc.complete());
}

#[specforge_test(
    behavior = "lsp_format_range",
    verify = "range is expanded to block boundaries"
)]
fn the_range_widens_to_whole_blocks() {
    let lines = Lines { first: 6, last: 6 };

    let edits = document(
        Place::Detached,
        TWO_BLOCKS,
        Some(lines),
        None,
        &crate::authoring::format::Forms::none(),
    )
    .edits();

    let line = |n: usize| TWO_BLOCKS.match_indices('\n').nth(n - 1).unwrap().0 + 1;
    assert_eq!(
        edits,
        [Edit {
            range: line(6) + 2..line(6) + 6,
            new_text: String::new()
        }]
    );
    // The whole document would also realign `foo` (lines 0–4).
    let full = document(
        Place::Detached,
        TWO_BLOCKS,
        None,
        None,
        &crate::authoring::format::Forms::none(),
    )
    .edits();
    assert!(full.iter().any(|e| e.range.start < line(5)), "{full:?}");
}

#[specforge_test(
    behavior = "lsp_format_range",
    verify = "range formatting matches full formatting for affected blocks"
)]
fn a_range_formats_its_blocks_as_the_whole_document_does() {
    let range = document(
        Place::Detached,
        TWO_BLOCKS,
        Some(Lines { first: 4, last: 6 }),
        None,
        &crate::authoring::format::Forms::none(),
    );
    let full = document(
        Place::Detached,
        TWO_BLOCKS,
        None,
        None,
        &crate::authoring::format::Forms::none(),
    );

    let block = |text: &str| text.lines().skip(4).take(4).collect::<Vec<_>>().join("\n");
    assert_eq!(block(&range.formatted), block(&full.formatted));
}

#[specforge_test(
    behavior = "lsp_format_range",
    verify = "parse errors within range are left unchanged per format_with_parse_errors"
)]
fn a_parse_error_in_a_range_is_kept() {
    let text = "behavior a \"A\" {\n  contract \"a\"\n}\n\nbehavior b \"B\" {\n  contract \"b\"\n}\n\nbehavior c \"C\" {\n      contract \"c\"\n}\n\n}}}\n";

    let doc = document(
        Place::Detached,
        text,
        Some(Lines { first: 8, last: 12 }),
        None,
        &crate::authoring::format::Forms::none(),
    );

    assert_eq!(doc.formatted.lines().nth(12), Some("}}}"));
    assert_eq!(doc.formatted.lines().nth(9), Some("  contract \"c\""));
    let kept = kept_regions(&doc);
    assert_eq!(kept.len(), 1, "{:?}", doc.diagnostics);
    assert_eq!(kept[0].span.as_ref().unwrap().start_line, 13);
}

#[specforge_test(
    behavior = "lsp_format_range",
    verify = "formats range within 20ms for ranges under 200 lines"
)]
fn formats_a_range_within_20ms() {
    let mut source = String::from("use types/core\n\n");
    for i in 0..20 {
        source.push_str(&format!(
            "behavior b{i} \"Behavior {i}\" {{\n  contract \"thing {i}\"\n}}\n\n"
        ));
    }

    let start = std::time::Instant::now();
    document(
        Place::Detached,
        &source,
        Some(Lines {
            first: 10,
            last: 20,
        }),
        None,
        &crate::authoring::format::Forms::none(),
    );
    let elapsed = start.elapsed();

    assert!(
        elapsed.as_millis() < 20,
        "range formatting took {}ms",
        elapsed.as_millis()
    );
}

#[specforge_test(
    behavior = "lsp_format_range",
    verify = "LSP Format Range: LSP range formatting holds — document_open, format_config_loaded, range_expanded, textedit_list_returned, full_format_parity, format_complete_emitted"
)]
fn range_contract() {
    // document_open, format_config_loaded: a project file, with its
    // project's configuration.
    let dir = on_disk(&[
        (".specforgefmt.toml", "indent_width = 4\n"),
        ("spec/a.spec", TWO_BLOCKS),
    ]);
    let file = dir.path().join("spec/a.spec");
    let lines = Lines { first: 6, last: 6 };

    let range = document(
        Place::File(&file),
        TWO_BLOCKS,
        Some(lines),
        None,
        &crate::authoring::format::Forms::none(),
    );
    let full = document(
        Place::File(&file),
        TWO_BLOCKS,
        None,
        None,
        &crate::authoring::format::Forms::none(),
    );

    assert_eq!(range.config.indent_width, 4);
    // range_expanded, textedit_list_returned: one block's edits, in order.
    let edits = range.edits();
    let line = |n: usize| TWO_BLOCKS.match_indices('\n').nth(n - 1).unwrap().0 + 1;
    assert!(edits.iter().all(|e| e.range.start >= line(5)), "{edits:?}");
    assert!(in_order_without_overlap(&edits), "{edits:?}");
    // full_format_parity: the block reads as full formatting has it.
    let tail = |text: &str| text.lines().skip(5).collect::<Vec<_>>().join("\n");
    assert_eq!(tail(&range.formatted), tail(&full.formatted));
}

#[specforge_test(
    behavior = "lsp_respect_editor_config",
    verify = "editor settings are used for a document outside any project"
)]
fn editor_settings_apply_outside_a_project() {
    let doc = document(
        Place::Detached,
        FOUR,
        None,
        Some(EDITOR_4),
        &crate::authoring::format::Forms::none(),
    );
    assert_eq!(doc.config.indent_width, 4);
    assert_eq!(doc.config_source, ConfigSource::Editor);
    assert!(doc.edits().is_empty());

    // A file with no specforge.json above it.
    let dir = tempfile::TempDir::new().unwrap();
    let loose = dir.path().join("loose.spec");
    std::fs::write(&loose, FOUR).unwrap();
    let doc = document(
        Place::File(&loose),
        FOUR,
        None,
        Some(EDITOR_4),
        &crate::authoring::format::Forms::none(),
    );
    assert_eq!(doc.config_source, ConfigSource::Editor);
    assert_eq!(doc.config.indent_width, 4);
    assert!(doc.edits().is_empty());
}

#[specforge_test(
    behavior = "lsp_respect_editor_config",
    verify = "config file takes precedence over editor settings"
)]
fn config_file_takes_precedence_over_editor() {
    let dir = on_disk(&[
        (".specforgefmt.toml", "indent_width = 4\n"),
        ("spec/a.spec", FOUR),
    ]);
    let file = dir.path().join("spec/a.spec");
    let editor = EditorOptions {
        tab_size: 8,
        insert_spaces: true,
    };

    let doc = document(
        Place::File(&file),
        FOUR,
        None,
        Some(editor),
        &crate::authoring::format::Forms::none(),
    );

    let ConfigSource::File(config_file) = &doc.config_source else {
        panic!("{:?}", doc.config_source);
    };
    assert!(config_file.ends_with(".specforgefmt.toml"));
    assert_eq!(doc.config.indent_width, 4);
    assert!(doc.edits().is_empty());
}

#[specforge_test(
    behavior = "lsp_respect_editor_config",
    verify = "a project without a config file formats with the defaults, not the editor's settings"
)]
fn a_project_without_a_config_file_uses_the_defaults() {
    let dir = on_disk(&[("spec/a.spec", FOUR)]);
    let file = dir.path().join("spec/a.spec");

    let doc = document(
        Place::File(&file),
        FOUR,
        None,
        Some(EDITOR_4),
        &crate::authoring::format::Forms::none(),
    );

    assert_eq!(doc.config_source, ConfigSource::Defaults);
    assert_eq!(doc.config.indent_width, 2);
    assert!(doc.changed(), "reindented to the defaults");
}

#[specforge_test(
    behavior = "lsp_respect_editor_config",
    verify = "LSP Respect Editor Config: editor config respect holds — lsp_initialized_fired, config_precedence_enforced, editor_fallback_applied"
)]
fn editor_config_contract() {
    // lsp_initialized_fired: the editor's settings are given.
    let editor = Some(EDITOR_4);
    // editor_fallback_applied: outside any project.
    let detached = document(
        Place::Detached,
        FOUR,
        None,
        editor,
        &crate::authoring::format::Forms::none(),
    );
    assert_eq!(detached.config_source, ConfigSource::Editor);
    assert_eq!(detached.config.indent_width, 4);

    // config_precedence_enforced: inside a project, its config file …
    let dir = on_disk(&[
        (".specforgefmt.toml", "indent_width = 3\n"),
        ("spec/a.spec", FOUR),
        ("plain/specforge.json", "{}"),
        ("plain/spec/b.spec", FOUR),
    ]);
    let with_file = document(
        Place::File(&dir.path().join("spec/a.spec")),
        FOUR,
        None,
        editor,
        &crate::authoring::format::Forms::none(),
    );
    assert_eq!(with_file.config.indent_width, 3);
    // … else the defaults, never the editor's settings.
    let without = document(
        Place::File(&dir.path().join("plain/spec/b.spec")),
        FOUR,
        None,
        editor,
        &crate::authoring::format::Forms::none(),
    );
    assert_eq!(without.config_source, ConfigSource::Defaults);
    assert_eq!(without.config.indent_width, 2);
}

#[specforge_test(
    behavior = "format_from_stdin",
    verify = "stdin content is formatted and written to stdout"
)]
fn stdin_text_uses_the_config_of_its_directory() {
    let dir = on_disk(&[("spec/sub/.specforgefmt.toml", "indent_width = 4\n")]);
    let sub = dir.path().join("spec/sub");

    let doc = document(
        Place::InProject {
            root: dir.path(),
            dir: &sub,
        },
        MISINDENTED,
        None,
        None,
        &crate::authoring::format::Forms::none(),
    );

    assert_eq!(doc.config.indent_width, 4);
    assert!(
        doc.formatted.contains("\n    contract"),
        "{}",
        doc.formatted
    );
}
