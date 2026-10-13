//! Each enumerated argument on both surfaces (architecture plan 2026-10-06
//! 12): the names the CLI and MCP accept for it, the name an absent
//! argument selects, and what each surface advertises (the CLI's `-h`
//! line, the MCP input schema's `enum` and `default`).
//!
//! Each probe runs over a scratch copy of `fixtures/read_views/rv1`.
//! `cli_and_mcp_accept_the_same_names` holds the arguments both surfaces
//! read from an option table (ADR 0027) to it. `enumerated_options_today`
//! pins what the surfaces answer for the other probes, drifts included, as
//! one insta snapshot. It proves no spec obligation (it pins current
//! behaviour, bugs included), so it carries no `specforge_test` link; a
//! change that alters what it pins re-blesses the snapshot in the same
//! commit, where the diff shows it.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};
use specforge_ops::export::{AGENT_FORMAT, FORMAT};
use specforge_ops::model::{
    DEPS, GROUP_BY, MODEL_FIELDS, MODEL_FORMAT, OUTLINE_FIELDS, OUTLINE_FORMAT,
};
use specforge_ops::options::OptionTable;

use crate::coverage_corpus::{mcp_calls, served};
use crate::support::{Cli, TestProject};

/// A scratch copy of `fixtures/read_views/rv1`.
fn rv1() -> TestProject {
    TestProject::copy_of(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/read_views/rv1"),
    )
}

/// The placeholders of a probe's CLI command line.
const ROOT: &str = "{root}";
const NAME: &str = "{name}";

/// One enumerated argument, probed on each surface it has.
struct Probe {
    label: &'static str,
    /// The CLI command line, `{root}` and `{name}` filled in; an absent
    /// argument drops `{name}` and the flag before it.
    cli: Option<&'static [&'static str]>,
    mcp: Option<McpProbe>,
    names: &'static [&'static str],
}

/// The MCP tool and argument a probe calls, with the other arguments every
/// call passes.
struct McpProbe(&'static str, &'static str, fn() -> Value);

fn no_arguments() -> Value {
    json!({})
}

fn entity_alpha() -> Value {
    json!({"entity_id": "alpha"})
}

const PROBES: &[Probe] = &[
    Probe {
        label: "export format",
        cli: Some(&["export", "--path", ROOT, "--format", NAME]),
        mcp: Some(McpProbe("specforge.export", "format", no_arguments)),
        names: &["graph", "context", "brief", "dot", "json", "yaml"],
    },
    Probe {
        label: "schema publish format",
        cli: Some(&["schema", "--path", ROOT, "--publish", "--format", NAME]),
        mcp: None,
        names: &["graph", "context", "brief", "dot", "json"],
    },
    Probe {
        label: "model format",
        cli: Some(&["model", "--path", ROOT, "--format", NAME]),
        mcp: Some(McpProbe("specforge.model", "format", no_arguments)),
        names: &["markdown", "mermaid", "dot", "json", "dbml", "svg"],
    },
    Probe {
        label: "model group_by",
        cli: Some(&["model", "--path", ROOT, "--group-by", NAME]),
        mcp: Some(McpProbe("specforge.model", "group_by", no_arguments)),
        names: &["extension", "none", "flat"],
    },
    Probe {
        label: "model fields",
        cli: Some(&["model", "--path", ROOT, "--fields", NAME]),
        mcp: Some(McpProbe("specforge.model", "fields", no_arguments)),
        names: &["none", "keys", "all", "some"],
    },
    Probe {
        label: "outline format",
        cli: Some(&["outline", "--path", ROOT, "--format", NAME]),
        mcp: Some(McpProbe(
            "specforge.outline_extensions",
            "format",
            no_arguments,
        )),
        names: &["markdown", "mermaid", "dot", "json", "svg"],
    },
    Probe {
        label: "outline fields",
        cli: Some(&["outline", "--path", ROOT, "--fields", NAME]),
        mcp: Some(McpProbe(
            "specforge.outline_extensions",
            "fields",
            no_arguments,
        )),
        names: &["none", "keys", "all", "most"],
    },
    Probe {
        label: "outline deps",
        cli: Some(&["outline", "--path", ROOT, "--deps", NAME]),
        mcp: Some(McpProbe(
            "specforge.outline_extensions",
            "deps",
            no_arguments,
        )),
        names: &["direct", "effective", "full", "transitive"],
    },
    Probe {
        label: "render format",
        cli: None,
        mcp: Some(McpProbe("specforge.render", "format", no_arguments)),
        names: &["json", "graph", "dot", "context", "brief", "yaml"],
    },
    Probe {
        label: "query format",
        cli: Some(&["query", "alpha", "--path", ROOT, "--format", NAME]),
        mcp: Some(McpProbe("specforge.query", "format", entity_alpha)),
        names: &["graph", "context", "brief", "json", "dot", "yaml"],
    },
    Probe {
        label: "coverage status_filter",
        cli: None,
        mcp: Some(McpProbe(
            "specforge.coverage",
            "status_filter",
            no_arguments,
        )),
        names: &["covered", "partial", "uncovered", "coverd"],
    },
    Probe {
        label: "find_references direction",
        cli: None,
        mcp: Some(McpProbe(
            "specforge.find_references",
            "direction",
            entity_alpha,
        )),
        names: &["incoming", "outgoing", "both", "sideways"],
    },
    Probe {
        label: "analyze pass",
        cli: Some(&["analyze", NAME, "--path", ROOT]),
        mcp: Some(McpProbe("specforge.analyze", "pass", no_arguments)),
        names: &[
            "all",
            "coverage",
            "contracts",
            "@specforge/testing:coverage",
            "nosuch",
        ],
    },
];

