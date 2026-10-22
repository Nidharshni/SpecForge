# SpecForge Diagnostic Codes

<!-- Generated file: do not edit by hand. -->

This page is generated from the catalog table in `crates/leaf/diagnostics/src/catalog.rs`,
the single registry of diagnostic codes; `specforge explain <CODE>` prints the
same text. Every code emitted by the compiler, the CLI, or a first-party
extension has exactly one entry, and each entry names its owner: `core` for the
compiler and CLI, or the `@specforge/<name>` extension that emits it. The
compiler and CLI name each code through a typed constant generated from this
table (`specforge_diagnostics::codes`), and a test fails when a code an
extension emits is missing here, belongs to another owner, or when a listed
code is never emitted.

Codes follow the pattern `E###` (error), `W###` (warning) and `I###` (info);
`A###` codes are `specforge analyze` findings, whose severity the pass sets.
The registry client keeps its own family, `R###` and `R-<AREA>-###`, whose
prefix doesn't state the severity; no other family is accepted. Each entry's
`Level` is the severity a diagnostic of that code has when it is reported;
`specforge check --strict` raises warnings to errors afterwards. A code an
extension reports at another level, or a code it does not own, is reported as
W150. The ranges
`E900`-`E998`, `W900`-`W998` and `I900`-`I998` are reserved for third-party
extensions and never appear in this catalog; `I999` is a core code.

Regenerate this page after editing the catalog table:

```sh
SPECFORGE_BLESS=1 cargo test -p specforge-diagnostics explain_docs_sync
```

## A001

```
A001: Testable entity without obligations

`specforge analyze coverage` found a testable entity (one whose kind an
extension declares testable) that declares no `verify` obligations, so nothing
states what a test must prove about it. Add `verify unit "..."` (or another
obligation kind) statements. What W004 exempts (union types, abstract entities,
governance kinds) is not reported.

Owner: @specforge/testing
Level: set by the analyze pass
```

## A002

```
A002: Invariant without obligations

An invariant declares no `verify` obligations. It is an error when the
invariant's `risk` is `high`, a warning otherwise. Add a `verify property` or
`verify unit` statement stating how the guarantee is checked.

Owner: @specforge/testing
Level: set by the analyze pass
```

## A010

```
A010: Entity without contract obligations

`specforge analyze contracts` found an entity whose kind registers contract
reference fields (`requires`, `ensures`, `maintains`, ...) that declares none of
them, so no invariant constrains it. Add the references, or ignore this
info-level finding for entities that need none.

Owner: core
Level: set by the analyze pass
```

## A014

```
A014: Failing tests

The recorded test results (`specforge-report.json`, written by `specforge
collect`) include failing tests for this entity, so what it promises is not
proven. Fix the code or the test and run `specforge collect` again.

Owner: @specforge/testing
Level: set by the analyze pass
```

## A015

```
A015: Unproven obligations

With recorded test results, some of an entity's `verify` obligations are named
by no passing test. A test proves an obligation by naming its exact text:
`verify = "..."` in `#[specforge_test(...)]`, or in a vitest test's
`meta.specforge`. Link a test to each listed obligation, or write the missing
tests.

Owner: @specforge/testing
Level: set by the analyze pass
```

## A016

```
A016: Test names an undeclared obligation

A recorded test names a `verify` obligation that its entity does not declare,
usually a typo or an obligation reworded in the spec. The test proves nothing
until its text matches the spec's statement exactly; fix whichever side is
wrong.

Owner: @specforge/testing
Level: set by the analyze pass
```

## E001

```
E001: Parse error

A `.spec` file could not be parsed — invalid syntax such as a missing brace,
unclosed string, or malformed field at the reported location; the same code also
covers an internal reparse failure in the language server, and the formatter's
parser producing no syntax tree at all. Fix the syntax at the reported span and
re-save.

Owner: core
Level: error
```

## E002

```
E002: Duplicate entity ID

Two entities of the same kind declare the same ID; the diagnostic points at the
duplicate and its message names where the ID was first declared (file:line:col).
Rename one of the entities so each ID is unique within its kind.

Owner: core
Level: error
```

## E003

```
E003: Unresolved reference

A reference field names an entity ID that doesn't resolve to any declared
entity: a `reference` field's value or a `reference_list` field's item, quoted
or not, or a scheme ref ID in a string list. A `string_list` field's other items
are strings, never references. Fix the typo or add the missing entity; a `did
you mean` suggestion is included when a close match exists.

Owner: core
Level: error
```

## E004

```
E004: Unknown type in port method

A `port` entity's method field references a `type` ID that isn't declared
anywhere in the spec. Declare the missing `type` entity or fix the reference to
point at an existing one.

Owner: @specforge/software
Level: error
```

## E006

```
E006: Missing required field

An entity is missing a field marked `required: true` for its kind in the field
registry. Add the missing field to the entity.

Owner: core
Level: error
```

## E007

```
E007: Module dependency cycle

The `depends_on` edges between `module` entities form a cycle. Break the cycle
by removing or inverting one of the dependencies.

Owner: @specforge/product
Level: error
```

## E010

```
E010: Invalid milestone behavior range

A `milestone` entity's `behaviors` range field is malformed (the reported reason
explains what's wrong, e.g. bad syntax or start after end). Correct the range to
a valid form.

Owner: @specforge/software
Level: error
```

## E013

```
E013: Reserved entity ID

An entity's ID is a reserved word — a structural DSL keyword (`spec`, `ref`,
`use`, `define`) or an entity kind keyword contributed by an installed
extension. Rename the entity, e.g. by appending a suffix like `_rule` or
`_spec`.

Owner: core
Level: error
```

## E014

```
E014: Entity ID length violation

An entity ID is outside the 2-60 character identifier contract. Pick a
descriptive identifier within that length.

Owner: core
Level: error
```

## E015

```
E015: Milestone dependency cycle

The `depends_on` edges between `milestone` entities form a cycle. Break the
cycle by removing or inverting one of the dependencies.

Owner: @specforge/product
Level: error
```

## E016

```
E016: Referenced file does not exist

A file-reference field on an entity points at a path that doesn't exist under
the spec root. Fix the path, or create the missing file; a similarly-named file
is suggested when one is found.

Owner: core
Level: error
```

## E019

```
E019: Unsupported format version

A `.spec` file's `// specforge-format: MAJOR.MINOR` header declares a version
newer than this build supports, or the header itself doesn't parse; `specforge
migrate --target-version` reports the same for a target it can't parse or
doesn't support. Lower the declared version or upgrade SpecForge.

Owner: core
Level: error
```

## E022

```
E022: Reference targets wrong kind

A reference field is declared to only accept entities of a specific kind, but
the target ID resolves to an entity of a different kind. Point the field at an
entity of the expected kind. It is reported at the reference, by the linker,
whether or not an extension declares the expected kind; the suggestion names a
close id of that kind, or the extension that declares it.

Owner: core
Level: error
```

## E024

```
E024: Unknown entity kind

An entity block uses a kind keyword that neither the core grammar nor any
installed extension declares. Install the extension that provides the keyword,
or fix a typo in the kind name.

Owner: core
Level: error
```

## E025

```
E025: Import resolution failed

Either a `.spec` file discovery found can't be read (not UTF-8, no permission):
it is named, left out of the graph, and an import of it reports nothing more; or
a `use` import names no source of the project: no `.spec` file by that path
under the spec root that discovery keeps. A file an `exclude` entry leaves out,
one under a skipped directory (`target`, `build`, ...), one inside a nested
project (a directory below the spec root holding its own `specforge.json`), one
above the spec root and one whose name differs only in case are no source. Fix
the import path or the `exclude` entry; a `did you mean` suggestion is included
when a close match exists.

Owner: core
Level: error
```

## E026

```
E026: Entity kind registration conflict

Two extensions register the same entity kind keyword; the first registration
wins and the later one is rejected. Rename the conflicting kind keyword.

Owner: core
Level: error
```

## E027

```
E027: Unsatisfiable peer dependency

An extension's peer dependency isn't satisfied: a required peer isn't installed,
or an installed peer (optional or not) is at a version the range doesn't accept,
a version that isn't SemVer included; or required peers form a cycle, reported
once naming its extensions by every compile. `specforge add`, `init` and
`update` judge the project's peer set as their change leaves it (the extensions
its `specforge.json` entries load: builtins, installed extensions and `.wasm`
file entries) and refuse a change that would leave a peer it touches unsatisfied
(from a registry, a version diamond is R-RES-005/R-RES-006 instead); `add` and
`init` first enable the builtins what they add requires. `specforge remove`
refuses to uninstall an extension others still require. Install or upgrade the
named peer, install a version of the dependent that accepts the builtin this
specforge embeds, or make one peer of a cycle optional.

Owner: core
Level: error
```

## E028

```
E028: Extension load or execution failure

An extension failed somewhere in its lifecycle: its binary is missing,
unreadable or does not load as a component, or `specforge.lock` can't be read so
nothing is known to be installed (a `.wasm` file entry of `specforge.json` also
when the file declares another name than the entry writes, the name of a
builtin, or an extension another entry already loads); its handshake or one of
its describe categories failed or does not parse, so its declaration cannot be
read (`specforge add` and `specforge publish` refuse such a binary); or a call
the host makes on the loaded extension failed. The host calls ten exports: the
handshake and describe, a command, an MCP tool, an MCP resource, a compiler
pass, a collector, a custom validator, a scanner and the migration hook. Each
call fails when the export traps (a limit of its sandbox included: its time, its
fuel or its memory, the trap naming which), when the extension does not route
it, or when it answers output that is not the protocol type the operation owes
(a collector's test whose status is not `passed`, `failed` or `skipped`, say);
the message names the operation, the export and the extension (`command cmd__x()
of '@acme/x' trapped: ...`). What the failure costs is the operation's: a check
pass's is the compile's error, an analyze pass's a finding of that pass, a
scanner's makes `infer` approximate. Report the failure to the extension's
author, and confirm the extension is installed and up to date.

Owner: core
Level: error
```

## E030

```
E030: Invalid extension declaration

An extension's declaration (its handshake and describe answers) can't be used as
declared: its name or version is empty, its `ext_short` isn't lowercase kebab
case (`[a-z][a-z0-9-]*`; it names the extension's CLI subcommand and MCP tool
prefix), or an analyzer has no language, no file extensions or an empty export
name. The registry build reports it on every load, and `specforge publish`
refuses such a binary before any network call. Fix the declaration in the
extension's source (with the SDK: `#[extension(name, version, short)]` and the
builders) and rebuild it.

