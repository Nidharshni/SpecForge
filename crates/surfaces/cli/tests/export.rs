use crate::support::{Cli, TestProject};
use specforge_project::testing::specs;
use specforge_test_macros::test as specforge_test;

const SPEC_CONTENT: &str = r#"
behavior alpha "Alpha Behavior" {
    contract "The system MUST do alpha"
    status done features [gamma]
}
behavior beta "Beta Behavior" {
    contract "The system MUST do beta" features [gamma]
}
feature gamma "Gamma Feature" {
    problem "Alpha and beta are needed"
}
"#;

#[test]
fn export_graph_produces_valid_json_with_all_nodes() {
    let dir = TestProject::new()
        .without_config()
        .files(&[("main.spec", SPEC_CONTENT)]);

    let output = Cli::on(&dir)
        .command(&["export", "--format=graph"])
        .output()
        .unwrap();

    assert!(output.status.success(), "export should succeed");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("invalid JSON: {}\noutput: {}", e, stdout));

    assert!(parsed["schema_version"].is_string());
    let nodes = parsed["nodes"].as_array().unwrap();
    assert_eq!(nodes.len(), 3);
    let edges = parsed["edges"].as_array().unwrap();
    assert_eq!(edges.len(), 2);
}

#[test]
fn export_brief_produces_minimal_output() {
    let dir = TestProject::new()
        .without_config()
        .files(&[("main.spec", SPEC_CONTENT)]);

    let output = Cli::on(&dir)
        .command(&["export", "--format=brief"])
        .output()
        .unwrap();

    assert!(output.status.success());

    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed: serde_json::Value = serde_json::from_str(&stdout).unwrap();

    let nodes = parsed["nodes"].as_array().unwrap();
    // Brief: no fields, no contract, no file/line
    for node in nodes {
        assert!(node.get("fields").is_none(), "brief should not have fields");
        assert!(node.get("file").is_none(), "brief should not have file");
    }
}

#[test]
fn export_context_includes_contracts() {
    let dir = TestProject::new().without_config().files(&[
        (
            "specforge.json",
            r#"{"name":"t","version":"0.1.0","extensions":["@specforge/software"]}"#,
        ),
        ("main.spec", SPEC_CONTENT),
    ]);

    let output = Cli::on(&dir)
        .command(&["export", "--format=context"])
        .output()
        .unwrap();

    assert!(output.status.success());

    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed: serde_json::Value = serde_json::from_str(&stdout).unwrap();

    let nodes = parsed["nodes"].as_array().unwrap();
    let alpha = nodes.iter().find(|n| n["id"] == "alpha").unwrap();
    assert_eq!(alpha["contract"], "The system MUST do alpha");
}

#[test]
fn export_dot_produces_graphviz() {
    let dir = TestProject::new()
        .without_config()
        .files(&[("main.spec", SPEC_CONTENT)]);

    let output = Cli::on(&dir)
        .command(&["export", "--format=dot"])
        .output()
        .unwrap();

    assert!(output.status.success());

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("digraph"));
    assert!(stdout.contains("alpha"));
    assert!(stdout.contains("gamma"));
}

#[test]
fn export_with_errors_still_works() {
    let dir = TestProject::new().without_config().files(&[(
        "main.spec",
        r#"
feature alpha "A" { problem "first" }
behavior gamma "G" { features [alpha, nonexistent] }
"#,
    )]);

    let output = Cli::on(&dir)
        .command(&["export", "--format=graph"])
        .output()
        .unwrap();

    // Export succeeds even with resolution errors (outputs what it can)
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert!(parsed["nodes"].as_array().unwrap().len() >= 2);
}

