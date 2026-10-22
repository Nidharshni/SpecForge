# SpecForge domain terms

Terms the code, the specs and the docs use with one meaning. Architecture decisions are in
`docs/adr/`; ADR 0004 records the one-project migration these terms come from.

- **Environment**: everything derived from `specforge.json` and the loaded extensions before any
  `.spec` file is read: config, spec root, registries, rules, surfaces, load diagnostics, and the
  runtime the extensions were loaded in (`runtime()`; none when it was loaded without one, so nothing
  loaded). Every extension call an operation makes over the project, a check's included, runs in
  that runtime (`specforge_project::Environment`, ADR 0015 "The runtime travels with the
  environment"). It hands out its diagnostics with the setup part each is about
  (`setup_diagnostics()`; `diagnostics()` is the same list without the parts). A `specforge.json` that is there and can't be used is the
  default config (for the unusable file or key), with each reason kept (`config_problems()`) and
  reported as the error E069. It also holds the project's **installed extensions** (`installed()`:
  what `specforge.lock` held when it was read, absent, read or unreadable, once, for every
  operation over the project) and what each `extensions` entry enabled, as the **extension load**
  left it. An environment is assembled from those parts (`Environment::assemble`: the config read,
  the installed extensions, whose root is the project root, and the extension load), and the assembly
  derives the rest: the spec root, the registry build, the custom rules' probes, the providers'
  registration and I002. `Environment::from_read` is its disk adapter: it reads the lock and runs
  the extension load. Its fields are private; it is read through accessors (ADR 0047). A session
  reads `specforge.json` once per load and builds its extension runtime and, holding it, its
  environment from that read, stamping each environment input in the read that reads it (ADR 0030, ADR 0106, ADR 0114). It
  A project opened in a session's place loads its environment first and hands it over before any
  source is read (`ProjectSession::reopen`), so an editor answers what needs only the environment
  (keyword completion) while the sources are still being read.
- **Compiled project**: an environment plus the sources it read, their graph build, what resolving their
  imports and running the checks reported, and the memo of its entity snapshot and coverage. Its
  diagnostics are, by definition, what `specforge check` reports under the default policy, in the one
  report order: the environment's, the imports', the graph build's, the checks', then surface conflicts
  (`specforge_project::CompiledProject`). A one-shot compile (every CLI command) is one and keeps no
  stamp; a project session holds one and keeps it current (`ProjectSession::project`, ADR 0047); it is
  compiled from a project root (`CompiledProject::compile(&ProjectRoot, …)`), or with its sources read through a
  disk the surface hands it (`compile_through`, ADR 0166). A copy shares all but its graph
  (`Clone`, ADR 0108). Rooted or detached is its `root()` (a detached project has none and resolves no
  import); the project root it was loaded at is its environment's (`Environment::project_root`), read to reopen
  it and to hand an operation a project root (ADR 0121). It reads its recorded test report through the disk it is
  handed (`CompiledProject::recorded`, ADR 0139). After an update that skipped the checks its report has none; its
  checked report runs them over a copy (`CompiledProject::checked_diagnostics`, ADR 0167).
- **Project root**: the directory holding the `specforge.json` a path is in, located from any path inside the
  project (a subdirectory, a file) by walking up, the nearest winning
  (`specforge_common::ProjectRoot::of`, ADR 0053). Only `specforge.json` marks a project. With none at or
  above the path, the path's own directory is a **loose root**: a project with the default configuration and
  no extension (I002 says so). A path that does not exist is `file_not_found`. The compile, the session open,
  the environment load and every operation that reads a project from disk take a project root, never a raw
  path; only `init` takes its directory as given. On the command line a project is named by `--path` (any
  path inside it, `.` by default) on every command that reads one, extension commands included. Distinct from the **spec root**, where the project's
  `.spec` files are discovered (`spec_root` in its `specforge.json`, else the project root): the spec root is
  never a project of its own, but a directory below it that holds `specforge.json` is a **nested project**,
  and a path inside it names that project, whose files are none of the outer project's sources (ADR 0080).
- **Enclosing project**: the project a directory lies inside, the nearest directory above it holding
  `specforge.json`. `init` judges it before writing: a directory whose files it reads as sources, or from which it
  reads them, is refused (`inside_project_sources`, ADR 0197), since a nested project ends its sources (ADR 0080).
- **Project path**: how a surface document names a file an operation opened or wrote: relative to the project root
  with `/` between components, absolute and canonical outside it (`specforge_common::ProjectPath`, ADR 0102). A
  declaration's location (a span, a navigation result) is not one: it is named by its source key, relative to the spec
  root (ADR 0055). The CLI's human output names files as typed.
- **Named spec file**: a spec file a request names (a typed create's `file`, `new --in`, outline's `file`, the fixes'
  `file_path`): read by the project view into its source key, relative to the spec root, every spelling of one file
  being that file, or refused once (`not_a_source` outside the spec root or where no source can be, `file_not_found`
  for a read of no source); a new file's project-root spelling is refused like an existing one's; the refusal names the file as the request spelled it (`OpError::file`, MCP's
  `McpError.file`) (`ProjectView::source`, ADR 0195).
- **Format version**: the `.spec` file format a file was written against, `MAJOR.MINOR`, declared by
  the `// specforge-format:` header on its first non-blank line; a file with no header is at the
  current version and reports nothing (migration steps still run on it). The parser reads it with the file
  (`specforge_parser::detect_format_version`, `SpecFile::format_version`) and the compile reports
  I007 (older) and E019 (newer, or a header that is not `MAJOR.MINOR`) on the header line, like any
  parser diagnostic; `specforge migrate` reads and rewrites it through the same module
  (`with_format_version`), so a rewritten header stands where the parser reads it. It is
  distinct from the Graph Protocol's schema version; it is never a block's doc comment.
- **Title**: the string after a block's id in its header (`type Task "A unit of work" {`, `type S "States" = a |
  b`), the one human-readable label a node carries. Every block that becomes a node has one: E076 otherwise; a blank one (only
  whitespace) is none. A `spec` block's name is its title; a `define` block has none. A `//` comment is never a title.
  `specforge migrate` lifts a block's doc comment into a missing or blank title (a spec block's never). Where it is written is the parse's
  (`Entity::title_span`): a typed retitle replaces it there, and the titles step writes a missing one after the name
  (ADR 0187, amended).
- **Untitled**: a block that becomes a node whose title is absent or only whitespace (`graph::title::owes_title`, ADR
  0225): E076 reports it, the titles step writes its title, and a typed create or retitle refuses to make one.
- **Doc comment**: the whole-line `//` comments directly above a block's first line, no blank line between; a
  comment ending the line of the item above trails it, and the format header is never one. Where it is written is
  the parse's (`specforge_parser::Entity::doc_span`): a typed delete removes it with its block, `specforge migrate`'s
  titles step lifts it into a missing title, and the defaults hoist writes its block above it. It never reaches the
  graph (ADR 0148, ADR 0202).