Owner: core
Level: error
```

## E031

```
E031: Refinement drops ensures condition

A behavior that `refines` an `abstract` behavior drops one or more of the
abstract behavior's `ensures` conditions. A refinement may only strengthen its
abstraction's postconditions, never weaken them — restore or strengthen the
missing `ensures` condition(s) in the concrete behavior.

Owner: @specforge/formal
Level: error
```

## E032

```
E032: Extension install or uninstall failed

An install, update or uninstall step failed: creating the staging directory,
writing the binary, or moving it into place or aside. Everything the change
wrote was put back; when something couldn't be, the error names it. Check
filesystem permissions and retry.

Owner: core
Level: error
```

## E033

```
E033: Lock file error

`specforge.lock` couldn't be serialized, written, read, or parsed; `specforge
update` also reports it when there is no lock file to update (run `specforge
add` first). A lock that is there and can't be read loads no installed extension
(each entry that needs it is E028), and every change whose answer it would
decide is refused with this code before anything is read or written: an `add` or
`init` that installs, any `update`, a `remove` of an extension whose origin only
the lock could tell. Enabling or disabling a builtin, or a `.wasm` file entry,
still works. Fix specforge.lock, or delete it and install the extensions again
with `specforge add`.

Owner: core
Level: error
```

## E034

```
E034: Unmitigated event cycle

`specforge analyze`'s `event_graph_analyze` pass found a cycle in the event flow
graph (a behavior `produces` an event another behavior `consumes`, and so on
back to the first) through two or more behaviors, and no behavior or event in it
declares `sync`. The message gives one cycle path. Declare a `sync` constraint
(a timeout, barrier or delivery bound) on one of its members, or break the
cycle. The analysis is structural: a declared `sync` counts as the mitigation,
what it says is not checked.

Owner: @specforge/formal
Level: error
```

## E039

```
E039: Duplicate surface contribution

Two contributions claim one name. Two commands route under one name, `specforge
<short> <command>`: one short name, and ids that spell one command name (`_` and
`-` alike), whether from two extensions or declared twice by one; the first in
load order is routed and the later runs on no surface. Or two extensions
register one MCP tool name, or one MCP resource name. One command id under two
short names is two names, not a collision. Rename the later contribution, or
give its extension another short name (`ext_short`; the SDK's `short`).

Owner: core
Level: error
```

## E040

```
E040: Extension project not found or not built

`specforge extension build`, `extension validate`, `publish`, or `add`/`init
--extensions` given a local path, found no extension to work on: nothing is at
the path, the directory has no `Cargo.toml`, the build (`cargo build --release
--target wasm32-wasip2`) failed, no built component is in
`target/wasm32-wasip2/release/` (or several are, and none is the crate's), or
the `.wasm` file couldn't be read (over MCP, also a file with more than one
name). Build the extension, fix the build error the message quotes, or name the
component to use.

Owner: core
Level: error
```

## E041

```
E041: Refinement chain cycle

The `refines` layering graph between behaviors contains a cycle. Break the cycle
by removing or redirecting one of the `refines` edges.

Owner: @specforge/formal
Level: error
```

## E042

```
E042: Process composition cycle

A `process` entity composes, transitively, with itself through its composition
edges. Remove or redirect one of the composition steps to break the cycle.

Owner: @specforge/formal
Level: error
```

## E045

```
E045: Invalid test report

`specforge collect` couldn't get a test report: the runner's command couldn't be
started, it finished without writing a report at the collector's declared
location (often because the tests didn't build), `--no-run` found no existing
report, or a report file couldn't be read. Check the runner's output above the
error and the report path. `specforge stats`, `specforge analyze` and every MCP
tool that reads coverage report the same code when `specforge-report.json`
exists but can't be read or parsed (a recorded test's status is `passed`,
`failed` or `skipped`, and every test has a name: a report in another
vocabulary, such as one written before 1.4, is not usable), or when a
`--test-results` (`test_results`) file can't be read or doesn't exist (over MCP:
`schema_mismatch`, `permission_denied`, `file_not_found`, `invalid_input` when
`specforge-report.json` is a link that leads outside the project, or
`internal_error`), rather than scoring the project as if no test ran: run
`specforge collect` again to rewrite it, or fix or remove the file.

Owner: core
Level: error
```

## E046

```
E046: Declared bounds are contradictory

`specforge analyze --prove`'s SMT solver found the declared bounds mutually
unsatisfiable; the cited bounds form the conflicting core. Bounds are the fields
an extension declares with the `bound` proof role (a governance constraint's
`metric`, a formal axiom's `expression`). Relax or correct one of the listed
bounds.

Owner: core
Level: error
```

## E048

```
E048: Proof coverage below the minimum

`specforge analyze coverage --min N` found that fewer than N% of testable
entities are proven, where an entity is proven when a passing test names each of
its `verify` obligations or a formal claim discharges it. Prove more
obligations, or lower the threshold. The run exits 1.

Owner: core
Level: error
```

## E052

```
E052: Deliverable dependency cycle

The `depends_on` edges between `deliverable` entities form a cycle. Break the
cycle by removing or inverting one of the dependencies.

Owner: @specforge/product
Level: error
```

## E054

```
E054: Invalid extension specifier

The argument given to `specforge add` (or `specforge.add_extension`, or `init
--extensions`) isn't a builtin's name, a local path (ending in `.wasm`, or
starting with `./`, `../` or `/`), a `git+https://...` URL, or a registry
package `@scope/name` with an optional `@version` or `@requirement`. A registry
package name is lowercase letters, digits, `.`, `_` or `-` in two parts that
each start with a letter or digit. A local path that names nothing, or no
readable component, is E040. Nothing is asked of any registry.

Owner: core
Level: error
```

## E055

```
E055: Invalid surface contribution schema

An extension's explicit MCP tool declares an `input_schema` or `output_schema`
that isn't a JSON object, so the tool is not registered. Declare the schema as a
JSON Schema object. (A command argument type outside `string`, `path`, `bool`,
`enum` and `integer` fails the extension's load as E028: its surfaces don't
parse.)

Owner: core
Level: error
```

## E056

```
E056: Failed to write collected report

`specforge collect` ingested test results but couldn't write its aggregated
coverage report file to disk. Check that the output path is writable.

Owner: core
Level: error
```

## E057

```
E057: Provider scheme conflict

Two extensions both register a `ref` provider for the same scheme. Configure
distinct schemes for each provider extension.

Owner: core
Level: error
```

## E058

```
E058: No test collector

`specforge collect` found no collector to use: no enabled extension provides
one, none of the enabled collectors' detection files (such as `Cargo.toml`) are
present at the project root, the `--runner` name matches no collector, a
collector declares a report path outside the project, or `--report` was passed
while several collectors apply. Enable a runner extension (e.g. `specforge add
@specforge/cargo-test`) or pick one with `--runner`. Over MCP an unknown
`runner` is `invalid_input` naming `runner`; the other cases are
`precondition_failed`.

Owner: core
Level: error
```

## E059

```
E059: Test command not approved

A runner extension declares the command that runs its tests, and `specforge
collect` only runs it after you approve it for the project. The approval is
asked at an interactive prompt and remembered per project, extension and
command, in your user-level `~/.specforge/collector-consent.json`, never in the
project. Without a terminal (CI, `--format json`) nothing is asked: pass `--yes`
to run the command, or `--no-run` to parse a report the runner already wrote.
The MCP server never asks: approve the command once with `specforge collect` in
a terminal, or call `specforge.collect` without `run` to parse the existing
report.

Owner: core
Level: error
```

## E061

```
E061: Field value is not the declared type

The extension that registers a field declares its type, and the value given
can't be that type. A string field takes text, a bare word, a date, a type union
or an `expr { }` group; an enum one of its declared values (the suggestion names
the closest one; the closed vocabularies the builtins declare, a feature's
`status`, a failure mode's `severity`, a property's `property_type`, and the
like, are enum fields); an integer field an integer; a bool field true or false;
a reference field one id; a string_list or reference_list field a list; a block
field a block. Values that can be read as the declared type are converted
without a diagnostic: a single string, id, integer, boolean or date on a list
field becomes a one-item list, a list is stored as written, a quoted integer or
boolean the number or boolean. Fix the value, or check the field's type with
`specforge schema --kind <kind>`.

Owner: core
Level: error
```

## E062

```
E062: Token budget too small for the export

