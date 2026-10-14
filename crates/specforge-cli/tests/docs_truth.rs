//! C2-06: entity-kind counts in the docs are derived claims — they rot
//! silently when an extension adds a kind. Derive the number from the
//! builtin manifests and pin the docs to it.

use crate::support::{Cli, TestProject};
use specforge_repo_root::repo_root;
use std::collections::BTreeSet;

fn manifest_domain_kinds() -> BTreeSet<String> {
    let mut kinds = BTreeSet::new();
    // The builtins' pinned wire declarations (the SDK greet fixture beside
    // them is not a builtin).
    let pinned = repo_root().join("crates/specforge-component/tests/declarations");
    for entry in std::fs::read_dir(&pinned).expect("pinned declarations") {
        let dir = entry.unwrap().path();
        let handshake: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(dir.join("handshake.json")).expect("pinned handshake"),
        )
        .expect("valid handshake JSON");
        if !handshake["name"]
            .as_str()
            .is_some_and(|n| n.starts_with("@specforge/"))
        {
            continue;
        }
        let path = dir.join("describe_entities.json");
        let raw =
            std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let doc: serde_json::Value = serde_json::from_str(&raw).expect("valid describe JSON");
        for item in doc["items"].as_array().expect("items array") {
            let keyword = item["keyword"]
                .as_str()
                .or_else(|| item["name"].as_str())
                .expect("kind keyword");
            kinds.insert(keyword.to_string());
        }
    }
    kinds
}

/// The canonical repository URL is defined once, as the workspace's Cargo
/// `repository` (ADR 0004 D6-b); the VS Code manifest can't inherit it, so
/// it is pinned to it here.
#[test]
fn vscode_manifest_names_the_workspace_repository() {
    let raw = std::fs::read_to_string(repo_root().join("integrations/vscode/package.json"))
        .expect("integrations/vscode/package.json");
    let manifest: serde_json::Value = serde_json::from_str(&raw).expect("valid package.json");
    assert_eq!(
        manifest["repository"]["url"].as_str(),
        Some(env!("CARGO_PKG_REPOSITORY"))
    );
}

/// The VS Code extension types `.spec` files with the formatter's default
/// indentation (ADR 0021 D1), so typing indents as `specforge format` will
/// in a project without `.specforgefmt.toml`.
#[test]
fn vscode_manifest_indents_spec_files_as_the_formatter_does() {
    let raw = std::fs::read_to_string(repo_root().join("integrations/vscode/package.json"))
        .expect("integrations/vscode/package.json");
    let manifest: serde_json::Value = serde_json::from_str(&raw).expect("valid package.json");
    let defaults = &manifest["contributes"]["configurationDefaults"]["[specforge]"];
    let formatter = specforge_formatter::FormatConfig::default();

    assert_eq!(
        defaults["editor.tabSize"].as_u64(),
        Some(formatter.indent_width as u64)
    );
    assert_eq!(
        defaults["editor.insertSpaces"].as_bool(),
        Some(!formatter.use_tabs)
    );
}

/// One CLI invocation found in the VS Code extension's sources: the argv
/// elements, each either a string literal (`Some`) or something computed at
/// runtime (`None`, or a template literal kept as its literal prefix).
#[derive(Debug)]
struct VscodeCliCall {
    site: String,
    args: Vec<Option<String>>,
}

