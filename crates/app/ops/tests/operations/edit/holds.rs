//! What a typed edit leaves a block's field holding, own or inherited (ADR 0149, ADR 0151 D5 and its round-16
//! amendment, ADR 0154, ADR 0175): one row per verb, path and holder, read back from the graph of the planned text.

use super::*;
use crate::navigate::compile;
use serde_json::{Value, json};
use specforge_graph::field_form;

const SOFTWARE_FORMAL: &[&str] = &["@specforge/software", "@specforge/formal"];

/// `inherits` writes nothing, `bystander` keeps every default reached (so no "reaches no block" W161), `owns`
/// overrides each default, `owns_more` adds an entry to the default's.
const HOLDS: &str = r#"defaults behavior {
  category   command
  invariants [done_stamped]
  requires {
    graph_ready "Graph built"
  }
}

invariant task_unique "Task Unique" {
  guarantee "ids are unique"
}

invariant done_stamped "Done Stamped" {
  guarantee "done tasks carry a time"
}

behavior inherits "Inherits" {
  contract "c"
}

behavior bystander "Bystander" {
  contract "c"
}

behavior owns "Owns" {
  category   query
  invariants [task_unique]
  contract   "c"
  requires {
    graph_ready "Graph rebuilt"
  }
}

behavior owns_more "Owns More" {
  contract "c"
  requires {
    graph_ready "Graph built"
    disk_ready  "Disk readable"
  }
}
"#;

#[derive(Debug, PartialEq)]
enum Held {
    /// What the field holds in the graph of the planned text (`Value::Null`: nothing), whether the plan writes a
    /// file, and whether the edit added a W161.
    Holds {
        value: Value,
        changed: bool,
        w161: bool,
    },
    /// The refusal's code.
    Refused(String),
}
use Held::*;

fn holds(value: Value, changed: bool) -> Held {
    Holds {
        value,
        changed,
        w161: false,
    }
}

fn outcome(c: &Compiled, edits: &[Edit], id: &str, field: &str) -> Held {
    let planned = match plan(c, edits) {
        Ok(planned) => planned,
        Err(refusal) => return Refused(refusal.error.code.to_string()),
    };
    let w161 = planned.report().added.0.iter().any(|d| d.code == "W161");
    let text = planned
        .after()
        .remove("g.spec")
        .unwrap_or_else(|| HOLDS.to_string());
    let after = compile(SOFTWARE_FORMAL, &[("g.spec", &text)]);
    let value = after
        .project
        .graph()
        .node(id)
        .and_then(|n| n.fields.get(field))
        .map_or(Value::Null, |v| field_form::write(v).into());
    Holds {
        value,
        changed: !planned.is_empty(),
        w161,
    }
}

type Row<'a> = (Edit, &'a str, &'a str, Held);

/// One line per row whose outcome is not the one it wants.
fn mismatches(c: &Compiled, rows: Vec<Row<'_>>) -> Vec<String> {
    rows.into_iter()
        .filter_map(|(edit, id, field, want)| {
            let got = outcome(c, std::slice::from_ref(&edit), id, field);
            (got != want).then(|| format!("{edit:?}: wanted {want:?}, got {got:?}"))
        })
        .collect()
}

#[specforge_test_macros::test(
    behavior = "edit_entity_field",
    verify = "every verb leaves a field holding what it says, own or inherited"
)]
fn every_verb_leaves_a_field_holding_what_it_says() {
    let c = compile(SOFTWARE_FORMAL, &[("g.spec", HOLDS)]);
    let rows: Vec<Row<'_>> = vec![
        (
            set("inherits", "requires.disk_ready", one("Disk readable")),
            "inherits",
            "requires",
            holds(
                json!({"graph_ready": "Graph built", "disk_ready": "Disk readable"}),
                true,
            ),
        ),
        (
            set("inherits", "requires.graph_ready", one("Graph built")),
            "inherits",
            "requires",
            holds(json!({"graph_ready": "Graph built"}), false),
        ),
        (
            set("inherits", "requires.graph_ready", one("Graph rebuilt")),
            "inherits",
            "requires",
            holds(json!({"graph_ready": "Graph rebuilt"}), true),
        ),
        (
            unset("inherits", "requires.graph_ready"),
            "inherits",
            "requires",
            holds(json!({}), true),
        ),
        (
            unset("inherits", "requires"),
            "inherits",
            "requires",
            holds(json!({}), true),
        ),
        (
            unset("inherits", "invariants"),
            "inherits",
            "invariants",
            holds(json!([]), true),
        ),
        (
            link("inherits", "invariants", "task_unique"),
            "inherits",
            "invariants",
            holds(json!(["done_stamped", "task_unique"]), true),
        ),
        (
            unlink("inherits", "invariants", "done_stamped"),
            "inherits",
            "invariants",
            holds(json!([]), true),
        ),
        (
            link("owns", "invariants", "done_stamped"),
            "owns",
            "invariants",
            holds(json!(["task_unique", "done_stamped"]), true),
        ),
        (
            set("owns", "requires.graph_ready", one("Graph built")),
            "owns",
            "requires",
            holds(json!({"graph_ready": "Graph built"}), true),
        ),
        (
            unset("owns_more", "requires.disk_ready"),
            "owns_more",
            "requires",
            holds(json!({"graph_ready": "Graph built"}), true),
        ),
        (
            unset("inherits", "category"),
            "inherits",
            "category",
            Refused("not_editable".into()),
        ),
    ];
    let bad = mismatches(&c, rows);
    assert!(bad.is_empty(), "{}", bad.join("\n"));
}

#[specforge_test_macros::test(
    behavior = "edit_entity_field",
    verify = "unset leaves a field holding none, once"
)]
fn unset_leaves_a_field_holding_none_once() {
    let c = compile(SOFTWARE_FORMAL, &[("g.spec", HOLDS)]);
    let rows: Vec<Row<'_>> = vec![
        (
            unset("owns", "requires.graph_ready"),
            "owns",
            "requires",
            holds(json!({}), true),
        ),
        (
            unset("owns", "requires"),
            "owns",
            "requires",
            holds(json!({}), true),
        ),
        (
            unset("owns", "invariants"),
            "owns",
            "invariants",
            holds(json!([]), true),
        ),
        (
            unset("owns", "category"),
            "owns",
            "category",
            Refused("not_editable".into()),
        ),
    ];
    let mut bad = mismatches(&c, rows);
    for e in [
        unset("owns", "invariants"),
        unset("owns", "requires"),
        unset("inherits", "invariants"),
    ] {
        let planned = |n: usize| {
            format!(
                "{:?}",
                plan(&c, &vec![e.clone(); n])
                    .map(|p| p.after())
                    .map_err(|r| r.error.code.to_string())
            )
        };
        if planned(2) != planned(1) {
            bad.push(format!("{e:?} twice is not {e:?} once"));
        }
    }
    assert!(bad.is_empty(), "{}", bad.join("\n"));
}
