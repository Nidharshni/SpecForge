use crate::queries::{invalid_input, not_found, suggest};
use crate::tests::G;
use specforge_extension_sdk::prelude::{CommandFormat, CommandGraph, CommandInput};
use specforge_extension_sdk::testing::MockHost;

/// Run the declared command behind `export`, as the host's call would.
fn run(export: &str, input: &CommandInput) -> Option<specforge_extension_sdk::CommandOutput> {
    crate::specforge_extension_build().call_command(export, input)
}

fn input(args: serde_json::Value, graph: CommandGraph) -> CommandInput {
    CommandInput {
        args: args.as_object().unwrap().clone(),
        cwd: "/p".into(),
        graph,
        ..Default::default()
    }
}

/// [`input`], asked for json.
fn json_input(args: serde_json::Value, graph: CommandGraph) -> CommandInput {
    CommandInput {
        format: CommandFormat::Json,
        ..input(args, graph)
    }
}

fn sample() -> CommandGraph {
    G::default()
        .node(
            "f1",
            "feature",
            &[("status", "proposed"), ("priority", "high")],
        )
        .node("f2", "feature", &[("status", "done")])
        .build()
}

#[test]
fn a_list_command_renders_human_by_default_and_json_on_request() {
    let human = run(
        "cmd__product_features",
        &input(serde_json::json!({}), sample()),
    )
    .unwrap();
    assert_eq!(human.exit_code, 0);
    assert_eq!(
        human.stdout,
        "id  title  status    priority\nf1  f1     proposed  high\nf2  f2     done      -\n"
    );
    let paged = run(
        "cmd__product_features",
        &input(serde_json::json!({"limit": 1}), sample()),
    )
    .unwrap();
    assert!(
        paged
            .stdout
            .ends_with("1 of 2 features; --offset 1 for more\n"),
        "{}",
        paged.stdout
    );
    let json = run(
        "cmd__product_features",
        &json_input(serde_json::json!({"status": "done"}), sample()),
    )
    .unwrap();
    let value: serde_json::Value = serde_json::from_str(&json.stdout).unwrap();
    assert_eq!(value["total"], 1);
    assert_eq!(value["features"][0]["id"], "f2");
    assert!(value["features"][0].get("priority").is_none());
}

#[test]
fn a_list_command_refuses_an_arg_it_cannot_use() {
    for (args, says) in [
        (
            serde_json::json!({"limit": -1}),
            "must be a non-negative integer",
        ),
        (
            serde_json::json!({"offset": "many"}),
            "must be a non-negative integer",
        ),
        (
            serde_json::json!({"status": "draft"}),
            "status must be one of proposed,",
        ),
        (
            serde_json::json!({"priority": "urgent"}),
            "priority must be one of",
        ),
        (
            serde_json::json!({"sort_by": "nope"}),
            "a feature has no field 'nope'",
        ),
        (
            serde_json::json!({"sort_order": "up"}),
            "sort_order must be one of asc, desc",
        ),
    ] {
        let out = run("cmd__product_features", &input(args.clone(), sample())).unwrap();
        assert_eq!(out.exit_code, 2, "{args}");
        assert_eq!(out.stdout, "", "{args}");
        assert!(out.stderr.contains(says), "{args}: {}", out.stderr);
    }
}

#[test]
fn a_query_about_a_missing_entity_fails_on_stderr() {
    let out = run(
        "cmd__product_milestone_completion",
        &input(serde_json::json!({"milestone": "nope"}), sample()),
    )
    .unwrap();
    assert_eq!(out.exit_code, 1);
    assert_eq!(out.stdout, "");
    assert_eq!(out.stderr, "error: milestone 'nope' not found\n");
}

#[test]
fn an_id_list_says_none_when_empty() {
    let out = run(
        "cmd__product_feature_dependents",
        &input(serde_json::json!({"feature": "f1"}), sample()),
    )
    .unwrap();
    assert_eq!(out.stdout, "Features depending on 'f1':\n  (none)\n");
}

#[test]
fn every_declared_command_answers_its_export() {
    let described: serde_json::Value = serde_json::from_str(
        &MockHost::new(crate::specforge_extension_build())
            .describe_json("surfaces")
            .unwrap(),
    )
    .unwrap();
    let commands = described["items"][0]["commands"].as_array().unwrap();
    assert_eq!(commands.len(), 40);
    for command in commands {
        assert_eq!(
            command["export"],
            format!("cmd__product_{}", command["id"].as_str().unwrap())
        );
    }
    assert!(run("cmd__product_nope", &input(serde_json::json!({}), sample())).is_none());
}

/// Every command, called with every arg it declares set (an entity id of
/// the kind it names, a date, a sort field the kind has), answers: none
/// reads an arg it does not declare, or as another type (a panic here, a
/// trap in the host), on the path a successful call takes.
#[test]
fn every_command_answers_with_every_arg_set() {
    let kinds = [
        "feature",
        "journey",
        "deliverable",
        "milestone",
        "module",
        "term",
        "persona",
        "channel",
        "release",
    ];
    let graph = kinds
        .iter()
        .fold(G::default(), |g, kind| g.n(&format!("{kind}1"), kind))
        .build();
    let outputs = specforge_extension_sdk::testing::call_every_command(
        &crate::specforge_extension_build(),
        &graph,
        "2026-10-04",
        |_, arg| match arg {
            kind if kinds.contains(&kind) => format!("{kind}1"),
            "as_of" => "2026-10-04".to_string(),
            "sort_by" => "id".to_string(),
            _ => "x".to_string(),
        },
    );
    assert_eq!(outputs.len(), 40);
    for (id, out) in outputs {
        assert_eq!(out.exit_code, 0, "{id}: {out:?}");
    }
}