/// Every argv the VS Code extension hands the `specforge` binary: the array
/// literal passed to `runCliCommand`, `runCliCommandOutput`, `execCli`, or
/// `cp.execFile(findCliBinary(), [...])`.
fn vscode_cli_calls() -> Vec<VscodeCliCall> {
    const CALLERS: [&str; 4] = [
        "runCliCommand(",
        "runCliCommandOutput(",
        "execCli(",
        "findCliBinary(),",
    ];
    let src = repo_root().join("integrations/vscode/src");
    let mut calls = Vec::new();
    for entry in std::fs::read_dir(&src).expect("integrations/vscode/src") {
        let path = entry.unwrap().path();
        if path.extension().and_then(|e| e.to_str()) != Some("ts") {
            continue;
        }
        let text = std::fs::read_to_string(&path).expect("read .ts source");
        for caller in CALLERS {
            for (at, _) in text.match_indices(caller) {
                let rest = text[at + caller.len()..].trim_start();
                let Some(body) = rest.strip_prefix('[') else {
                    continue; // a definition or a call with a non-literal argv
                };
                let body = &body[..body.find(']').expect("closing bracket")];
                let line = text[..at].lines().count();
                let args = body
                    .split(',')
                    .map(str::trim)
                    .filter(|a| !a.is_empty())
                    .map(|a| {
                        if let Some(lit) = a.strip_prefix('"').and_then(|a| a.strip_suffix('"')) {
                            Some(lit.to_string())
                        } else {
                            // `--format=${format}` → its literal prefix `--format=`
                            a.strip_prefix('`')
                                .map(|tpl| tpl.split("${").next().unwrap_or("").to_string())
                        }
                    })
                    .collect();
                calls.push(VscodeCliCall {
                    site: format!("{}:{line}", path.file_name().unwrap().to_string_lossy()),
                    args,
                });
            }
        }
    }
    calls
}

fn specforge_help(args: &[&str]) -> String {
    let mut argv = args.to_vec();
    argv.push("--help");
    let out = Cli::on(&TestProject::new().without_config()).bare(&argv);
    assert_eq!(
        out.code,
        0,
        "`specforge {} --help` failed: {}",
        args.join(" "),
        out.stderr
    );
    out.stdout
}

