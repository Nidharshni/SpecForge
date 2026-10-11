# Extending SpecForge — Zero to Production

A guided, end-to-end tutorial that takes you from an empty directory to a shipped,
installable extension: domain vocabulary, validation rules, and compiler passes —
all authored in Rust, compiled to Wasm, and loaded by every SpecForge surface.

This is the extension **tutorial**. Companion reading:

- **[Authoring `.spec` Files](authoring-spec-files.md)** — the spec-authoring tutorial (start there if you have never written a `.spec` file).
- **[SDK Reference](../extension-sdk.md)** — the full attribute and builder API.
- **[Extension Protocol](../extension-protocol.md)** — the wire protocol underneath: `__handshake` / `__describe`.

> 💡 **Every code block in this guide is real.** The snippets mirror
> [`fixtures/greet-extension/`](../../fixtures/greet-extension/) and
> [`extensions/formal/`](../../extensions/formal/) — extensions that ship in
> this repository and are exercised by its test suite.

---

## Table of contents

- [Act I — What an extension is](#act-i--what-an-extension-is)
  - [The bet: vocabulary over forks](#the-bet-vocabulary-over-forks)
  - [What an extension can contribute](#what-an-extension-can-contribute)
- [Act II — Scaffold](#act-ii--scaffold)
- [Act III — Vocabulary: kinds, fields, edges, rules](#act-iii--vocabulary-kinds-fields-edges-rules)
  - [A kind](#a-kind)
  - [A validation rule](#a-validation-rule)
- [Act IV — Contracts and compiler passes](#act-iv--contracts-and-compiler-passes)
  - [Structured conditions](#structured-conditions)
  - [Your first compiler pass](#your-first-compiler-pass)
  - [Ordering passes](#ordering-passes)
  - [A command](#a-command)
- [Act V — Build, install, test](#act-v--build-install-test)
  - [Build to Wasm](#build-to-wasm)
  - [Install into a project](#install-into-a-project)
  - [Test without a runtime](#test-without-a-runtime)
- [Act VI — Where to go next](#act-vi--where-to-go-next)

---

# Act I — What an extension is

## The bet: vocabulary over forks

SpecForge's built-in entity kinds — `type`, `behavior`, `feature`, `invariant`,
`event`, `port` — cover general software specification. The moment your domain
talks about something else (hardware registers, compliance controls, financial
instruments), you have three options:

1. **Shoehorn** your domain into existing kinds. Loses validation and tooling.
2. **Fork the compiler.** Loses updates, splits the ecosystem.
3. **Extend it.** Declare your vocabulary in an extension; the compiler,
   LSP, and MCP server all learn it at load time.

SpecForge is built on option 3 — the protocol is the *only* mechanism for adding
domain vocabulary. There is no bypass. The four built-in extensions
(`@specforge/product`, `@specforge/software`, `@specforge/governance`,
`@specforge/formal`) are not privileged; they ride the same protocol you are
about to use.

## What an extension can contribute

| Contribution | What it gives the graph |
|---|---|
| **Entity kinds** | New first-class nouns (`greeting`, `compliance_control`) with fields, constraints, and verify support |
| **Edges** | Typed relationships between kinds, with visual styling for diagrams |
| **Validation rules** | Structural checks the compiler runs on every build |
| **Compiler passes** | Analysis that runs during `specforge analyze`, receiving the resolved graph and returning diagnostics |
| **Feature flags** | User-configurable behavior, read from `specforge.json` |
| **Commands** | Queries over the compiled graph: `specforge <ext> <command>` on the CLI, the tool `specforge.<ext>.<command>` over MCP |

---

# Act II — Scaffold

```console
$ specforge new --extension @you/my-ext
```

That scaffolds a ready-to-build project:

```text
my-ext/
├── Cargo.toml              # cdylib; depends on specforge-extension-sdk
├── .cargo/config.toml      # pins wasm32-wasip2
└── src/lib.rs              # a working extension: builds and describes as-is
```

The scaffold compiles and installs *before you change a line* — the generated
`src/lib.rs` already declares a working kind. Treat it as a smoke test: build it
(Act V) before you start editing.

---

# Act III — Vocabulary: kinds, fields, edges, rules

An extension is one struct implementing `Contributions`. Everything is declared
through the builder passed to `contribute` — the SDK generates the protocol
exports (`__handshake`, `__describe`) from what you declare. You never write
protocol JSON by hand.

## A kind

This is the greet extension, complete:

```rust
use specforge_extension_sdk::prelude::*;

#[specforge_extension_sdk::extension(
    name = "@you/greet",
    version = "0.1.0",
    short = "Friendly greetings"
)]
struct Greet;

impl Contributions for Greet {
    fn contribute(c: &mut ContributionsBuilder) {
        c.kind("greeting", |k| {
            k.description("A friendly greeting").testable(false);
            k.field("style", |f| {
                f.field_type(FieldType::Enum);
                f.enum_values(&["warm", "formal"]);
                f.required();
            });
        });
    }
}
```

To make it a component, wire the bridge (in the same file):

```rust
specforge_extension_sdk::component_guest!(build = specforge_extension_build);
```

The `#[extension]` attribute generates `specforge_extension_build()`;
`component_guest!` serves the protocol and every export you declare with its
handler: commands ([A command](#a-command)), MCP tools and resources,
compiler passes (`p.run`), collectors (`k.collect`), custom rules
(`r.validate`), scanners (`a.scan`) and the migration hook. The SDK decodes
each export's input and encodes its answer, as the protocol's types
(`docs/extension-protocol.md`, "Operate"). `specforge new --extension`
scaffolds all of this for you.

Three things happened:

1. **`greeting` is now a keyword.** Any `.spec` file in a project with this
   extension installed can write `greeting hello { style "warm" }`, and the
   parser, resolver, and validators all understand it.
2. **`style` is a constrained enum field.** The compiler rejects values outside
   `warm | formal` at compile time — before your rule engine ever runs.
3. **`.testable(false)`** tells the tooling that greetings carry no verify
   obligations, so `specforge stats` and `specforge analyze coverage` exclude
   them from coverage accounting.

Fields can also be **references** (`field_type(Reference)` / reference lists)
that create typed edges between entities, and they can carry constraints
(`required`, enums, file references).

## A validation rule

Rules are structural checks the compiler runs on every graph build:

```rust
c.rule("E901", |r| {
    r.check(CheckKind::FieldValueConstraint);
    r.target_kind("greeting");
    r.field("style");
    r.constraint(|fc| {
        fc.kind(ConstraintKind::Matches);
        fc.pattern("^(warm|formal)$");
    });
    r.severity(ValidationSeverity::Error);
    r.message_template("greeting '{id}' has unknown style");
});
```

A rule's code names your extension's diagnostic, so it comes from the range
third-party extensions own: `E900`-`E998` for an error, `W900`-`W998` for a
warning, `I900`-`I998` for an info, with the prefix matching the rule's
severity (`E901` above is an error). A rule that uses a code core or another
extension owns, or a prefix that contradicts its severity, still runs, and the
host adds a `W150` warning naming it; under `--strict` that warning fails the
check.

Rules are declarative patterns — they run in-process, cost nothing at Wasm
boundaries, and are the right tool for per-field shape checks. When a check
needs the *relationships between* entities rather than the shape of one entity,
that is a compiler pass (Act IV).

The host checks each rule's shape when your extension loads (and in
`specforge extension validate` / `publish`), against the table in
[the protocol reference](../extension-protocol.md#category-validation_rules):

- **W112** — the rule cannot work as declared and is not registered: an unknown
  `check`, or a `field`, constraint, `edge_type` or `wasm_function` its check
  requires and lacks (a `cycle_detection` rule needs an `edge_type`, a
  `verify_kind_allowlist` rule a constraint with values), an empty values list,
  a regex that does not compile, a rule reading `verify` statements on a kind
  that accepts none, or a `custom` function that cannot answer the load probe.
- **W147** — the rule sets a property its check does not read (an `edge_type`
  on a field check, a `constraint` on an edge check, a `wasm_function` on a
  declarative check, a constraint kind, `pattern` or `values` its check does not
  read). It is registered without it, so remove the property or pick the check
  that reads it.
- **W021** — the rule's `target_kind` or `edge_type` is neither your
  extension's, a declared peer's nor its `target_extension`'s. A rule on
  another extension's kind that yours works without says so with
  `r.target_extension("@their/ext")`: while that extension is not loaded the
  rule is inert and silent; loaded without the kind, it is W021.
- **W148** — at check time, a `custom` rule's function failed on some entities
  (a trap, or an answer that is not a verdict), so they were not checked. One
  per rule per check; its data lists every entity with its error.

---

# Act IV — Contracts and compiler passes

## Structured conditions

`@specforge/formal` extends `behavior` with contract clauses — `requires`,
`ensures`, `maintains` — that reference invariants:

```spec
behavior publish "Publish" {
  title "Publish"

  requires {
    graph_valid "the graph compiles without errors"
  }
  ensures {
    artifacts_written "output files exist on disk"
  }

  verify contract "Publish: artifacts land on disk — artifacts_written"
}
```

These are not comments. Each clause item becomes a typed edge
(`BehaviorRequiresInvariant`, `BehaviorEnsuresInvariant`) in the graph, which
is what makes contract analysis possible.

## Your first compiler pass

A compiler pass runs during `specforge analyze`, receives a snapshot of the
resolved project, and returns diagnostics. You write a plain function:

```rust
use specforge_extension_sdk::{PassDiagnostic, PassEntity, PassInput};

fn pass_condition_check(input: &PassInput) -> Vec<PassDiagnostic> {
    let mut findings = Vec::new();
    for entity in &input.entities {
        if entity.kind != "behavior" {
            continue;
        }
        let requires = non_empty(entity, "requires");
        let ensures = non_empty(entity, "ensures");
        if requires && !ensures {
            findings.push(
                PassDiagnostic::warning(
                    "W096",
                    format!(
                        "behavior '{}' declares requires but no ensures",
                        entity.id
                    ),
                )
                .with_suggestion(
                    "add an ensures clause: a caller's obligation must buy a guarantee",
                ),
            );
        }
    }
    findings
}

fn non_empty(entity: &PassEntity, field: &str) -> bool {
    entity
        .fields
        .get(field)
        .is_some_and(|v| !v.trim().is_empty())
}
```

Declare the pass with it, so the host knows it exists and the SDK routes its
`__pass_condition_check` export to it:

```rust
c.pass("condition_check", |p| {
    p.after("resolve").run(pass_condition_check);
});
```

Then run it:

```console
$ specforge analyze --json
```

Design notes, so your passes age well:

- **Passes are pure.** Same snapshot in, same diagnostics out. No IO, no
  clocks. This is what makes them testable without a Wasm runtime and safe to
  reorder.
- **Prefer structural checks.** The v1 pass ABI hands you an entity snapshot
  (id, kind, [field texts](../extension-sdk.md#field-text), edge counts)
  plus the resolved edge list —
  checks that reason about *structure* (symmetry, presence, cycles, coverage)
  are its sweet spot. Semantic verification (SMT-backed `analyze --prove`) is
  a separate, future rung of the formality ladder.
- **Codes are yours.** Pick a prefix your extension owns and stay consistent;
  document your codes the way `@specforge/formal` does
  ([Entity Model → diagnostics](../entity-model.md)).

## Ordering passes

`after` / `before` name other passes. The host topologically sorts each
extension's passes by these constraints before running them — declaration
order in the source file does not matter. Constraints naming unknown passes
(host phases like `"resolve"`, or other extensions' passes) are ignored.
Constraint cycles fall back to declaration order with a warning; don't rely
on that — cycles are bugs.

## A command

A command is a query over the compiled graph. Declare it with the function
that answers it:

```rust
c.command("greetings", |cmd| {
    cmd.title("List greetings")
        .description("Every greeting of a style")
        .arg("style", |a| {
            a.one_of(&["warm", "formal"]).description("Only greetings of this style");
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
```

From that one declaration the SDK gives the host the `surfaces` payload and
routes the `cmd__greetings` export to the handler (call
`c.command_prefix("greet")` first to name it `cmd__greet_greetings`). The
command runs as `specforge greet greetings --style warm` and is served to
agents as the MCP tool `specforge.greet.greetings`.

- **Args are read through the declaration.** Before the handler runs, the
  SDK checks what the caller set against each arg's type (`string`, `path`,
  `flag`, `integer`, `count`, `one_of`) and whether it is `required`: a
  value it cannot use is an `INVALID_INPUT` error, exit 2. `call.str`,
  `call.count`, `call.integer` and `call.flag` read only args the command
  declares, as the type it declares them; anything else panics, so a test
  that runs the command catches the slip (`testing::call_every_command`
  runs every command with every arg set). A `one_of` arg's values are also
  what `--help` and the MCP tool's schema list, and the command line
  refuses any other.
- **`default_value` holds on every surface.** An arg the caller leaves out
  takes it, over MCP too; a `required` arg or a `flag` takes none.
- **The host owns `--path`, `--format` and `--help`.** Don't declare them;
  `call.render` writes your payload as JSON under `--format json` (always,
  over MCP) and your own layout otherwise. `call.fail` writes an error in
  the same format.
- **The graph is all a command reads.** The export gets no file system, no
  environment and no network; `call.today()` is the host's date.

`c.mcp_tool` and `c.mcp_resource` declare an explicit MCP tool or resource
the same way, with a handler taking the tool's arguments or the resource's
URI.

---

# Act V — Build, install, test

## Build to Wasm

```console
$ cargo build --release --target wasm32-wasip2
```

The scaffold's `.cargo/config.toml` already pins the target, so plain
`cargo build --release` works too. The artifact lands in
`target/wasm32-wasip2/release/<your_ext>.wasm`.

## Install into a project

```console
$ specforge add ./target/wasm32-wasip2/release/my-ext.wasm
```

`specforge add` copies the module into the project and registers it in
`specforge.json`. From that moment, `check`, `analyze`, `watch`, the LSP, and
the MCP server all load your vocabulary.

While you iterate, `specforge.json` can name the built file instead, so every
run loads your latest build without re-adding it:

```json
{ "extensions": ["../my-ext/target/wasm32-wasip2/release/my_ext.wasm"] }
```

The path is relative to the project root; the extension is the one the file
declares (`"@acme/my-ext=…/my_ext.wasm"` names it explicitly, and must match).
A file that is missing, does not load, or declares another name is E028.

Verify with a round trip:

```console
$ specforge check          # your validation rules run
$ specforge analyze        # your compiler passes run
$ specforge outline        # your kinds render
```

## Test without a runtime

The SDK's describe output is plain data — build your contributions in a unit
test and assert on them. No Wasm runtime, no SpecForge binary:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn describes_greeting_kind() {
        let mut c = ContributionsBuilder::new(ExtensionMeta::new(
            "@you/greet",
            "0.1.0",
        ));
        Greet::contribute(&mut c);

        let describe = c.describe_response_json("entities").unwrap();
        assert!(describe.contains("\"greeting\""));
    }
}
```

A command runs the same way, as the host would call it:

```rust
let input = CommandInput {
    args: serde_json::json!({"style": "warm"}).as_object().unwrap().clone(),
    format: CommandFormat::Json,
    ..Default::default()
};
let out = c.call_command("cmd__greetings", &input).unwrap();
assert_eq!(out.exit_code, 0);
```

To check every handler reads only what its command declares, run them all
with every arg set (strings from your closure, ids of entities in the graph
reaching the most code):

```rust
let graph = CommandGraph::default();
for (id, out) in specforge_extension_sdk::testing::call_every_command(&c, &graph, "2026-10-04", |_, _| String::new()) {
    assert_eq!(out.exit_code, 0, "{id}");
}
```

The shipped extensions test exactly this way — see the `raw_category_tests`
module in the SDK, the pass tests in `extensions/formal/src/lib.rs` and the
command tests in `extensions/product/src/tests.rs`.

---

# Act VI — Where to go next

- **Host API (v2 pass ABI).** Passes currently receive an entity snapshot and
  return diagnostics. The next ABI revision adds host-query functions —
  `query`, `read_file`, `emit_diagnostic` — so passes can pull data on demand
  instead of receiving it all up front.
- **`specforge analyze --prove`.** The formal ladder's top rung: contracts
  become verification conditions discharged by an SMT backend. Design
  groundwork is in [RES-25](../../spec/research/RES-25-sources/).
- **Publish.** `cargo publish` your SDK extension crate; the SDK itself
  (`specforge-extension-sdk`) is on crates.io.

The source of truth for everything in this guide:
[`docs/extension-sdk.md`](../extension-sdk.md) and
[`docs/extension-protocol.md`](../extension-protocol.md).