`specforge export --max-tokens` (or the MCP `export` tool's `max_tokens`) keeps
the most central entities that fit the budget, but some of the export never
shrinks: the envelope (`format_version`, `schema_version`), the `token_budget`
block listing the dropped entity IDs, and, with `--with-schema`, the embedded
schema, which is never cut short, and, in a scoped export (`--scope`), the
scope's own entity. The budget is below that fixed part, so no export fits.
Raise the budget, or drop `--with-schema` when the schema alone is over it.

Owner: core
Level: error
```

## E063

```
E063: No registry configured

`specforge add @scope/name@version`, `update`, `search`, `publish` and `login`
(and the MCP `add_extension` tool) talk to an extension registry, and SpecForge
has no built-in one: the only registries are those the project's
`specforge.json` lists. None is listed, so the command stopped before making any
network call. Add a `registries` array, for example `"registries": [{"alias":
"main", "url": "<registry URL>", "default_registry": true}]`; an entry with
`"scope_filter": "@acme"` serves only that scope, and the entry marked
`default_registry` serves the rest. For `login`, `--registry <alias>` must name
one of the entries, or one must be the default. Builtin extensions (`specforge
add @specforge/product`) and local `.wasm` files need no registry.

Owner: core
Level: error
```

## E064

```
E064: Unsupported extension source

`specforge add` was given a `git+https://...` extension specifier. It parses,
but installing from git isn't supported yet. Install from a registry
(`name@version`) or from a local `.wasm` path instead.

Owner: core
Level: error
```

## E065

```
E065: Invalid scaffold request

`specforge extension init` can't scaffold what was asked: the extension name is
empty or is a scoped name that isn't `@scope/name`, or the destination directory
already exists. Fix the name, or pick a destination that doesn't exist yet.

Owner: core
Level: error
```

## E066

```
E066: Extension scaffold failed

`specforge extension init` couldn't write the project: creating a directory or
writing one of the generated files failed. Check permissions and free space at
the destination, remove the partial project, and retry.

Owner: core
Level: error
```

## E067

```
E067: Invalid registry configuration

The registry configuration in `specforge.json` can't be read: the file isn't
valid JSON, `registries` isn't an array, or an entry at the reported index is
missing a required field, has the wrong type, or has a `url` that isn't a
registry's root (an `http` or `https` URL with no query, fragment or user name;
write `https://registry.example`, not `https://registry.example/v1`: SpecForge
adds the protocol's `/v1`), or has a `scope_filter` that isn't a scope (`@acme`:
`@` and one name part; the message names the scope to write). No registry is
asked while an entry can't be read, so a package whose scope that entry serves
never goes to another registry. Fix the reported entry.

Owner: core
Level: error
```

## E068

```
E068: Coverage gate without a coverage figure

`specforge analyze --min N` (and `specforge.analyze`'s `min`) gates on proof
coverage, which the `coverage` pass of `@specforge/testing` computes. Either
that pass will not run (the extension isn't enabled, or the run selects a
different pass), and the analysis is refused before any pass runs; or it ran and
gave no figure this specforge reads (it failed, or `@specforge/testing` is a
different version), and the run is unjudged. Enable it with `specforge add
@specforge/testing`, run the `coverage` (or `all`) pass, and keep the extension
at the version this specforge ships. The run exits 2.

Owner: core
Level: error
```

## E069

```
E069: Unusable specforge.json

The project root has a `specforge.json` that isn't used as written. When it
can't be read, isn't valid JSON, isn't a JSON object, or its `extensions` value
isn't an array, the compile uses the default config: no extension loads and only
structure is checked (I002 says so), and `specforge add`, `specforge update` and
`specforge remove` refuse (config_invalid) without changing anything. `specforge
format`, `specforge migrate` and the rename tools refuse the same way when the
file can't be used or `spec_root`, `exclude` or `extensions` has the wrong type,
since they could not tell which files are the project's sources; on any other
problem they still run, report the E069 and fail. When a key has the wrong type
(`name`, `version` or `spec_root` not a string, `exclude` not an array), that
key's default is used; when an item of `extensions` or `exclude` isn't a string,
or an `exclude` item can match no source (empty, starting with `!`, with a `.`
or `..` name, or not a glob pattern), that item is ignored; the rest of the file
is used. Each problem is one E069, and `check` fails on it, so a broken config
can't pass a CI that checks nothing. Fix the file: the message names the JSON
error's line and column, or the key and the item. A missing `specforge.json` is
not this: it is a project with the default config (`specforge doctor` says so).

Owner: core
Level: error
```

## E070

```
E070: Installed extension does not match its lock entry

An installed extension's binary, `.specforge/extensions/<name>/extension.wasm`,
is not the one its `specforge.lock` entry pins: its SHA-256 differs from the
recorded hash (it was changed or replaced after install), or it declares another
extension, another version or other peers than the entry records. It is not
loaded, and `check` fails. Reinstall it with the command the suggestion names
(`specforge add <path>` for a local install, `specforge add <name>@<version>`
for a registry one); `specforge doctor` lists it as `stale_hash`.

Owner: core
Level: error
```

## E071

```
E071: Unusable inference manifest

A file inference keeps at the project root is there and can't be used as
written: `specforge-infer.json` (the analyzed source files and the inference
sessions) or `specforge-anchors.json` (the source anchors navigation reads). It
can't be read, isn't valid JSON, doesn't have the manifest's shape (a field
missing or of the wrong type, a session status other than `active`, `paused` or
`completed`), or, for `specforge-infer.json`, has a `version` other than 1.
Nothing reads such a file as empty, and nothing writes over it: `specforge
infer-status`, `specforge.infer_progress`, `specforge.infer_gaps`, the infer
prompt's plan and every `specforge.infer_session` action refuse with this code,
`specforge.find_implementation`, `specforge.find_spec_for_source`,
`specforge.infer_gaps`, `specforge infer-status --gaps-detail` and the infer
prompt's file scope refuse when it is the anchors manifest, and `specforge check
--lint inferred` reports it as an error instead of I200 and I202. Fix the file
where the message says (it names the line and column of a JSON or shape error),
or move it aside to start over. A missing file is not this: inference has
recorded nothing yet.

Owner: core
Level: error
```

## E072

```
E072: Invalid package name or version

A name that must be a package name is not one, or a version that must be a
SemVer version is not one. A package name is `@scope/name` (what a registry
holds) or `name` alone (a local module); each part is lowercase letters, digits,
`.`, `_` or `-`, and starts with a letter or digit, so it is always a directory
inside `.specforge/extensions`. It is checked where a name does not come from
the `add` argument (that is E054): the name a local module declares when
`specforge add` installs it, a peer a declaration names, a `specforge.json`
`extensions` entry (the entry is not loaded), the name `specforge remove` is
given when no `specforge.json` entry or lock entry has it, and the name and
version of the declaration `specforge publish` uploads (which must also be
scoped and a full version). Nothing is written or deleted. Fix the extension's
declared name or version (its SDK `name`/`version`) and rebuild, or fix the
`specforge.json` entry.

Owner: core
Level: error
```

## E073

```
E073: Unreadable peer requirement

An extension declares a peer dependency whose range is not a SemVer requirement
as Cargo reads one (`^1.2`, `~1`, `>=1, <2`, `1.x`, `*`, or a bare version such
as `1.2.0`, which means `^1.2.0`), so no version of the peer can satisfy it,
whether the peer is installed or not. `specforge check` (and the LSP and MCP)
and `doctor` report it, `specforge add` and `update` refuse to install or update
around it, and `specforge publish` refuses to upload it. Fix the range in the
extension's declaration (its SDK `peer` or `optional_peer` call) and rebuild, or
install a version of the extension that declares one.

Owner: core
Level: error
```

## E074

```
E074: Publisher signing key unusable

`specforge publish` signs every package with your publisher key, kept in
`~/.specforge/signing-key.json` and created on your first publish. That file is
there and can't be read as a key (it is not JSON, or its `secretKey` is not 32
hex-encoded bytes), or the key could not be created there (the directory or the
file can't be written). Nothing was sent. Fix the file's permissions, or move it
aside: the next publish creates a new key, and whoever pinned the old one then
sees a changed publisher key (R-TRUST-003) for your packages.

Owner: core
Level: error
```

## E076

```
E076: Block has no title

A block that becomes a graph node has no title, or a blank one: write it after
the block's id (`type Task "A unit of work" { … }`, `type Status "Task status"
= open | done`). The title is the one label every surface shows for the node; a
`//` comment above the block never reaches the graph. `specforge migrate` writes
the missing titles of a project: the comment directly above each block when
there is one, else its id in words. `spec` takes its name as its title, `ref`
requires one, and `define` blocks are never nodes.

Owner: core
Level: error
```

## E077

```
E077: Default for a field its kind does not declare

A `defaults <kind> { … }` block (ADR 0149) sets a field that `<kind>` does not
declare, or names a kind no loaded extension declares, or a structural kind
(`spec`, `ref`, `use`, `define`, `defaults`). A default only sets a field an
extension declares for the kind: a field of an `open_fields` kind's members is
not declared. The default is not applied, so no block of the file takes it.
Write the field on the blocks that need it, or fix the kind or the field name.
Reported by the graph build, once per default, at its value; a project whose
extensions declare no kind checks none.

Owner: core
Level: error
```

## E078

```
E078: Malformed defaults block

A `defaults <kind> { … }` block (ADR 0149) holds only fields, once each, and
comes before the file's first entity block; a file has at most one per kind.
This one has a title, a `verify` or `method` member, a field given twice (the
first is applied), is a second block for its kind in the file (it is not
applied; the message names the first), or comes after the file's first block (it
still applies). Remove the title or member, merge the blocks, or move the block
above every entity block.

Owner: core
Level: error
```

## E079

```
E079: Relation written on its other side

An entity writes a field its kind does not declare, and that field is the other
side of a relation another kind owns. A relation is written on its owning field
only (ADR 0150): the one reference field an extension declares for it. The other
side is derived, never written: `query` shows it as the incoming edges and
`trace` as the upstream. The field is recognised in two ways: an owning field
names it in its `inverse_of` (so a documented reverse such as `behaviors` on a
feature is refused even when its ids do not resolve), or every id it lists is
one other kind's entity and that kind owns a reference field into this one
(`journeys [..]` on a feature). A self-relation's other side is not recognised,
and a kind that accepts any field is not checked. The graph still carries the
field's untyped edge, and `check` fails. Remove the field and write the owning
field on each entity it listed; the suggestion names both.

Owner: core
Level: error
```

## E080

```
E080: Typed edit would add errors

A typed edit (`specforge new`, `set`, `unset`, `link`, `unlink`, `delete`, or
MCP `specforge.edit`) was planned, and the project it would leave reports errors
the project does not report now: the edit is refused and nothing is written. The
refusal lists each new error (an unresolved reference, a missing required field,
a value its field does not take, …). Fix what they name first: create the
referenced entity, give the required field, or make both changes in one
`specforge.edit` call. `delete --unlink-references` removes the references a
delete would leave dangling.

Owner: core
Level: error
```

## I002

```
I002: Structural-only mode

Emitted when no extensions are installed, or when every installed extension
failed to load, so the compiler falls back to structural-only validation.
Install an extension (for example `specforge add @specforge/software`) to enable
kind-specific checks.

Owner: core
Level: info
```

## I003

```
I003: No registry configured

The registry configuration has no `registries` array, or none of the configured
registries is marked as the default. Add a `registries` entry and set
`"default_registry": true` on one of them. Without a default, a package no
`scope_filter` matches has no registry (R-OPS-001).

Owner: core
Level: info
```

## I004

```
I004: Extension not installed

A reference field targets a kind no enabled extension declares, an extension
enhances a kind no enabled extension declares, or a `.spec` file has an
`@scope/name` extension import for a known but not-installed extension. Install
the missing extension with `specforge add <name>` to resolve the reference; for
a reference field the suggestion names the builtin extension that declares the
kind, as E024's does. An entity whose keyword no enabled extension declares is
an error instead (E024), whose suggestion names the extension to install. An
extension's enhancement of a kind no enabled extension declares is skipped
silently, not reported, when the extension declares an optional peer the project
does not load.

Owner: core
Level: info
```

## I005

```
I005: Unknown provider scheme

A `ref` entity's `scheme` field, or a `scheme:target` provider reference,
doesn't match any provider scheme registered by an installed extension. Install
an extension that contributes that provider, or configure it in
`specforge.json`.

Owner: core
Level: info
```

## I007

```
I007: Older format version detected

The `.spec` file's declared format version is older than the compiler's current
format version. Run `specforge migrate` to upgrade the file to the current
format.

Owner: core
Level: info
```

## I008

```
I008: Coverage item proven by tests

`specforge analyze`'s `coverage_tracking` pass found a coverage item (an
invariant or a testable entity) whose every `verify` obligation a passing
recorded test names, under @specforge/testing's coverage rule. The message names
those tests. An item proven only by an entailed claim is not reported. Nothing
to do.