/// The VS Code extension shells out to the CLI. Every subcommand it names
/// must exist, and every `--flag` it passes must be one that subcommand
/// takes; otherwise the editor command fails at runtime with nothing to
/// show (it called `specforge inspect` and `specforge list`, neither of
/// which exists).
#[test]
fn vscode_extension_invokes_only_existing_cli_commands() {
    let calls = vscode_cli_calls();
    assert!(
        calls.len() >= 10,
        "the scan found too few CLI calls ({}); the call helpers were renamed",
        calls.len()
    );
    let top = specforge_help(&[]);
    let subcommands: BTreeSet<String> = top
        .lines()
        .skip_while(|l| !l.starts_with("Commands:"))
        .skip(1)
        .take_while(|l| l.starts_with("  "))
        .filter_map(|l| l.split_whitespace().next().map(str::to_string))
        .collect();

    let mut problems = Vec::new();
    for call in &calls {
        let Some(Some(sub)) = call.args.first() else {
            problems.push(format!("{}: the subcommand is not a literal", call.site));
            continue;
        };
        if !subcommands.contains(sub) {
            problems.push(format!("{}: `specforge {sub}` does not exist", call.site));
            continue;
        }
        let help = specforge_help(&[sub]);
        for flag in call.args.iter().skip(1).flatten() {
            if !flag.starts_with("--") {
                continue;
            }
            let name = flag.split('=').next().unwrap();
            if !help.contains(&format!("{name} ")) && !help.contains(&format!("{name}\n")) {
                problems.push(format!(
                    "{}: `specforge {sub}` takes no `{name}` flag",
                    call.site
                ));
            }
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

#[test]
fn entity_model_doc_count_matches_builtin_manifests() {
    let kinds = manifest_domain_kinds();
    let count = kinds.len();
    assert_eq!(
        count, 22,
        "builtin manifest kinds drifted; update docs/entity-model.md"
    );

    let doc = std::fs::read_to_string(repo_root().join("docs/entity-model.md"))
        .expect("docs/entity-model.md");
    let claimed = format!("{count}, declared by the four builtin extensions");
    assert!(
        doc.contains(&claimed),
        "entity-model.md must claim '{claimed}' — the manifest-derived count changed"
    );
    let total = 2 + count;
    assert!(
        doc.contains(&format!("= {total} entity kinds")),
        "entity-model.md total must equal 2 structural + {count} domain = {total}"
    );
}

/// One declared edge type: the extension that declares it and the kinds it
/// joins (`None` for the two software edges that declare no kinds).
struct DeclaredEdge {
    extension: String,
    source: Option<String>,
    target: Option<String>,
}

/// Every builtin's pinned `describe_edges.json`, by label.
fn declared_edges() -> std::collections::BTreeMap<String, DeclaredEdge> {
    let mut edges = std::collections::BTreeMap::new();
    let pinned = repo_root().join("crates/specforge-component/tests/declarations");
    for entry in std::fs::read_dir(&pinned).expect("pinned declarations") {
        let dir = entry.unwrap().path();
        let handshake: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(dir.join("handshake.json")).expect("pinned handshake"),
        )
        .expect("valid handshake JSON");
        let Some(name) = handshake["name"]
            .as_str()
            .filter(|n| n.starts_with("@specforge/"))
        else {
            continue;
        };
        let path = dir.join("describe_edges.json");
        let Ok(raw) = std::fs::read_to_string(&path) else {
            continue;
        };
        let doc: serde_json::Value = serde_json::from_str(&raw).expect("valid describe JSON");
        for item in doc["items"].as_array().expect("items array") {
            let kind = |key: &str| item[key].as_str().map(str::to_string);
            edges.insert(
                item["label"].as_str().expect("edge label").to_string(),
                DeclaredEdge {
                    extension: name.to_string(),
                    source: kind("source_kind"),
                    target: kind("target_kind"),
                },
            );
        }
    }
    edges
}

/// The backticked tokens of a markdown cell.
fn backticked(cell: &str) -> Vec<&str> {
    cell.split('`').skip(1).step_by(2).collect()
}

/// A cell that names exactly one kind (`` `invariant` ``, `feature (product)`,
/// `[feature](feature.md)`); `None` for `a/b`, `any` and prose.
fn single_kind(cell: &str) -> Option<String> {
    let cell = cell.trim();
    let cell = match cell.strip_prefix('[') {
        Some(link) => link.split(']').next()?,
        None => cell,
    };
    let cell = cell.trim_matches('`');
    let word = cell.split(" (").next()?;
    (!word.is_empty()
        && word
            .chars()
            .all(|c| c.is_ascii_lowercase() || c == '_' || c == '-')
        && word != "any")
        .then(|| word.to_string())
}

/// A markdown table row: its line number and cells.
type Row = (usize, Vec<String>);

/// The markdown tables of `text`: (line of the header, header cells, rows).
fn tables(text: &str) -> Vec<(usize, Vec<String>, Vec<Row>)> {
    let cells = |line: &str| -> Vec<String> {
        line.trim()
            .trim_matches('|')
            .split('|')
            .map(|cell| cell.trim().to_string())
            .collect()
    };
    let lines: Vec<&str> = text.lines().collect();
    let mut found = Vec::new();
    let mut at = 0;
    while at < lines.len() {
        let is_table = lines[at].trim_start().starts_with('|')
            && lines
                .get(at + 1)
                .is_some_and(|next| next.trim_start().starts_with("|-") || next.contains("---"));
        if !is_table {
            at += 1;
            continue;
        }
        let header = cells(lines[at]);
        let mut rows = Vec::new();
        let mut row = at + 2;
        while row < lines.len() && lines[row].trim_start().starts_with('|') {
            rows.push((row + 1, cells(lines[row])));
            row += 1;
        }
        found.push((at + 1, header, rows));
        at = row;
    }
    found
}

/// The entity reference names the edge types the extensions declare, in the
/// direction they declare them: a cell in an `Edge` column is a declared
/// label, an entity page's outgoing and incoming rows match the label's
/// kinds, `Creates `X` edges` names a declared label, and an extension's edge
/// heading counts its declared labels.
#[test]
fn the_entity_docs_name_declared_edge_types() {
    let edges = declared_edges();
    let root = repo_root();
    let mut files = vec![
        "docs/entity-model.md".to_string(),
        "docs/extension-inventory.md".to_string(),
    ];
    for entry in std::fs::read_dir(root.join("docs/entities")).expect("docs/entities") {
        let path = entry.unwrap().path();
        if path.extension().is_some_and(|e| e == "md") {
            files.push(format!(
                "docs/entities/{}",
                path.file_name().unwrap().to_string_lossy()
            ));
        }
    }

    let mut problems = Vec::new();
    for file in &files {
        let text =
            std::fs::read_to_string(root.join(file)).unwrap_or_else(|e| panic!("{file}: {e}"));
        let page_kind = file
            .strip_prefix("docs/entities/")
            .map(|page| page.trim_end_matches(".md").replace('-', "_"));

        for (_, header, rows) in tables(&text) {
            let column = |name: &str| header.iter().position(|h| h == name);
            let Some(edge_col) = header.iter().position(|h| h.contains("Edge")) else {
                continue;
            };
            for (line, row) in rows {
                let Some(cell) = row.get(edge_col) else {
                    continue;
                };
                for label in backticked(cell) {
                    let Some(edge) = edges.get(label) else {
                        problems.push(format!("{file}:{line}: `{label}` is no declared edge type"));
                        continue;
                    };
                    // (direction, the other end's kind) as the row states it.
                    let stated = if let (Some(dir), Some(entity)) =
                        (column("Direction"), column("Entity"))
                    {
                        row.get(dir)
                            .zip(row.get(entity))
                            .map(|(d, e)| (d.as_str() == "outgoing", single_kind(e)))
                    } else if let Some(to) = column("To").filter(|_| page_kind.is_some()) {
                        row.get(to).map(|e| (true, single_kind(e)))
                    } else if let Some(from) = column("From").filter(|_| page_kind.is_some()) {
                        row.get(from).map(|e| (false, single_kind(e)))
                    } else {
                        None
                    };
                    let (Some(page), Some((outgoing, other))) = (&page_kind, stated) else {
                        // A table of the extension pages: both ends are columns.
                        let (from, to) = (
                            column("From").or(column("Source")),
                            column("To").or(column("Target")),
                        );
                        if let (Some(from), Some(to)) = (from, to)
                            && let (Some(f), Some(t)) = (
                                row.get(from).and_then(|c| single_kind(c)),
                                row.get(to).and_then(|c| single_kind(c)),
                            )
                            && let (Some(source), Some(target)) = (&edge.source, &edge.target)
                            && (&f, &t) != (source, target)
                        {
                            problems.push(format!(
                                "{file}:{line}: `{label}` joins {source} -> {target}, the row says {f} -> {t}"
                            ));
                        }
                        continue;
                    };
                    let (Some(source), Some(target), Some(other)) =
                        (&edge.source, &edge.target, other)
                    else {
                        continue;
                    };
                    let (here, there) = if outgoing {
                        (source, target)
                    } else {
                        (target, source)
                    };
                    if here != page || *there != other {
                        problems.push(format!(
                            "{file}:{line}: `{label}` is {source} -> {target}; the {} row of `{page}` says the other end is `{other}`",
                            if outgoing { "outgoing" } else { "incoming" }
                        ));
                    }
                }
            }
        }

        for (index, line) in text.lines().enumerate() {
            let mut rest = line;
            while let Some(at) = rest.find("Creates `") {
                rest = &rest[at + "Creates `".len()..];
                let label = rest.split('`').next().unwrap_or_default();
                if !edges.contains_key(label) {
                    problems.push(format!(
                        "{file}:{}: `Creates `{label}` edges` names no declared edge type",
                        index + 1
                    ));
                }
            }
            if let Some(heading) = line.strip_prefix("### @specforge/")
                && let Some((extension, count)) = heading.split_once(" Edges (")
                && let Ok(count) = count.trim_end_matches(')').parse::<usize>()
            {
                let declared = edges
                    .values()
                    .filter(|e| e.extension == format!("@specforge/{extension}"))
                    .count();
                if declared != count {
                    problems.push(format!(
                        "{file}:{}: @specforge/{extension} declares {declared} edge types, the heading says {count}",
                        index + 1
                    ));
                }
            }
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}