/// What a CLI command printed, and its exit code.
struct Run {
    code: Option<i32>,
    stdout: String,
}

/// `specforge <args>` (naming its own `--path`) on `project`.
fn cli(project: &TestProject, args: &[String]) -> Run {
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let out = Cli::on(project).bare(&args);
    Run {
        code: Some(out.code),
        stdout: out.stdout,
    }
}

/// `line` with `{root}` and `{name}` filled in; with no name, `{name}` and
/// the flag before it are dropped.
fn command_line(line: &[&str], root: &Path, name: Option<&str>) -> Vec<String> {
    let mut args: Vec<String> = Vec::new();
    for token in line {
        match *token {
            ROOT => args.push(root.to_str().unwrap().to_string()),
            NAME => match name {
                Some(name) => args.push(name.to_string()),
                None => {
                    if args.last().is_some_and(|flag| flag.starts_with("--")) {
                        args.pop();
                    }
                }
            },
            other => args.push(other.to_string()),
        }
    }
    args
}

/// The `-h` lines of the flag (or positional) `line` passes the name with,
/// joined into one.
fn help_line(project: &TestProject, line: &[&str]) -> String {
    let at = line.iter().position(|token| *token == NAME).unwrap();
    let previous = line[at - 1];
    let (command, wanted) = if previous.starts_with("--") {
        (line[0], format!("{previous} <"))
    } else {
        (line[0], "[PASS]".to_string())
    };
    let help = cli(project, &[command.to_string(), "-h".to_string()]).stdout;
    let mut lines = help
        .lines()
        .skip_while(|l| !l.trim_start().starts_with(&wanted));
    let mut joined = lines.next().unwrap_or("").trim().to_string();
    // Help text clap wraps or moves under the flag is indented deeper than
    // any flag (2 or 6 spaces).
    for next in lines {
        let indent = next.len() - next.trim_start().len();
        if next.trim().is_empty() || indent <= 6 {
            break;
        }
        joined.push(' ');
        joined.push_str(next.trim());
    }
    joined.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The MCP input schema's `enum` and `default` of `tool`'s `argument`.
fn advertised(tool: &str, argument: &str) -> String {
    let spec = specforge_mcp::tools::CORE_TOOLS
        .iter()
        .find(|spec| spec.name() == tool)
        .unwrap_or_else(|| panic!("no tool {tool}"));
    let property = spec.input_schema()["properties"][argument].clone();
    let required = spec.input_schema()["required"]
        .as_array()
        .is_some_and(|names| names.iter().any(|name| name == argument));
    format!(
        "enum {} default {}{}",
        property.get("enum").unwrap_or(&Value::Null),
        property.get("default").unwrap_or(&Value::Null),
        if required { " (required)" } else { "" }
    )
}

/// An MCP result as the snapshot shows it: `ok`, or the refusal's code,
/// message and suggestion.
fn mcp_outcome(result: &Value, root: &Path) -> String {
    let Some(error) = result.get("isError").or_else(|| result.get("error")) else {
        return "ok".to_string();
    };
    let mut text = format!(
        "refused: {}: {}",
        error["code"].as_str().unwrap_or("?"),
        error["message"].as_str().unwrap_or("?")
    );
    let suggestion = error["data"]["suggestion"]
        .as_str()
        .or_else(|| error["diagnostic"]["suggestion"].as_str());
    if let Some(suggestion) = suggestion {
        write!(text, " (suggestion: {suggestion})").unwrap();
    }
    if let Some(data) = error["data"].as_object()
        && let Some(renderers) = data.get("available_renderers")
    {
        write!(text, " (available_renderers: {renderers})").unwrap();
    }
    text.replace(root.to_str().unwrap(), "[ROOT]")
}

/// Which of `names` gave what the absent argument gave.
fn defaults<T: PartialEq>(absent: &T, outputs: &[(&str, T)]) -> String {
    let matching: Vec<&str> = outputs
        .iter()
        .filter(|(_, output)| output == absent)
        .map(|(name, _)| *name)
        .collect();
    if matching.is_empty() {
        "(none)".to_string()
    } else {
        matching.join(" = ")
    }
}

/// What each surface answered for one probe.
struct Probed {
    probe: &'static Probe,
    root: PathBuf,
    help: Option<String>,
    advertised: Option<String>,
    /// The CLI run with the argument absent, then one per name.
    cli: Option<(Run, Vec<Run>)>,
    /// The MCP result with the argument absent, then one per name.
    mcp: Option<(Value, Vec<Value>)>,
}

/// Run `probe` on each surface it has, over its own copy of rv1.
fn run_probe(probe: &'static Probe) -> Probed {
    let mut tmp = served(rv1());
    let root = tmp.root().to_path_buf();
    let root = root.as_path();
    let help = probe.cli.map(|line| help_line(tmp.project(), line));
    let cli = probe.cli.map(|line| {
        let absent = cli(tmp.project(), &command_line(line, root, None));
        let runs = probe
            .names
            .iter()
            .map(|name| cli(tmp.project(), &command_line(line, root, Some(name))))
            .collect();
        (absent, runs)
    });
    let advertised = probe
        .mcp
        .as_ref()
        .map(|McpProbe(tool, argument, _)| advertised(tool, argument));
    let mcp = probe.mcp.as_ref().map(|McpProbe(tool, argument, base)| {
        let mut calls = vec![json!({"name": tool, "arguments": base()})];
        for name in probe.names {
            let mut arguments = base();
            arguments[argument] = json!(name);
            calls.push(json!({"name": tool, "arguments": arguments}));
        }
        let mut results = mcp_calls(&mut tmp, &calls).into_iter();
        let absent = results.next().unwrap();
        (absent, results.collect())
    });
    Probed {
        probe,
        root: root.to_path_buf(),
        help,
        advertised,
        cli,
        mcp,
    }
}

/// Every probe whose label `wanted` selects, run in parallel.
fn run_probes(wanted: impl Fn(&str) -> bool) -> Vec<Probed> {
    std::thread::scope(|scope| {
        let handles: Vec<_> = PROBES
            .iter()
            .filter(|p| wanted(p.label))
            .map(|p| scope.spawn(move || run_probe(p)))
            .collect();
        handles.into_iter().map(|h| h.join().unwrap()).collect()
    })
}

/// One probe's section of the snapshot.
fn render(probed: &Probed) -> String {
    let probe = probed.probe;
    let root = probed.root.as_path();
    let mut section = format!("## {}\n", probe.label);
    if let Some(help) = &probed.help {
        writeln!(section, "cli -h: {help}").unwrap();
    }
    if let Some(advertised) = &probed.advertised {
        writeln!(section, "mcp schema: {advertised}").unwrap();
    }

    let mut default_line = String::from("default:");
    if let Some((absent, runs)) = &probed.cli {
        let stdouts: Vec<(&str, (Option<i32>, &str))> = probe
            .names
            .iter()
            .zip(runs)
            .map(|(name, run)| (*name, (run.code, run.stdout.as_str())))
            .collect();
        let absent = (absent.code, absent.stdout.as_str());
        write!(default_line, " cli={}", defaults(&absent, &stdouts)).unwrap();
    }
    if let Some((absent, results)) = &probed.mcp {
        let named: Vec<(&str, Value)> = probe
            .names
            .iter()
            .copied()
            .zip(results.iter().cloned())
            .collect();
        write!(default_line, " mcp={}", defaults(absent, &named)).unwrap();
    }
    section.push_str(&default_line);
    section.push('\n');

    for (i, name) in probe.names.iter().enumerate() {
        let cli = probed
            .cli
            .as_ref()
            .map_or("-".to_string(), |(_, runs)| match runs[i].code {
                Some(code) => code.to_string(),
                None => "signal".to_string(),
            });
        let mcp = probed.mcp.as_ref().map_or("-".to_string(), |(_, results)| {
            mcp_outcome(&results[i], root)
        });
        writeln!(section, "{name:<28} cli={cli:<3} mcp={mcp}").unwrap();
    }
    section
}

/// The names a table accepts and its default's name.
struct Names {
    accepted: Vec<&'static str>,
    default: Option<&'static str>,
}

fn names<T: Copy + PartialEq>(table: &OptionTable<T>) -> Names {
    Names {
        accepted: table.accepted().collect(),
        default: table.default_name(),
    }
}

/// The probes both surfaces answer from an option table: the probe's
/// label, the table the CLI parses with, the table MCP parses with.
/// `specforge export` takes every export format; `specforge.export` the
/// agent formats (dot is `specforge.render`'s).
fn paired() -> Vec<(&'static str, Names, Names)> {
    vec![
        ("export format", names(&FORMAT), names(&AGENT_FORMAT)),
        ("model format", names(&MODEL_FORMAT), names(&MODEL_FORMAT)),
        ("model group_by", names(&GROUP_BY), names(&GROUP_BY)),
        ("model fields", names(&MODEL_FIELDS), names(&MODEL_FIELDS)),
        (
            "outline format",
            names(&OUTLINE_FORMAT),
            names(&OUTLINE_FORMAT),
        ),
        (
            "outline fields",
            names(&OUTLINE_FIELDS),
            names(&OUTLINE_FIELDS),
        ),
        ("outline deps", names(&DEPS), names(&DEPS)),
        ("query format", names(&AGENT_FORMAT), names(&AGENT_FORMAT)),
    ]
}

/// MCP-only probes `specforge-mcp`'s `tests/option_tables.rs` holds to
/// their option table.
const LINKED_ELSEWHERE: [&str; 3] = [
    "render format",
    "coverage status_filter",
    "find_references direction",
];

#[test]
fn enumerated_options_today() {
    let mut linked: Vec<&str> = paired().iter().map(|(label, _, _)| *label).collect();
    linked.extend(LINKED_ELSEWHERE);
    let sections: Vec<String> = run_probes(|label| !linked.contains(&label))
        .iter()
        .map(render)
        .collect();
    insta::assert_snapshot!("enumerated_options_today", sections.join("\n"));
}

#[specforge_test_macros::test(
    behavior = "name_enumerated_options_once",
    verify = "the CLI and MCP accept the same names for each enumerated argument"
)]
fn cli_and_mcp_accept_the_same_names() {
    let paired = paired();
    let probed = run_probes(|label| paired.iter().any(|(l, _, _)| *l == label));
    assert_eq!(probed.len(), paired.len());
    for (label, cli_table, mcp_table) in &paired {
        let probed = probed.iter().find(|p| p.probe.label == *label).unwrap();
        let probe = probed.probe;
        let (cli_absent, cli_runs) = probed.cli.as_ref().unwrap();
        let (mcp_absent, mcp_results) = probed.mcp.as_ref().unwrap();
        for name in cli_table.accepted.iter().chain(&mcp_table.accepted) {
            assert!(probe.names.contains(name), "{label}: {name} is not probed");
        }

        // A name is accepted exactly where the surface's table accepts it,
        // so a name both tables list is accepted on both.
        for (i, name) in probe.names.iter().enumerate() {
            let cli_accepts = cli_runs[i].code == Some(0);
            let mcp_accepts = mcp_results[i].get("isError").is_none();
            assert_eq!(
                cli_accepts,
                cli_table.accepted.contains(name),
                "{label}: CLI on {name} (exit {:?})",
                cli_runs[i].code
            );
            assert_eq!(
                mcp_accepts,
                mcp_table.accepted.contains(name),
                "{label}: MCP on {name}: {}",
                mcp_results[i]
            );
        }

        // One default, the table's, on both surfaces.
        assert_eq!(cli_table.default, mcp_table.default, "{label}");
        let default = cli_table.default.unwrap();
        let at = probe.names.iter().position(|n| *n == default).unwrap();
        assert_eq!(cli_absent.code, Some(0), "{label}");
        assert_eq!(
            cli_absent.stdout, cli_runs[at].stdout,
            "{label}: the CLI's absent argument is {default}"
        );
        assert_eq!(
            mcp_absent, &mcp_results[at],
            "{label}: MCP's absent argument is {default}"
        );
    }
}