#[test]
fn export_with_scope_returns_subgraph() {
    let dir = TestProject::new()
        .without_config()
        .files(&[("main.spec", SPEC_CONTENT)]);

    let output = Cli::on(&dir)
        .command(&["export", "--format=graph", "--scope=alpha"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed: serde_json::Value = serde_json::from_str(&stdout).unwrap();

    let nodes = parsed["nodes"].as_array().unwrap();
    let ids: Vec<&str> = nodes.iter().map(|n| n["id"].as_str().unwrap()).collect();
    // alpha is connected to gamma (via its features edge), beta connects to gamma
    assert!(ids.contains(&"alpha"));
    assert!(ids.contains(&"gamma"));
}

#[test]
fn export_with_nonexistent_scope_exits_one() {
    let dir = TestProject::new()
        .without_config()
        .files(&[("main.spec", SPEC_CONTENT)]);

    Cli::on(&dir)
        .command(&["export", "--format=graph", "--scope=nonexistent"])
        .assert()
        .code(1);
}

#[specforge_test(
    behavior = "export_agent_context_format",
    verify = "non-existent scope entity produces E003 and exit code 1"
)]
fn a_context_export_of_a_missing_scope_is_e003() {
    let dir = TestProject::new()
        .without_config()
        .files(&[("main.spec", SPEC_CONTENT)]);

    let output = Cli::on(&dir)
        .command(&["export", "--format=context", "--scope=nope"])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty(), "no partial export");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.starts_with("error[E003]: unresolved entity 'nope'"),
        "{stderr}"
    );
}

// B:embed_schema_in_export — V2 format with embedded schema (default)
#[test]
fn export_default_produces_v2_with_schema() {
    let dir = TestProject::new()
        .without_config()
        .files(&[("main.spec", SPEC_CONTENT)]);

    let output = Cli::on(&dir)
        .command(&["export", "--format=graph"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed: serde_json::Value = serde_json::from_str(&stdout).unwrap();

    assert_eq!(parsed["format_version"], "2.0");
    assert!(parsed["schema"].is_object());
    assert!(parsed["schema_version"].is_string());
}

#[specforge_test(
    behavior = "embed_schema_in_export",
    verify = "--no-schema suppresses schema and keeps format_version 1.0"
)]
fn export_no_schema_flag_produces_v1() {
    let dir = TestProject::new()
        .without_config()
        .files(&[("main.spec", SPEC_CONTENT)]);

    let output = Cli::on(&dir)
        .command(&["export", "--format=graph", "--no-schema"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed: serde_json::Value = serde_json::from_str(&stdout).unwrap();

    assert_eq!(parsed["format_version"], "1.0", "{parsed}");
    assert!(
        parsed.get("schema").is_none(),
        "V1 format has no schema key"
    );
    assert!(
        parsed["schema_version"].is_string(),
        "V1 format has schema_version"
    );
}

#[specforge_test(
    behavior = "export_agent_brief_format",
    verify = "the brief export leaves the schema out unless --with-schema is given"
)]
fn export_brief_omits_the_schema_unless_asked() {
    let dir = TestProject::new()
        .without_config()
        .files(&[("main.spec", SPEC_CONTENT)]);
    let export = |extra: &[&str]| {
        let mut argv = vec!["export", "--format=brief"];
        argv.extend(extra);
        let output = Cli::on(&dir).command(&argv).output().unwrap();
        assert!(output.status.success());
        serde_json::from_slice::<serde_json::Value>(&output.stdout).unwrap()
    };

    let lean = export(&[]);
    assert!(lean.get("schema").is_none(), "{lean}");
    assert!(!lean["nodes"].as_array().unwrap().is_empty());

    let full = export(&["--with-schema"]);
    assert_eq!(full["format_version"], "2.0");
    assert!(full["schema"].is_object());
}

#[specforge_test(
    behavior = "export_agent_context_format",
    verify = "the context export leaves the schema out unless --with-schema is given"
)]
fn export_context_omits_the_schema_unless_asked() {
    let dir = TestProject::new()
        .without_config()
        .files(&[("main.spec", SPEC_CONTENT)]);
    let export = |extra: &[&str]| {
        let mut argv = vec!["export", "--format=context"];
        argv.extend(extra);
        let output = Cli::on(&dir).command(&argv).output().unwrap();
        assert!(output.status.success());
        serde_json::from_slice::<serde_json::Value>(&output.stdout).unwrap()
    };

    let lean = export(&[]);
    assert!(lean.get("schema").is_none(), "{lean}");
    assert!(!lean["nodes"].as_array().unwrap().is_empty());

    let full = export(&["--with-schema"]);
    assert_eq!(full["format_version"], "2.0");
    assert!(full["schema"].is_object());
}

