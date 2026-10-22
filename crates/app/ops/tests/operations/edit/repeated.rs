//! A typed edit of a field its block writes twice (ADR 0215): the graph holds the first entry only, so the
//! edit leaves the block writing the field once, holding what the edit says.

use super::*;
use serde_json::{Value, json};
use specforge_graph::field_form;

const SOFTWARE_FORMAL: &[&str] = &["@specforge/software", "@specforge/formal"];

const REPEATED: &str = r#"port port_one "P1" {
  direction inbound
  direction outbound
}

port port_two "P2" {
  direction inbound
}

behavior beh "B" {
  category command
  ports    [port_one]
  ports    [port_two]
  contract "first contract"
  contract "second contract"
  requires {
    ready "Graph built"
    ready "Disk readable"
  }
}
"#;

/// Lines of `block_id`'s block (header to the closing brace at column 0).
fn block_lines<'t>(text: &'t str, block_id: &str) -> Vec<&'t str> {
    let mut lines = text.lines().skip_while(|l| {
        !(l.starts_with(char::is_alphabetic) && l.contains(&format!(" {block_id} ")))
    });
    let mut out = Vec::new();
    for line in lines.by_ref() {
        out.push(line);
        if line == "}" {
            break;
        }
    }
    out
}

#[specforge_test_macros::test(
    behavior = "edit_entity_field",
    verify = "every verb that changes a field written twice leaves the block writing it once, holding what it says"
)]
#[ignore = "flipped by T4"]
fn every_verb_leaves_a_field_written_twice_written_once() {
    let c = compile(SOFTWARE_FORMAL, &[("g.spec", REPEATED)]);
    // (edit, entity, field the graph is read at, what it holds after, key whose lines are counted, how many, writes)
    let rows: Vec<(Edit, &str, &str, Value, &str, usize, bool)> = vec![
        (
            set("port_one", "direction", one("bidirectional")),
            "port_one",
            "direction",
            json!("bidirectional"),
            "direction ",
            1,
            true,
        ),
        (
            set("beh", "contract", one("third")),
            "beh",
            "contract",
            json!("third"),
            "contract ",
            1,
            true,
        ),
        (
            unset("beh", "ports"),
            "beh",
            "ports",
            Value::Null,
            "ports ",
            0,
            true,
        ),
        (
            link("beh", "ports", "port_two"),
            "beh",
            "ports",
            json!(["port_one", "port_two"]),
            "ports ",
            1,
            true,
        ),
        (
            unlink("beh", "ports", "port_two"),
            "beh",
            "ports",
            json!(["port_one"]),
            "ports ",
            2,
            false,
        ),
        (
            set("beh", "requires.ready", one("Graph rebuilt")),
            "beh",
            "requires",
            json!({"ready": "Graph rebuilt"}),
            "ready ",
            1,
            true,
        ),
        (
            unset("beh", "requires.ready"),
            "beh",
            "requires",
            Value::Null,
            "ready ",
            0,
            true,
        ),
    ];
    let mut bad = Vec::new();
    for (edit, id, field, holds, key, lines, writes) in rows {
        let planned = match plan(&c, std::slice::from_ref(&edit)) {
            Ok(planned) => planned,
            Err(refusal) => {
                bad.push(format!("{edit:?}: refused {}", refusal.error.code));
                continue;
            }
        };
        let text = planned
            .change()
            .after()
            .remove("g.spec")
            .unwrap_or_else(|| REPEATED.to_string());
        let after = compile(SOFTWARE_FORMAL, &[("g.spec", &text)]);
        // The graph JSON, as export and query write a node's fields.
        let value = after.project.graph().node(id).map_or(Value::Null, |n| {
            field_form::write_fields(&n.fields)
                .get(field)
                .map_or(Value::Null, |v| {
                    serde_json::to_value(v).unwrap_or(Value::Null)
                })
        });
        let written = block_lines(&text, id)
            .iter()
            .filter(|l| l.trim_start().starts_with(key))
            .count();
        let wrote = !planned.change().is_empty();
        if value != holds || written != lines || wrote != writes {
            bad.push(format!(
                "{edit:?}: wanted {holds} in {lines} line(s), writes {writes}; got {value} in {written}, writes {wrote}"
            ));
        }
    }
    assert!(bad.is_empty(), "{}", bad.join("\n"));
}