Owner: @specforge/formal
Level: info
```

## I009

```
I009: No unmitigated event cycle

`specforge analyze`'s `event_graph_analyze` pass found no unmitigated cycle in
the event flow graph: no cycle through two or more behaviors (E034) and no
behavior re-producing an event it consumes (W032), unless a member declares
`sync`. Reported once, when behaviors produce or consume events. The analysis is
structural; cycles that depend on runtime conditions are not analyzed. Nothing
to do.

Owner: @specforge/formal
Level: info
```

## I010

```
I010: Unreferenced term

A `term` entity has no edges at all, meaning nothing links to or from it via
`see_also` or similar references. Link the term from a relevant entity, or
remove it if it's unused.

Owner: @specforge/product
Level: info
```

## I011

```
I011: Ensures without requires

`specforge analyze`'s `condition_check` pass found a `behavior` that declares an
`ensures` clause but no `requires` clause, so it guarantees its postconditions
for every input. Add a `requires` clause naming what callers must establish
first, or leave it out if the behavior accepts every input.

Owner: @specforge/formal
Level: info
```

## I014

```
I014: Specification depth

`specforge analyze`'s `coverage_tracking` pass reports a `behavior`'s
specification depth, a ladder in which each level holds the one below it:
`prose` (no edges), `entity_graph` (edges, no requires or ensures), `conditions`
(level 2: requires or ensures), `invariants` (level 3: also names invariants in
`maintains` or `invariants`) and `proofs` (level 4: also proven under
@specforge/testing's coverage rule). A behavior at level 2 or deeper is reported
with the step to the next level. When more than five behaviors sit at `prose` or
`entity_graph`, one more I014 suggests adding requires/ensures to the critical
ones.

Owner: @specforge/formal
Level: info
```

## I015

```
I015: Formal analysis available

Behaviors in the project declare `requires` or `ensures`, which the formal
passes of `specforge analyze` (condition_check, layering_verify,
event_graph_analyze, coverage_tracking) check. Reported once per compile by
@specforge/formal's `analysis_available` pass. Run `specforge analyze`.

Owner: @specforge/formal
Level: info
```

## I016

```
I016: Schema cache unreadable

`.specforge/schema-cache.json` exists but cannot be read: it is not JSON, not a
recorded schema, or names a field type this host does not read. The schema is
versioned as if no export had been made, at 1.0.0, by every reader of the
schema, until the next `specforge export` records a new cache. Agents that read
earlier exports must not take the version as continuing theirs.

Owner: core
Level: info
```

## I017

```
I017: Extension surface not served under its name

MCP serves each name once (ADR 0017): the core tools and resources first, then
each extension's explicit MCP tools and resources in extension load order, then
its commands as the tools `specforge.<ext>.<command>`. A contribution not served
under its name is reported here, with why: an explicit tool named as a core tool
or an earlier extension's tool; a resource whose URIs a core resource or an
earlier extension resource already serves; a command whose tool name an explicit
tool (or a core tool) already has; a command the host refuses (an arg taking
`--path`, `--format` or `--help`, two args spelling one option, or a default its
declaration contradicts); a command another command routes first under its name
(`specforge check` reports that one as E039). I017 itself only MCP reports.
Rename the contribution if the collision is unintended; otherwise no action is
needed.

Owner: core
Level: info
```

## I020

```
I020: Unknown entity kind in a filter

A kind filter (`specforge export --kinds`, `specforge query --kind`, `specforge
explore --kind`, `specforge model --kinds`, and the `kinds` or `kind` of the
`specforge.export`, `specforge.query`, `specforge.search`, `specforge.list`,
`specforge.coverage` and `specforge.model` MCP tools, the explore prompt and the
graph resources) names a kind that no loaded extension declares and no entity is
written with. The kind matches nothing and is dropped from the filter; the
report goes to stderr on the CLI and rides in the tool result's
`_meta.diagnostics` over MCP, with a `did you mean` suggestion when a known kind
is close (a kind equal but for case first: kind names are case-sensitive
keywords). Fix the spelling, or enable the extension that defines the kind.

Owner: core
Level: info
```

## I046

```
I046: Unreferenced persona

A `persona` entity has no incoming edges, meaning no `journey` references it.
Reference the persona from a journey, or remove it if it's no longer needed.

Owner: @specforge/product
Level: info
```

## I047

```
I047: Unreferenced channel

A `channel` entity has no incoming edges, meaning no `journey` references it.
Reference the channel from a journey, or remove it if it's no longer needed.

Owner: @specforge/product
Level: info
```

## I048

```
I048: Feature without acceptance criteria

A `feature` has no `acceptance` field, or an empty one. Add acceptance criteria
describing when the feature is done; they can be added progressively.

Owner: @specforge/product
Level: info
```

## I050

```
I050: Journey with empty flow

A `journey` declares `flow []`: it has no steps. Add the steps the user goes
through. (A journey with no `flow` field at all is the core's E006, since `flow`
is required.)

Owner: @specforge/product
Level: info
```

## I053

```
I053: Milestone target date not YYYY-MM-DD

A `milestone`'s `target_date` doesn't match `YYYY-MM-DD` (for example `Q3 2026`
or `June`). Write the date as an ISO 8601 calendar date, e.g. `2026-06-30`.

Owner: @specforge/product
Level: info
```

## I054

```
I054: Journey without persona

A `journey` has no `persona` field, so it names no user role. Set `persona` to
the persona who takes the journey.

Owner: @specforge/product
Level: info
```

## I055

```
I055: Journey without channels

A `journey` has no edge to a `channel`: no `channels` field, an empty list, or
only references that don't resolve. List the channels the journey happens
through in `channels`.

Owner: @specforge/product
Level: info
```

## I057

```
I057: Blocked milestone without dependencies

A `milestone` has `status: blocked` but no `depends_on` entries, so nothing in
the plan says what it waits for; the status may be stale. Add the milestones it
depends on to `depends_on`, or update the status.

Owner: @specforge/product
Level: info
```

## I059

```
I059: Deferred feature missing reason

A `feature` has `status: deferred` but no `reason` field explaining why. Add a
`reason` field describing why the feature was deferred.

Owner: @specforge/product
Level: info
```

## I060

```
I060: Blocked milestone missing blockers

A `milestone` has `status: blocked` but no `blockers` field listing what's
blocking it. Add a `blockers` field describing what is blocking progress.

Owner: @specforge/product
Level: info
```

## I061

```
I061: Deliverable version not semver

A `deliverable`'s `version` isn't a Semantic Versioning 2.0.0 version (for
example `v1.0`, `1.0` or `latest`). Use `MAJOR.MINOR.PATCH`, optionally with a
pre-release tag (`1.0.0-alpha.1`) or build metadata (`1.0.0+build.42`).

Owner: @specforge/product
Level: info
```

## I062

```
I062: Non-standard module family

A `module`'s `family` isn't one of the standard families: core, platform,
extension, integration, advisory. Custom families are allowed; use a standard
one if it fits.

Owner: @specforge/product
Level: info
```

## I063

```
I063: Done feature depends on an unfinished feature

A `feature` has `status: done` but a feature its `depends_on` names is not done.
Reported once per such dependency, by the `lifecycle` pass of
@specforge/product. Finish the dependency (or mark it done), or set the feature
back to `in_progress`.

Owner: @specforge/product
Level: info
```

## I064

```
I064: Milestone due before its dependency

A `milestone`'s `target_date` is earlier than that of a milestone its
`depends_on` names: it cannot realistically complete before what it waits on.
Reported once per such pair; a milestone or dependency without a `YYYY-MM-DD`
date is not compared. Move the date, or drop the dependency.

Owner: @specforge/product
Level: info
```

## I065

```
I065: Shipped deliverable tracks an incomplete milestone

A `deliverable` has `status: shipped` but a milestone its `milestones` names is
not `completed`. Reported once per such milestone, by the `lifecycle` pass of
@specforge/product. Complete the milestone, or set the deliverable back to
`in_progress`.

Owner: @specforge/product
Level: info
```

## I066

```
I066: Deprecated deliverable missing reason

A `deliverable` has `status: deprecated` but no `reason` field explaining why.
Add a `reason` field documenting why it was deprecated.

Owner: @specforge/product
Level: info
```

## I067

```
I067: Module without features

A `module` has no edge to a `feature`: no `features` field, an empty list, or
only references that don't resolve. A module that implements no features is
likely incomplete. List the features it implements in `features`.

Owner: @specforge/product
Level: info
```

## I068

```
I068: Tag not lowercase-hyphenated

An entity of a product kind has a `tags` entry that isn't lowercase-hyphenated:
2 to 50 characters of a-z, 0-9 and `-`, not starting or ending with `-` (for
example `Core`, `my_tag`, `a` or a tag with spaces). Empty entries are ignored.
Rewrite the tag, e.g. `My Tag` as `my-tag`.

Owner: @specforge/product
Level: info
```

## I069

```
I069: Deprecated persona missing reason

A `persona` has `status: deprecated` but no `reason` field explaining why. Add a
`reason` field documenting why it was deprecated.

Owner: @specforge/product
Level: info
```

## I070

```
I070: Deprecated channel missing reason

A `channel` has `status: deprecated` but no `reason` field explaining why. Add a
`reason` field documenting why it was deprecated.

Owner: @specforge/product
Level: info
```

## I071

```
I071: Done feature the recorded tests do not prove

A `feature` has `status: done` but the project's recorded test report
(`specforge collect`) does not prove it: no behavior implements it (names it in
`features`), or not every implementing behavior is proven (an obligation no
passing test names, or a failing test). A feature's status is a claim and its
evidence is derived (ADR 0039). Reported by the `delivery_evidence` pass under
`specforge analyze`, only when a report is recorded. Link tests to the unproven
obligations, or name the feature in the behaviors that deliver it.

Owner: @specforge/product
Level: info
```

## I080

```
I080: Entity without owner

A `feature`, `milestone`, `deliverable` or `release` has no `owner` field. Set
`owner` to the person or team responsible.

Owner: @specforge/product
Level: info
```

## I081

```
I081: Feature without effort estimate

A `feature` has no `effort` field. Set `effort` to one of xs, s, m, l, xl.

Owner: @specforge/product
Level: info
```

## I082

```
I082: Release without deliverables

A `release` has no edge to a `deliverable`: no `deliverables` field, an empty
list, or only references that don't resolve. List the deliverables it ships in
`deliverables`.

Owner: @specforge/product
Level: info
```

## I083

```
I083: Release without milestones

A `release` has no edge to a `milestone`: no `milestones` field, an empty list,
or only references that don't resolve. List the milestones it completes in
`milestones`.

Owner: @specforge/product
Level: info
```

## I086

```
I086: Release date not YYYY-MM-DD

A `release`'s `release_date` doesn't match `YYYY-MM-DD` (for example `June
2026`). Write the date as an ISO 8601 calendar date, e.g. `2026-06-01`.

Owner: @specforge/product
Level: info
```

## I087

```
I087: Milestone start date not YYYY-MM-DD

A `milestone`'s `start_date` doesn't match `YYYY-MM-DD` (for example `Jan 15`).
Write the date as an ISO 8601 calendar date, e.g. `2026-01-15`.

Owner: @specforge/product
Level: info
```

## I089

```
I089: Recalled release without reason

A `release` has `status: recalled` but no `reason` field, or an empty one. Add a
`reason` explaining why the release was recalled.

Owner: @specforge/product
Level: info
```

## I098

```
I098: Solver could not decide bounds

The `specforge analyze --prove` SMT solver returned an undecided result rather
than `sat`/`unsat` when checking the combined declared bounds, or whether they
entail a declared claim. Simplify the bound or claim expressions, or supply
tighter bounds, so the solver can decide.

Owner: core
Level: info
```

## I200

```
I200: Stale inferred entities

A source file has changed on disk since it was last analyzed by inference, so
the entities inferred from it may no longer be accurate. Re-analyze the file to
refresh its inferred entities.

Owner: core
Level: info
```

## I202

```
I202: High inference density

A source file produced an unusually high number of inferred entities relative to
its line count, exceeding the configured density threshold. Review the file for
over-eager inference, or adjust the density threshold if that density is
expected.

Owner: core
Level: info
```

## I999

```
I999: Diagnostic output truncated

More diagnostics were produced than the 100-diagnostic display limit, so only
the first batch is shown. Fix the listed diagnostics and rerun the compiler to
see the rest.

Owner: core
Level: info
```

## R-AUTH-020

```
R-AUTH-020: Stored registry token expired

The token `specforge login` stored for this registry expired at the reported
time. Log in again with a new token: `specforge login --registry <alias> --token
<NEW_TOKEN>`.

Owner: core
Level: error
```

## R-AUTH-021

```
R-AUTH-021: Stored registry token unreadable

The secret `specforge login --token` kept for this registry (a file under
`~/.specforge/secrets/`) is missing or can't be read, although the credentials
file refers to it. Log in again: `specforge login --registry <alias> --token
<NEW_TOKEN>`. The file is 0600 and its directory 0700 on unix only; on Windows
nothing sets a mode, so the secret relies on the user profile directory's
default ACL (private to the user).

Owner: core
Level: error
```

## R-LOGIN-001

```
R-LOGIN-001: No single login token source

`specforge login` needs exactly one of `--token <TOKEN>`, `--token-env <VAR>` or
`--token-file <PATH>`; none or several were given.

Owner: core
Level: error
```

## R-LOGIN-002

```
R-LOGIN-002: Login token not stored

`specforge login` couldn't write the token's secret file under
`~/.specforge/secrets/`. Check that `~/.specforge` is writable.

Owner: core
Level: error
```

## R-OPS-001

```
R-OPS-001: No registry for the package

No configured registry serves this package: none has a scope that matches it,
and none is marked as the default (or no registries are configured at all). Add
a `registries` entry to `specforge.json` with a matching scope, or mark one
`"default_registry": true`. `specforge add`, `update` and `publish` (and MCP
`add_extension`) choose the registry by this one rule and ask no other registry;
a refused name sends no request.

Owner: core
Level: error
```

## R-OPS-002

```
R-OPS-002: Package integrity check failed

The SHA-256 hash of the downloaded package doesn't match the hash the registry
published for it, so the download is corrupt or was tampered with. Retry the
download; if it keeps failing, don't install the package.

Owner: core
Level: error
```

## R-OPS-003

```
R-OPS-003: Manifest not serializable

`specforge publish` couldn't serialize the extension manifest to JSON for the
upload. This is a SpecForge bug, not a mistake in your manifest; please report
it.

Owner: core
Level: error
```

## R-OPS-004

```
R-OPS-004: Package manifest unreadable

The registry served no manifest for the package, or one that isn't a valid
extension manifest. The manifest declares the package's peer dependencies, which
`specforge add` checks against the installed extensions before installing
anything, so a package whose manifest can't be read is refused rather than
treated as having no peers. Nothing is installed and no publisher key is pinned.
Don't install the package, and check the registry.

Owner: core
Level: error
```

## R-RES-001

```
R-RES-001: Package not in the registry

The registry has no package with this name. Check the package name and which
registry the configuration sends it to.

Owner: core
Level: error
```

## R-RES-002

```
R-RES-002: No published versions

The registry lists the package, but no version of it (or no version with a valid
semver number). Ask the publisher to publish a release, or install from another
source.

Owner: core
Level: error
```

## R-RES-003

```
R-RES-003: Invalid version range

The version after `@` isn't `latest`, a full version or valid SemVer requirement
syntax, so no registry was asked. Use a version such as `1.2.0`, a requirement
such as `^1.0`, `~2.3`, `1.x` or `>=1.0.0, <2.0.0`, or `latest`.

Owner: core
Level: error
```

## R-RES-004

```
R-RES-004: No version satisfies the range

The registry has versions of the package, but none inside the requested range;
the message lists the available ones (for a builtin, the one version this
specforge embeds). Widen the range, or pick one of the listed versions; for a
builtin, drop the version or use the specforge release that embeds the one you
need.

Owner: core
Level: error
```

## R-RES-005

```
R-RES-005: Unresolvable version diamond

Several extensions require this package in ranges that no single published
version satisfies (see ADR 0001). Upgrade the requirer with the narrowest range,
or pin a compatible version manually.

Owner: core
Level: error
```

## R-RES-006

```
R-RES-006: Locked peer breaks a version diamond

An extension being installed or updated requires a peer at a version the lock
does not have, or a locked extension requires a version of the package being
installed or updated that it would not have. The message names a version of the
peer that would satisfy every requirer. Install the peer at that version
(`specforge add @scope/peer@<version>`) and retry; when a locked extension holds
the peer back, `specforge update --major` moves them together.

Owner: core
Level: error
```

## R-TRUST-001

```
R-TRUST-001: Unsigned package

The registry package carries no publisher signature, so where it came from can't
be verified. Install it anyway only if you trust the source, with
`--allow-unsigned`.

Owner: core
Level: error
```

## R-TRUST-002

```
R-TRUST-002: Invalid package signature

The package's publisher signature doesn't verify: the Wasm binary or the
manifest isn't what the publisher signed. Don't install the package.
`--allow-unsigned` doesn't accept it. (A signature that isn't one at all makes
the registry's answer unreadable: R009.)

Owner: core
Level: error
```

## R-TRUST-003

```
R-TRUST-003: Publisher key changed

The package is signed with a different key than the one expected for it: the key
pinned for it when you first installed it (trust on first use), or, when you
have no pin for it, the key the project's `specforge.lock` records. That can be
a key rotation or a compromised publisher. If you trust the new key, accept it
with `specforge add` or `specforge update` (yes at the prompt, or `--yes`), or
add it to `trusted_keys`; the MCP server never accepts a key change.

Owner: core
Level: error
```

## R-TRUST-004

```
R-TRUST-004: Registry answer mismatch

The registry's answer doesn't match: it, or the manifest it serves, describes
another package or version than the one requested, or the binary declares other
than the served declaration. Don't install the package, and check the registry.

Owner: core
Level: error
```

## R-TRUST-005

```
R-TRUST-005: Publisher key denied

The package is signed with a key listed in `denied_keys` in your known-keys
file. Remove the key from `denied_keys` only if you trust it again.

Owner: core
Level: error
```

## R-TRUST-006

```
R-TRUST-006: Known-keys file unusable

Your known-keys file (`~/.specforge/known-keys.json`) can't be read as the store
of publisher pins, or the pins an install accepted can't be saved to it. Nothing
is installed either way: a store read as empty would trust any key and lose your
`denied_keys`, and an install whose pin can't be saved is put back. Fix the
file's JSON, or move it aside (every package is then trusted on first use
again), and check the permissions on the file and its directory.