// B:negotiate_schema_version — invalid --schema-version exits 1
#[test]
fn export_invalid_schema_version_exits_one() {
    let dir = TestProject::new()
        .without_config()
        .files(&[("main.spec", SPEC_CONTENT)]);

    let refused = Cli::on(&dir)
        .command(&["export", "--format=graph", "--schema-version=invalid"])
        .assert()
        .code(1);
    let stderr = String::from_utf8_lossy(&refused.get_output().stderr).into_owned();
    assert!(
        stderr.starts_with("error[invalid_schema_version]"),
        "{stderr}"
    );
}

// B:serve_schema_resource — specforge schema command outputs JSON
#[test]
fn schema_command_outputs_json() {
    let dir = TestProject::new()
        .without_config()
        .files(&[("main.spec", SPEC_CONTENT)]);

    let output = Cli::on(&dir).command(&["schema"]).output().unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed: serde_json::Value = serde_json::from_str(&stdout).unwrap();

    assert!(parsed["schema_version"].is_object());
    assert!(parsed["entity_kinds"].is_array());
    assert!(parsed["edge_types"].is_array());
}

// B:publish_schema_specification — specforge schema --publish outputs JSON Schema
#[test]
fn schema_publish_outputs_json_schema() {
    let dir = TestProject::new()
        .without_config()
        .files(&[("main.spec", SPEC_CONTENT)]);

    let output = Cli::on(&dir)
        .command(&["schema", "--publish"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed: serde_json::Value = serde_json::from_str(&stdout).unwrap();

    assert_eq!(
        parsed["$schema"],
        "https://json-schema.org/draft/2020-12/schema"
    );
    assert_eq!(parsed["title"], "SpecForge Graph Protocol");
}

#[specforge_test(
    behavior = "export_agent_context_format",
    verify = "export --format context keeps an invariant's guarantee"
)]
fn export_context_keeps_an_invariants_guarantee() {
    let dir = TestProject::new()
        .raw_config(r#"{"name":"t","version":"0.1.0","extensions":["@specforge/software"]}"#);
    dir.write("app.spec", "invariant unique_ids \"Unique ids\" {\n  guarantee \"Ids MUST be unique\"\n  risk \"duplicates\"\n}\n");
    let out = Cli::on(&dir)
        .command(&["export", "--format", "context"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let parsed: serde_json::Value = serde_json::from_slice(&out).unwrap();
    let node = parsed["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|n| n["id"] == "unique_ids")
        .unwrap_or_else(|| panic!("no unique_ids in {parsed}"));
    assert_eq!(node["fields"]["guarantee"], "Ids MUST be unique", "{node}");
    assert!(
        node["fields"].get("risk").is_none(),
        "risk isn't normative: {node}"
    );
}

// ── --max-tokens on the graph export ──────────────────────────────────

/// Nine entities, each well over 100 tokens: eight behaviors with long
/// contracts and a feature linking them all. Two extensions give the export
/// a schema of real size.
fn budget_project() -> TestProject {
    const CONFIG: &str = r#"{"name":"t","version":"0.1.0","extensions":["@specforge/software","@specforge/product"]}"#;
    let mut spec = String::new();
    for i in 0..8 {
        spec.push_str(&format!(
            "behavior b{i} \"Behavior {i}\" {{\n  contract \"The system MUST {}\"\n  features [f]\n}}\n",
            "handle this case with care ".repeat(20)
        ));
    }
    spec.push_str("feature f \"All behaviors\" {\n  problem \"Eight behaviors implement it: handle this case with care handle this case with care handle this case with care\"\n}\n");
    TestProject::new()
        .without_config()
        .files(&[("specforge.json", CONFIG), ("main.spec", &spec)])
}

/// `specforge export <dir> --format=graph <args>`: the exit code, stdout and
/// stderr.
fn export_graph(dir: &TestProject, args: &[&str]) -> (i32, String, String) {
    let mut argv = vec!["export", "--format=graph"];
    argv.extend(args);
    let output = Cli::on(dir).command(&argv).output().unwrap();
    (
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

/// The embedded schema of this project's graph export, and what it costs by
/// the estimator the budget uses.
fn schema_tokens(dir: &TestProject) -> (serde_json::Value, usize) {
    let (code, stdout, stderr) = export_graph(dir, &["--with-schema"]);
    assert_eq!(code, 0, "{stderr}");
    let full: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    let schema = full["schema"].clone();
    assert!(schema.is_object(), "{full}");
    let tokens = specforge_emitter::estimate_tokens(&serde_json::to_string(&schema).unwrap());
    (schema, tokens)
}

#[specforge_test(
    behavior = "enforce_token_budget",
    verify = "the graph export honours --max-tokens with the schema left out unless --with-schema is given"
)]
fn graph_export_honours_max_tokens_without_the_schema() {
    let dir = budget_project();
    let budget = 500;

    let (code, stdout, stderr) = export_graph(&dir, &["--max-tokens", &budget.to_string()]);
    assert_eq!(code, 0, "{stderr}");
    let parsed: serde_json::Value = serde_json::from_str(&stdout).unwrap();

    let used = specforge_emitter::estimate_tokens(&stdout);
    assert!(used <= budget, "{used} tokens over the {budget} budget");
    assert!(parsed.get("schema").is_none(), "{parsed}");
    assert_eq!(parsed["format_version"], "1.0", "{parsed}");
    let kept = parsed["nodes"].as_array().unwrap().len();
    assert!(kept > 0 && kept < 9, "kept {kept} of 9: {parsed}");
    let truncated = parsed["token_budget"]["truncated_entities"]
        .as_array()
        .unwrap();
    assert_eq!(kept + truncated.len(), 9, "{parsed}");
}

#[specforge_test(
    behavior = "enforce_token_budget",
    verify = "an embedded schema counts toward the token budget"
)]
fn embedded_schema_counts_toward_the_budget() {
    let dir = budget_project();
    let (schema, schema_cost) = schema_tokens(&dir);
    // The schema plus room for a few entities, not all nine.
    let budget = schema_cost + 400;

    let (code, stdout, stderr) = export_graph(
        &dir,
        &["--with-schema", "--max-tokens", &budget.to_string()],
    );
    assert_eq!(code, 0, "{stderr}");
    let parsed: serde_json::Value = serde_json::from_str(&stdout).unwrap();

    let used = specforge_emitter::estimate_tokens(&stdout);
    assert!(used <= budget, "{used} tokens over the {budget} budget");
    assert_eq!(parsed["format_version"], "2.0", "{parsed}");
    assert_eq!(parsed["schema"], schema, "the schema travels whole");
    let kept = parsed["nodes"].as_array().unwrap().len();
    assert!(kept > 0 && kept < 9, "kept {kept} of 9");
    assert_eq!(
        parsed["token_budget"]["truncated_entities"]
            .as_array()
            .unwrap()
            .len(),
        9 - kept
    );
}

#[specforge_test(
    behavior = "enforce_token_budget",
    verify = "a budget smaller than the embedded schema fails with E062 instead of truncating the schema"
)]
fn budget_smaller_than_the_schema_fails() {
    let dir = budget_project();
    let (_, schema_cost) = schema_tokens(&dir);
    let budget = schema_cost - 1;

    let (code, stdout, stderr) = export_graph(
        &dir,
        &["--with-schema", "--max-tokens", &budget.to_string()],
    );
    assert_eq!(code, 1, "stdout: {stdout}");
    assert!(stdout.trim().is_empty(), "no partial export: {stdout}");
    assert!(stderr.contains("E062"), "{stderr}");
    assert!(stderr.contains("schema"), "{stderr}");

    // Without --with-schema the same budget is plenty.
    let (code, _, stderr) = export_graph(&dir, &["--max-tokens", &budget.to_string()]);
    assert_eq!(code, 0, "{stderr}");
}