- **Migration step**: a rewrite `specforge migrate` applies to every source it changes, whatever the file's format
  version, after the header rewrite (`specforge_migrate`'s `titles` and `defaults`).
  The plan reads the environment the judge compiles in: the defaults step hoists only what `FileDefaults` applies
  under its field forms (ADR 0186, amended). The graph comparison keeps exactly the changes the steps declare: for
  `titles`, a title where the block had none.
- **Added error**: an error the edited compile reports that the checked report has no diagnostic left to pair, by code,
  severity, file and message with its digits collapsed (ADR 0211); the one rule behind E080 and a migration's refusal.
- **Migration verdict**: whether a planned migration is kept, decided before anything is written: its hooks pass, and
  the edited compile of its planned texts adds no **added error** and has the graph the project has, less the steps' declared changes
  (`specforge_ops::authoring::migrate::{judge, Verdict}`, ADR 0186). A dry run reports it; a refused migration writes
  nothing.
- **Schema version**: the Graph Protocol schema's `MAJOR.MINOR.PATCH`, computed, never declared: the version the
  schema record holds, bumped by the largest change between the recorded schema and the one the loaded extensions
  produce (major when something a reader relies on is removed or reshaped, minor for an addition, patch for
  documentation and provenance; 1.0.0 with no record). One version names one document (one content hash). A
  requested version is admitted when its readers read the current schema, never applied
  (`specforge_ops::schema_version::VersionedSchema`, ADR 0067).
- **Schema record**: `<root>/.specforge/schema-cache.json`, the versioned schema the last successful `specforge
  export` recorded, with its content hash (`specforge_ops::schema_version::SchemaCache`). Every reader of the
  project's schema versions against it; only export writes it, into the project it exports. One that cannot be
  read is I016 and the version starts again at 1.0.0. Every reader reports what versioning found, the I016 or one
  W053 per breaking change, as its **schema notices** (`VersionedSchema::notices`, ADR 0111), which only a noticed read
  hands to a surface (ADR 0130).
- **Migration record**: `<root>/.specforge/migration.json`, what `specforge migrate --rollback` undoes: the
  last kept migration made with backups, each file with its backup and the SHA-256 of the file as the migration
  left it (`specforge_migrate::MigrationRecord`). A rollback restores only a recorded file that still holds that
  text. `--rollback` never removes a backup; a refused migration writes none (ADR 0186).
- **Project session**: a compiled project kept current, plus what it is built from: its **session
  inputs**, the stamps of what it last read, and where each environment load gets its extension runtime
  (ADR 0047). It turns any batch of changed paths into **changes**
  (`ProjectSession::changes`): the comparison with what it last read that bringing it up to date
  makes (`stale`), narrowed to what the paths reach (a directory reaches every source and input
  under it, a symlinked source's target reaches that source, and a reached file is a change only
  when it differs from its stamp). It applies changes (made only by the session itself, ADR 0100) as an update, an environment reload or a
  re-check, and can bring itself up to date with disk without a watcher (`ensure_fresh`, ADR 0073). A session is opened from disk or detached (no project: what the LSP
  holds with no workspace folder and MCP while nothing is served). Watch, the LSP and MCP each hold
  one (`specforge_project::ProjectSession`; MCP's served one is always opened from disk, ADR 0025);
  watch and the LSP feed it watcher events and follow every update that changes its inputs
  (`Update::inputs_changed`) by watching them anew and then bringing the session up to date for what
  changed meanwhile, MCP asks it to be fresh before every request that reads the project (ADR 0014,
  ADR 0030, ADR 0035). In a debug build it checks every update against a cold rebuild, whichever
  surface holds it, and each surface reports a divergence where it reports (ADR 0035). The LSP gives it its open documents as **held buffers**, each batch of edits as one update
  (`ProjectSession::hold_with`), and releases a closed document's buffer (`ProjectSession::release`); what an open
  buffer is to its file is the session's to say (ADR 0023, ADR 0046). After every update that runs the
  checks, its compiled project reports exactly what a fresh compile of the same sources reports, in the
  same order (an editor update that skips the checks while a file does not parse is the one exception).
- **Read ledger**: what a project session read and when (`specforge_project::ledger`, crate-private): its session
  inputs and the stamp of each file, taken in the call that reads it. It does every disk read the session makes
  (the config, the lock and modules, the sources, the check inputs), each through the one recording adapter, derives every source key, and answers `changes`
  and `stale`. A one-shot compile runs the same steps through an adapter that records nothing and reads each source through the disk it is handed (ADR 0106, ADR 0166).
- **Check reads**: every file-system read a check, a rule or a check pass makes, through the `Disk` in `RuleInput`
  (`exists`, `list`, `read`; `specforge_registry::Disk`, ADR 0107). A project session's ledger stamps and records each
  one; its check inputs are the reads its checks made the last time they ran. Tests give the checks a `MemoryDisk`.
- **Environment reads**: every file-system read the environment load makes (`specforge.json`, `specforge.lock`, each
  module the extension load reads) through the `Disk` it is handed (`Installed::read`, `Installed::load`; ADR 0114). A
  project session's ledger stamps and records each one, each link hop included: its environment inputs are what its
  last environment load read.
- **Source reads**: each read of a `.spec` file a session makes, the cold read and every reread, through the `Disk` it
  is handed (`sources::read`; ADR 0129): the surface's, plain for the CLI, watch and the LSP, contained for MCP
  (`ProjectSession::open_through`, ADR 0145). A project session's ledger stamps each source by its key, with the link
  chain it followed, and records each link
  hop the read followed. A held buffer's file is not read, so nothing of it is recorded until it is released.
  A migration reads each source, and compiles the project before and after, through the request's containment
  (`migrate_project`'s `read`, `CompiledProject::compile_through`; ADR 0158, ADR 0166); an edited project reads none.
- **Session inputs**: everything a project session depends on besides its sources' text: where its
  sources are discovered (the spec root and `exclude`), its **environment inputs**
  (the environment reads of its last load) and its **check inputs**
  (the check reads of the last check run: `specforge-cache.json` when a check pass read it, each path a check looked
  for, each directory a missing named file's suggestion listed, and the target of each that is a symlink) and its **source hops**
  (each link hop a symlinked source's last read followed, its target included). They are one table of **recorded
  inputs** (ADR 0133). Every input is keyed by its own name (a symlink stays itself), and each link hop it
  followed is an input of its own. One value per
  environment load, renewed each time the checks run (`specforge_project::SessionInputs`,
  `ProjectSession::inputs`). What a changed path reaches, which directories watch watches, which files
  the LSP asks its client to report and what the session stamps for freshness are all read from it; the
  read ledger stamps each when it reads it (ADR 0106);
  a detached session's inputs are empty (ADR 0030). Nothing else: an import names one of the sources,
  never a file on disk (ADR 0055).
- **Recorded input**: one row of a session's inputs: a file or directory a recorded read touched, as the read
  spelled it, keyed by its own name in its canonical directory, with the directory links on the way to it
  (its way, ADR 0173), with its class (an environment input, a check
  input, a directory the checks listed, or a hop of one source). The class decides what reaches it, which
  directory is watched for it and what its change is; a missing directory on the way to it outside the root is
  watched from its nearest existing ancestor (`specforge_project::ledger`, crate-private; ADR 0133). A source
  read as gone still records the hops it followed (ADR 0141).
- **Link chain**: what a read of a name follows on the file system: its **hops** (each link at the name, in order,
  at most 40, its target included) and its **way** (each directory link it goes through before its own name, below
  the project root as opened). A stamp records where each hop lands; a recorded input keeps its way, and a change at
  a link of either is that input's change (`specforge_project` `ledger::chain`, crate-private; ADR 0129, ADR 0173).
  The containment's walk (ADR 0104) answers a different question: where a name lands.
- **Dangling source link**: a `.spec` symlink whose target does not exist. It is no source (`SourceRoot::files`
  omits it) but the session reads it (`SourceRoot::dangling`), finds it gone and records its chain, so it is that
  source's change when its target appears (ADR 0141).
- **Held buffer**: an editor buffer a project session holds (`specforge_project::Buffer`: the file's path, the
  editor's text and version). While held it is the truth for its file, whatever happens to the file on disk:
  the session's freshness and classification leave it out, and an open or an environment reload builds it in
  place of its file. Releasing it reads the file through the session's one read. Diagnostics computed from it
  are published with its version. Only the LSP holds buffers (ADR 0046).
- **Reopen**: opening the project at a root in a project session's place (`ProjectSession::reopen`). The
  environment loads first and is handed to the caller before any `.spec` file is read; the one cold build reads
  each held buffer in place of its file. Whether the session verifies its updates carries over; the update is
  the replacement (every file rebuilt, the graph delta between the two). An environment reload is a reopen at
  the session's own root; the LSP's open and MCP serving a project are reopens (ADR 0109).
- **Open document**: a `.spec` buffer the editor has open in the LSP (`specforge_lsp::Document`: its text, its
  version, its line index and its syntax). Its identity is its file's session key (`Environment::source_key` of the
  path its URI names, percent-decoded): the document, the diagnostics published for its file and what its
  text is to the compile (the compiled text, changed since, or a text the project does not build:
  `navigation::EditorText`) are all found by that key, so two spellings of one URI (VS Code writes `@` as `%40`) are one document. The
  URI the client opened it with is kept only to name the file back to the client (`specforge_lsp::files::Files`,
  ADR 0056). It is given to the project session as a held buffer.
- **Editor text**: what the editor's text of a session file (its open buffer, else the file on disk) is to the compile (`navigation::EditorText`): the compiled text, changed since, or a text the project does not build. A span of the compile is a range only of an editor text that is the compiled text (`Compiled::range`): a list leaves the item out, an edit is refused `ContentModified`; a diagnostics publish, labelled with the compiled version, places its diagnostics in the compiled text (ADR 0205, ADR 0023).
- **Published diagnostics**: what the LSP last told the editor about one file, kept by session key in two parts,
  each labelled with the version of the buffer it is positions in: the compile's (what the project reports) and the
  formatter's (a W142 per region formatting kept verbatim, while the document is the text it formatted). A publish
  sends the file's whole list, the compile part plus the formatter part of the same version, under the one URI
  the client names the file by. Hover and code actions read the parts; one module writes them (ADR 0056, which
  calls them layers).
- **Call target**: the project one MCP call acts on, resolved from the call's optional `path` and its
  entry's target before the handler runs: the served session (brought up to date), another project opened as a
  session for that call only and brought up to date after the call writes, as the served one is, the directory
  `init` creates, or, with nothing served and no project named, the empty session for an entry that reads only the
  project view (`specforge_mcp::target::CallTarget`). The revision of the request decides whether the call may
  change what is served (`target::Serving`): in a handshake session a `path` naming a project while none is served
  is adopted and the project `init` creates is served (ADR 0014 D5); a stateless request serves nothing it names
  (ADR 0077). An entry's target is derived from what its handler is given (nothing, the project view, the
  project, the new directory), never declared beside it: an entry whose handler acts on the project is refused as
  no project before it runs (ADR 0014, ADR 0024). The `path`, and every path an argument names, resolves inside
  the served project (while none is served, the server's root): one that leaves it is `invalid_input`, so
  "another project" is one nested in the served one (`specforge_ops::Containment`, ADR 0092, ADR 0104).
- **Containment**: what a request may reach on disk: every path (the CLI, ADR 0092 D6) or the paths that land inside
  one directory (an MCP request's bound, or the call's project). Lent by the **Caller** in the operation's request and
  applied where the operation opens or writes the file (a lock plan narrows it to its project, ADR 0137) (`specforge_ops::Containment`, ADR 0104). MCP builds one per call (`Call`), bounds the call's views by it and
  lends it as the caller's (ADR 0183); the call's project hands it to the tools (`ProjectRef::caller`, ADR 0112, ADR 0124), and render's view input hands it the request's bound (ADR 0131).
  Containment does not see a hard link: a write never goes through one (a project file is written by `specforge_common::fresh::replace_file`, which replaces
  the name, ADR 0144), and a read of one is a read of the file in the project: MCP reads the sources of a served project only from
  inside it (a source that resolves outside is E025), and a file with more than one name is not read through a bound: over MCP it is E025 (ADR 0146); the CLI, watch and LSP are unbounded and read it.
- **Contained disk**: a containment as the `Disk` port (`specforge_ops` `ContainedDisk`, ADR 0139): how a crate
  below app reads a project file the surface bounds; a name that lands outside reads as `invalid_input`. Also the disk MCP
  hands a session to read its sources through (ADR 0145); it refuses a regular file with more than one name (ADR 0146), and the disk a bounded caller's format configuration is read through (ADR 0169).
- **Replace file**: how a project file is written (`specforge_common::fresh::replace_file`, ADR 0144): a new file beside the name,
  renamed onto it, so the inode that held the name is never opened for writing. `create_fresh` is the same for a stream.
- **Private replace**: how a file of the user's registry directory is written (`specforge_common::fresh::replace_private`, ADR 0193): missing directories 0700, a temporary created new at 0600, synced, renamed onto the name; a link at either name is replaced, never followed, and the file is 0600 whatever was there. A signing key that is there is never replaced.
- **Caller**: what the surface that calls an operation lends it, one value (`specforge_ops::Caller`, ADR 0124): what the
  call may reach (**Containment**) and who can be asked (**Asking**). The CLI lends every path and its terminal when
  someone can answer; MCP lends the call's one containment (the view's; init: the request's bound) and nobody. Every request
  that opens a project file or can ask carries it as `caller`. The runtime, the registry, the solver and collect's
  machine are passed beside it, not in it. Not the **host** (the process that loads extensions).
- **Project file**: a file of the project an operation reads or writes. It is opened only as a `Held`
  (`specforge_ops::Containment::resolve`): at where its name landed (`real`), named as given (`spelled`), and replaced
  atomically through a temp created new beside it, keeping its permissions (ADR 0104, ADR 0112, ADR 0128).
- **Owned name**: a project file name only one operation writes (a migration's backup, `<source>.spec.bak`): staged
  in the project change as owned, never read, created new at the name (whatever is there, a link or a file, is
  unlinked first, never its target) with the permissions it is staged with, and removed when the change is put back
  (`ProjectChange::stage_owned`, ADR 0171).
- **Project change**: several project files written as one change: each staged as a `Held` with what it holds and will
  hold (or as an owned name), checked, replaced in order, put back in reverse on any failure; what it could not put back is the operation's
  writes (`specforge_ops::change::ProjectChange`, ADR 0128). Its failure is its own: one code per cause
  (`file_changed`, `file_unreadable`, `file_unwritable`), the file named from the root the change was opened under,
  after what the operation was doing (ADR 0155).
- **Planned change**: what a plan of a project's sources will write: per file, relative to the spec root, the text it
  was planned on (none for a new file) and the text it will hold; an unchanged file is not in it. It answers the
  affected files, the after texts, the text edits, the diff and the entities whose blocks it creates, changes or
  deletes; a new file reads as empty, so one left empty is not in it (ADR 0184, amended). It is written as one project change
  (`specforge_ops::authoring::planned::PlannedChange`, ADR 0184). A typed edit's plan and a rename plan each hold one.
- **Served project**: the project an MCP server serves to every call that names none: the nearest project (a
  directory holding `specforge.json`) at or above the server's root (the path `specforge mcp` was started with, or
  a handshake client's `projectRoot`), else nothing; while none is served, every request looks there again first
  (`McpState::follow_root`). In a handshake session a call may also serve one (Call target). It is replaced whole or
  not at all, and brought up to date with disk before a request reads it (ADR 0014, ADR 0077).
- **Update**: one change applied to a project session. Every update goes through one bracket: what the session
  reports and depends on is taken before it and compared after it (`inputs_changed`, ADR 0136). It re-reads and re-parses exactly the changed files (an
  importer parses the same, since references resolve without `use`), applies them to the session's graph
  build, resolves every file's imports again and re-runs the checks (`specforge_project::Update`, ADR 0006,
  ADR 0032). An update says whether the session's inputs changed (`inputs_changed`), what the session
  reported before it and reports after (its diagnostics delta), and, when it was verified, how it differs
  from a cold rebuild (`divergence`). An update that changes no file (a held buffer, a
  released one or a file on disk whose text is already the one it was built from) runs no check, unless a check input changed with it or an earlier update skipped them. One batch of changes is one update: an environment change reloads and covers the rest (ADR 0159).
- **Graph delta**: what an update or a reload changed in the graph: added and removed nodes (each with its
  kind, file and line), modified nodes (the fields whose values differ, and apart from them which of its
  kind, title, methods and outgoing edges differ; a changed annotation, a block field's entry's included, is a changed field), added and removed edges, and the affected files; source
  positions ignored (`specforge_graph::GraphDelta`, re-exported as `specforge_project::GraphDelta`). A graph
  build computes it. `specforge watch --format json` prints it and MCP sends it whole as
  `specforge/graphChanged` (ADR 0058). `specforge migrate` keeps a migration only when the delta from the graph
  before it to the graph after is empty, and reports each change as W054. A node that only moved is no change; the
  update lists it apart (**moved node**, `Update::moved`), so what writes positions (MCP's `specforge://graph`) is
  notified. A typed edit reads it over the files it changes (`compute_graph_delta_over`).
- **Moved node**: a node in both graphs declared at another file or start line, where it is now
  (`specforge_graph::NodeChange` in `Applied::moved` / `Update::moved`). Not part of the graph delta; MCP notifies
  the resources that write positions (`Watched::Graph`, what a resource declares with
  `ResourceSpec::changes_with`).
- **Diagnostics delta**: what an update or a reload changed in what a session reports: the diagnostics added and
  removed, each compared and presented whole, as every JSON surface presents one (code, severity, message, span,
  suggestion, data, origin). A diagnostic that moved or changed in any part is removed and added; identical ones
  count one by one; order is not part of it (`specforge_project::DiagnosticsDelta`). An update holds what the
  session reported before and reports after, shared with its compiled project, and compares them only when asked
  (`Update::diagnostics_delta`). `specforge watch --format json` prints it; MCP sends it, with the I017 its
  extension surfaces add and remove, as `specforge/diagnosticsChanged` (ADR 0074). The LSP does not read it: it
  publishes every file's diagnostics as the editor shows them.
- **Debounce rule**: changes that arrive less than 50 ms apart are one batch, due 50 ms after the last
  of them, each change once. Watch batches the file changes under every directory it watches as one
  stream, and the LSP batches edited documents, by the same rule (`specforge_watch::Coalescer`,
  ADR 0035).
- **Graph build**: the graph of a set of parsed `.spec` files and what building it reported (parse errors,
  duplicates, define blocks, unknown ref schemes, unresolved references, reference cycles), kept current one
  whole file at a time (`specforge_graph::GraphBuild`). Files are taken in path order; each entity ID is the
  node of its first declaration that is not a `define` block. A compile applies every file at once; a session
  applies each update's files to its own, and is then exactly the build of the same files at once; with
  verification on (every debug build) each update is checked against that (ADR 0032).
- **Cycle**: a set of entities (or files) that reach one another through references (or imports), with one closed
  walk through all of them, from the smallest id. W061, W113, E041, E042 and the cycle rules each read the one answer
  (`specforge_cycles::cycles`, ADR 0196); which entity the walk starts from never depends on the written order.
- **Dependency**: an edge `a -> b` says `a` depends on `b`, whatever its label: a written, single or derived
  reference (ADR 0204). Entities on one cycle of dependencies depend on one another and are done together;
  `Graph::cycles` answers them. W061 reads a narrower relation, the written references (ADR 0093 D7).
- **Extension declaration**: everything one extension declares — its handshake and every describe
  category — as the protocol types (`specforge_protocol_types::ExtensionDeclaration`). The SDK
  builds it, the guest serves it, the host loads it once, the Registry build reads it, a package
  registry stores it (ADR 0012). What it contributes is what it declares (`ExtensionDeclaration::declares`
  over the ten describe categories); its handshake's `providers` says it implements configured providers
  (ADR 0050).
- **Guest wire**: what the host and an extension's guest both compile (`specforge-protocol-types`): the
  extension declaration, the operation payloads, the extension vocabulary and the command-arg rule. Every
  builtin blob is built from it, so it holds nothing only the host reads: the host's reading of package names,
  version requirements and peer ranges is `specforge-package` (ADR 0050).
- **Committed blob**: a wasm component the repository commits, at `<crate>/wasm/<artifact>` with the `inputs.json` its
  build recorded: a builtin (`extensions/<name>`, embedded in the host) or a test fixture (`fixtures/<name>`). Listed
  once, `specforge_component::committed::COMMITTED`; git tracks exactly that list. An SDK blob answers the host's
  protocol and its wire answers are pinned; a bridge blob (the runtime's own test component) is written without the
  SDK and declares the canned protocol it answers. `revendor` rebuilds every one (ADR 0070).
- **Layer**: one of the eight folders under `crates/` that a workspace crate sits in, bottom to top `leaf kernel
  pipeline runtime session app adapters surfaces` (`crates/<layer>/<crate>/`). A crate's normal and build dependencies
  stay in its layer or go down; the gate's `layers` check holds it (ADR 0084). What each layer owns is in
  `ARCHITECTURE.md`, and every crate's line (its `Cargo.toml` `description`) is in `crates/README.md`; both are
  rendered by `cargo gate crate-map` and checked by `crate-map` (ADR 0091). Not the parts of the LSP's published
  diagnostics.
- **Installed extension**: an extension placed in the project under
  `.specforge/extensions/<name>/extension.wasm` and pinned by its `specforge.lock` entry (version,
  source `registry` or `local:<path>`, the SHA-256 of the binary and, for a signed registry package, its
  publisher key id). It loads only while its binary is
  the one its entry pins and declares the name, version and peers the entry records (E070 otherwise; W149 when the entry
  pins no hash). A project's installed
  extensions are one value, its lock read once (`specforge_installed::Installed`); installing,
  updating and removing them is one **change** (`Installed::change`), written all at once or not at
  all, its last step the operation's project change (ADR 0128) and the directories it created (ADR 0028);
  `add`, `update`, `init` and `remove` change them through the **lock plan**.
- **Lock entry**: what `specforge.lock` records for one installed extension: name, version, source, the module's SHA-256, its
  publisher key id and peers. The version and peers are its module's, and the load holds the entry to them
  (`LockFileEntry::declares`, E070, ADR 0217).
- **Extension load**: turning a project's `extensions` entries into loaded extensions and their
  declarations, once per environment load (`Installed::load`, over the `WasmRuntime` port): a
  builtin from its embedded binary, an installed extension from its pinned module, a `.wasm` file
  entry from its file under the package name it declares (E072 when it declares none, ADR 0223). Which extension each entry names is decided once
  (`Installed::resolve`: a builtin's name before a lock entry, `.wasm` file entries after every named
  entry), and the lock plan reads the same decision: a named entry names an
  extension by its text (`ExtensionEntry::names`), a `.wasm` file entry only by what its component declares, once
  admitted (ADR 0160; never under a builtin's name, ADR 0214). Its declarations come in entry order; the registry
  build puts them in load order. What does not load is a typed `LoadFailure` on its
  entry with one diagnostic and leaves nothing loaded, and what an entry reports though it loaded (W149, W153, W138) is on the
  entry too; the runtime keeps none, and `Installed::load_diagnostics` orders them, entry by entry. Without a runtime nothing loads, and each entry
  enables what it names (`Installed::unloaded`); each entry records where the load takes it from, its **origin** (`Origin`: `builtin`, the lock entry's source, `file:<path>`, `unknown`; `Resolution::origin`, a repeated entry the one it repeats'), and whether it loaded, with or without a runtime; what an environment is assembled from says which
  (`specforge_project::ExtensionLoad`: `NoRuntime`, or what was loaded `In` a runtime).
- **Builtin's name**: the name a builtin this specforge embeds declares. Only that builtin loads under it: a `.wasm` file entry whose component declares it is refused by the load's admit (E028), and add, update, init, publish, extension validate and extension init refuse a candidate declaring it (`extension_conflict`). One predicate, `Builtins::contains` (ADR 0028 D7, ADR 0214).
- **Load key**: the key the runtime holds an entry's component under while its declaration is read: a builtin's or a locked entry's name, a `.wasm` file entry's `file:<entry>`, a candidate's `__candidate`. Every name the load admits is a package name and no temporary key is one, so a load never replaces another entry's extension (`load_key`, ADR 0223).
- **Extension inventory**: every extension a project has, one row per name: enabled by a
  `specforge.json` entry, pinned in the `specforge.lock` the compile read, or loaded. Each row has its
  origin (the origin the load recorded on the entry that loaded it, else on the first entry the load tried, `EnabledExtension::loader`; else its lock entry's source; else `builtin` for a builtin's name; else `unknown`), its status (loaded, not loaded, not
  configured), the entries that enable it, its declaration (the registry build's, under its name), its lock entry and why it did not load
  (`specforge_ops::extension::inventory`, over the project view). The extensions listing, doctor and
  remove read it, so they cannot disagree about which extensions a project has, where one comes from
  or whether it loaded (ADR 0028).
- **Candidate**: an extension a project does not load yet — a builtin `add` or `init` enables, a
  binary `add`, `update` or `init` installs, a binary `publish` uploads — and the extension
  declaration read from its binary once, as the load answers it (`Loaded`: the declaration and its
  warnings, which name the extension as it declares itself), through the `WasmRuntime` the operation's
  caller passes (the CLI's component runtime with the per-user compile cache, the MCP call's runtime, a
  test's in-process runtime), never through one the operation builds (`specforge_ops::extension::candidate`,
  ADR 0028 D7, ADR 0172). A binary to install or publish must load (E028) and must not declare a builtin's name (`extension_conflict`, one
  rule: `not_a_builtins_name`; `extension init` holds a new crate's name to it); the lock plan
  enables the builtins an extension requires (CONTEXT "Lock plan"), and a builtin that does not load
  refuses the operation.
- **Local component**: the component a path names, read once through the request's containment: a `.wasm`
  file, or the component an extension's crate directory builds (`target/wasm32-wasip2/release/<crate>.wasm`,
  the layout `specforge extension build` builds into), named as the caller spelled it. add, init, publish and
  extension validate read a path only through it, so one cause is one code and one message in every verb:
  E040 for nothing there, no built component, several, or a file that can't be read
  (`specforge_ops::extension::LocalComponent`, ADR 0172).
- **Candidate vocabulary**: what the extensions a host could enable declare, read only for suggestions: each kind
  keyword and each field an enhancement adds, with the extension that declares it
  (`specforge_registry::CandidateVocabulary`, ADR 0122). The session hands the builtins' to the registry build;
  `revendor` generates it from their pinned declarations
  (`crates/runtime/component/data/builtin-vocabulary.json`). It never decides a diagnostic, only names the extension
  to install.
- **Lock plan**: the change one operation makes to what the project installs and enables (`add` its
  installs and builtins, `update` every upgrade, `init` its local files and builtins, `remove` its
  disabled entries and uninstall), staged as the operation leaves them: each package's lock entry
  replaces its previous one, never sits beside it, each new `specforge.json` entry is appended, each
  dropped one removed; the change writes exactly those entries into the `specforge.json` it read, other keys
  kept, and refuses (`file_changed`) when the file no longer holds what the plan opened with (ADR 0126). Its writes land inside its own project: it is opened with the operation's **Caller** and narrows the caller's **Containment** to the project root itself (ADR 0137)
  (`specforge_ops::extension::lock_plan::LockPlan`, ADR 0041, ADR 0065). An
  operation names what it asks for, each an **ask** (its position among the operation's requests), and the plan answers
  it when it is asked: **changes**, or **unchanged** when the project already has it (a named entry names it; for an
  install, the lock pins it at the version asked, with the same bytes and kind of source, and its module on disk is
  the one pinned), which stages nothing. A finished plan answers the builtins it enabled for each ask; a refused one,
  the asks it refused (ADR 0168). A plan whose asks change nothing is neither judged
  nor written (ADR 0138). An
  `add` or `init` of an extension a `.wasm` file entry already loads is refused (`extension_conflict`), nothing
  written (ADR 0160).
  `add` and `init` first enable the builtins what they add requires, dependencies first;
  `update` enables nothing. The peer rule then judges the **project peer set** as the change leaves it,
  once: every declared peer whose dependent or peer the change touches (what it stages or appends; for a removal,
  each member the set loses or loads instead, ADR 0041). A required peer no member
  provides is E027; a peer installed from a registry is unified against the registry the install came
  from (R-RES-006/005, R-RES-002 when it publishes none, ADR 0001); anything else out of range (a local install, a builtin, a `.wasm`
  file entry) is E027. Lock order and entry order decide nothing. A registry install's binary loads
  only after that and must declare what was published; then the plan is written as one change,
  `specforge.json` included, whose last step pins the publisher keys its fetches accepted
  (`Registry::pin`): a refused or failed plan pins nothing (ADR 0059). A **dry run** is the plan
  checked and not committed: it answers what the operation would, its refusal included, and writes,
  pins and downloads nothing; a registry package is judged by its published manifest and its
  publisher decided (`Registry::published`). The lock the change leaves is built when the plan is checked, from the lock as read and what
  it staged; a change that stages nothing accepts any lock (ADR 0165). A change that installs or uninstalls refuses a
  `specforge.lock` that is there and can't be read once, before anything is read (E033, the lock's own remedy,
  `lock_plan::writable_lock`), and `update` and `remove` answer that refusal, never their own (ADR 0028).
- **Claim**: what one specifier of an `add` or `init` asks for an extension name: a second specifier naming it the
  same way is a duplicate, another way `extension_conflict` (`specforge_ops::extension::claims`, ADR 0098).
- **Extension names**: what one change's extension strings name, read once by `specforge_ops::extension::names`
  (ADR 0105). Each string is one extension specifier; no operation splits one, and `init --extensions` spells several
  to a flag on the command line. A relative local path is joined onto the change's base: add reads it from the project
  root, or from the CLI's working directory; init from the directory it creates. The file is read within the change's
  containment (ADR 0104), each specifier is claimed (ADR 0098), and a local file is read once. `add` and `init` both
  take raw strings. Once read, the module asks the change's opened lock plan for each, an ask per specifier by its
  position, checks the plan once and names its refusal by the specifier it is charged to (ADR 0134).
- **Project peer set**: the extensions a project's `specforge.json` entries load, as the peer rule reads
  them: a builtin by the declaration this specforge embeds, an installed extension by what its module declares
  (a staged install by its pin), a `.wasm` file entry by its file's declaration, each entry resolved by the extension load's one rule
  (`Installed::resolve`). An entry that loads nothing and a locked extension no entry enables are not in
  it (`specforge_ops::extension::peer_set::ProjectPeers`). The registry build judges it as loaded; the
  lock plan judges it as a change will leave it, before anything is written, so `add`, `init`,
  `update`, `remove`, their dry runs and `check` judge one set (ADR 0041). For the project as it is
  (ADR 0160's refusal), a locked extension is a member only when its module on disk is the one pinned and
  declares what its entry records, as the load answers it; for the set a change leaves, the same for a locked
  extension the change does not stage (ADR 0181, ADR 0217).
- **Load walk**: the one pass over a project's resolved `extensions` entries that the extension load and
  the lock plan's peer set share (`specforge_installed::Resolution::walk`, ADR 0181): load order, each
  entry's declared name admitted against the names the entries before it were admitted under, the
  answer in entry order. The load reads a declaration through the runtime and the disk; the peer set
  from the embedded builtin, the module's declaration (a staged install by its pin) or the file.
- **Setup part**: what part of a project's setup a diagnostic the environment reports is about: the
  config, the lock, one `extensions` entry, structural-only, the configured providers, one declaration,
  the peers, the registries, a rule, a surface, or a collision (two contributions claiming one name: a
  kind keyword, an edge label, a rule code, a command, tool or resource, a provider scheme). Decided
  where the diagnostic is raised (`specforge_project::SetupPart`, the registry build's `BuildPart`);
  doctor makes one finding per setup diagnostic from it and reads no code but E013's (ADR 0054).
- **Registry build**: the pure result of putting extension declarations in load order and turning
  them into kind, field and edge registries (each edge type resolved once: every field that writes it, its
  ends, and the edge type each field writes; ADR 0060), the rule set, pass order, the graph inputs
  (`RegistryBuild::graph_inputs`) and every graph read with it (`RegistryBuild::graph_of`, with the provider schemes it is handed) and the diagnostics of those declarations, in one order
  (`RegistryBuild::reported`: the declarations' own, the registries', the custom rules' probes, the
  surfaces', each with the setup part it is about) (`specforge_registry::build_registries`). The custom rules' probe is the one call it
  makes out, through the `CustomVerdicts` port, when the environment probes it in the runtime its
  extensions loaded in (`RegistryBuild::probe`). It also runs every check over a built graph's entity
  records, in one order behind one gate: the structural checks, then the rule set
  (`RegistryBuild::check`), reading the file system only through the disk in its input. Its
  outcomes are the `registry_build_*` behaviors and `check_entities_in_one_order`. Tests and every
  caller reach it only through `build_registries` and that build's methods; its steps, its checks and
  its fields are private (ADR 0031).
- **Graph inputs**: what the graph build reads of the loaded extensions (`specforge_graph::GraphInputs`):
  the kinds whose bodies an extension parses, the extension that owns each target kind no loaded extension declares (`target_owners`), the
  each field's form with the kinds that take any field (`FieldForms`, with each reference field's target, ADR 0117) and the derived reference fields. Only the registry
  build derives them (`RegistryBuild::graph_inputs`), which holds them and builds every graph a snapshot reads with them
  (`RegistryBuild::graph_of`, ADR 0123); with the provider schemes the environment registers they are every graph's
  `GraphConfig` (`Environment::graph_of`, ADR 0032).
- **Edge type**: a relationship an extension declares (`edges`), or that fields name without one declaring it
  (W021). The registry build resolves each once (`RegistryBuild::edges()`): its writers, every field that names it
  as its `edge` (a kind's own, shared and enhancement fields), its ends (the declared kind on a side it declares,
  else its writers'), and the edge type each field writes (`EdgeRegistry::written_by`). A field writing a declared
  edge type outside its declared ends is W021 and writes none. A graph edge's label is the field that wrote it; its
  edge type is read from the field, once, in the entity snapshot. Rules count by it, the schema prints its ends,
  trace names it (ADR 0060).
- **Owning field**: the one reference field that writes a relation's edges, declared by an extension (ADR 0150).
  A relation is written there and nowhere else; its other side is derived.
- **Other side** (of a relation): what the target entity reads the relation as: its incoming edges, which `query`
  and `trace` (upstream) show. Never a field; an `inverse_of` may name it. Writing it is E079. A typed edit naming it is refused `other_side`, naming the owning field (ADR 0157).
- **Entity enhancement**: fields and verify kinds one extension declares on an entity kind another extension declares
  (`c.enhance(kind, ..)`). Its fields are the declaring extension's: trace treats them as opt-in for the kind, and
  the model and schema name their extension. The kind stays its declarer's. An enhancement of a kind no loaded
  extension declares is skipped silently when its extension declares an optional peer the project does not load, and
  is I004 otherwise (ADR 0096).
- **Rule set**: the extensions' declared validation rules plus the host's E006 rules for required
  fields and E016 rules for file-reference fields, each turned once per registry build into a typed rule that carries only what its check
  reads, resolved against the registries (a compiled regex, an edge rule's edge type and the fields that
  write it). An edge rule counts the edges of its edge type; a cycle rule follows them and reports every member of a cycle (ADR 0196). It runs itself over the entity snapshot's records (`RuleInput`, ADR
  0019), cycles included, and answers which rule applies to a kind, which the snapshot's standing
  reads (`specforge_registry::rules::Rules`, ADR 0020). A declared rule that cannot work is W112; a
  property its check does not read is W147.
- **Named paths**: the paths an entity's field names: a scalar's text, or each item of a list, when not
  empty; a block, an expression group, a variant list or a type union names none. Every `file_exists` rule,
  E016 among them, reads them and suggests a close sibling of a missing one
  (`specforge_registry::rules::files`, ADR 0118).
- **Structural checks**: the host's own checks over the entity snapshot's records, run by the
  registry build before the rule set, in this order: W012 (a `ref` nothing references), then, unless the project is structural-only, E024, E013, E014, W020 and E061
  (`specforge_registry`'s `checks`, ADR 0031). E022 is the linker's (ADR 0117). None reads a graph node. Whether every reference
  became an edge is the linker's own debug assertion, not a check.
- **Structural-only**: no loaded extension declares an entity kind
  (`RegistryBuild::structural_only`). The checks that read kinds, fields and identifiers do not
  run; with no extension loaded I002 says so, with extensions loaded W151 names the entities left
  unchecked.
- **File default**: a field value a `defaults <kind> { … }` block states once for every block of that kind in
  its file (ADR 0149). A block's own field replaces it whole. The graph build resolves it, so the graph, every read
  view, export and check hold the resolved value; its spans stay the default's, so diagnostics, references and
  rename point at the default's line. `defaults` is a structural keyword: never a kind, never an id. A typed edit
  and the migrate defaults step ask the graph's defaults module (`specforge_graph::FileDefaults`) over the text it plans which default applies and
  whether a field restates it, through one module of its own (**Own field**), so it never writes what W161 would
  flag (ADR 0154, ADR 0175). `FileDefaults::held` answers what a block holds for a field: own, inherited or absent
  (ADR 0182). A value a defaults block writes in the other form is one W159, at the default (ADR 0224).
- **Body kind**: the kind a block's body is read as: its keyword, or for a `defaults <kind>` header the kind it
  names (`specforge_common::structural::body_kind`, ADR 0224). The formatter writes a body's values in that kind's
  forms; the editor completes, hovers and colours them as that kind; the graph's defaults module applies them to it.
- **Own field**: a field a block writes, as opposed to one it inherits from its file's default (ADR 0149). A typed
  edit changes a field through one module (`edit::defaults::change`, ADR 0175): an inherited list or block field is
  copied into an own field first, the verb changes it, an own field that then holds what the default holds, as stored, is removed, and a
  field left empty over a default holds none (`[]`, `{}`; a field of one value is refused). `unset` leaves a field
  holding none. A block's own field is its first entry with the key (ADR 0215).
- **Repeated field**: a key one block writes more than once, at any depth (E081, ADR 0215). The graph build keeps the first
  entry and reports each later one; every reader of a node reads that one entry; the finder still reads the later
  entries' names, so a rename edits them; a typed edit that changes the field removes the later entries.
- **Custom verdict**: an extension's answer, through its `wasm_function`, on one entity for a
  `check: "custom"` rule: pass, or fail naming a field and value. The rule set asks for it through
  the `CustomVerdicts` port; the project's adapter calls the extension (`ExtensionCalls::validate`),
  tests answer with a closure. A function that cannot answer is W112 at load, W148 at check.
- **Entity snapshot**: every entity of one built graph as every check after the build reads it, taken
  once per compile and per session check (`specforge_registry::EntitySnapshot`, ADR 0019), taken of a graph build made in its Registry build, and of no other (ADR 0115, ADR 0123).
  Each entity's record holds:
  - what it writes, as field text, with each value's shape (quoted, bare, a number, a list of
    strings or references, …) and span beside it for the host's own checks
    (`specforge_graph::ValueShape`);
  - its obligations and methods;
  - its edge counts, in total and by edge type (each edge typed once, by the edge type its field writes from the
    kind it starts at);
  - what exempts it, if anything (a union body, an exempting flag, a kind that accepts no `verify`).

  The registry checks and the rules read the records (`specforge_registry::entity::EntityRecord`).
  The pass input and a custom validator's context are adapters over it; the coverage rule reads the pass
  input's entities (`specforge_coverage::Entity::from`), on the host as in every extension pass.
  Never read a graph node to answer what an entity says or owes.
- **Field text**: the one string a field value is to a rule, a custom validator, a compiler pass
  and every read view that asks what a field says (search, the headline; ADR 0019, ADR 0228): scalars as written; lists of strings or references and mixed lists joined by `", "`;
  variant lists and type unions by `" | "`; expressions by `", "`; a
  block's keys by `", "`. An entity's `verify` statements are not a field and have no field text (ADR 0094). An empty value is `""`. Every written field has one, and none is null. It differs from the written value (`field_form::write`) only for an expression group, which every output writes one conjunct per line (ADR 0064, ADR 0115).
- **Field type**: how the host reads a field's value, one of nine names an extension declares:
  `string`, `integer`, `bool`, `enum` (its values are the declaration's `enum_values`, its absent value its `default_value`: a field
  vocabulary; an enum without values, or a default outside them, is W021), `identifier` (one word from an open set,
  written bare; ADR 0147), `string_list`,
  `reference`, `reference_list`, `block` (`specforge_protocol_types::FieldType`). The registry build
  reads a declared type once, into the field's registry entry (`FieldRegistryEntry::new`, the only way
  to make one); every output names it by that one name — the schema and exports, the published JSON
  Schema, the model, the hover and E061 (ADR 0034). The older spellings (`_type` suffixes, `boolean`)
  are read, never written; any other name is W019. What a field's type takes, how a written value is
  stored as it, the JSON every output writes for it and that JSON's schema are its field form
  (ADR 0064).
- **Value form**: how a field's value is written: bare for an `enum`, `identifier`, `reference` and each
  `reference_list` item, quoted for a `string` and each `string_list` item (ADR 0147). `specforge format` writes it;
  W159 reports a value written otherwise. Spelled by one module, `specforge_graph::field_form::spell` (the escape
  table, the bare rule and each declared type's one-line spelling); `FieldForms::rewrite` reads its bare rule, and
  `format`, W159, migrate's `titles` step, the typed-edit writer and init's starter spell through it (ADR 0153,
  ADR 0226); `spell::escaped` is the inside of a quote. A `defaults` body's values are written in the forms of its
  body kind (ADR 0224).
- **Stored value**: a field value as the graph build stores it: coerced by its field form (`FieldForms::stored`,
  ADR 0182), so a list written as one value is a one-item list and `"100"` on an integer field is `100`. Every reader
  of references, derived names and inherited values reads it; what a file *writes* (W159, W161, the formatter) reads
  the written value.
- **Field vocabulary**: the closed set of values an enum field takes, in their order, and the value an absent one
  counts as (its declared `default_value`), declared once by the extension that declares the field, as a `const`
  (`specforge_extension_sdk::Vocabulary`, through `FieldBuilder::vocabulary`, the only way to declare an enum).
  The host reads it from the field's declaration: a written value outside it is E061, the editor completes it, the
  schema and the published JSON Schema list it, the guide and the hover show it and its default; the host never
  writes the default into the graph. The extension's own readers (a list filter and its help, a sort, a lifecycle
  pass, a rollup, a conditional rule's value) read the same constant. An open set of standard values (a module's
  family) is not one: its field is a string, and an info rule names the standard values (`{allowed}`) (ADR 0071).
- **Field form**: a field's declared type with an enum's values, its target kind, read once
  (`specforge_graph::FieldForm`, from `FieldRegistryEntry::form`; `FieldForms::reading(kind, field)` is a
  `Declared` form, a `Member` of an open-fields kind or `Undeclared`, and `Reading::references` is the one rule
  for what a written value references, ADR 0093): which
  stored values the field takes (E061 reads it), how the graph build stores a
  written value (coercion), and the JSON Schema of the written value (the
  published schema reads it). Every output writes a field value through one
  writer (`field_form::write`, the `FieldJson` shape): text as a string, an
  expression group as its conjuncts one per line, lists as arrays, a block as
  an object; never a parse tree or a position. A value E061 admits is a value
  the published schema admits (ADR 0064).
- **Members**: what an entity holds besides its id, kind, title and obligations, as every output writes it
  (`specforge_graph::Members`, ADR 0222): its fields' values (`FieldJson`), the annotations written after them, by the
  field's name (an entry of a block field's by `outer.key`), and its methods (`MethodJson`: name, parameters with their
  type, `?` and annotations, return type; `MethodJson::signature` is the line `specforge format` writes). No position.
  The graph, context and markdown exports, inspect, the context prompt, the hover and the outline read it; the graph
  delta diffs it.
- **Reference target**: the kind a reference field's ids must name, from its declaration
  (`FieldForm::target_kind`), and whether a loaded extension declares it (`FieldForms::target` gives a
  `specforge_graph::Target`: `Loaded` or `Absent`; an absent kind's owner is `GraphInputs::target_owners`). The linker
  decides each written reference against it once: an edge, E003, I004 (absent kind, missing id) or E022 (an entity of
  another kind), each at the reference (ADR 0117).
- **Standing**: how the obligation rule sees an entity (`specforge_registry::Standing`):
  - whether its kind is testable;
  - which `no_verify_statements` rule requires its kind to declare obligations (a rule without a
    target kind applies to every kind);
  - what exempts it (a union body, an exempting flag, a kind that accepts no `verify` statements);
  - how many obligations it declares.

  It **declares** obligations when it has at least one (`Standing::declares_obligations`). It **owes** obligations when a rule applies and nothing exempts it. It **counts** toward coverage
  when testable and owing or declaring, and is **exempt** when testable and neither (its **Coverage status** says so): the coverage
  rule's one counting rule (`specforge_coverage::Counting`), read with the standing's facts
  (`Standing::counting`). W004, stats, plan validation and the verify-stub fix read it. The pass
  input's `owes_none` is not this word: it says the entity owes none, and such an entity still counts
  when it declares obligations.
- **Opening a registry**: what an operation does once before it asks a registry anything it will ask: the adapter
  reads the project's `registries` and the user's credential store, and refuses there with E063 (none configured),
  E067 (an entry that can't be read) or R012 (a store that can't be read), before any request (`Registry::open`,
  ADR 0044 as amended). The refusal is the operation's, given once; a per-package ask then fails only for its
  package. `update` opens when it selects a registry install, `search` always; an add that may ask nothing (an
  installed exact version) opens nothing and refuses at its first ask.
- **Package registry client**: what talks to a package registry: search, versions, metadata, download and
  publish over HTTP, credentials, publisher trust and package signing
  (`specforge-registry-client`). Not the Registry build, which is pure and needs none of it.
  Operations reach it only through the `Registry` port, which lists a package's versions, fetches
  one, pins the publisher keys a committed install accepted, and publishes a declared package
  (ADR 0036, 0045, 0059, 0179); its adapter,
  `specforge_ops_registry::ConfiguredRegistry`, opens the project's registries once (**Opening a registry**), asks the one that
  serves a name and runs the fetch policy over the `RegistryClient` seam (ADR 0044). It is linked by
  the CLI and MCP, never the LSP (ADR 0010), and holds what needs the user's `~/.specforge`: the
  credential, the signing key, the known keys. Publish derives the stored declaration from the
  binary; `add` checks the binary declares what was published (ADR 0012). Each seam has a second
  adapter for tests, held with the first to one contract: `MemoryRegistry` beside the port, `MemoryClient`
  beside the client (`assert_registry_contract`, `assert_client_contract`). A rate-limited call is sent again
  by the spec's backoff (`Retrying`), and every request carries the credential the user keeps for that
  registry (ADR 0044 as amended).
- **Registry credential**: the token a request to one registry carries, kept per registry root in
  `~/.specforge/credentials.json`, never in `specforge.json`: a secret `specforge login --token` stored (a 0600 file under
  `~/.specforge/secrets/`), or a reference to an environment variable (`--token-env`) or a file
  (`--token-file`), resolved before any request (`CredentialStore::credential`; R010, R011, and R-AUTH-021 for a lost secret). The client sends it only to the registry root it was kept for (ADR 0178). A `credentials.json` that cannot be read is R012 for every reader: login and logout refuse and leave it, doctor counts it unusable, update and search refuse it once, when they open their registry (ADR 0045 as amended). Every read carries
  it; a publish takes `SPECFORGE_REGISTRY_TOKEN` first (ADR 0045 as amended).
- **User's registry files**: the directory `specforge_ops_registry::User` reads and writes: `credentials.json` (keyed by
  registry root), `secrets/`, `signing-key.json` and `known-keys.json`, each written by **private replace**. `User::current()` is `~/.specforge`,
  `User::at(dir)` any other; nothing else reads them, and login, logout, doctor, fetch, search and publish ask it (ADR 0178).
- **Fetch policy**: what a package passes before an operation sees it: the registry's reply reads as the protocol's JSON (R009 else;
  a malformed signature is that) and names the package and version asked for, the served manifest reads as that
  package's declaration (one from before ADR 0012 is refused), at the version the project's lock pins the bytes it pins
  (**locked pin**), the publisher signature verifies over what the reply describes and the publisher is decided (the
  **trust decision**), and only then is the binary downloaded and held to the served SHA-256, so the signature covers it
  (`ConfiguredRegistry::published` and `fetch`; ADR 0010, 0012, 0044, 0059, 0213). The fetch decides and
  writes nothing: the decision is the package's `Publisher`, and the key is pinned when the lock plan that
  fetched it commits. Accepting an unsigned package (`--allow-unsigned`, the CLI only) is reported as W155 at
  fetch, a re-pin as W156 once it is saved. A known-keys store that can't be read refuses the fetch, signed or not
  (R-TRUST-006). The
  check writes nothing to the terminal: a key change is put to the policy's **Asking**.
- **Trust decision**: how a registry package's publisher is accepted, decided once from what the registry describes, before any
  download, for a dry run as for an install: the **expected key** first, for every package; unsigned with an expected key
  (R-TRUST-007), unsigned with none (R-TRUST-001 unless allowed), a denied key (R-TRUST-005, whatever the consent), the
  expected key matched or first use, a key change (accepted for
  a `trusted_keys` key, else put to **Asking**, R-TRUST-003). `specforge_ops::registry::TrustPolicy::decide`; the adapter
  verifies the signature (R-TRUST-002) and reads the user's store whatever the signer (R-TRUST-006) (ADR 0213, 0221).
- **Key pin**: the publisher key id a package name is held to, kept per user in `~/.specforge/known-keys.json`
  (`pins`, beside the `trusted_keys` allowlist and the `denied_keys` deny list) and per project as the lock
  entry's `key_id`. The user's pin wins; the lock's stands in when the user has none. Pins are written only by
  a committed lock plan, through `Registry::pin`, after the binaries and the lock
  (`specforge_ops::registry::KeyPin`, ADR 0059).
- **Expected key**: the publisher key a package name is held to when it is fetched: the user's **key pin**, else the
  **locked pin**'s key, else none (first use). A package signed by another key is a key change (R-TRUST-003); one
  arriving unsigned is refused (R-TRUST-007), whatever `--allow-unsigned` or consent (ADR 0221).
- **Locked pin**: what a project's `specforge.lock` entry records for a registry install, which every fetch of the name
  is held to: its publisher key at any version (the **key pin** when the user has none) and its SHA-256 at its own
  version. A registry serving that version as other bytes is refused (R014) before the download, any key question or any
  pin, and a dry run refuses it from the published manifest; an entry that pins no hash (W149) holds no bytes and is
  pinned by its next add (`specforge_ops::registry::LockedPin`, ADR 0201).
- **Package signature**: a publisher's Ed25519 signature over a package's signed content (its name and version as
  declared, and the SHA-256 of the exact binary and manifest uploaded), with the public key and the time that verify it
  offline (`specforge_registry_wire::PackageSignature`, JSON `{sig, public_key, signed_at}`). Its key id is the first
  16 hex of the public key's SHA-256. Only a well-formed one exists. `specforge publish` signs with the user's key; the
  registry refuses an upload whose signature isn't one or doesn't verify over it (INVALID_SIGNATURE), before storing
  anything, so the version stays publishable; `add` verifies what it downloads again, because the registry is not the
  trust anchor: which keys to trust is the user's pins (ADR 0061).
- **Package registry contract**: what a package registry and its client exchange over HTTP: each call's address under a registry
  root, the JSON bodies (the package signature among them), the publish form and the error codes, each defined once in
  `specforge-registry-wire`, which `specforge-registry-server` and `specforge-registry-client` both compile against
  (ADR 0044, ADR 0061). A registry's refusal (HTTP 400) carries a typed code and reaches the user as R008; an
  answer that isn't the call's wire type is R009. So is a 404 without the protocol's NOT_FOUND error: an address
  no registry serves, not a missing package. A test reaches a registry through the real server in process or
  an in-memory client, never through JSON written by hand. A search matches by one rule, an ASCII
  case-insensitive substring of a package's name, description or one keyword
  (`specforge_package::search::matches`), on the server and in both in-memory registries.
- **Registry account**: who a reference registry issues tokens to, named by its operator
  (`specforge-registry token create --account`). It outlives every token: rotating or letting a token expire changes
  nothing it owns. A package's `publisher` is its account's name. Not a publisher key: the registry binds nothing to a
  key (ADR 0061, ADR 0185).
- **Scope owner**: the registry account a scope (`@acme`) belongs to, claimed by the first publish that stores a
  package in it. Only tokens issued to it publish or yank in it (`specforge_registry_server::ownership::may_write`,
  after the token's own filter, a package scope), SCOPE_OWNED otherwise; the operator hands it to another account with
  `specforge-registry scope assign` (ADR 0185).
- **Registry root**: the URL a `registries` entry names: where a package registry is, an `http` or `https` URL
  without the protocol's version prefix (`https://registry.example`, or `https://example.com/registry` for one served
  under a path) (`specforge_registry_wire::RegistryRoot`). Every call goes to the root, then `/v1`, then the call's
  path (`RegistryRoot::address`); the server mounts the same routes (`Endpoint::route`). A url ending in `/v1`, or
  one that isn't an http(s) URL, is E067 (ADR 0075).
- **Registry for a package**: the one configured registry that serves a package name: the first
  `registries` entry whose `scope_filter` covers the name, else the first marked
  `default_registry`; with neither, none does, and the operation refuses with R-OPS-001 before any
  request (`specforge_ops_registry::Configured::registry_for`, ADR 0045); a `registries` entry that can't be
  read refuses every registry operation with E067 first, so a scope it claims never falls to the default. `add`, `update` and
  `publish` ask that one registry; `search` asks every entry. A registry credential is kept under
  the registry's root (`login --registry <alias>`, else the default registry's; `logout --url <root>` needs no project).
- **Package name**: what an extension package is called, `@scope/name` (a registry holds only
  these) or `name` alone (a local module); each part `a-z 0-9 . _ -`, starting with a letter or
  digit, so it is always a relative path inside the directory it is joined to and one URL segment
  (`specforge_package::PackageName`, ADR 0036, 0050). A declaration's `name` is checked as one where
  `add` installs, `publish` uploads and `extension validate` reads it, and where the extension load admits a `.wasm` file entry; a `specforge.json` entry and a lock entry are read as one.
- **Package scope**: the namespace a scoped package name starts with, `@` and one name part (`@acme`)
  (`specforge_package::Scope`, ADR 0200). A `registries` entry's `scope_filter`, a registry token's filter and
  `scope assign` name one; text that is not one is refused where it is written, naming the scope to write. A scope
  covers exactly the names that start with it.
- **Declared package**: an extension declaration read as a registry package: its name a scoped package name, its
  version a full SemVer version, parsed once and carried with the declaration
  (`specforge_package::DeclaredPackage`, E072 when it is none, ADR 0179). `publish` builds one and it is what the
  registry port, the registry client and the signed upload are addressed by; the registry checks the upload again.
- **Version requirement**: which version of a package is asked for: `latest` (the highest release,
  a pre-release only when there is none), one full version, or a SemVer requirement read as Cargo
  reads one (`VersionRequirement`); `pick` is the one rule that chooses among published versions:
  ops runs it for `add`, and every registry's search lists a package at what it picks for `latest`
  (`pick_from`). The `Registry` port lists versions and fetches one; it does not resolve. A registry's
  list of a package's versions is read once (`extension::resolve::published`): an empty list is
  R-RES-002, for `add`'s pick, `update`'s upgrade target and the lock plan's version diamond.
- **Upgrade target**: what `update` moves a locked registry extension to: the highest published version
  newer than the locked one that the update's reach admits, `^locked` (`Reach::Compatible`) or any
  (`Reach::Major`, `--major`), a pre-release only on the locked pre-release's own `MAJOR.MINOR.PATCH`. None
  means up to date, never a lower version; a lock version that is not SemVer moves to what `latest` picks
  (`specforge_package::upgrade_target`, ADR 0036 as amended).
- **Peer requirement**: what an extension declares it needs from another extension: the peer's name,
  a SemVer range read as Cargo reads one, and whether the peer may be absent (an extension declares
  one with the SDK's `peer` or `optional_peer`)
  (`specforge_package::peers::PeerRequirement`, ADR 0050). One rule judges it (`peers::verdict`): an
  unreadable range satisfies nothing (E073); a required peer must be installed; an installed one must
  be at a version the range accepts (E027 otherwise). The registry build, `doctor` and the lock plan
  read that rule and nothing else, over one set, the project peer set (ADR 0041).
- **Load order**: the order a project's extensions load in, which every first-wins rule and in-order
  list follows: entry order, except that an extension comes after the peers it declares (required
  ones always, optional ones unless that would close a cycle). The registry build produces it
  (`Peers::load_order`); a cycle among required peers is E027 and its extensions load together in
  entry order (ADR 0041).
- **Extension specifier**: an `add` argument (also each of `specforge.add_extension`'s
  `specifiers` and `init --extensions`'): a builtin's name (with an optional `@requirement` the embedded builtin must
  meet), a local path (anything that is no
  builtin's name, `git+` URL or package reference: a `.wasm` file, or an extension's crate directory,
  read as a local component, `LocalComponent`), a `git+` URL, or a
  package reference `@scope/name[@requirement]` (`PackageRef`; it starts with `@`), read once by
  `specforge_ops::extension::names` (each string one specifier). An add takes one or more and adds them as one change, judged on
  the whole set (one lock plan, ADR 0041); a specifier naming an extension an earlier one of the same
  add names is a *duplicate* of it.
- **Project view**: the compiled project as one surface sees it, borrowed: the graph, the
  environment it was compiled in (config, what each `extensions` entry enabled, the registry build:
  kinds, fields, edges, rules, the extension declarations and their ordered passes), the root it was
  compiled from, and what the surface reports for it; the runtime its environment's extensions were
  loaded in (`ProjectView::runtime`), the one every operation that calls an extension over the view
  calls it in (analyze's passes, collect's collectors, inference gaps' scanners, an extension
  command), so none takes a runtime of its own; its entity snapshot, through the per-compile
  memo (`ProjectView::entities`) (`specforge_ops::view::ProjectView`). It reads
  the project's recorded test report through its compiled project, over MCP held inside the root
  through the containment it carries (`ProjectView::bounded_by`, ADR 0183): the recorded report, the schema cache,
  a typed edit's format configuration, and its versioned schema, both at that root and never an
  ancestor's. Built only from a compiled project (`ProjectView::of`): the CLI from its compiled project and the LSP
  from its session's, or, while the session is out for an update, from a copy of it (ADR 0108); MCP from its call target (`ProjectRef::view`: the served session's compiled
  project with its I017 notices, or that of a session opened for one call) (ADR 0015, ADR 0047).
- **Assembled project**: a compiled project a test builds in memory from a graph and an environment rooted at its
  directory, reporting what the test lists; no check runs (`specforge_project::testing::Assembled`, ADR 0108). Its root is a
  directory, not a project root: its environment was loaded from none (ADR 0121).
- **Read view**: an operation that only reads the project view: stats, trace, the coverage view, the
  model and outline diagrams, the versioned schema, inspect, query, list and search (the entities a
  selection over the view returns, `specforge_ops::read::query`), the exploration and the review
  (`specforge_ops::read::{explore, review}`), the entities a trace's chains reach (`specforge_ops::read::trace::affected`, the trace prompt's
  affected and unverified entities), the plan check (`specforge_ops::read::plan::check`), the anchors of a source file and of an entity
  (`specforge_ops::read::navigate::{anchors_of_file, anchors_of_entity}`, ADR 0140), and the inference guide and plan
  (`specforge_ops::read::infer::{guide, kind_guide, inference_plan}`). Each returns a typed outcome; the CLI,
  MCP (its tools and its prompts) and the LSP only render it. Its rules are tested in ops and stated by an ops-level
  behaviour; a surface's tests hold what the surface decides (ADR 0015).
- **Noticed read**: a read view that reports what it noticed about the run beside its value: the schema notices and I020. It
  returns `specforge_ops::Noticed<T>`, whose value is reached only with its notices, and only the surfaces' envelopes
  open it: MCP's `Answer::noticed` and `ResourceText::noticed` (into `_meta.diagnostics`) and the CLI's
  `Notes::noticed` (on stderr, before the document) (ADR 0130). export (its I020 included, ADR 0177), query, list,
  search, the exploration, trace, the schema and its publication, and the model are noticed reads; the others return
  their value.
- **Entity facts**: what inspect returns for one entity: its node and kind entry, headline
  statement, standing and obligations (the snapshot's own, borrowed), references in both directions,
  and the reported diagnostics about it (`specforge_ops::read::inspect::EntityFacts`); its coverage is asked
  for apart (`specforge_ops::read::inspect::coverage`) by the surfaces that show it. MCP `specforge.inspect`
  renders both as JSON and the LSP hover as markdown, so the two cannot disagree.
- **Headline**: the field an entity's extension declares headline and normative, and its field text
  (`specforge_emitter::context::Headline`), whatever the value's written shape: the context export lifts
  it, MCP inspect reports it as `contract`, the context prompt as `contract_text`, the hover quotes it as
  its summary (ADR 0228).
- **Known kind**: a kind a loaded extension declares, or that an entity is written with (an
  undeclared one is E024's). A kind filter (over entities, or the model's `kinds`) knows both and
  reports any other kind as I020; an argument that needs a kind's declaration (a schema entry, an
  inference guide, the model's root) knows only the declared ones and refuses others with
  `unknown_kind`. Names are exact; both name the closest kind, a kind equal but for case first
  (`specforge_ops::view::KnownKinds`, `ProjectView::kinds`).
- **Kinds list**: a kind filter written as text (`--kinds a,b` on the CLI, `?kinds=a,b` on a graph resource): the
  items between its commas, each trimmed, in order; an empty item is the empty kind (I020). Read by one rule,
  `specforge_ops::view::kinds_of_text` (ADR 0220). An MCP tool's `kinds` array is a list already, not text.
- **Unconnected entity**: an entity no edge links to another entity, in either direction
  (`ProjectView::connectivity`, `Graph::degree`, `Degree::is_unconnected`). A reference that does not resolve is no edge
  (E003 or I004 reports it), and an edge from an entity to itself links it to nothing else. Stats counts
  them, the exploration lists them and the review flags those that count toward coverage; the export's token
  budget ranks by the same degree. Not the same as unreferenced.
- **Reach** (of an entity): the entities a breadth-first walk from it reaches, nearest first: over edges both ways in
  (id, label) order (`Graph::reach`: the neighbourhood the exploration and the review read, a read selection's, `Graph::select`), or
  one way in written order (`Graph::reach_directed`: a trace's upstream and downstream, [ADR 0174](docs/adr/0174-the-graph-owns-directed-reach-and-degree.md)).
  Each reads only the edges of the entities it reaches. Not the entities a trace's chains reach (`trace::affected`).
- **Unreferenced entity**: an entity no edge points at: W012 for a `ref`, an extension's
  `no_incoming_edges` rule for its kinds, the coverage summary's `invariant_unreferenced`. No output calls one
  an orphan: the catalog titles, product `health`'s `unreferenced_counts` and the coverage summary's
  `invariant_unreferenced` all say unreferenced.
- **Exploration**: where to start reading a project's graph (`specforge_ops::read::explore`): a selection (the
  entities one entity reaches within a depth, or every entity; of one kind when given), the paths from that
  entity, the starting points (the selected connected entities that reference more than they are referenced),
  the most connected and the unconnected ones. The explore prompt and `specforge explore` render it.
- **Review**: the coverage gaps of a neighbourhood (`specforge_ops::read::review`): the coverage view's rows of
  the entities that count toward coverage within some hops of one entity, or of the whole project, each with
  a finding when it declares no obligation or is unconnected. The review prompt and `specforge review`
  render it.
- **Inference guide**: what an agent is told to look for to infer a kind's entities from code
  (`specforge_ops::read::infer::KindGuide`): the kind's description and extension, every field the registry build
  registered on it (with, for an enum, its values and its default), the extension's `inference_guide` followed by the project's own guide for the kind
  (`inference.<kind>` in `specforge.json`, under **Project-specific:**), an example entity writing each field
  the way its type is written, and the kind's entities. A project's guide is every declared kind's, its
  global conventions (`inference.global`) and its spec directory. The infer prompt, `specforge infer-guide`
  and the LSP's keyword completion render it.
- **Inference plan**: the infer prompt's plan (`specforge_ops::read::infer::inference_plan`): the inference
  progress, the unanalyzed and stale files paged from a cursor, the target spec directory (the spec root by
  default) and the kind priorities: kinds with no entity first, each after the kinds its reference fields
  target.
- **Configured providers**: the `providers` `specforge.json` lists (scheme, alias, extension,
  settings), registered once per environment against the loaded declarations, each with its
  status (registered, extension not loaded, not a provider (its handshake does not say `providers`), scheme taken) and the W118/E057 the
  registration reported (`specforge_project::providers::Providers`). The compile's I005 and the
  providers listing read this one registration.
- **Management operation**: an operation about a project's setup and tooling rather than its
  graph: the extensions and providers listings, doctor, remove, collect, inference progress and
  gaps, and the inference session steps. Like a read view it takes the project view and a request
  and returns a typed outcome; unlike one it also reads what the view does not own (installed
  binaries, source files, the inference manifest), and remove, collect and the session steps write,
  at the view's root only. `add`, `update`, `init` and `migrate` are operations but not over a view:
  they run before or instead of a compile (ADR 0015), in the runtime their surface passes (ADR 0028
  D7); `add` and `update` read `specforge.json`
  through the compile's own reader and the installed extensions through `Installed::at`, and refuse
  an unusable `specforge.json` with the refusal `remove` gives. `format` and
  `migrate` read it through the same reader (`specforge_ops::config::rewritable`), and `rename` from the
  compile's problems. All three refuse with that refusal a file that does not say which files are the
  project's sources or which extensions it loads (`ConfigProblem::blocks_rewrites`), and report every
  other problem as its E069, which fails their verdict.
- **Stranded entity**: an entity whose kind only the extension being removed declares: `remove` lists it
  (`specforge_ops::extension::StrandedEntity`), still removes, and the next compile reports it E024. They are the
  same entities `specforge extensions` counts as the extension's `entity_count` (`extension::entities_of`).
- **Recorded test report**: `<root>/specforge-report.json`, what `specforge collect` last wrote: per entity id,
  its recorded tests, each the protocol's `TestResult` (name, status, the obligation it names, duration) with the
  collector that recorded it. A status is `passed`, `failed` or `skipped`, the one vocabulary of the collect wire,
  the report and a pass's test results (`specforge_protocol_types::TestStatus`); any other is no collector's
  answer (E028) and no usable report (E045). The project view reads it once per compile and per content
  (`specforge_project::coverage::RecordedCoverage`); the coverage rule reads it as the pass input's `TestResults`,
  on the host as in every extension pass.
- **Binary report**: what the `#[specforge_test]` attribute's exit handler writes for one test target:
  `<package>--<target>.json` in `$SPECFORGE_REPORT` when `specforge collect` sets it, else in `specforge/` in the
  cargo target directory the binary was built into, never the working directory
  (`schema/specforge-binary-report.schema.json`). It lists the tests that ran, each with its entity, obligation text,
  module path, duration and status, and `@specforge/cargo-test` reads it during collect. It scores nothing: which
  obligations are proven is the coverage rule's, over the recorded test report (ADR 0068).
- **Stray test record**: a record of the recorded test report naming an entity the graph does not have
  (W097): `analyze` lists it apart from the passes with the closest id, never promotes it under strict,
  and it proves nothing (`specforge_ops::analysis::analyze::StrayRecord`).
- **Inference manifest**: `<root>/specforge-infer.json`, what inference has recorded: the source
  roots, each analyzed source file (root-relative path, content hash, the entities produced) and
  the inference sessions (`specforge_ops::read::infer::InferenceManifest`). One reader and one writer in
  `specforge_ops::read::infer`: a file that cannot be used is E071 for every reader, never an empty
  manifest, and a write keeps the keys it does not define. Not a project input: the compile and the
  session never read it.
- **Inference session**: one agent's run of inference, recorded in the manifest: started, then
  ended as completed or paused; at most one is active (`SessionStatus`). Its steps (start, mark a
  source file analyzed, end) are one management operation, `specforge_ops::read::infer::session`; MCP's
  `specforge.infer_session` is its adapter.
- **Inference sources**: the files inference covers: under the inference manifest's source roots (the project
  root when none), each file an enabled analyzer claims by its extension, keyed by the source path rule
  (root-relative, `/`-separated, no `..`). They are walked by the shared discovery policy
  (`specforge_common::walk_files`): no symlinked directory, no hidden name, no nested project, and the default
  skipped directories united with any analyzer's `excluded_dirs`. A source root, a file or an indexed path that lands
  outside the project root is none of them and is never read. Progress, gaps, the `inferred` lint and the mark read
  them through one module (`specforge_ops::read::infer::sources`, ADR 0113).
- **Anchors manifest**: `<root>/specforge-anchors.json`, which source item each entity is anchored
  to. Read only by navigation's anchors read view (`specforge_ops::read::navigate::{anchors_of_file,
  anchors_of_entity}`, and the covered source items inference gaps leave out): the entities anchored to a source file, each with its kind, and the anchors of an entity;
  E071 when it cannot be used, `no_project` without a project, E003 for an id neither the graph nor the manifest
  knows (ADR 0140).
- **Covered source item**: a public item a scanner found that the anchors manifest anchors, by its file and its
  name, to an entity the graph declares; or, when no anchor names that entity, whose name is the entity's id
  (`specforge_ops::read::navigate` `CoveredItems`, ADR 0199). Inference gaps are the items that are not covered. An
  anchor to an entity the graph does not declare covers nothing.
- **Obligation**: one `verify` statement on an entity, its own member beside its methods (ADR 0094), never a field. **Proven** when a passing test names its
  exact text, or a formal claim discharges it. Who owes obligations is the entity's standing. Where each is written is the parse's
  (`Entity::obligation_spans`), never the graph's; an entry named `verify` inside a block field is a field, not an
  obligation (ADR 0187, amended).
- **Unverified**: an entity that counts toward coverage and is not proven
  (its standing counts and its verdict is not proven; `ProjectCoverage::is_unverified`). Stats reports no "verified" count; its proof figure is `proof_pct`.
- **Delivery evidence**: what the recorded test report proves of a feature, beside the `status`
  its author declares (a claim): the behaviors that implement it (name it in `features`), how many
  the coverage rule counts proven, their obligations and failing tests. A feature is **proven** when
  at least one behavior implements it and every one is proven. The host passes each entity's score
  to an extension command (`CommandInput.evidence`: none, unreadable, or recorded;
  computed by `specforge_ops::extension::command::run` from the project view's recorded report); `@specforge/product` aggregates it per feature
  (`milestone-completion`'s `proven_count`) and its `delivery_evidence` pass reports a done feature
  that is not proven (I071). Status stays the input of every status query (ADR 0039).
- **Lifecycle consistency**: what the product kinds' statuses claim across entities, checked by
  `@specforge/product`'s `lifecycle` pass with every compile: a completed milestone with a feature
  neither done nor deprecated (W154), a done feature depending on an unfinished one (I063), a
  milestone due before its dependency (I064), a shipped deliverable tracking an incomplete milestone
  (I065). Its declarative rules check one entity at a time (ADR 0039).
- **Missing link**: an expected edge, from the registries, that a traced entity lacks
  (`MissingLink`). The only gap a trace reports.
- **Agent plan**: what an agent intends to do, entity by entity, in the order it will do it
  (`specforge_ops::read::plan::AgentPlan`, spec type `AgentPlan`: a `plan_id` and entries of `entity_id` and `action`).
  Its entry order is its dependency order; the graph's edges are the dependencies, and entities on one cycle come
  in any order among themselves. Read from a JSON object or its
  text, checked against the type's schema first (`AgentPlan::read`): a value that departs is refused naming every
  place. The **plan check** (`specforge_ops::read::plan::check`) reads the graph and the entity snapshot, never the
  recorded test report; it passes (`ok`) when no entry is unresolved and none comes before an entry of another cycle or entity it depends on; its outcome lists the entries that resolve (`resolved_entries`).
- **Plan gap**: how an agent plan falls short of the graph: an unresolved entry, a missing entry for
  an entity that counts toward coverage, or an entry ordered before what it depends on (entities on one cycle never are) (`PlanGap`). An
  edge to an entity that does not exist is neither: it is E003. Written as an `McpTraceGap`
  (`source_entity`, `target_entity`, `missing_link_type`, `gap_context`). A missing entry is a warning; an
  unresolved or misordered entry fails the plan.
- **Verdict**: an entity's obligations, the proven ones, and the tests that bear on them. It gives
  "proven" (the gate) and, for an entity that counts, the covered/partial/uncovered of its **Coverage status**
  (`specforge_coverage::Verdict`).
- **Coverage status**: an entity's coverage at a glance, one of `covered`, `partial`, `uncovered` (it counts; how its
  verdict proves it), `exempt` (testable, owing and declaring none) or `not_testable` (its kind is not testable): the
  coverage rule's `Counting::status`, asked through `Standing::status`. The coverage view, inspect, the hover and a
  query with coverage write it (ADR 0216).
- **Operation**: one user-level command (init, add, remove, …) as a typed request and outcome,
  independent of surface. What its request's values mean (a default, a value standing for another, arguments that
  exclude or need each other) is the operation's, read once in ops; a surface only spells the arguments (ADR 0101); an argument that names an entity, the request's subject or a filter, is refused by the operation when the graph lacks it, and a depth needs the entity it counts from; an argument a surface owns that picks which reader answers (the infer prompt's `scope`) refuses a value it does not know, never reading it as a default.
  An operation that writes names the files it changed on disk in its
  outcome (`specforge_ops::Writes`), recorded where it wrote. It fails with an `OpError` whose kind
  (`OpErrorKind`: invalid input, not found, conflict, schema mismatch, compilation failed, …) is
  stated where it is raised — at the operation's own raise site (`OpError::coded`), or where a lower
  crate's diagnostic crosses into ops (`OpError::of`), a recorded test report that cannot be used included
  (`specforge_ops::report`) — never by a table of codes and never by a surface; a refusal that rejects a value
  the caller gave also says which field of the request it is about (its **subject**: a `specforge_ops::Refusal<F>`,
  `F` the operation's field enum), stated where the operation reads the field and never by a table of codes; its
  code is what the CLI prints. Its outcome prints as one outcome document on
  every surface (ADR 0052). The CLI and MCP
  are adapters over it (`specforge-ops`); the CLI ends every core command through one renderer
  (`specforge_cli::outcome`: the refusal, the exit code) and writes it through its **Terminal**
  (ADR 0029).
- **Refusal subject**: the field of an operation's request a refusal is about, typed by the operation
  (`Refusal<F>`, `F` one variant per field a refusal of it can be about: `AddField::Extensions`,
  `InitField::Dir`). Only a refusal that rejects a value the caller gave has one (invalid input, an entity, file or
  extension not there, a conflict); a refusal of the project or the environment has none. Each surface spells it:
  MCP names the argument (`specifiers`, `path`), the CLI and the LSP print the error (ADR 0072). It is stated where the
  operation fails, with the refusal: no operation keeps an error enum of its own to convert into one (ADR 0132).
- **Entity argument**: an argument of a request that names one entity: the subject of a read (inspect's, trace's,
  coverage's `entity_id`) or a filter (search's `references`, export's `scope`). One the graph lacks is refused by the
  operation with `view::not_found` (E003, did-you-mean) about that field, never answered as an empty result (ADR 0015
  Q5, ADR 0101); the read selection checks it.
- **Read selection**: what a read view is about, checked once by the project view (`ProjectView::select`, ADR 0177): the
  entity a request names, with its record and standing, or `view::not_found` about the field that named it; the hops
  from it (a depth without it is `depth_without_entity` about that field); the kinds it keeps, with I020 for each the
  project lacks (an empty kind among them). A read of one entity asks `ProjectView::entity`; a filter by kind alone,
  `ProjectView::kind_filter`. It also answers the entities the read covers (`Selection::selected`, `Graph::select`, ADR 0190):
  what the entity reaches within the depth, or every entity, of the kinds kept. The entity is the selection's **root**:
  selected itself only when its kind is kept, and always drawn by an export or a query, whose token budget never cuts it.
- **Remedy**: what a refusal, a finding or a diagnostic tells the caller to do (an OpError's message tail or
  suggestion, a doctor finding's remediation). Written below the surfaces, it names a whole command or the request's
  own words, never a bare flag, so it reads true on every surface; the catalog's explanations, which name each
  surface's spelling, are not remedies; a command description an extension declares is read by both surfaces and
  follows the same rule (ADR 0135).
- **Mutation outcome**: what one MCP mutation call produced, typed: a run (`specforge_mcp::mutation::Mutated`:
  its reply and what it wrote — the files its operation changed, the entities it changed and the domain
  event it produces, `Written`) or a failure (`mutation::Failed`: its McpError and the files it left
  written; a writing operation's failed run arrives as its `OpError`, ADR 0170). Whether the call only previews is read from its tool arguments before it runs
  (`MutationArgs::previews`); a preview is no mutation, whatever its operation answered. The request
  pipeline's tools adapter alone (`mutation::complete`) turns it into the call target's refresh, the reply
  sent once (`WrittenReply`: the tool reply, `files_written` — the one place a client learns which files
  the call wrote — and what its reply type adds after the write, rename's fresh diagnostics), the domain
  event and `mcp_mutation_completed` (ADR 0022); its outcome says the same (ADR 0162).
- **Preview**: a mutation asked only to report (`--dry-run`/`dry_run`; format's `--check` or `--diff`). It writes
  nothing; its operation's outcome answers `Wrote::Previewed` (`specforge_ops::Wrote`), its document says
  `dry_run: true`, and neither surface lists `files_written`. A run that found nothing to change is no preview:
  `Wrote::Ran` with no file, `files_written: []` (ADR 0162).
- **Outcome document**: the one typed JSON document of an operation's outcome, defined in ops beside
  the operation and deriving `Serialize` and `Shape`: the outcome itself when it is plain data
  (`stats::Stats`, `ExtensionsDocument`, `ProvidersDocument`, `SchemaDocument`; an outcome whose every field reaches its
  document and that no reader reads past it is that document), else its `document()` (`AddDocument`,
  `RemoveDocument`, `UpdateDocument`, `InitDocument`, `MigrateDocument`, `RenameDocument`, `AnalyzeDocument`, `DoctorDocument`,
  `ValidateDocument`, `SearchDocument`, `PublishDocument`, `explain::Explanation`, …). A command with no ops
  operation (extension init, build, login, logout) keeps its document in its command module.
  The CLI prints it under `--format json` and MCP answers it as the tool reply; a surface adds only
  what it alone knows, typed and flattened beside it (MCP's `note` on an add, rename's `edits`, the spec's `TextEdit`, a mutation's
  `files_written`, the CLI's credential health on doctor), and never renames, drops or re-derives a
  key. It carries no `null`. Its spec type is the tool's `Mcp*` reply type (ADR 0052).
- **Graph export**: the project's graph, or the entities a read selection covers (its scope drawn always, never cut by a budget), written in one export format by
  `specforge_emitter::emit`, read with the Registry build its graph was built in (`EmitOptions::registries`, ADR 0123), through `specforge_ops::read::export::export`: graph (the Graph Protocol), context and brief (JSON
  documents whose schema the emitter derives from the types that write them, published by `specforge schema
  --publish`), DOT, or markdown (a **Review document**). It is text: the CLI prints it, MCP sends it as text or writes it into a directory of the project (`export::write_into`, as `<dir>/<format's file name>`, replaced atomically, ADR 0131), an extension command receives it
  spliced in its input; only `specforge.query` reads it back, into its structured result. A node carries its coverage
  status (`coverage_status`) when the export is given the statuses, as a query with coverage is (ADR 0007).
- **Review document**: the graph export in the markdown format (ADR 0152), for a person who signs: one section
  per selected entity headed `## <kind> \`<id>\` — <title>`, its normative fields quoted, its other fields, its
  Links (each linked entity's kind and title; an unresolved reference marked so), the entities that reference
  it and its numbered obligations. Built from field declarations only; the selection chooses sections, the whole
  graph names links. Not a **Graph export** an agent reads back.
- **Project sources**: the `.spec` files under the spec root (`spec_root`, else the project root) that
  discovery keeps — no skipped directory, no `exclude` entry. What a compile reads, what format and migrate
  rewrite (`specforge_common::SourceRoot`, built by `ProjectConfig::sources` from a config read through
  `config::rewritable`, never past a problem that hides them), and the only files a `use` import names: a
  compile holds each by its **key**, its path relative to the spec root, and resolves imports against the
  keys alone (`specforge_resolver`, ADR 0055). An editor buffer the session holds for such a path is a
  source, saved or not.
  `SourceRoot::files` lists them and `SourceRoot::holds` answers for one path, by one policy: the spec root is
  read whatever its own name and whatever it holds, no skipped directory below it is entered, no symlink is
  followed into a directory, only files are sources (a symlink is one when it resolves to a file, read through
  the link and keyed by its own path, ADR 0073; one that points nowhere is no source, and a **Dangling source
  link** the session reads, ADR 0141), and the walk ends at every directory below the spec root that
  holds `specforge.json`: a nested project, whose files are its own project's sources, on every surface
  (ADR 0080). Format's walk of a named directory that is no project is the same walk, taking the sources of
  each project it reaches.
  An **exclude entry** is a gitignore-style pattern over a source's path (`specforge_common::Exclude`): no
  `/` but a trailing one matches a name at any depth, a `/` at the start or in the middle anchors it at the
  spec root, a trailing `/` matches directories only, and a file is left out when an entry matches it or
  one of its directories; an entry that can match no source is E069.
  A discovered source that can't be read (not UTF-8, no permission) is E025 naming it and stays out of the
  graph, in a compile and after every update alike; it is still a source, so an import of it reports
  nothing more.
- **Format configuration**: how one `.spec` document is laid out: inside a project, the nearest
  `.specforgefmt.toml` from the document's directory up to its own project root, else the defaults;
  outside any project, the editor's tab settings. `specforge format`, MCP `specforge.format` and the
  LSP all format through `specforge_ops::authoring::format::document` (ADR 0021); it is looked up by `Configs`, through
  the disk the caller hands it (ADR 0169): over MCP a `.specforgefmt.toml` that leaves the project or has more than one
  name is a W141 and the defaults apply. A typed edit lays a multi-line value out with it, looked up once per plan
  through its view's disk (`Site::config`, ADR 0153, ADR 0169). Its values are written in the field forms of the same
  project (**Lent forms**).
- **Formatted document**: one `.spec` text formatted with its format configuration, whole or in the
  blocks a range touches (`specforge_ops::authoring::format::document` → `FormattedDocument`): the formatted text,
  the configuration and where it came from, what formatting reported (W141, W142), and the edits that
  turn the text into it. Applied to the text, the edits give the formatted text byte for byte; they
  may span line breaks (a blank line removed, a final newline added, CRLF made LF) and replace only
  what formatting changes. The LSP turns them into positions through the document's line index. Not
  the format run's outcome document (`FormatDocument`, what `specforge format --format json` and
  `specforge.format` print) (ADR 0021 D8).
- **Lent forms**: the field forms of a project a caller already holds (the LSP's served environment, an MCP call's
  project), lent to a format through `specforge_ops::authoring::format::Forms` so a document of that project is written
  in them without loading its extensions again. A document of any other project is written in that project's forms,
  loaded through the caller's runtime source; outside any project, in none (ADR 0147 D6).
- **Option table**: one enumerated argument an operation takes — its listed names in order, the
  aliases it also accepts, a one-line help per name, and its default — beside that operation
  (`specforge_ops::options::OptionTable`; `export::FORMAT`, `model::MODEL_FORMAT`, …). The CLI's
  possible values and MCP's input-schema `enum`/`default` are built from it and both parse with it
  (ADR 0027). A set the project decides (analysis passes, entity kinds) is not one.
- **Check**: the operation that turns what a compile reported into what a surface reports: the
  diagnostic policy (lint profiles, then strict), the verdict (no error among everything reported),
  the severity filter (what is shown, never the verdict) and the opt-in build-cache record
  (`specforge_ops::analysis::check`). `specforge check` and MCP `specforge.validate` are its adapters; watch and
  the LSP report a compile's diagnostics without a policy (ADR 0018).
- **Run verdict**: whether an operation that judges the project passed, failed, or could not judge it,
  computed by the operation (`ok()`, or `verdict()` → `specforge_ops::RunVerdict` where it can be
  unjudged): check (no error reported), analyze (no error finding, and with a minimum the coverage
  gate met; unjudged when the gate has no figure), format (every target read and written, nothing
  left unformatted, every `specforge.json` it read used as written (no E069), and under `--check`
  nothing that would change), migrate (no file failed, the migration not refused, `specforge.json` used as
  written), doctor (no error-level finding: an error the environment reports, E013, a binary that
  does not match the lock, an unreadable lock). The CLI exits 0, 1 or 2 by it (`specforge_cli::outcome::Exit`, the only value a core
  command ends with); MCP returns it as `ok`
  (`specforge.validate`: `_meta["specforge/check"].ok`) and keeps `isError` for refusals and failed runs
  (ADR 0004 D4-a, ADR 0170). A refusal (`OpError`) is not a verdict; a measuring command's (stats, analyze)
  refusal exits 2. Not to be confused with an entity's coverage **Verdict** (ADR 0029).
- **Failed run**: a run of a writing operation that did not do all it was asked: a file it could not read,
  write or migrate (a migration's write failure is its project change's error, ADR 0171), or a change it undid (a
  rollback). Its outcome answers it (`format::Outcome::failed`,
  `migrate::Outcome::failed`: an `OpError` carrying the run's document as `data` and its writes); MCP returns
  it as the failed call (`isError`). A finding (a problem in `specforge.json` the run went around, a region
  left unformatted, a change under `--check`) fails the **Run verdict** and is no failed run (ADR 0170).
- **Terminal**: where a core CLI command writes and whom it asks: its stdout, its stderr, the input a
  prompt reads (or `specforge mcp` serves, byte for byte), whether each stream is coloured (`NO_COLOR`, then `CLICOLOR_FORCE`, then whether it is
  a terminal) and whether anyone can answer, fixed when `main` makes it from the process
  (`specforge_cli::terminal::Terminal`); a test captures one. It is the CLI's **Asker**; nobody to ask is no; a reader that leaves stdout ends the output
  quietly (ADR 0029 D4b).
- **Command notes**: what a core CLI command writes on stderr beside its outcome document: **notices** (coded
  diagnostics about the run: the schema notices, I020, W097, a failed registry's R005, the registry configuration's
  W140/I003/W155/W156) and **notes** (prose for a person: `note: …`, collect's announcement). One rule, by output
  format (`specforge_cli::notes::Notes`, ADR 0120): under human output both go on stderr; under `--format json` a
  note is never written, and a notice is written once, in the document when the document carries it, else on stderr
  before the document. A read's notices go through `Notes::noticed` (ADR 0130).
- **Fixed-format command**: a core CLI command whose stdout is a document in a format a reader parses (export,
  schema, model, query, outline; `completions`, `format --stdin`), rather than a human or `--format json`
  rendering of an outcome. It takes no `--format human|json`. Its notices are on stderr (ADR 0120 D1), and its
  refusal is `error[CODE]` on stderr under every format, JSON documents included, with nothing on stdout (ADR
  0029 D5c, `Refusal::fixed`).
- **Asking**: who can be asked to consent to what an operation would otherwise refuse (a publisher key change, a
  collector's command): yes to everything (`--yes`), nobody (MCP, `--format json`, stdin that is no terminal), or an
  `Asker` (the CLI's terminal, a test's script). A surface builds it once per command, inside its **Caller**, which add,
  update and collect read it from (`specforge_ops::asking`, ADR 0110). Collect asks it only for a command the
  **Collector machine** has no approval for.
- **Diagnostic policy**: lint profiles (a closed set: `inferred`, `pedantic`) and strict promotion
  (`specforge_project::DiagnosticPolicy`). It is the only thing that changes a diagnostic's severity
  after the diagnostic is built. A profile's diagnostics come from the check that applies it
  (`inferred`: `specforge_ops::read::infer::lint`, I200/I202 or E071); the policy reads no file.
- **Extension command**: a CLI command an extension declares in its surfaces (with the SDK, together
  with its handler: `ContributionsBuilder::command`), answered by its `cmd__` export over the graph
  the host passes (`specforge_protocol_types::CommandInput`: args, project root, graph, the
  command format and today's date, UTC). The CLI runs it as `specforge <short> <command>`, MCP as
  the auto-promoted tool `specforge.<short>.<id>`, `short` being the declaration's (`ext_short`,
  else its name's last segment); neither knows any command (ADR 0008). The registry build routes it
  under `(short, CLI name)`, the first in load order; a later one of that name is E039 and runs on no
  surface (`RegistryBuild::commands`, ADR 0097). One derivation serves both
  surfaces (`specforge_ops::extension::command::ExtensionCommand`): its CLI name, its tool name, its args'
  command-line shapes, its input schema and the args both send, normalized by the one arg rule the
  SDK also runs (`specforge_protocol_types::command_args`): declared defaults applied by the host,
  an unset flag `false`, each value its declared type (ADR 0017). Both surfaces run it through one
  operation over the project view (`specforge_ops::extension::command::run`): it normalizes the given args,
  sends the project root (absolute and canonical), the evidence and the date, and calls the export
  in the view's runtime; a surface only reads its arguments in and renders the output or the
  failure (`RunError`: refused args, no project, or an export that did not answer) (ADR 0011, "One
  operation runs a command").
- **Extension surface table**: what MCP serves from the project's extensions, built once per
  reload from their declarations: each tool once (an explicit `mcp__` tool, or an extension
  command), each resource with its URI template; listings are the core tables plus it, and a call
  is one lookup in it. Its entries are run by MCP's extension dispatch: an explicit tool's `mcp__`
  export, a command's `cmd__` export or a resource's `mcp__` export, one call each over the
  `WasmRuntime` port (`specforge_mcp`'s `extension_dispatch`). A contribution it does not serve is
  reported with I017 (`specforge_mcp`'s `ExtensionSurfaceTable`, ADR 0017).
- **Extension call**: one typed operation the host performs on a loaded extension — handshake,
  describe, command, MCP tool, MCP resource, compiler pass, collector, custom validator, scanner,
  migration hook — over the `WasmRuntime` port. Its input and answer are protocol types
  (`specforge_protocol_types`) the SDK shares; every failure is one `CallError`, E028, naming the
  operation, the export and the extension (`specforge_wasm::calls::ExtensionCalls`, ADR 0013).
- **Sandbox**: what the host holds an extension to. It is granted no capability (no preopened
  directory, environment, arguments, stdin, socket or name lookup), and it is held to two limits its
  handshake's `sandbox_policy` may declare, each at most the host's ceiling (30 000 ms per call,
  512 MB of linear memory) and the ceiling when undeclared; every call also gets the whole fuel
  budget. The loader applies them once it has read the handshake; the component runtime enforces them, and a call that
  crosses one traps with the limit's kind (E028). What a declaration asks for that the host does not
  give is W153 (`specforge_wasm::sandbox`, ADR 0037).
- **In-process runtime**: the test adapter of the `WasmRuntime` port that runs an SDK-declared
  extension in the host process through the guest's own routing (`Served`), unsandboxed
  (it records the limits the host applies and enforces none;
  `specforge_wasm::testing::InProcessRuntime`). It serves a binary's bytes under the name they are
  loaded as (`binary`), so the extension load and a candidate's read (add, init, update, publish)
  run in process (`specforge_ops::testing` serves `@sdk/greet` and builtin look-alikes that way); a
  test that serves an extension
  installs it (`specforge_installed::testing::install`) and the project loads it through the
  production path. Host tests declare their extensions with it; the
  component runtime is the production adapter, and both keep one contract
  (`assert_runtime_contract`). MCP's tests serve every project from a temporary directory through
  it (`tests/support`): no test writes a registry, a graph or a diagnostic into a server (ADR 0025).
  An extension's own command tests call its commands through it as well, from source
  (`@specforge/product`'s `extensions/product/src/tests/`, with `ExtensionCalls::run_command`); the
  component runtime tests a builtin only for what its vendored bytes prove: its declaration snapshots,
  that each command answers through it, and what a native test cannot hold it to (ADR 0013 D8).
- **Command format**: the output an extension command is asked for, `human` (the CLI default) or
  `json` (always, over MCP). The host owns the `--format` flag; the extension renders both, since
  only it knows its payloads (ADR 0011).
- **Tool spec**: the single definition of a core MCP tool: its own module under `specforge_mcp::tools`,
  implementing `CoreTool` once: its name, description and effect, the spec type its reply is, the tool
  arguments it reads, what its handler is given (nothing, the project view, the project, or the
  directory init creates, with how it reaches its project), the tool reply it answers (typed, text,
  or a mutation's) and, for each operation it calls that can refuse about a field of its request, the argument
  that sends each field (`ArgumentOf<F>`, one exhaustive map, ADR 0072). Its category, annotations, call target, descriptor (input and output schemas
  included) and dispatch derive from it; the core tool table lists each once (`CORE_TOOLS`, each seen
  as a `ToolSpec`), and a declaration that contradicts its handler does not build. A prompt names a
  tool by its typed name (ADR 0024, ADR 0049).
- **Tool effect**: what a tool does to its environment, declared once on its tool spec
  (`specforge_mcp::tool::Effect`): reads, writes output artifacts (collect, render), or mutates the
  target's project files (a mutation, ADR 0022), with how a writing tool writes (destructive,
  idempotent, open world) as MCP's annotations say it. An extension tool only reads: the host grants
  an extension no capability (ADR 0037).
- **Prompt spec**: a core MCP prompt, one module under `specforge_mcp::prompts` implementing `CorePrompt`
  once: its name, description, the spec type its payload is, its tool arguments and its typed payload,
  rendered over the project view (the empty session's when nothing is served) with its instruction (tools
  named as the tool table names them). Its descriptor and reading derive from it; `CORE_PROMPTS` lists each
  once (each seen as a `PromptSpec`). It refuses with an McpError, sent as a JSON-RPC error's data since
  prompts have no isError (ADR 0049, ADR 0058).
- **Tool arguments**: the one typed struct a tool or prompt reads its call's arguments into
  (`#[derive(Arguments)]`, `specforge_mcp::args`, ADR 0033). Each field is an argument: its name, its
  doc comment as description, its type (how a value is read, and the JSON type listed), its default,
  and its option table or name list. The input schema (or the prompt's listed arguments) and the
  reading both derive from it: absent is the default, a value is read by its type under the arg rule
  extension commands follow (a boolean or a count may come as a string), and an argument neither it
  nor the call target declares is refused.
- **Tool reply**: the one typed definition of what a core tool answers (`Reply` beside its handler,
  or the outcome document both surfaces print, `pub use … as Reply`, with MCP's own extras
  flattened beside it), deriving `Serialize` and `Shape`
  (`specforge_common::shape`, ADR 0048). Its outputSchema is derived from it as one document, built
  once per process (every object closed, every array typed, the root an object; a named shape it
  holds defined once under `$defs`; content an extension defines is an open value), its structured
  result is it
  serialized, and a result its schema refuses is never sent as one (`schema_mismatch`). An optional
  value is absent, never `null`. `spec/types/mcp.spec`'s `Mcp*` types state the same shapes, and a
  test holds them to the derived schemas. A tool answering a document in text (validate, export,
  model, outline_extensions) has no outputSchema.
  A mutation's reply is sent as `WrittenReply`: its tool reply with what the mutation pipeline adds,
  declared beside it (`files_written`; `diagnostics` when its type says so, rename), never by the tool
  reply itself.
- **Named shape**: a type whose JSON Schema a document defines once, under its root's `$defs`, and
  refers to as `{"$ref": "#/$defs/<name>"}` wherever it appears (`#[shape(define)]`,
  `specforge_common::shape`, ADR 0051): a domain type a reply embeds from another spec file
  (`Diagnostic`, `MigrationResult`, ...) or one a reply holds more than once (`SourceSpan`,
  `GraphProtocolSchema`, ...). Each MCP tool's schema is its own document, so a named shape is defined
  once per tool, never once per listing.
- **MCP message**: a document the MCP server sends whose content SpecForge decides besides a tool reply: the
  `initialize` and `server/discover` results, a listing's descriptors, the SpecForge notifications' params,
  the McpError a refusal carries (`specforge_mcp::message::Message`), and a prompt's payload. Each is one
  type deriving `Serialize` and `Shape` that names its spec type, and a test holds the two together; a frame
  whose keys MCP fixes is MCP's and is not restated (ADR 0058).
- **Surface call**: one MCP request that invokes a named tool, resource or prompt (`tools/call`,
  `resources/read`, `prompts/get`), run through one pipeline: read the request, find the entry (the
  core table, then, with the served project brought up to date, the extension surface table), record
  the invocation, resolve the call target, run the handler (which records its own events), record the
  completion, answer with the kind's envelope; every record goes through the server's event sink. Each kind is one adapter; a refusal without `isError` (resources, prompts) is a JSON-RPC
  error whose data is its McpError (`specforge_mcp::surface_call`, ADR 0024).
- **Resource spec**: the single definition of a core MCP resource: one module under `specforge_mcp::resources`
  implementing `CoreResource` once (its URI or template, name, description, MIME type, what it changes with and its
  read of the project view); `CORE_RESOURCES` lists each once, seen as a `ResourceSpec` (ADR 0103). A graph view
  (`graph`, `context`, `brief`, scoped or not, and `graph/{entity_id}`) is `specforge export` over the call's project
  view; the entity list and the diagnostics read what `specforge.list` and `specforge.validate` read. A URI names a
  resource, core or extension, by one rule (`specforge_mcp`'s `uri_template::names`): a template names every URI that
  starts with its text before the first placeholder, a URI without one names itself, alone or with a query. A core
  template reads each placeholder as one non-empty, percent-decoded path segment and refuses any other value as
  `invalid_input` naming it (`uri_template::read`); a query is the reader's to read or refuse (ADR 0049, ADR 0103).
- **Stateless request**: an MCP request whose `_meta` names its protocol version (MCP 2026-07-28),
  answered on its own without `initialize`; every other request follows the revision `initialize`
  negotiated (`specforge_mcp::modern`). Every request is served under its revision, which travels
  with it from the router to the reply (`specforge_mcp::lifecycle::Revision`); the server state
  keeps only the negotiated one. A stateless request never changes what the server serves (its `path` is read
  for that call only, `init` serves nothing); on a connection that sent `initialize` it reads what that session
  serves (ADR 0077). The method table `specforge_mcp::protocol::method` names which revision serves
  each method (ADR 0103).
- **Stdio transport**: how one MCP client and the server talk over a pair of byte streams (`specforge mcp`:
  its standard input and output). Each line of input is one JSON-RPC message; a line of only JSON whitespace is
  skipped, and one that is not UTF-8 is answered as a parse error. A message's response, then each notification it
  queued, is written one line each, and the output is flushed. The end of the input, a failed read or a failed write
  ends the connection, and its end disconnects the client (`specforge_mcp::stdio`, `McpServer::serve_stdio`). The
  CLI only hands it the terminal's input and stdout; `specforge mcp` exits 0 however the connection ends.
- **Subscription**: a client's interest in one resource the MCP server serves, made with
  `resources/subscribe` (the handshake revisions) or named by a `subscriptions/listen` stream
  (2026-07-28). A resource changes with the graph or the environment (the graph views, an
  extension's resources), with the environment alone (`specforge://schema`) or with the diagnostics
  (`specforge://diagnostics`). After an update that changed it, each subscription hears
  `notifications/resources/updated` once, and a handshake subscriber of a graph view or of the
  diagnostics then hears the delta, whole (`specforge/graphChanged`: the graph delta;
  `specforge/diagnosticsChanged`: the update's diagnostics delta, then the I017 the server's extension
  surfaces added and removed).
  Subscriptions belong to the connection and end with it or at shutdown
  (`specforge_mcp::subscriptions::Subscriptions`, ADR 0024 D6).
- **MCP event**: what an MCP server records it did — a tool, prompt or listing invoked, a resource
  read, a mutation completed, a subscription made or ended, a delta notified, an extension surface
  dispatched, the server initialized or shut down, a protocol error answered, a cancel acknowledged —
  one typed value per event the spec declares for it, its payload the spec's field for field
  (`specforge_mcp::events::McpEvent`; `spec/events/mcp.spec`, `surface-contributions.spec`, and the
  `extension_added` and `project_initialized` a mutation produces). Every module records through the
  server's one **event sink** (`EventSink`): a production server's keeps nothing (`Discard`), so what
  a server holds never grows with the requests it answered; a test's keeps every event in order,
  stamped with the time it was recorded (`events::testing::EventLog`, given with
  `McpServer::with_events`). Events never leave the process: a client learns what a call wrote from
  its reply (ADR 0022 D7, ADR 0057). The CLI, the LSP and the operations keep no event.
- **Diagnostic catalog**: the one table of diagnostic codes (`specforge_diagnostics`'s `catalog!`):
  each code's title, owner, level and explanation. It generates `CATALOG`, which the explain operation
  (`specforge_ops::read::explain`: `specforge explain` and MCP `specforge.explain`, one document),
  diagnostics JSON titles, doctor and the LSP hover read and from which
  `docs/diagnostics.md` is generated, and a **code constant** for every core code.
- **Code constant**: a core diagnostic code as a typed value, `specforge_diagnostics::codes::W112`
  (`Code`, at a fixed level; `GradedCode` for an `A###` finding whose pass sets the severity). Its
  level is the catalog's: a table entry whose `E`/`W`/`I`/`A` prefix contradicts its level does not
  compile. The host builds a diagnostic from one (`specforge_common::Diagnostic::new`, or
  `Diagnostic::graded` with the pass's severity), so its severity is the code's catalog level. Codes
  an extension reports cross the protocol as text and have no constant (`Diagnostic::from_extension`); they
  are checked against the catalog where they enter the host (`check_extension_code`, W150).
- **Diagnostic origin**: the extension that reported a diagnostic (`Diagnostic::origin`, JSON `origin`;
  a rule's declaring extension, a pass's extension; absent for the host's own). The catalog describes
  a diagnostic (title, explanation, docs link: `CodeEntry::docs_href`) only for the host's own or for the code's owner
  (`specforge_diagnostics::describes`), so an extension's finding with a code it may not use (W150) is
  kept but never presented as that code's owner's.
- **Diagnostic data**: a diagnostic's optional typed payload, the values its message names
  (`specforge_common::DiagnosticData`, e.g. an E003's unresolved target, a W061's cycle, the entity
  an extension pass named). Consumers that act on a diagnostic (the LSP's quick fixes, MCP's
  suggest_fixes, and navigation's attribution of a diagnostic to the entities it is about) read it;
  none parses the message, which is presentation.
- **Diagnostic placement**: where a diagnostic about entities with no span of its own sits: the block
  of the first entity its data names that the project holds (`Diagnostic::placed_at_subject`, applied once in
  `CompiledProject::reported` and to a pass's findings); every surface shows it there, the LSP adds related
  information at each other's name (ADR 0227).
- **Reference**: an id a field's declared type reads as a reference (a `reference` field's value, a
  `reference_list` field's items, a scheme ref ID in a `string_list`, ADR 0093), an occurrence of an entity's ID in another entity's field that resolves to it (one
  edge; where it is written is a written token the graph's finder reads). The references *to* an entity are incoming; what an entity
  *refers to* are its outgoing references. "Find references" means incoming, with the declaration
  only on request (`specforge_ops::read::navigate`). Navigation answers the references to an entity two ways: one per
  holder (each block the finder says holds the token: an inherited reference once per block that takes it, none for a
  default no block takes; `Navigator::references`, what find-references returns) and one per written token (`Navigator::tokens`, read by the graph's finder, a default no block takes
  included: what rename edits and an editor shows) (ADR 0149 D11, ADR 0156, ADR 0163, ADR 0176).
- **Reference span**: where a reference writes its ID: the ID's own bytes, inside the quotes of a quoted value
  (`"user"` is `user`; `"""user"""` too), else the value as written. `Reading::references` answers it
  (`field_form::spell::id_span`), so the finder's tokens, the linker's E003/E022/I004, navigation, rename and the replace fix
  read the same span; W159 and the formatter read the whole value (ADR 0218).
- **Written token**: a place a file writes an entity's ID as a reference: a field's reference (a block's own, or a
  `defaults` entry whether or not a block takes it) or a type name a derived field reads. The graph's one finder answers
  them per parsed file (`specforge_graph::written_tokens`), with who writes each and how, and the blocks that hold it (`held_by`). Every question of where a name is written asks
  it: rename and the editor's references (`Navigator::tokens`), find-references (`Navigator::references`), the token at
  a position (`Navigator::occurrence_at`: prepareRename, hover, definition) and a delete's referrer scan (ADR 0163,
  ADR 0176, ADR 0218: at its reference span). The graph keeps no position of a reference; a declaration's name is the parse's (ADR 0187). The finder reads each block the way the graph build stores it
  (its entries coerced), so a list written as one value is one item, never a type name.
- **Declaration name**: where a block's name is written, as the parse read it (`specforge_parser::Entity::name_span`;
  a method's, `MethodDecl::name_span`): an entity's identifier on whichever line of its header, a scheme ref's ID, a
  spec block's quoted string. Navigation reads it from the compile's parses to answer a definition, an outline, a
  rename's declaration edit and the token at a position; it is token-precise when the compiled text spells the id there
  (ADR 0187, ADR 0116); a title and each verify statement are read the same way (ADR 0187, amended).
- **Derived name**: a type name a derived field links from one block, other than the block's own id, naming an entity of
  the field's target kind: `specforge_graph`'s `derive::derived_names`, which the linker makes an edge and the finder a
  token (ADR 0127 D3, ADR 0163).
- **Navigation**: where an entity is declared, its references, entity lookup and ranking, which
  entities a diagnostic is about, and the fixes a diagnostic's data names. The LSP and MCP answer
  from one module (`specforge_ops::read::navigate`) in source spans. A span is a position in the text the
  project was compiled from (`CompiledProject::source_text`), not in the buffer typed since nor the disk
  now. The navigator reads it from its view's compiled project; no surface hands it a text, and outline answers
  only for the compile's sources (ADR 0116). The LSP converts spans to UTF-16 ranges through that text's line index (a span in a file
  whose editor text is not the compiled text has no range, ADR 0205), MCP renders them as JSON (ADR 0016, ADR 0023).
- **Rename plan**: the edits that rename one entity (`specforge_ops::authoring::rename::RenamePlan`), text edits and the planned change they make (ADR 0184) (`rename::run` plans it and writes it unless its request previews: `Renamed`, whose document is `RenameDocument`; ADR 0162):
  the declaration's name and each incoming reference's token, positions in the text the project was compiled from,
  which its planned change keeps, with the spec root they are relative to. MCP writes it only to files that still hold that text, refusing `conflict` otherwise; the LSP
  hands it to the editor only over buffers that are that text (`ContentModified` otherwise; ADR 0116, ADR 0023 D3).
  The new ID is refused (`authoring::entity_id::validate_id`) when it is not an identifier of 2–60 characters, is a reserved word (E013), is taken, or the project already writes it for no entity (a capture, `name_captured`). A
  rename is not a typed edit and is not checked by E080 (ADR 0151, amended).
- **Starter**: the spec file init writes (`spec/hello.spec`): the first template the extensions it enables declare,
  in `specforge.json` order, else the structural one, its `{project}` the project's spec ID and `{version}` its
  version, each filled with the inside of a `.spec` string (`spell::escaped`). It is checked and laid out by the
  registry build of those extensions, made before the project exists (ADR 0189), and parsed before it is written:
  one that does not parse is refused, `starter_unparsed` (ADR 0226), so its first `check` and `format --check`
  accept it.
- **Entity ID rule**: what `check` refuses of an entity ID: outside 2–60 characters (E014) or a reserved word (E013)
  (`specforge_registry::refused_id`). A token ID is also the grammar's identifier (`authoring::entity_id::validate_id`);
  a spec block's ID, its header's string, is not.
- **Text edit**: one change of a project file's text, a span of the text the project was compiled from and the
  text it becomes (`specforge_ops::read::navigate::TextEdit`; an empty span inserts). Navigation's fixes and a
  rename plan answer them; `navigate::apply_edits` applies a file's edits whole or not at all; the LSP converts
  their spans to UTF-16 ranges and MCP returns them as the spec's `TextEdit`. The formatter's edits are byte ranges
  of one buffer instead (ADR 0021 D8, ADR 0164).
- **Declared change**: what a typed edit may change in the graph: set, unset, link and unlink their field, retitle
  the title, add-verify and remove-verify the obligations, create and delete their entity whole (a create's every edge in, the names already written for it included), a delete with
  `unlink_references` each referrer's field; an edge by a declared field of its source or a created or deleted end
  (`specforge_ops::authoring::edit`, ADR 0198). The plan's graph delta over the files it changes, less the declared
  changes, must be empty, as a migration's less its steps' (ADR 0186). A rename declares a substitution of its new ID
  for its old one, nothing else.
- **Capture**: a name the project writes for no entity (an open-world type name, a reference that does not resolve, a
  defaults entry) that a new entity ID makes name that entity. A create captures by design (a forward reference,
  E004's remedy); a rename refuses one (`name_captured`, ADR 0198, amended).
- **Typed edit**: one change to a project's entities, named by what it does rather than by text: create, set,
  unset, retitle, link, unlink, add or remove a verify obligation, delete (`specforge_ops::authoring::edit::Edit`).
  A list of them is planned over the texts the project was compiled from (each edit reading what the ones before
  it left), each value written in its field's form (a multi-line one laid out with the file's format configuration, and left out when it restates its file's default)
  and each touched block formatted; the plan is refused with E080
  when the edited project reports an **added error** (ADR 0211), an error the project's **checked report** did not (its report with every check run, even after an update that skipped them: `CompiledProject::checked_diagnostics`, ADR 0167) (the **edited project** being compiled from what the project holds: the text of each source it read, the E025 of each it could not read, the planned texts; it reads no source, `CompiledProject::edited`, ADR 0158), and is written as one project change (ADR 0151). It is also refused (`undeclared_change`) when the edited graph changes what its edits do not declare (a **declared change**, ADR 0198). Its report names the net change of the list: the entities and the files its planned change answers (ADR 0184, amended).
  A delete with `unlink_references` removes every reference the graph build would link, found by the graph's finder
  in the planned texts (ADR 0163); one written in a `defaults` block, an undeclared field or as a derived type name refuses it (`not_editable`, ADR 0156).
  A list value is taken by its field's type: a reference list's items split at commas however given, a string list's never (ADR 0157). The stub fixes are typed edits too (ADR 0161).
  A `field.entry` path names one entry of a block field the kind declares; `set` and `unset` take one, `link`, `unlink` and `create` refuse it (ADR 0151, amended).
  A union's members are not edited: every verb but retitle and delete refuses one (`not_editable`), before its field is read (ADR 0219).
  Where a value goes (its column and its line) is the block's answer (`Block::put`): a verb names the field and the input, never a line or a column; a create is its header, one set per field and one add-verify per obligation (ADR 0175 D6, D7).
- **Skeleton**: a new block of a kind, or a new member, as a typed create writes it with no values, and a slot where each
  value goes: the id, the title, each required field, the final cursor (`specforge_ops::authoring::edit::Skeletons`).
  The LSP's keyword and field snippets are its slots as tabstops; it is never planned or judged (ADR 0191).
- **Fix**: an edit offered for a diagnostic or an entity, the same as an LSP code action and from MCP `suggest_fixes`
  (`specforge_ops::authoring::fix`): a replacement of an unresolved token by its close match, or a stub (a create of the
  unresolved target's kind, a verify obligation) planned as a typed edit and offered as the position edits that apply it
  (`PlannedChange::text_edits`); a stub the plan refuses is not offered (ADR 0161); the LSP lists none over a buffer typed since the compile (ADR 0023 D3).
- **Lexeme**: one token of a `.spec` text as the language's one lexer reads it without a parse
  (`specforge_parser::lex`): an identifier, a scheme ref ID (one lexeme), a number, a string (read as
  the grammar reads it, across lines; one the grammar would not close is marked unclosed and, if a
  `"…"`, ended at its line's end, if a `"""…"""`, before the next form start), a comment or a punctuation character. Navigation, the LSP's
  document, the parser's recovery from unclosed strings and the formatter read text through it, and
  where a top-level form starts is read from lexemes too (form start); a test holds it to the grammar
  on the repository's spec. The prove pass's expression tokenizer is the one
  other reader (ADR 0023, ADR 0038).
- **Form start**: a line whose first lexemes open a top-level form of the grammar (an import, a spec,
  ref, define, union or entity block), indented no deeper than the last line begun with no bracket open
  (`specforge_parser::lex::starts_top_level_form`, asked over a document by `lex::form_starts`). Such a
  line closes every body, block, list and parameter list left open before it: the parser's recovery cuts
  the source there when the grammar's tree missed it, the LSP's document closes its frames there, the
  grammar's scanner ends an unclosed body there (its MISSING `}`), and the lexer ends an unclosed
  `"""…"""` before it (ADR 0038 D7), so a block being typed never takes the forms after it. Tests hold it
  to the grammar's top-level nodes on the repository's spec, both ways, and the scanner's copy to it
  (ADR 0038 D6-D8, ADR 0063).
- **Body shape**: how a block's body is written: **braced** (`kind name "Title" { ... }`: fields, methods, `verify`
  statements) or a **union** (`kind name "Title" = a | b`: its variants and nothing else). The parse answers it once,
  from the syntax (`specforge_parser::FieldMap::is_union`, the parser's own variant list), never from a field's name:
  a braced block's member named `variants` is a field. The standing's union exemption and the typed edits read it
  (ADR 0219).
- **Recovered syntax**: a `.spec` source as SpecForge reads its concrete syntax
  (`specforge_parser::cst`): the grammar's tree of the whole text, or, when the tree missed a form start
  or a string was left unclosed, one tree per piece cut before those form starts. The parser's walk and
  the formatter read the same pieces, so the graph, the formatted text and the editor's document agree on
  a broken document's forms; a form the grammar could not read whole carries an E001 (an unclosed body at
  its `{`, an unclosed string at its quote) and is kept as written by the formatter (ADR 0063).
- **Cursor**: what the LSP knows about a position in an open document, read from the document's lexemes
  (`specforge_parser::lex`) and their block structure, never from the graph: the word under it, whether
  it is in code, a string or a comment, the entity block, field and reference list around it, and the
  `use` statement it is on. The entity a cursor names is the declaration or reference token under it,
  else an identifier at a reference position (a header's name, a value or item of a field whose
  reading may name an entity, a `use` binding's imported name) that names an entity;
  hover, definition, references and rename all ask the cursor, completion asks it what completes
  there, and semantic tokens mark the same reference positions (`specforge_lsp::document`, ADR 0023).
  While the document is not the text the project was compiled from, the cursor never asks navigation
  about its position (a token of the compiled text): it names what its own word names, and
  prepareRename and rename wait for the compile. Every request's answer is decided synchronously over
  the LSP state (`specforge_lsp::answers`); the backend only carries requests.
- **LSP reaction**: what the LSP does after the client reports a change (a workspace opened, documents
  opened, edited or closed, files changed on disk): apply it to the project session as one update,
  publish what the project reports (a closed document's file always, as the project reports it),
  announce a reload, follow the session's inputs and catch up (the follow loop watch shares, ADR 0099), ask the editor to refresh its
  highlighting. It runs one change at a time, synchronously, and tells the editor everything through
  one port (`specforge_lsp::reaction::Reaction` over `specforge_lsp::editor::Editor`; ADR 0035,
  ADR 0043).
- **Proof role**: what a field's value is to the prove pass, declared by its extension
  (`proof_role`): a **bound** the solver assumes (bounds must be consistent, E046) or a **claim**
  that must follow from the bounds (W139 when not; an entailed claim is a proved claim). A field
  with no role is not read by the prove pass (ADR 0009).
- **Solver**: the SMT solver the prove pass asks (`specforge_ops::solver::Solver`, port `SmtSolver`):
  whether it runs here (its version, or the W098 that says why not) and its answer to one SMT-LIB2 script.
  Its adapters are z3 from PATH (`Z3`, which owns its timeouts and that W098) and a scripted solver
  (tests). `specforge analyze --prove` and doctor take it as an argument; doctor's toolchain finding is
  the W098 the prove pass reports. A test chooses the solver it sees (ADR 0079).
- **Collector machine**: the machine collect runs on (`specforge_ops::analysis::collect::CollectorMachine`, port
  `CollectorMachine`): it runs a collector's command (and answers when it started) and keeps the user's approvals of
  commands. Its adapters are a child process with the user-level consent store (`ProcessMachine`) and a scripted
  machine (tests). The caller passes it; collect decides every refusal that needs no run before the machine runs
  anything (ADR 0125).
- **Collect mode**: how `collect` gets each collector's evidence (`specforge_ops::analysis::collect::Mode`): run
  the declared command and read its report (`Run`), read the report it already wrote (`NoRun`, `--no-run`), or
  read only the files the request names (`Reports`, `--report`). Collect checks only what the mode reads: under
  `--report` the collector's declared report, capture and command are not looked at (ADR 0125 D7).
- **Risk grading**: the coverage owner's policy for one kind: its entities' risk is tallied, and
  one with no obligations is A002, an error at the grading's error level
  (`specforge_coverage::RiskGrading`, supplied by `@specforge/testing`; ADR 0009).
- **Lifecycle field**: the one field of a kind that holds its entities' lifecycle state
  (`lifecycle_field` on the kind). The build cache records its value so check-phase passes can
  compare against the previous build (ADR 0009).
- **Call containment**: the one **Containment** an MCP call reads and writes its project through, built when the call is
  resolved; its views carry it (`ProjectView::bounded_by`) and its caller lends it (ADR 0183). The CLI's and the LSP's is
  every path.
