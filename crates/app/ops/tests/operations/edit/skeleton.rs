//! The skeleton of a new block or member is the typed create's text with a slot where each value goes (ADR 0191).

use super::*;
use crate::navigate::compile;
use specforge_formatter::FormatConfig;
use specforge_ops::authoring::edit::{Skeleton, Skeletons, SlotOf};
use specforge_ops::authoring::format::Place;

const EXTENSIONS: [&str; 3] = [
    "@specforge/software",
    "@specforge/governance",
    "@specforge/formal",
];
const CONFIGS: [&str; 3] = ["", "indent_width = 4\n", "use_tabs = true\n"];

fn configured(toml: &str) -> Compiled {
    compile(
        &EXTENSIONS,
        &[("seed.spec", "\n"), (".specforgefmt.toml", toml)],
    )
}

fn skeletons<R>(c: &Compiled, then: impl FnOnce(&ProjectView<'_>, &Skeletons<'_>) -> R) -> R {
    let view = ProjectView::of(&c.project);
    let path = view.env().spec_root().join("new.spec");
    then(&view, &Skeletons::at(&view, Place::File(&path)))
}

/// `skeleton` with each slot holding what `value` answers for it, and the `End` line removed.
fn filled(skeleton: &Skeleton, value: impl Fn(&SlotOf) -> Option<String>) -> String {
    let mut text = skeleton.text.clone();
    for slot in skeleton.slots.iter().rev() {
        if slot.of == SlotOf::End {
            let line = text[..slot.at.start].rfind('\n').map_or(0, |n| n + 1);
            text.replace_range(line..=slot.at.start, "");
        } else if let Some(value) = value(&slot.of) {
            text.replace_range(slot.at.clone(), &value);
        }
    }
    text
}