#[specforge_test(
    behavior = "enforce_token_budget",
    verify = "a budget below one entity yields the envelope with no entities and the truncation marker"
)]
fn budget_below_one_entity_yields_an_empty_envelope() {
    let dir = budget_project();
    // Every entity costs more than 100 tokens; the empty envelope less.
    let budget = 100;

    let (code, stdout, stderr) = export_graph(&dir, &["--max-tokens", &budget.to_string()]);
    assert_eq!(code, 0, "{stderr}");
    let parsed: serde_json::Value = serde_json::from_str(&stdout).unwrap();

    let used = specforge_emitter::estimate_tokens(&stdout);
    assert!(used <= budget, "{used} tokens over the {budget} budget");
    assert_eq!(parsed["format_version"], "1.0", "{parsed}");
    assert!(parsed["schema_version"].is_string(), "{parsed}");
    assert_eq!(parsed["nodes"], serde_json::json!([]), "{parsed}");
    assert_eq!(parsed["edges"], serde_json::json!([]), "{parsed}");
    let meta = &parsed["token_budget"];
    assert_eq!(meta["strategy"], "prioritize", "{parsed}");
    assert_eq!(meta["budget_tokens"], budget, "{parsed}");
    let mut truncated: Vec<&str> = meta["truncated_entities"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    truncated.sort();
    assert_eq!(
        truncated,
        vec!["b0", "b1", "b2", "b3", "b4", "b5", "b6", "b7", "f"]
    );
}

/// `specforge export <dir> --format=<format> <args>`: the exit code, stdout and
/// stderr.
fn export_format(dir: &TestProject, format: &str, args: &[&str]) -> (i32, String, String) {
    let format_flag = format!("--format={format}");
    let mut argv = vec!["export", format_flag.as_str()];
    argv.extend(args);
    let output = Cli::on(dir).command(&argv).output().unwrap();
    (
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

#[specforge_test(
    behavior = "enforce_token_budget",
    verify = "the context and brief exports honour --max-tokens, listing the dropped entities under token_budget"
)]
fn context_and_brief_exports_honour_max_tokens() {
    let dir = budget_project();
    for format in ["context", "brief"] {
        let (code, whole, stderr) = export_format(&dir, format, &[]);
        assert_eq!(code, 0, "{format}: {stderr}");
        let cost = specforge_emitter::estimate_tokens(&whole);

        // Half the cost: entities are dropped, and the export says which.
        let budget = cost / 2;
        let (code, stdout, stderr) =
            export_format(&dir, format, &["--max-tokens", &budget.to_string()]);
        assert_eq!(code, 0, "{format}: {stderr}");
        let parsed: serde_json::Value = serde_json::from_str(&stdout).unwrap();
        let used = specforge_emitter::estimate_tokens(&stdout);
        assert!(
            used <= budget,
            "{format}: {used} tokens over the {budget} budget"
        );
        let meta = &parsed["token_budget"];
        assert_eq!(meta["strategy"], "prioritize", "{format}: {parsed}");
        assert_eq!(meta["budget_tokens"], budget, "{format}: {parsed}");
        let mut all: Vec<&str> = parsed["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|n| n["id"].as_str().unwrap())
            .chain(
                meta["truncated_entities"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| v.as_str().unwrap()),
            )
            .collect();
        all.sort();
        assert_eq!(
            all, NINE,
            "{format}: kept and dropped are the nine, disjoint"
        );

        // Exactly the unbudgeted cost: nothing dropped, nothing added.
        let (code, stdout, stderr) =
            export_format(&dir, format, &["--max-tokens", &cost.to_string()]);
        assert_eq!(code, 0, "{format}: {stderr}");
        assert_eq!(stdout, whole, "{format}");
        assert!(!stdout.contains("token_budget"), "{format}");
    }
}

#[specforge_test(
    behavior = "enforce_token_budget",
    verify = "a context or brief export that cannot fit even without entities, or whose embedded schema is over the budget, fails with E062"
)]
fn context_and_brief_budgets_too_small_fail_with_e062() {
    let dir = budget_project();
    for format in ["context", "brief"] {
        let (code, stdout, stderr) = export_format(&dir, format, &["--max-tokens", "5"]);
        assert_eq!(code, 1, "{format}: {stdout}");
        assert!(
            stdout.trim().is_empty(),
            "{format}: no over-budget export: {stdout}"
        );
        assert!(stderr.contains("E062"), "{format}: {stderr}");

        let (code, stdout, stderr) =
            export_format(&dir, format, &["--with-schema", "--max-tokens", "300"]);
        assert_eq!(code, 1, "{format}: {stdout}");
        assert!(stdout.trim().is_empty(), "{format}: {stdout}");
        assert!(stderr.contains("E062"), "{format}: {stderr}");
        assert!(
            stderr.contains("embedded schema alone"),
            "{format}: {stderr}"
        );
    }
}