Owner: core
Level: error
```

## R001

```
R001: Registry authentication failed

The registry rejected the request as unauthenticated (HTTP 401). Log in again
with `specforge login --registry <alias> --token <TOKEN>`. `specforge publish`
refuses before any request when there is no credential for the registry that
serves the package: none stored for its alias and `SPECFORGE_REGISTRY_TOKEN`
unset or blank.

Owner: core
Level: error
```

## R002

```
R002: Registry access forbidden

The registry accepted the credentials but refused the request (HTTP 403). Check
your permissions for the registry or the package scope.

Owner: core
Level: error
```

## R003

```
R003: Registry rate limit

The registry is rate limiting requests (HTTP 429); the message says how long to
wait. Retry after that delay.

Owner: core
Level: warning
```

## R004

```
R004: Registry request timed out

A request to the registry didn't answer in time. Check the network connection
and the registry URL, or try again later.

Owner: core
Level: error
```

## R005

```
R005: Registry network error

A request to the registry failed at the network level. Check the network
connection and the registry URL.

Owner: core
Level: error
```

## R006

```
R006: Package version not found

The registry answered that it has no such package or version (HTTP 404 with the
registry protocol's NOT_FOUND error). Check the package name and version.

Owner: core
Level: error
```

## R007

```
R007: Version already published

`specforge publish` tried to publish a version that already exists for the
package, and published versions are immutable. Bump the version in the manifest
and publish again.

Owner: core
Level: error
```

## R008

```
R008: Registry refused the request

The registry read the request and refused it (HTTP 400), naming its reason and
code: for a publish, a package that isn't signed or whose signature isn't one or
doesn't verify over the uploaded binary and manifest (INVALID_SIGNATURE), a
binary that isn't a Wasm module, or a manifest that isn't an extension
declaration or names another package. Nothing was stored, and the version can
still be published. Fix what the message names and try again; sending the same
request again is refused the same way.

Owner: core
Level: error
```

## R009

```
R009: Registry reply unreadable

The registry's answer isn't one the registry protocol defines for that request:
a body that isn't the request's JSON (such as a publisher signature that isn't
an Ed25519 signature object, or a publish receipt that describes another upload
than the one sent), a 404 without the protocol's NOT_FOUND error (no registry
serves that address: the configured `url` isn't a SpecForge registry's root), or
a download URL that is neither an `http`/`https` URL nor a path at the
registry's root. The registry is broken, or the URL isn't a SpecForge registry's
root. Nothing is installed and no key is pinned, and `--allow-unsigned` doesn't
accept such a package. Check the registry and its `url` in `specforge.json`.