#[specforge_test_macros::test(
    behavior = "create_entity",
    verify = "a create's skeleton filled in is the block new writes"
)]
fn a_filled_skeleton_is_the_block_new_writes() {
    type Row<'a> = (&'a str, Vec<(&'a str, Input)>);
    let rows: [Row; 4] = [
        (
            "decision",
            vec![
                ("status", one("accepted")),
                ("context", one("c")),
                ("decision", one("d")),
            ],
        ),
        (
            "failure_mode",
            vec![
                ("severity", one("high")),
                ("cause", one("c")),
                ("effect", one("e")),
            ],
        ),
        (
            "process",
            vec![("alphabet", many(&["a"])), ("initial_state", one("s"))],
        ),
        ("behavior", vec![("contract", one("c"))]),
    ];
    for toml in CONFIGS {
        let c = configured(toml);
        for (kind, fields) in &rows {
            let expected = after(
                &c,
                "new.spec",
                &[Edit::Create {
                    kind: (*kind).into(),
                    id: "x1".into(),
                    title: "X".into(),
                    file: "new.spec".into(),
                    fields: fields
                        .iter()
                        .map(|(name, input)| ((*name).to_string(), input.clone()))
                        .collect(),
                    verify: vec![],
                }],
            );
            let skeleton = skeletons(&c, |_, s| s.block(kind)).unwrap();
            let text = filled(&skeleton, |of| match of {
                SlotOf::Id => Some("x1".into()),
                SlotOf::Title => Some("X".into()),
                SlotOf::Field(name) => {
                    fields
                        .iter()
                        .find(|(n, _)| n == name)
                        .map(|(_, input)| match input {
                            Input::One(text) => text.clone(),
                            Input::Many(items) => items.join(", "),
                            Input::Entries(_) => unreachable!(),
                        })
                }
                SlotOf::End => None,
            });
            assert_eq!(
                text,
                expected.trim_end_matches('\n'),
                "{kind} under {toml:?}: {skeleton:?}"
            );
        }
    }
}

#[specforge_test_macros::test(
    behavior = "create_entity",
    verify = "every kind's skeleton formats to itself"
)]
fn every_kinds_skeleton_formats_to_itself() {
    for toml in CONFIGS {
        let c = configured(toml);
        let config = specforge_formatter::FormatConfig {
            indent_width: if toml.contains("indent_width") { 4 } else { 2 },
            use_tabs: toml.contains("tabs"),
            ..FormatConfig::default()
        };
        let forms = c.project.environment().registries().field_forms();
        let keywords: Vec<String> = ProjectView::of(&c.project)
            .registries()
            .kinds()
            .keywords()
            .cloned()
            .collect();
        for kind in keywords {
            let skeleton = skeletons(&c, |_, s| s.block(&kind))
                .unwrap_or_else(|| panic!("no skeleton for {kind} under {toml:?}"));
            let required: Vec<String> = skeletons(&c, |view, _| {
                edit::fields_of(view, &kind)
                    .into_iter()
                    .filter(|f| f.required)
                    .map(|f| f.name)
                    .collect()
            });
            let starts: Vec<usize> = skeleton.slots.iter().map(|s| s.at.start).collect();
            assert!(starts.is_sorted(), "{kind}: {skeleton:?}");
            assert!(
                skeleton
                    .slots
                    .windows(2)
                    .all(|w| w[0].at.end <= w[1].at.start),
                "{kind}: {skeleton:?}"
            );
            assert_eq!(skeleton.slots[0].of, SlotOf::Id, "{kind}");
            assert_eq!(skeleton.slots[1].of, SlotOf::Title, "{kind}");
            assert_eq!(skeleton.slots.last().unwrap().of, SlotOf::End, "{kind}");
            let fields: Vec<&str> = skeleton
                .slots
                .iter()
                .filter_map(|s| match &s.of {
                    SlotOf::Field(name) => Some(name.as_str()),
                    _ => None,
                })
                .collect();
            let declared: Vec<&str> = required.iter().map(String::as_str).collect();
            assert_eq!(fields, declared, "{kind}: {skeleton:?}");
            let text = filled(&skeleton, |of| match of {
                SlotOf::Field(_) | SlotOf::Id | SlotOf::Title => Some("x".to_string()),
                SlotOf::End => None,
            });
            // An integer or bool slot holds its first value; the filled word is a name there, which formats as is.
            let formatted = specforge_formatter::format_source(&text, &config, forms).formatted;
            assert_eq!(
                formatted.trim_end_matches('\n'),
                text,
                "{kind} under {toml:?}"
            );
        }
    }
}

#[specforge_test_macros::test(
    behavior = "create_entity",
    verify = "a member's skeleton is written in its field's form"
)]
fn a_member_skeleton_is_written_in_its_fields_form() {
    let c = configured("indent_width = 4\n");
    let member = |kind: &str, field: &str| skeletons(&c, |_, s| s.member(kind, field));
    let expect = |kind: &str, field: &str, text: &str, slot: std::ops::Range<usize>| {
        let skeleton = member(kind, field).unwrap();
        assert_eq!(skeleton.text, text, "{kind}.{field}");
        assert_eq!(skeleton.slots.len(), 1);
        assert_eq!(skeleton.slots[0].at, slot, "{kind}.{field}");
        assert_eq!(skeleton.slots[0].of, SlotOf::Field(field.to_string()));
    };
    expect("failure_mode", "cause", "cause \"\"", 7..7);
    expect(
        "failure_mode",
        "post_mitigation",
        "post_mitigation {\n    \n}",
        22..22,
    );
    expect("failure_mode", "rpn", "rpn 0", 4..5);
    expect("failure_mode", "severity", "severity ", 9..9);
    expect("process", "alphabet", "alphabet [\"\"]", 11..11);
    expect("behavior", "invariants", "invariants []", 12..12);
    assert_eq!(member("behavior", "nope"), None);

    let tabs = configured("use_tabs = true\n");
    let skeleton = skeletons(&tabs, |_, s| s.member("failure_mode", "post_mitigation")).unwrap();
    assert_eq!(skeleton.text, "post_mitigation {\n\t\n}");
}
