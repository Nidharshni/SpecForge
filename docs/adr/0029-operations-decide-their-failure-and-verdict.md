# Operations decide their failure and their verdict; a command ends in one place

**Status:** accepted (2026-10-07). Amends ADR 0015 (I1, D8), ADR 0021 (D4) and ADR 0011 (the
stdout/stderr bullet).

ADR 0024 D7 said operations fail with a kind decided where the failure is raised. The read views that
read the recorded test report did not: they returned the project crate's `ReportError`, and MCP (a
classifier at eight call sites), analyze (`From<Diagnostic>`: always schema mismatch) and plan (the
same) each decided what it was. The same report the OS refused to read was `internal_error` from
`specforge.stats`, `schema_mismatch` from `specforge.analyze`, never `permission_denied`; a missing
`test_results` was a schema mismatch, and a relative one was read from the server's working directory.
Format and migrate had no verdict in ops: the CLI restated ADR 0021 D4's rule and migrate's, MCP
returned none and chose migrate's failure kind itself. The CLI printed an operation's failure in seven
shapes and five commands ignored `--format json` when they refused. MCP refused to format or migrate a
directory its own call target had resolved, and format, migrate and the call target each wrote "the
nearest project, else the directory itself" on their own.

## Decisions

- **D1. A report failure is classified once, in ops.** `specforge_project::coverage::ReportError` says
  what happened (unreadable with the OS error's kind, or malformed; invalid UTF-8 is malformed);
  `specforge_ops::report::unusable` turns it into the `OpError` every view returns: code E045,
  `schema_mismatch` when malformed, else the OS error's kind (`OpErrorKind::of_io_kind`:
  `permission_denied`, `file_not_found` for a named file that does not exist, else `internal_error`),
  with a suggestion for that kind. No `ReportError` crosses the ops interface; `EntityFacts::coverage`
  is `Result<EntityCoverage, OpError>` (amends ADR 0015 I1).
- **D2. Typed operation errors convert through one `From` in ops.** `CheckError`, `TraceError`,
  `PlanError` and `AnalyzeError` stay where a surface names the argument a variant is about (the
  spelling of an argument is a surface's: `severity_filter` against `--severity`); their kind and code
  are their `From<…> for OpError`, which every surface goes through. `AnalyzeError` gains it
  (`unknown_pass` with a did-you-mean, the classified E045, `no_test_results`).
  *Amended by ADR 0072 (round 9):* a typed error's variant also decides the field of the request its refusal is
  about (`From<CheckError> for Refusal<CheckField>`, likewise `AnalyzeError` and `TraceError`); its
  `From<…> for OpError` is that refusal's error. The surface still spells the argument.
- **D3. Format and migrate carry their verdict.** `format::Outcome::ok()` (with `Mode::Preview` for a
  diff that does not fail), `migrate::Outcome::ok()` and `failure()`; `OpErrorKind::CompilationFailed`
  makes the map to MCP's error codes total, so MCP decides no kind. MCP returns `ok` in the result (and
  in a failed migration's `data`) and keeps `isError` for refusals and failed runs as before (ADR 0004
  D4-a: a run that finds something is a successful call).
- **D4. One exit table**: 0 the run passed; 1 its verdict failed or its operation refused; 2 it could
  not judge (a refused command line, or a refusal of `stats` or `analyze`). Rejected: exit by kind,
  which contradicts `init` and `add`'s specified exit 1 for invalid input.
- **D5. One refusal shape**: `error[CODE]: message`, `  hint: …`, `  wrote: …` on stderr, or the error
  document on stdout under `--format json` (`analyze --json`; ADR 0011 keeps core errors on stdout), for
  every core command (`specforge_cli::outcome::Refusal`). A command run outside a project it needs
  refuses with `no_project`.
- **D6. One rule for the project a path is in** (`specforge_common::project_root_of`): format and
  migrate act on it on both surfaces, a directory that is no project included; migrate compiles the
  root it migrates, so a migration started from a sub-path runs the project's hooks. The LSP opens
  the project its workspace folder is in by the same rule, spelled as the editor spells the folder
  (`specforge_common::enclosing_project_root`, which `find_project_root` runs over the canonical path),
  so a folder inside a project is never its own config-less project (round 6 follow-ups, 2026-10-11).
- **D7.** Export's schema-cache protocol is `ops::export::export_recorded` (as ADR 0018 D1 for check);
  MCP still never records (ADR 0015 D10). An extension command's date is read by the operation that runs it (`specforge_ops::command::run`, ADR 0011 O1). MCP's
  no-project refusal reads `McpError::file`, never its message; `Reach::takes_path` is the one predicate
  for an entry that takes a `path`.
- **D8. MCP resolves a relative `test_results` against the call's project root**, as the paths of
  `specforge.format`; the CLI keeps shell semantics (relative to the working directory).

## Consequences

- MCP: a locked report is `permission_denied` on every tool that reads it; a missing `test_results` is
  `file_not_found`; a relative `test_results` is read under the project root; `specforge.format` and
  `specforge.migrate` return `ok` and act on a directory that is no project; the outline's
  missing-file refusal carries `file`.
- CLI: `stats`, `trace`, `analyze --json`, `migrate` and `init` print the error document under JSON
  output; `migrate`, `export`, `schema`, `init`, `analyze` and `check` refusals take the one shape
  (`= help:` becomes `  hint:`); `collect` outside a project is `no_project`, not E045;
  `migrate --path <sub>` runs the project's hooks. Exit codes are unchanged.