Owner: core
Level: error
```

## R010

```
R010: Registry token variable not set

The registry's credential is a reference to an environment variable (`specforge
login --token-env`, or a `token_env` entry in `~/.specforge/credentials.json`)
that isn't set, or is blank, where the command runs. Set it, or log in again
with another source.

Owner: core
Level: error
```

## R011

```
R011: Registry token file unreadable

The registry's credential is a reference to a token file (`specforge login
--token-file`, or a `token_file` entry) that can't be read, or is empty. Check
the path and its permissions, or log in again with another source.

Owner: core
Level: error
```

## R012

```
R012: Credentials file unreadable

`~/.specforge/credentials.json` can't be read, or isn't in the expected format.
Check its permissions, or delete it and log in again.

Owner: core
Level: error
```

## R013

```
R013: Credentials file not writable

The credentials couldn't be saved: creating `~/.specforge`, serializing the
credentials, or writing `~/.specforge/credentials.json` failed. Check the
permissions on `~/.specforge`.

Owner: core
Level: error
```

## R014

```
R014: Locked package served as other bytes

The registry serves the version of a package that `specforge.lock` pins, but not
the binary the lock pins: its SHA-256 is another. A published version never
changes (R007), so the registry, or the address `specforge.json` sends the
package to, now serves something else under that version. Nothing was downloaded
or installed, no publisher key was pinned or asked about, and the lock is left
as it was; `specforge add --dry-run` reports it too. Check the registry. If you
trust the bytes it now serves, remove the extension (`specforge remove <name>`)
and add it again, which pins them.

Owner: core
Level: error
```

## W001

```
W001: Behavior implements no feature

A `behavior` entity has no outgoing edge to any `feature`, meaning it doesn't
implement anything declared. Add an `implements` reference to a feature, or
remove the behavior if it's unused.

Owner: @specforge/software
Level: warning
```

## W002

```
W002: Unreferenced type

A `type` entity has no incoming references from any `behavior`, `port`, or other
`type`. Reference the type where it's used, or remove it if it's dead.

Owner: @specforge/software
Level: warning
```

## W003

```
W003: Unenforced invariant

An `invariant` that nothing references: no behavior lists it in `invariants`,
`requires`, `ensures` or `maintains`, so no code path is bound to preserve it.
Reference it from the behaviors that must preserve it, or remove it if it no
longer applies. `specforge analyze coverage` counts these invariants but does
not report them again.

Owner: @specforge/software
Level: warning
```

## W004

```
W004: Untested testable entity

A testable entity (`behavior`, `invariant`, `event`, `type`, or `port`) declares
no `verify` obligations, so nothing states what a test must prove about it. Add
`verify unit "..."` (or another obligation kind) statements. Union types and
entities marked `abstract true` (through a flag their kind declares) are exempt;
a struct member named `verify` is a field, not an obligation.

Owner: @specforge/testing
Level: warning
```

## W005

```
W005: Unreferenced port

A `port` entity is not referenced by any `behavior`. Reference the port from a
behavior that uses it, or remove it if it's unused.

Owner: @specforge/software
Level: warning
```

## W006

```
W006: Behavior missing category

A `behavior` entity has no `category` field, which agents rely on for task
routing. Add a `category` field to the behavior.

Owner: @specforge/software
Level: warning
```

## W007

```
W007: Event never produced

An `event` entity is not produced by any `behavior`. Add a `produces` reference
from the behavior that emits it, or remove the event if it's unused.

Owner: @specforge/software
Level: warning
```

## W008

```
W008: Unimplemented feature

A `feature` entity has no incoming edge from any `behavior`, meaning nothing
implements it. Add a behavior that implements the feature, or remove it if it's
not planned.

Owner: @specforge/software
Level: warning
```

## W009

```
W009: Disallowed verify kind

An entity uses a `verify` kind (for example `unit`, `contract`, `integration`)
that isn't in the allowed set for its entity kind. Use one of the verify kinds
listed as allowed in the diagnostic.

Owner: @specforge/testing
Level: warning
```

## W010

```
W010: Unknown field annotation

A `type` field carries an annotation that isn't recognized by the compiler.
Remove the annotation or correct its spelling.

Owner: @specforge/software
Level: warning
```

## W012

```
W012: Unreferenced ref entity

A `ref` entity has no incoming edges, meaning nothing in the project references
it. Reference the `ref` from another entity, or remove it if it's unused.

Owner: core
Level: warning
```

## W017

```
W017: Testable kind lacks verify support

An extension registers an entity kind as `testable` but does not set
`supportsVerify: true`, so verify statements can't be declared on it. Set
`supportsVerify: true` in the extension manifest.

Owner: core
Level: warning
```

## W018

```
W018: Duplicate edge type

Two extensions register an edge type with the same label; the first-registered
extension's definition wins and the later one is ignored. Rename one of the
conflicting edge types to avoid the collision.

Owner: core
Level: warning
```

## W019

```
W019: Unknown field type

An extension manifest declares a field whose `field_type` value the compiler
doesn't recognize (it must be one of `string`, `integer`, `bool`, `enum`,
`string_list`, `reference`, `reference_list`, or `block`). Correct the field's
`field_type` in the manifest.

Owner: core
Level: warning
```

## W020

```
W020: Unrecognized field

An entity sets a field that isn't declared for its kind by any installed
extension. Remove the field, fix a typo in its name, or install the extension
that declares it.

Owner: core
Level: warning
```

## W021

```
W021: Undeclared target kind or edge label

A field, edge type or validation rule references a `target_kind` or edge label
that isn't declared — either in the extension's own manifest when it declares
no peer dependencies, or in the compiler's global kind/edge registry once all
extensions are loaded. Declare the missing kind or edge label, or add the
appropriate peer dependency (a validation rule on another extension's kind that
this one works without names that extension as its `target_extension` instead).
The same code reports a declaration the registry refuses: a `derived_from` that
derives nothing, a `proof_role` other than `bound` or `claim`, or a
`lifecycle_field` that is not one of the kind's fields, an `enum` field that
declares no values, or a `default_value` an enum field does not take. A field
that writes a declared edge type from a kind outside its declared `source_kind`,
or to a kind outside its declared `target_kind`, is W021 too, and its edges are
of no edge type.

Owner: core
Level: warning
```

## W023

```
W023: Duplicate validation rule code

Two extensions register a validation rule using the same diagnostic code. Change
one extension's rule to use a unique code.

Owner: core
Level: warning
```

## W027

```
W027: Re-export binding not found

A selective `pub use { A, B } from "target"` re-export names a binding that
isn't actually exported by the target module. Correct the binding name or remove
it from the re-export list.

Owner: core
Level: warning
```

## W029

```
W029: Event never consumed

An `event` entity is produced by one or more behaviors but has no consumer,
meaning nothing reacts to it. Add a behavior that consumes the event, or remove
the unused production.

Owner: @specforge/formal
Level: warning
```

## W030

```
W030: Abstract behavior unrefined

A `behavior` marked `abstract true` has no concrete refinement — no behavior
declares `refines` against it, and no `refinement` entity names it as the
abstract entity. Add a concrete behavior with `refines`, or a `refinement`
entity naming this behavior as the `abstract_entity`.

Owner: @specforge/formal
Level: warning
```

## W031

```
W031: Refinement chain too deep

A behavior sits in a refinement chain deeper than the maximum allowed depth of 4
layers. Split the refinement chain, or collapse intermediate abstraction layers.

Owner: @specforge/formal
Level: warning
```

## W032

```
W032: Unmitigated retry cycle

`specforge analyze`'s `event_graph_analyze` pass found a `behavior` that
consumes an event and produces the same event again, and neither the behavior
nor the event declares `sync`, so nothing bounds the retries. Declare a `sync`
constraint (a timeout or backoff) on the event or the behavior.

Owner: @specforge/formal
Level: warning
```

## W033

```
W033: Asymmetric port connectivity

`specforge analyze`'s `event_graph_analyze` pass found a `port` that two or more
behaviors use (`ports`) where the most-referenced of them has more than three
times the incoming edges of the least-referenced, a behavior nothing references
counting as one. This is a structural hint that one consumer dominates the
port's access, not a fairness check. Review how the behaviors share the port.

Owner: @specforge/formal
Level: warning
```

## W034

```
W034: Unbounded event channel

`specforge analyze`'s `event_graph_analyze` pass found an `event` that a
behavior produces and that declares no `sync` constraint, so nothing bounds how
many of its messages can accumulate. Declare `sync` on the event: a timeout, a
buffer limit or its delivery semantics.

Owner: @specforge/formal
Level: warning
```

## W035

```
W035: Undischarged coverage items

One or more coverage items (invariants and testable entities) are not proven
under @specforge/testing's coverage rule: some obligation has no passing
recorded test that names it, and no entailed formal claim discharges it. Link a
test to each obligation by its text and run `specforge collect`; `specforge
analyze coverage` lists what is unproven (A001, A015).

Owner: @specforge/formal
Level: warning
```

## W039

```
W039: Repeated precondition

`specforge analyze`'s `condition_check` pass found a `behavior` whose `requires`
names the same condition more than once. The repeat states nothing its first
occurrence does not, so it is redundant. Remove it. Conditions are names and
prose, so a precondition implied by a different one is not detected.

Owner: @specforge/formal
Level: warning
```

## W040