#[test]
fn a_missing_entity_id_is_invalid_input() {
    let out = run(
        "cmd__product_milestone_completion",
        &input(serde_json::json!({}), sample()),
    )
    .unwrap();
    assert_eq!(
        (out.exit_code, out.stderr.as_str()),
        (2, "error: missing required arg 'milestone'\n")
    );
}

#[test]
fn the_nearest_id_of_the_kind_within_two_edits_is_suggested() {
    let g = G::default()
        .n("launch", "milestone")
        .n("lunch", "milestone")
        .n("launchx", "feature")
        .build();
    assert_eq!(suggest(&g, "milestone", "launc").as_deref(), Some("launch"));
    // Equally near: the first by id.
    assert_eq!(
        suggest(&g, "milestone", "laanch").as_deref(),
        Some("launch")
    );
    assert_eq!(suggest(&g, "milestone", "lnch").as_deref(), Some("lunch"));
    assert_eq!(suggest(&g, "milestone", "dinner"), None);
    // Only ids of the kind asked about.
    assert_eq!(suggest(&g, "feature", "launch").as_deref(), Some("launchx"));
    assert_eq!(suggest(&g, "journey", "launch"), None);
    let error = not_found(&g, "milestone", "launc");
    assert_eq!(error.code, "ENTITY_NOT_FOUND");
    assert_eq!(error.entity_id.as_deref(), Some("launc"));
    assert_eq!(error.suggestion.as_deref(), Some("launch"));
    assert_eq!(invalid_input("bad").code, "INVALID_INPUT");
}

#[test]
fn an_error_is_json_on_stderr_when_json_was_asked_for() {
    let out = run(
        "cmd__product_milestone_completion",
        &json_input(serde_json::json!({"milestone": "nope"}), sample()),
    )
    .unwrap();
    assert_eq!((out.exit_code, out.stdout.as_str()), (1, ""));
    let error: serde_json::Value = serde_json::from_str(&out.stderr).unwrap();
    assert_eq!(
        error,
        serde_json::json!({"code": "ENTITY_NOT_FOUND", "message": "milestone 'nope' not found",
            "entity_id": "nope"})
    );
    let out = run(
        "cmd__product_features",
        &json_input(serde_json::json!({"limit": -1}), sample()),
    )
    .unwrap();
    assert_eq!(out.exit_code, 2);
    let error: serde_json::Value = serde_json::from_str(&out.stderr).unwrap();
    assert_eq!(error["code"], "INVALID_INPUT");
}

#[test]
fn a_table_aligns_its_columns_under_a_header() {
    let rows = vec![
        vec!["feature".to_string(), "done".into(), "12".into()],
        vec!["milestone".to_string(), "in_progress".into(), "3".into()],
    ];
    assert_eq!(
        crate::commands::table(&["kind", "status", "count"], &rows),
        "kind       status       count\nfeature    done         12\nmilestone  in_progress  3\n"
    );
    assert_eq!(crate::commands::table(&["id"], &[]), "id\n");
}

/// FNV-1a, 64 bits: a fingerprint stable across Rust releases.
fn fnv1a(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, &b| {
        (hash ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3)
    })
}

/// The `surfaces` describe payload, byte for byte, as the host receives
/// it. A deliberate surface change updates the pin, saying why:
///
/// - 35633 bytes, `0xcd3d_3de9_1ccf_a2b0`: the hand-written
///   `describe_surfaces.json`, which declaring the commands with the SDK's
///   builder reproduced byte for byte.
/// - 39985 bytes, `0xd8a4_3928_7a28_0e5e`: the 12 closed list filters
///   (`--status`, `--priority`, `--artifact-type`, `--technical-level`,
///   `--interaction-model`) and the 9 `--sort-order` args are `one_of` their
///   values rather than `string`; nothing else changed. The SDK refuses
///   another value with the message the command gave (`INVALID_INPUT`,
///   exit 2), the CLI first, and the MCP tools' schemas list the values.
/// - 40797 bytes, `0x8b30_7f56_fa96_83ca`: counts carry `minimum: 0` (the 29
///   `--limit`, `--offset` and `--max-hops` args, ADR 0017); nothing else
///   changed. The host refuses a negative one on both surfaces before the
///   export runs.
/// - 40920 bytes, `0x3f81_5be7_3dbf_98bc`: `milestone_completion`'s description says it
///   also reports what the recorded tests prove (ADR 0039); nothing else
///   changed.
/// - 40926 bytes, `0xaf70_6cd8_0efd_2f6a`: `health`'s description says "unreferenced
///   counts" where it said "orphan counts"; nothing else changed.
/// - 40406 bytes, `0x918e_c4ab_1cec_3bf5`: the closed filters' help no longer lists the
///   values (clap and MCP list them) and says what an absent value counts
///   as; nothing else changed.
/// - 40433 bytes, `0xced9_0e33_5754_998a`: the 13 descriptions say the request's words, not a
///   CLI flag (ADR 0135 D5); nothing else changed.
#[test]
fn the_surfaces_payload_is_pinned() {
    let payload = MockHost::new(crate::specforge_extension_build())
        .describe_json("surfaces")
        .unwrap();
    assert_eq!(
        (payload.len(), fnv1a(payload.as_bytes())),
        (40433, 0xced9_0e33_5754_998a),
        "the surfaces payload changed"
    );
}