// ── The export matrix (plan 14, P1) ───────────────────────────────────

/// What one cell of the export matrix does.
#[derive(Debug, PartialEq)]
enum Expect {
    /// Exit 0, all nine entities, and these top-level keys besides
    /// `nodes` and `edges`.
    Whole { keys: &'static [&'static str] },
    /// Exit 0, within the budget, the `token_budget` block naming the rest.
    Truncated { kept: usize, dropped: usize },
    /// Exit 1, no stdout, and this code on stderr.
    Fails(&'static str),
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Format {
    Graph,
    Context,
    Brief,
}

impl Format {
    fn name(self) -> &'static str {
        match self {
            Format::Graph => "graph",
            Format::Context => "context",
            Format::Brief => "brief",
        }
    }
}

/// The matrix: (format, with a schema, scoped to `f`, budget) and what
/// the export does. `f` reaches every entity, so scoping selects the same nine.
fn expected(format: Format, schema: bool, scoped: bool, budget: Option<usize>) -> Expect {
    const PLAIN: &[&str] = &["schema_version"];
    const V1: &[&str] = &["format_version", "schema_version"];
    const EMBEDDED: &[&str] = &["format_version", "schema", "schema_version"];
    const REFERENCED: &[&str] = &["format_version", "schema_ref", "schema_version"];
    let keys = match (format, schema, scoped) {
        (Format::Graph, false, _) => V1,
        (_, false, _) => PLAIN,
        (_, true, false) => EMBEDDED,
        (_, true, true) => REFERENCED,
    };
    // An embedded schema alone is over every small budget.
    let embedded = schema && !scoped;
    match (format, budget) {
        (_, None | Some(100_000)) => Expect::Whole { keys },
        // Below the empty export, or with an embedded schema over the budget,
        // every format fails alike.
        (_, Some(5)) => Expect::Fails("E062"),
        (_, Some(_)) if embedded => Expect::Fails("E062"),
        (Format::Graph | Format::Context, Some(_)) => Expect::Truncated {
            kept: 2,
            dropped: 7,
        },
        (Format::Brief, Some(_)) if schema => Expect::Truncated {
            kept: 7,
            dropped: 2,
        },
        (Format::Brief, Some(_)) => Expect::Whole { keys },
    }
}