```
W040: Invariant without expression

`specforge analyze`'s `condition_check` pass found an `invariant` whose
`guarantee` has no `expression`: the guarantee is prose only, so `specforge
analyze --prove` has no claim to check. Add an `expression` stating the
guarantee as a machine-checkable claim, or keep it prose and prove it with
tests.

Owner: @specforge/formal
Level: warning
```

## W041

```
W041: Unreferenced feature

A `feature` entity has no incoming edges, meaning no `journey`, `milestone`, or
`module` references it. Link it from at least one referencing entity, or remove
it if it is no longer needed.

Owner: @specforge/product
Level: warning
```

## W042

```
W042: Unreferenced journey

A `journey` entity has no incoming edges, meaning no `deliverable` references
it. Reference the journey from a deliverable's `journeys` field, or remove it if
it is unused.

Owner: @specforge/product
Level: warning
```

## W043

```
W043: Deliverable without journeys

A `deliverable` has no edge to a `journey`: no `journeys` field, an empty list,
or only references that don't resolve. Nothing says which user journeys it
supports. List the journeys it serves in `journeys`.

Owner: @specforge/product
Level: warning
```

## W044

```
W044: Unreferenced module

A `module` entity has no incoming edges, meaning no `deliverable` or `milestone`
references it. Reference the module from a deliverable or milestone, or remove
it if it is unused.

Owner: @specforge/product
Level: warning
```

## W045

```
W045: Feature dependency cycle

Two or more `feature` entities form a cycle through their `depends_on` edges.
Break the cycle by removing or restructuring one of the `depends_on` references.

Owner: @specforge/product
Level: warning
```

## W046

```
W046: Deliverable without modules

A `deliverable` has no edge to a `module`: no `modules` field, an empty list, or
only references that don't resolve, so it has no structural decomposition. List
the modules it ships in `modules`.

Owner: @specforge/product
Level: warning
```

## W049

```
W049: Milestone without features

A `milestone` entity has no `features` field, so it may be empty. Modules listed
in `modules` don't count: the check reads only `features`. List the features the
milestone delivers, or remove the milestone.

Owner: @specforge/product
Level: warning
```

## W053

```
W053: Breaking schema change

A graph protocol schema change classified as breaking since the last export:
something a reader of the recorded schema relies on was removed or reshaped —
a removed entity kind, edge type or field, a field added as required, a field
whose type, required flag, edge type or target kind changed, an enum value lost,
or an edge type that lost a source or target kind. `specforge export` compares
every attribute of the schema against the one the previous export recorded in
`.specforge/schema-cache.json`, bumps the major version, and names the versions
in the warning; the export is still written and the cache updated. Update the
agents and tools that read the export, or keep the extension versions that
produced the old schema.

Owner: core
Level: warning
```

## W054

```
W054: Migration structural drift

Comparing the entity graph before and after a migration found entities or edges
that appeared, disappeared, or changed unexpectedly. Review the migration logic
to ensure it preserves the entities and edges it did not intend to change.

Owner: core
Level: warning
```

## W057

```
W057: Missing milestone exit criteria

A `milestone` entity has `status: completed` but no `exit_criteria` field. Add
an `exit_criteria` field describing how completion was verified.

Owner: @specforge/product
Level: warning
```

## W060

```
W060: Cross-kind ID collision

The same entity ID is declared with two different entity kinds, either within
the same compilation pass or across different files. Entity IDs share one flat
namespace regardless of kind, so rename one of the conflicting declarations; the
first declaration encountered is retained and later ones are skipped.

Owner: core
Level: warning
```

## W061

```
W061: Reference cycle detected

The resolved reference graph contains a cycle among entity references. Cycles
that share an entity are one warning, whose path walks every entity on them.
Break each cycle by removing or inverting one of its references.

Owner: core
Level: warning
```

## W092

```
W092: Release dependency cycle

Two or more `release` entities form a cycle through their `depends_on` edges.
Break the cycle by removing or restructuring one of the dependency references.

Owner: @specforge/product
Level: warning
```

## W093

```
W093: Invalid release version format

A `release` entity's `version` field does not match semver format (e.g.
`1.0.0`). Set `version` to a valid semver string.

Owner: @specforge/product
Level: warning
```

## W096

```
W096: Behavior requires without ensures

A `behavior` entity declares a `requires` clause (an obligation on callers) but
no `ensures` clause (a guarantee in return). Add an `ensures` clause describing
what the behavior guarantees when its `requires` is satisfied.

Owner: @specforge/formal
Level: warning
```

## W097

```
W097: Test record for an unknown entity

`specforge analyze` read a recorded test result (in `specforge-report.json`) for
an entity ID that no spec declares, so the result counts toward nothing; a `did
you mean` hint names a close match when there is one. Fix the entity ID the test
names, or declare the entity.

Owner: core
Level: warning
```

## W098

```
W098: SMT solver unavailable

`specforge analyze --prove` could not ask the SMT solver z3 everything it had
to: z3 is not on `PATH` or does not run (`specforge doctor` states the same
warning as a toolchain finding), a solve ran past the timeout
(`--prove-timeout`, 30 seconds by default), or z3 failed mid-run. The checks it
could not answer were skipped: no bound was found contradictory and no claim was
proved by them. Install z3 (https://github.com/Z3Prover/z3) and make sure `z3
--version` runs.

Owner: core
Level: warning
```

## W110

```
W110: Refines non-abstract behavior

A behavior's `refines` field names a target behavior that is not marked
`abstract true`. Add `abstract true` to the target behavior, or point `refines`
at a behavior that is actually abstract.

Owner: @specforge/formal
Level: warning
```

## W112

```
W112: Validation rule cannot fire

An extension-declared validation rule cannot work as declared: its `check` kind
is unrecognized, it is missing a field or constraint its check needs (a
`cycle_detection` rule needs an `edge_type`, a `verify_kind_allowlist` rule a
constraint with values; a rule that reads `verify` statements needs a target
kind that accepts them), its values list is empty, its `matches` regex does not
compile, or its `wasm_function` is absent or failed a probe call. Fix or remove
the rule in the extension's manifest.

Owner: core
Level: warning
```

## W113

```
W113: Circular file import

Two or more `.spec` files import each other, forming a cycle in the import
graph. Break the cycle by removing one of the `use` imports or extracting the
shared entities into a separate file.

Owner: core
Level: warning
```

## W115

```
W115: Invalid collector report

A collector reported tests for an entity ID that no spec declares (usually a
renamed entity or a typo in the test's annotation). `specforge collect` drops
those results; fix the test annotation so it names a declared entity.

Owner: core
Level: warning
```

## W118

```
W118: Invalid provider configuration

A `providers` entry in `specforge.json` is missing its `alias`/`name` or
`scheme` field, or no installed extension contributes providers to back a
configured provider. Add the missing field, or install an extension that
contributes the provider.

Owner: core
Level: warning
```

## W123

```
W123: Unreferenced property

A `property` entity is not referenced by any `behavior`, so it may be unused.
Reference the property from a behavior's `verify` block, or remove it if it is
no longer needed.

Owner: @specforge/formal
Level: warning
```

## W124

```
W124: Empty property description

A `property` entity writes a `description` that is empty or only whitespace,
which makes the temporal assertion opaque to readers and agents. Write the
description, or remove the field. A property that writes no description is not
reported.

Owner: @specforge/formal
Level: warning
```

## W126

```
W126: Unreferenced axiom

An `axiom` entity is not referenced by any other entity, so it may be unused.
Reference the axiom from a relevant entity, or remove it if it is no longer
needed.

Owner: @specforge/formal
Level: warning
```

## W127

```
W127: Empty axiom description

An `axiom` entity writes a `description` that is empty or only whitespace, which
leaves the assumption unexplained. Write the description, or remove the field.
An axiom that writes no description is not reported.

Owner: @specforge/formal
Level: warning
```

## W128

```
W128: Unreferenced protocol

A `protocol` entity is not referenced by any `event`, so it may be unused.
Reference the protocol from an event, or remove it if it is no longer needed.

Owner: @specforge/formal
Level: warning
```

## W129

```
W129: Empty protocol description

A `protocol` entity writes a `description` that is empty or only whitespace,
which makes the synchronization contract opaque. Write the description, or
remove the field. A protocol that writes no description is not reported.

Owner: @specforge/formal
Level: warning
```

## W131

```
W131: Unreferenced refinement

A `refinement` entity is not referenced by any other entity, so it may be
unused. Reference the refinement from the entity it refines, or remove it if it
is no longer needed.

Owner: @specforge/formal
Level: warning
```

## W132

```
W132: Empty refinement description

A `refinement` entity writes a `description` that is empty or only whitespace,
which makes the abstract-to-concrete mapping opaque. Write the description, or
remove the field. A refinement that writes no description is not reported.

Owner: @specforge/formal
Level: warning
```

## W133

```
W133: Refinement without invariant deltas

A `refinement` entity declares no `invariant_deltas` (the field is absent or an
empty list), so it records no change between its abstract and concrete
behaviors. List the invariants the refinement adds or relaxes in
`invariant_deltas`.

Owner: @specforge/formal
Level: warning
```

## W134

```
W134: Unreferenced process

A `process` entity is not referenced by any other entity, so it may be unused.
Reference the process from a relevant entity, or remove it if it is no longer
needed.

Owner: @specforge/formal
Level: warning
```

## W135

```
W135: Empty process description

A `process` entity writes a `description` that is empty or only whitespace,
which makes the communicating process opaque. Write the description, or remove
the field. A process that writes no description is not reported.

Owner: @specforge/formal
Level: warning
```

## W136

```
W136: Empty process alphabet

A `process` entity writes an empty `alphabet` (`[]`), so it declares no events
it can engage in. List the events in its alphabet. An absent `alphabet` is E006,
since the field is required.

Owner: @specforge/formal
Level: warning
```

## W137

```
W137: Ambiguous convention mapping

`specforge collect` links a test that no annotation links by its name:
`entity_id__obligation_slug`, or a module named after an entity. This test's
name splits at more than one `__` into a declared entity ID (entity IDs aren't
meant to contain `__`), so it isn't linked. Rename the test or the entity, or
link the test explicitly (`#[specforge_test]`).

Owner: core
Level: warning
```

## W138

```
W138: Unknown describe key

An item of an extension's describe answer has a key the extension protocol
doesn't define, so the host ignores it. It's usually a misspelling in a
hand-written (`raw_category`) item, `testabel` for `testable`, which leaves the
item without what the key was meant to declare; or the extension was built with
a newer SDK than this host. The message names the extension, the category, the
item and the key. Fix the spelling, or update SpecForge.

