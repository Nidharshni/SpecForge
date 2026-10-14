//! The command examples of `docs/guides/extending-specforge.md` ("A command"
//! and "Test without a runtime"), the same code between the markers, so the
//! guide's code compiles and does what the guide says. Change both together.

use specforge_extension_sdk::prelude::*;

fn greet() -> ContributionsBuilder {
    let mut c = ContributionsBuilder::new(ExtensionMeta::new("@you/greet", "0.1.0"));
    // ── guide: A command ──
    c.command("greetings", |cmd| {
        cmd.title("List greetings")
            .description("Every greeting of a style")
            .arg("style", |a| {
                a.one_of(&["warm", "formal"])
                    .description("Only greetings of this style");
            })
            .arg("limit", |a| {
                a.count().description("Return at most this many");
            })
            .handler(|call| {
                let style = call.str("style");
                let ids: Vec<&str> = call
                    .graph()
                    .nodes_of_kind("greeting")
                    .filter(|n| style.is_none() || n.text("style") == style)
                    .take(call.count("limit").unwrap_or(100))
                    .map(|n| n.id.as_str())
                    .collect();
                call.render(&serde_json::json!({ "greetings": ids }), |out| {
                    for id in &ids {
                        out.push_str(&format!("{id}\n"));
                    }
                })
            });
    });
    // ── end ──
    c
}

#[test]
fn the_guides_command_runs_as_the_guide_tests_it() {
    let c = greet();
    // ── guide: A command runs the same way ──
    let input = CommandInput {
        args: serde_json::json!({"style": "warm"})
            .as_object()
            .unwrap()
            .clone(),
        format: CommandFormat::Json,
        ..Default::default()
    };
    let out = c.call_command("cmd__greetings", &input).unwrap();
    assert_eq!(out.exit_code, 0);
    // ── end ──
    assert_eq!(out.stdout, "{\n  \"greetings\": []\n}\n");
}

#[test]
fn the_guides_command_refuses_a_style_it_does_not_declare() {
    let input = CommandInput {
        args: serde_json::json!({"style": "rude"})
            .as_object()
            .unwrap()
            .clone(),
        format: CommandFormat::Json,
        ..Default::default()
    };
    let out = greet().call_command("cmd__greetings", &input).unwrap();
    assert_eq!(out.exit_code, 2);
    assert!(out.stderr.contains("INVALID_INPUT"), "{}", out.stderr);
}

#[test]
fn every_command_of_the_guide_reads_only_what_it_declares() {
    let c = greet();
    // ── guide: To check every handler ──
    let graph = CommandGraph::default();
    for (id, out) in
        specforge_extension_sdk::testing::call_every_command(&c, &graph, "2026-10-04", |_, _| {
            String::new()
        })
    {
        assert_eq!(out.exit_code, 0, "{id}");
    }
    // ── end ──
}