- The parity harness compares verdicts: the CLI's exit code and MCP's `ok` cannot disagree for check,
  analyze, format and migrate.

## Amendment (architecture round 5, plan 15)

- **D3a. Analyze's verdict includes its coverage gate; doctor carries one.** `AnalyzeOutcome::verdict()`
  (`specforge_ops::RunVerdict`: passed, failed, unjudged) folds in the `min` gate: below it fails (E048), a
  coverage pass that gave no figure leaves the run unjudged (E068), and a minimum the coverage pass will not
  answer is refused before any pass runs (E068). The analysis document carries `gate`; under `--json` nothing
  goes to stderr. `specforge.analyze` takes `min`; an unjudged gate there is `ok: false` with
  `gate.status: "unjudged"`, not an error. `DoctorReport::ok()` is doctor's verdict; `specforge doctor` exits by
  it and the user's credential health, which only the CLI shows. Doctor's verdict fails whenever `check` fails on a setup diagnostic (ADR 0054 S4).
- **D4a. A core command's `run` returns `Exit`.** `main` alone turns it into a process code; `Refusal::report`
  returns `Exit`, and `RunVerdict` (`specforge_ops`) is the verdict an operation that can be unjudged returns.
  Extension commands keep their own code (ADR 0011). The build-cache write failure, the watch start failure
  and an unknown `explain` code are refusals (`error[CODE]`).
- **D5a. Collect's precondition is the operation's.** `collect` refuses a root that holds no project
  (`no_project`, `OpError::no_project`) on both surfaces; the CLI finds the root by D6's rule
  (`project_root_of`), so MCP answers `no_project` where it answered E058 for a directory that is no project.
- **D8a. MCP resolves a relative `out_dir` of `specforge.render` against the call's project root** as it does
  `test_results`; with no project served a relative one is invalid input on `out_dir`. `output_files` lists
  absolute paths.

## Amendment (architecture round 6, plan 11)

- **D4b. A core command writes through the terminal `main` gives it.** `specforge_cli::terminal::Terminal`
  is a command's stdout, its stderr, the input a prompt reads, and two facts fixed when it is made: whether
  each stream is coloured (`NO_COLOR`, then `CLICOLOR_FORCE`, then whether the stream is a terminal) and
  whether anyone can answer (stdin is a terminal). `main` makes the process's terminal, runs the command
  with it and turns its `Exit` into the process code (D4a); a test captures one. No other module names the
  process's stdio: the crate denies `clippy::print_stdout` and `clippy::print_stderr`. Extension commands
  write through it too and keep their own code (ADR 0011). A prompt (a publisher key change, a collector's
  command) is asked of the terminal by one rule: nobody to ask is no; otherwise `y` or `yes`, in any case,
  is yes. A reader that closes stdout early (`| head`) ends the output quietly; the exit stays the
  command's. `specforge explain` bolds the code by the colour rule.
  - Rejected: a command returning what it rendered (`Rendered { stdout, stderr, exit }`) for `main` to
    print once. `watch` and `mcp` write without end, `collect` announces its runner before the runner's
    own output and asks before it runs, `add` and `update` ask in the middle of the operation, and in a
    terminal stderr lines interleave with stdout lines: a returned value needs a second shape for each of
    those; the terminal is one shape for all (`extension_command::ended` was its first instance).
- **D5b. A command's arguments are its module's.** Each command module declares its clap arguments
  (`Args`) and builds its operation's request from them; `main` only dispatches. Every core command whose
  output has a human and a JSON form takes `--format human|json` (`analyze` and `watch` lost `--json`;
  `trace`'s default stays json); every path argument is a path. This replaces D5's "(`analyze --json`;"
  parenthesis: the error document is printed under `--format json` on every core command.
- **D9. The CLI's process-level tests stay processes.** At `734f617f`, 47 of the 53 CLI test modules ran
  the binary. A process start cost about 7 ms; the suite's time was compiling, the LSP, cargo builds,
  watch debouncing and the local registry (34 tests of a second or more held 74% of it, and the slowest
  spawned nothing). Moving them in process would save under a second and lose the binary contract (clap,
  the exit table, the streams, the environment). In-process tests are for what a process cannot reach: a
  terminal that answers, colour as a fact, a closed stdout, an argument mapping.

What would reopen D4b and D9: process start dominating the CLI suite's time, or a second front end (a TUI,
an embedding) that needs a command's output as a value rather than as writes.

## What would reopen it

A surface that needs a failure kind ops cannot know (a transport failure), or a command whose exit
code must say more than passed, failed or unjudged.

## Amendment (architecture round 7, plan 01)

- **D6a.** D6's rule is `specforge_common::ProjectRoot` (ADR 0053): every surface, not only format and
  migrate, opens the project a path is in, and the operations take the root, not a path. `project_root_of`
  and `enclosing_project_root` are deleted.

*(Amended, architecture round 9, plan 06.)* D1: inspect's coverage is `inspect::coverage`'s
`Result<EntityCoverage, OpError>`, no longer a field of `EntityFacts`. D2: `PlanError` is gone — the plan check reads
no report and a plan is refused where it is read (ADR 0076 D3).