Owner: core
Level: warning
```

## W139

```
W139: Formal claim not entailed by declared bounds

`specforge analyze --prove` found that a declared claim (a field an extension
gives the `claim` proof role, such as a formal property's or invariant's
`expression`) isn't guaranteed by the declared bounds: the SMT solver found a
counterexample that satisfies every bound while violating the claim. The claim
isn't wrong; the bounds just don't guarantee it yet, and its `verify property`
obligation stays unproven. Strengthen the declared bounds or weaken the claim.
Use `--strict` to fail the run on it. This code was E047 until it was renumbered
to match its severity.

Owner: core
Level: warning
```

## W140

```
W140: Duplicate registry alias

Two entries in `registries` in `specforge.json` share an alias, so the alias
doesn't name one registry. Give each registry a unique alias.

Owner: core
Level: warning
```

## W141

```
W141: Invalid formatter configuration

The formatter's `.specforgefmt.toml` can't be read, isn't valid TOML, or sets
`indent_width` (an integer from 1 to 16), `use_tabs` (a boolean) or `max_width`
(an integer from 40 to 200) to an invalid value. The formatter uses the default
for anything it can't use. Fix the reported setting.

Owner: core
Level: warning
```

## W142

```
W142: Unparseable region left unformatted

The formatter hit a parse error in a `.spec` file. It keeps the reported line
range exactly as written and formats the rest. Fix the syntax there (see E001)
and format again.

Owner: core
Level: warning
```

## W143

```
W143: Define blocks are not supported

A `.spec` file has a `define <name> { ... }` block. Every entity kind comes from
an extension, so a project's kinds depend only on `specforge.json` (ADR 0005):
the block registers nothing and is left out of the graph. Declare the kind in an
extension (`specforge extension init`) and enable it, then remove the block.
`define` stays a reserved word.

Owner: core
Level: warning
```

## W144

```
W144: Invalid build cache

The project root has a `specforge-cache.json` that can't be read, isn't valid
JSON, or declares a `format` other than 1. The build cache records each entity's
status from the build `specforge check --cache` last wrote, and check-phase
passes compare against it (status transitions); with the file invalid they get
no previous statuses, so history rules stay silent. Rewrite it with `specforge
check --cache`, or delete it.

Owner: core
Level: warning
```

## W145

```
W145: Pass order constraints form a cycle

An extension's compiler passes declare `after`/`before` constraints that form a
cycle, so no order satisfies them all. The registry build reports it, naming the
passes in the cycle, and runs that extension's passes in the order it declares
them. Remove the constraint that closes the cycle.

Owner: core
Level: warning
```

## W147

```
W147: Validation rule property ignored

An extension-declared validation rule sets a property its check does not read
— an `edge_type` on a field check, a `constraint` on an edge check, a
`wasm_function` on a declarative check, a constraint kind, `pattern` or `values`
its check does not read. The rule is registered without it, so it does not do
what its author meant. Remove the property, or use the check that reads it
(`conditional_field_required` reads `constraint.pattern` as the condition
field's name, `field_value_constraint` with `matches` as a regex).

Owner: core
Level: warning
```

## W148

```
W148: Custom rule could not check entities

A `check: "custom"` rule's `wasm_function` failed (trapped, or answered
something that is not a verdict) on some entities during this check, so they
were not checked. Reported once per rule, with how many failed and the first
one's error. The load-time probe (W112) calls the function on an empty entity
only; fix the function so that it answers every entity of the rule's target
kind. The diagnostic's data lists every entity that was not checked, with its
error.

Owner: core
Level: warning
```

## W149

```
W149: Installed extension is not pinned

A `specforge.lock` entry records no hash (it was written before installs were
pinned), so its binary loads without being checked. Reinstall the extension with
the command the suggestion names to pin it.

Owner: core
Level: warning
```

## W150

```
W150: Extension reports a code it may not use

An extension declared a validation rule, or a pass reported a diagnostic, with a
code it may not use: a code the catalog gives to core or to another extension,
its own code at a level the catalog does not give it, a retired code, a
first-party extension's uncatalogued code, or a third-party code outside
`E900`-`E998`, `W900`-`W998` and `I900`-`I998` or whose prefix contradicts its
level. The rule still runs and the finding is still reported, with the code as
given, so its title and docs link may describe another diagnostic. Renumber it
in the extension's range (a third-party extension) or catalogue it (a
first-party one).

Owner: core
Level: warning
```

## W151

```
W151: Entity kinds left unchecked

Extensions are loaded, but none of them declares an entity kind, so the
entities' kinds, fields and identifiers are not checked: no E024 for an unknown
kind, no W020, E013, E014, E022 or E061. The warning names how many entities
that leaves unchecked and their kinds. Enable the extension that declares those
kinds (the suggestion names it when it is a builtin), or remove the entities. A
project with no extension at all is structural-only on purpose and gets I002
instead.

Owner: core
Level: warning
```

## W153

```
W153: Sandbox declaration not honoured

An extension's declaration asks its sandbox for something the host does not
give, so the extension runs without it. A component is granted no capability,
whatever it declares: no directory, environment, arguments, stdin, socket or
name lookup (ADR 0037). So a `sandbox_policy` key other than `max_execution_ms`
and `max_memory_mb` that asks for something (`network_access: true`, a non-empty
`allowed_paths`, a misspelled limit), and a surface's `sandbox` override, grant
nothing. A declared limit above the host's ceiling (30000 ms per call, 512 MB of
linear memory) is held to the ceiling. The message names the extension and the
key. Remove the key, or lower the limit. An extension that needs a file's
content gets it in its input (an analyzer is handed each file); one that needs
the network can't run in SpecForge.

Owner: core
Level: warning
```

## W154

```
W154: Completed milestone delivers an unfinished feature

A `milestone` has `status: completed` but a feature its `features` names is
neither `done` nor `deprecated`, so the milestone's completion claims what its
features do not. Reported once per such feature, by the `lifecycle` pass of
@specforge/product. Mark the feature done if it was delivered; otherwise move it
out of the milestone or set the milestone back to `in_progress`.

Owner: @specforge/product
Level: warning
```

## W155

```
W155: Unsigned package accepted

`specforge add` or `specforge update` accepted a registry package that carries
no publisher signature, because `--allow-unsigned` was given; it is reported
when the package is fetched, whether or not the install then goes ahead. Where
it came from can't be verified, and no publisher key is pinned for it, so a
later signed version is pinned on first use. Install signed packages where you
can; ask the publisher to sign with `specforge publish`.

Owner: core
Level: warning
```

## W156

```
W156: Publisher key re-pinned

A registry package is signed with a different key than the one pinned for it,
the change was accepted (`--yes`, a yes at the prompt, or the new key on
`trusted_keys`), and the install was written, so the new key is now pinned. It
is reported when the pin is saved: an install that is refused or fails pins
nothing and reports none. That is what a key rotation looks like, and also what
a compromised publisher looks like: confirm the change with the publisher if you
did not expect it. The message names both key ids.

Owner: core
Level: warning
```

## W158

```
W158: Verify statements on a kind that accepts none

An entity declares `verify` statements, but its kind accepts none: no extension
made the kind testable (`supports_verify`). `verify` is reserved syntax; what an
obligation means comes from an extension, so a kind takes statements only once
one enables them (ADR 0002, ADR 0094). The statements are not fields, so this is
not W020. Enable an extension that makes the kind testable (for software kinds,
`specforge add @specforge/testing`), or remove the statements.

Owner: core
Level: warning
```

## W159

```
W159: Value not written in its field's form

A field's declared type decides how its value is written (ADR 0147): bare for an
enum, identifier or reference value and each item of a reference list; quoted
for text and each item of a string list. This value is written the other way. It
reads the same, so the graph is unchanged; run specforge format (or format the
document in the editor) to rewrite every value in its form. A bare word on a
kind that takes members (type, port) is left alone: it may be a member's type.

Owner: core
Level: warning
```

## W161

```
W161: Default that changes nothing

Either a block writes a field with exactly its file's default value, so the line
restates the default, or a `defaults <kind>` field reaches no block: every block
of that kind in the file writes its own value, or the file declares none. Delete
the line. A block that must keep its value whatever the default becomes may
write it; a different value replaces the default whole (ADR 0149).

Owner: core
Level: warning
```

## W162

```
W162: Relation declared from both sides

A relation has two fields: one extension declares it from both of its ends.
Either a reference field's `inverse_of` names a field its target kind registers
(`milestone.behaviors` naming `features`, when `behavior` has its own
`features`), or two different kinds each register a reference field targeting
the other (`parent.children` and `child.parent`). A relation is written from one
side (ADR 0150): the other side is derived, so `query` and `trace` already show
it. Both fields still register, and an entity can write either. Remove one
field, or drop the `inverse_of` or name a word the target kind does not use. The
builtin extensions declare none.

Owner: core
Level: warning
```

## Retired codes

These codes are no longer emitted, and are never reused for another meaning.

| Code | Replaced by |
|------|-------------|
| E017 | (nothing) |
| E018 | (nothing) |
| E020 | (nothing) |
| E023 | (nothing) |
| E029 | (nothing) |
| E035 | (nothing) |
| E037 | (nothing) |
| E038 | (nothing) |
| E047 | [W139](#w139) |
| E051 | [E079](#e079) |
| E053 | (nothing) |
| E060 | (nothing) |
| I006 | (nothing) |
| W011 | (nothing) |
| W024 | (nothing) |
| W025 | (nothing) |
| W026 | (nothing) |
| W028 | (nothing) |
| W050 | [E061](#e061) |
| W051 | [E061](#e061) |
| W052 | [E061](#e061) |
| W062 | [E073](#e073) |
| W063 | (nothing) |
| W077 | [E061](#e061) |
| W078 | [E061](#e061) |
| W079 | [E061](#e061) |
| W080 | [E061](#e061) |
| W083 | [E061](#e061) |
| W084 | [E061](#e061) |
| W085 | [E061](#e061) |
| W095 | [E061](#e061) |
| W099 | (nothing) |
| W111 | (nothing) |
| W114 | (nothing) |
| W116 | (nothing) |
| W117 | (nothing) |
| W119 | (nothing) |
| W120 | (nothing) |
| W121 | [E061](#e061) |
| W122 | (nothing) |
| W125 | [E061](#e061) |
| W146 | (nothing) |
