//! A typed edit lays a multi-line value out with the project's format configuration (ADR 0153).

use super::*;
use specforge_formatter::FormatConfig;
use std::collections::BTreeMap;

const LONG: &str = "Given valid credentials the behavior returns an auth token that the client stores and sends with every later request it makes";

fn configured(toml: &str) -> Compiled {
    project(&[("tasks.spec", TASKS), (".specforgefmt.toml", toml)])
}

/// Whether formatting `text` with `config` leaves it as it is.
fn formats_to_itself(c: &Compiled, config: &FormatConfig, text: &str) -> bool {
    let forms = c.project.environment().registries().field_forms();
    specforge_formatter::format_source(text, config, forms).formatted == text
}

/// The lines from `header` to the next line that is exactly `}`, with its line break.
fn block_of<'a>(text: &'a str, header: &str) -> &'a str {
    let start = text.find(header).expect("header");
    let rest = &text[start..];
    let end = rest.find("\n}\n").expect("closing brace") + 3;
    &rest[..end]
}

#[specforge_test_macros::test(
    invariant = "typed_edit_canonical_form",
    verify = "every block a typed edit writes is unchanged by format"
)]
fn a_triple_quoted_value_is_indented_with_the_projects_tabs() {
    let c = configured("use_tabs = true\n");
    let text = after(
        &c,
        "tasks.spec",
        &[set("list_tasks", "contract", one(LONG))],
    );
    let block = block_of(&text, "behavior list_tasks");
    assert!(
        block.contains(&format!("\tcontract \"\"\"\n\t\t{LONG}\n\t\"\"\"\n}}")),
        "{text}"
    );
    let tabs = FormatConfig {
        use_tabs: true,
        ..Default::default()
    };
    assert!(formats_to_itself(&c, &tabs, block));
}

#[specforge_test_macros::test(
    behavior = "edit_entity_field",
    verify = "a multi-line value is laid out with the project's format configuration"
)]
fn a_value_follows_the_projects_indent_width() {
    let c = configured("indent_width = 4\n");
    let text = after(
        &c,
        "tasks.spec",
        &[set("list_tasks", "contract", one(LONG))],
    );
    assert!(
        text.contains(&format!("    contract \"\"\"\n        {LONG}\n    \"\"\"")),
        "{text}"
    );
    let text = after(
        &c,
        "tasks.spec",
        &[Edit::Create {
            kind: "behavior".into(),
            id: "log_out".into(),
            title: "Log out".into(),
            file: "tasks.spec".into(),
            fields: BTreeMap::from([("contract".into(), one(LONG))]),
            verify: vec![],
        }],
    );
    assert!(
        text.contains(&format!(
            "    contract \"\"\"\n        {LONG}\n    \"\"\"\n}}"
        )),
        "{text}"
    );
}

#[test]
fn a_value_past_the_projects_max_width_is_triple_quoted() {
    let c = configured("max_width = 40\n");
    let short = "When asked, the system MUST list every task.";
    let text = after(
        &c,
        "tasks.spec",
        &[set("list_tasks", "contract", one(short))],
    );
    assert!(
        text.contains(&format!("  contract \"\"\"\n    {short}\n  \"\"\"")),
        "{text}"
    );
}

#[test]
fn a_value_within_the_projects_max_width_stays_on_one_line() {
    let c = configured("max_width = 200\n");
    let text = after(
        &c,
        "tasks.spec",
        &[set("list_tasks", "contract", one(LONG))],
    );
    assert!(text.contains(&format!("  contract \"{LONG}\"\n")), "{text}");
}

/// A project whose `.specforgefmt.toml` is a link to `indent_width = 4`, held outside the project.
#[cfg(unix)]
fn linked() -> (Compiled, tempfile::TempDir) {
    let c = project(&[("tasks.spec", TASKS)]);
    let outside = tempfile::TempDir::new().unwrap();
    let target = outside.path().join("c.toml");
    std::fs::write(&target, "indent_width = 4\n").unwrap();
    let link = c.project.root().unwrap().join(".specforgefmt.toml");
    std::os::unix::fs::symlink(target, link).unwrap();
    (c, outside)
}

#[cfg(unix)]
#[specforge_test_macros::test(
    behavior = "load_format_config",
    verify = "a typed edit and init lay out with the defaults when their bound refuses the configuration"
)]
fn a_contained_view_lays_out_with_the_defaults_when_the_configuration_links_outside() {
    let (c, _outside) = linked();

    let planned = edit::plan(
        &ProjectView::of(&c.project).contained(),
        &[set("list_tasks", "contract", one(LONG))],
    )
    .unwrap();
    let text = planned.after().remove("tasks.spec").unwrap();

    assert!(
        text.contains(&format!("  contract \"\"\"\n    {LONG}\n  \"\"\"")),
        "{text}"
    );
    assert!(!text.contains("    contract"), "{text}");
}

#[cfg(unix)]
#[specforge_test_macros::test(
    behavior = "load_format_config",
    verify = "an unbounded caller follows a linked configuration"
)]
fn an_uncontained_view_follows_a_linked_configuration() {
    let (c, _outside) = linked();

    let text = after(
        &c,
        "tasks.spec",
        &[set("list_tasks", "contract", one(LONG))],
    );

    assert!(
        text.contains(&format!("    contract \"\"\"\n        {LONG}\n    \"\"\"")),
        "{text}"
    );
}