const NINE: [&str; 9] = ["b0", "b1", "b2", "b3", "b4", "b5", "b6", "b7", "f"];

#[test]
fn the_export_matrix() {
    let dir = budget_project();
    let budgets = [None, Some(100_000), Some(300), Some(5)];
    let mut whole: std::collections::BTreeMap<String, String> = Default::default();
    for format in [Format::Graph, Format::Context, Format::Brief] {
        for schema in [false, true] {
            for scoped in [false, true] {
                for budget in budgets {
                    let cell = format!(
                        "{} schema={schema} scoped={scoped} budget={budget:?}",
                        format.name()
                    );
                    let mut args = vec![format!("--format={}", format.name())];
                    args.push(
                        if schema {
                            "--with-schema"
                        } else {
                            "--no-schema"
                        }
                        .to_string(),
                    );
                    if scoped {
                        args.extend(["--scope".to_string(), "f".to_string()]);
                    }
                    if let Some(budget) = budget {
                        args.extend(["--max-tokens".to_string(), budget.to_string()]);
                    }
                    let mut argv = vec!["export"];
                    argv.extend(args.iter().map(String::as_str));
                    let output = Cli::on(&dir).command(&argv).output().unwrap();
                    let code = output.status.code().unwrap_or(-1);
                    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
                    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
                    let expect = expected(format, schema, scoped, budget);

                    if let Expect::Fails(error) = expect {
                        assert_eq!(code, 1, "{cell}: {stdout}");
                        assert!(stdout.trim().is_empty(), "{cell}: {stdout}");
                        assert!(stderr.contains(error), "{cell}: {stderr}");
                        continue;
                    }
                    assert_eq!(code, 0, "{cell}: {stderr}");
                    let parsed: serde_json::Value = serde_json::from_str(&stdout)
                        .unwrap_or_else(|e| panic!("{cell}: {e}: {stdout}"));
                    let object = parsed.as_object().unwrap();
                    let ids: Vec<&str> = parsed["nodes"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|n| n["id"].as_str().unwrap())
                        .collect();
                    let used = specforge_emitter::estimate_tokens(&stdout);
                    match expect {
                        Expect::Whole { keys } => {
                            let mut seen: Vec<&str> = object
                                .keys()
                                .map(String::as_str)
                                .filter(|k| *k != "nodes" && *k != "edges")
                                .collect();
                            seen.sort();
                            assert_eq!(seen, keys, "{cell}: {stdout}");
                            let mut sorted = ids.clone();
                            sorted.sort();
                            assert_eq!(sorted, NINE, "{cell}");
                            // A budget the export fits in changes nothing.
                            let key = format!("{} {schema} {scoped}", format.name());
                            match budget {
                                None => {
                                    whole.insert(key, stdout);
                                }
                                Some(100_000) => {
                                    assert_eq!(whole.get(&key), Some(&stdout), "{cell}")
                                }
                                Some(_) => {}
                            }
                        }
                        Expect::Truncated { kept, dropped } => {
                            let budget = budget.unwrap();
                            assert!(used <= budget, "{cell}: {used} tokens");
                            let meta = &parsed["token_budget"];
                            assert_eq!(meta["strategy"], "prioritize", "{cell}");
                            assert_eq!(meta["budget_tokens"], budget, "{cell}");
                            let gone: Vec<&str> = meta["truncated_entities"]
                                .as_array()
                                .unwrap()
                                .iter()
                                .map(|v| v.as_str().unwrap())
                                .collect();
                            assert_eq!((ids.len(), gone.len()), (kept, dropped), "{cell}");
                            let mut all: Vec<&str> = ids.iter().chain(&gone).copied().collect();
                            all.sort();
                            assert_eq!(all, NINE, "{cell}: kept and dropped are disjoint");
                        }
                        Expect::Fails(_) => unreachable!(),
                    }
                }
            }
        }
    }
}

#[specforge_test(
    behavior = "enforce_token_budget",
    verify = "a budget below the empty envelope fails with E062"
)]
fn budget_below_the_empty_envelope_fails() {
    let dir = budget_project();

    let (code, stdout, stderr) = export_graph(&dir, &["--max-tokens", "5"]);
    assert_eq!(code, 1, "stdout: {stdout}");
    assert!(stdout.trim().is_empty(), "no over-budget export: {stdout}");
    assert!(stderr.contains("E062"), "{stderr}");
}

#[specforge_test(
    behavior = "publish_schema_specification",
    verify = "--kind with --publish, and --format without it, are refused"
)]
fn publish_refuses_kind_and_format_needs_publish() {
    let dir = TestProject::new()
        .enabling(&["@specforge/software"])
        .config(|c| c["spec_root"] = "spec".into())
        .file("spec/main.spec", specs::SOFTWARE);
    let cli = Cli::on(&dir);

    let kind = cli.run(&["schema", "--publish", "--kind", "behavior"]);
    assert_eq!(kind.code, 2);
    assert!(kind.stderr.contains("--kind"), "{}", kind.stderr);

    let format = cli.run(&["schema", "--format", "brief"]);
    assert_eq!(format.code, 2);
    assert!(format.stderr.contains("--publish"), "{}", format.stderr);

    // `json` is `graph`'s alias; plain `schema` runs with the default
    // `--format` (a default does not trigger `requires`).
    let json = cli
        .run(&["schema", "--publish", "--format", "json"])
        .exits(0)
        .json();
    let graph = cli
        .run(&["schema", "--publish", "--format", "graph"])
        .exits(0)
        .json();
    assert_eq!(json, graph);
    cli.run(&["schema"]).exits(0).json();
}
