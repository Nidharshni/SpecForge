# SpecForge — Dream-Team Roster Index

125 engineers (living practitioners and foundational figures) selected to enhance **SpecForge**
per its architecture: a zero-domain-knowledge typed-graph compiler (`.spec` → validated entity
graph → Graph Protocol JSON) with a Wasm/Extism extension runtime, CLI/LSP/MCP surfaces, a
test-tracing protocol, and a formal-methods extension — consumed primarily by AI agents.

References in each file were verified by parallel research passes (canonical papers, official
repos, primary sources). URLs appear only where confidence in the link is high; otherwise works
are cited as title + venue + year.

## File format

Each `NNN-slug.md` contains: cluster + roster role + SpecForge anchors (the concrete crates,
files, docs, or PRDs this expertise maps to), *why this engineer* for this build, verified
references (papers/books/repos with why each helps), and a *study first* list.

## Clusters

| Cluster | Name | Focus |
| --- | --- | --- |
| C1 | Foundations of intent capture & specification | The ideas SpecForge operationalizes: compilers as intent-capturing machines, BNF, theory-building, literate programming. |
| C2 | DSL & language design | Designing a 5-minute-to-learn external DSL that evolves without breaking its users. |
| C3 | Parsing & grammar infrastructure | The tree-sitter layer: error-tolerant generic block parsing, incremental re-parse, name resolution, string interning. |
| C4 | Incremental compilation, LSP & diagnostics | The shared watch/LSP pipeline, IDE feature set, and rustc-grade error messages. |
| C5 | Graph engine & algorithms | Custom typed entity graph (`Graph{nodes,edges}` + indexes — note: petgraph is declared in workspace deps but consumed by no crate, despite README): cycle detection, subgraph queries, ERD semantics. |
| C6 | Schemas, validation & serialization | The Graph Protocol as an open standard: JSON Schema governance, schema evolution, token-efficient encodings. |
| C7 | Wasm plugin runtimes | The extension bet: Extism/Wasmtime, Component Model, WASI, capability sandboxes, warm engine pools. |
| C8 | Registries, packaging & supply-chain trust | `specforge add/remove/publish`: registry UX, semver compatibility, peer deps, signing, update frameworks. |
| C9 | MCP & AI-agent context engineering | The primary consumers: MCP protocol design, spec-driven development peers, token budgets, agent output contracts. |
| C10 | Formal methods | `@specforge/formal`: CSP processes, TLA+-style properties, Design by Contract, refinement, and the future `analyze --prove` path. |
| C11 | Testing & BDD | `verify` statements, Gherkin, the `specforge-report.json` protocol, framework-native adapters. |
| C12 | Requirements, ubiquitous language & docs-as-code | The requirements-engineering lineage behind behavior/invariant/feature chains, ADRs, glossary, event discovery. |
| C13 | Diagrams & rendering | `model`/`outline`/`dot`/mermaid renderers and the renderer-extension mechanism. |
| C14 | Direct-dependency maintainers | The crates the surfaces run on: tokio/tower-lsp/axum/clap/notify/insta/ariadne/regex. |

## Roster

| # | Agent | File | Cluster | Roster role | SpecForge anchors |
| ---: | --- | --- | --- | --- | --- |
| 001 | Grace Hopper | [`001-grace-hopper.md`](001-grace-hopper.md) | C1 | first compilers; machine-independent programming | README.md thesis, docs/spec-writing-flow.md |
| 002 | John Backus | [`002-john-backus.md`](002-john-backus.md) | C1 | FORTRAN lead; BNF; von Neumann lecture | tree-sitter-specforge grammar, docs/model/formats.md |
| 003 | Peter Naur | [`003-peter-naur.md`](003-peter-naur.md) | C1 | BNF; "Programming as Theory Building" | README.md thesis, vision/north-star.md |
| 004 | Donald Knuth | [`004-donald-knuth.md`](004-donald-knuth.md) | C1 | literate programming; Stanford GraphBase | docs-compile-like-code principle, specforge-graph.dot |
| 005 | Niklaus Wirth | [`005-niklaus-wirth.md`](005-niklaus-wirth.md) | C2 | Pascal/Oberon; "A Plea for Lean Software" | DSL learnability, CLI command surface |
| 006 | Anders Hejlsberg | [`006-anders-hejlsberg.md`](006-anders-hejlsberg.md) | C2 | Turbo Pascal → C#/TypeScript | FieldRegistry typed fields, LSP-first design |
| 007 | Graydon Hoare | [`007-graydon-hoare.md`](007-graydon-hoare.md) | C2 | created Rust | edition-2024 core, rustc-style diagnostics goal |
| 008 | Steve Klabnik | [`008-steve-klabnik.md`](008-steve-klabnik.md) | C2 | Rust language & docs culture | docs/guides authoring set, README quality bar |
| 009 | Martin Fowler | [`009-martin-fowler.md`](009-martin-fowler.md) | C2 | "Domain-Specific Languages"; evolutionary design | DSL evolution, specforge-migrate format versioning |
| 010 | Terence Parr | [`010-terence-parr.md`](010-terence-parr.md) | C2 | ANTLR creator; language implementation patterns | grammar.js, parser error recovery |
| 011 | Sven Efftinge | [`011-sven-efftinge.md`](011-sven-efftinge.md) | C2 | Xtext creator; Langium lead | textual-DSL tooling, integrations/vscode |
| 012 | Max Brunsfeld | [`012-max-brunsfeld.md`](012-max-brunsfeld.md) | C3 | tree-sitter creator; Zed co-founder | crates/tree-sitter-specforge, parse.rs recovery |
| 013 | Tim Clem | [`013-tim-clem.md`](013-tim-clem.md) | C3 | tree-sitter at GitHub code nav; Blackbird code search; now agent context | grammar-as-Wasm (RES-30) |
| 014 | Patrick Thomson | [`014-patrick-thomson.md`](014-patrick-thomson.md) | C3 | stack-graphs: precise name resolution | specforge-resolver link_references, LSP find-refs |
| 015 | Björn Linse | [`015-bjorn-linse.md`](015-bjorn-linse.md) | C3 | Neovim tree-sitter; incremental re-parse | parse_incremental, watch invalidation |
| 016 | Chase Wilson | [`016-chase-wilson.md`](016-chase-wilson.md) | C3 | lasso string interner (direct dep) | specforge-common interner.rs `Sym(Spur)` |
| 017 | Dirk Bäumer | [`017-dirk-baeumer.md`](017-dirk-baeumer.md) | C4 | Microsoft LSP spec lead | specforge-lsp backend.rs capabilities |
| 018 | Aleksey Kladov | [`018-aleksey-kladov.md`](018-aleksey-kladov.md) | C4 | rust-analyzer creator; salsa | shared watch+LSP incremental pipeline |
| 019 | Lukas Wirth | [`019-lukas-wirth.md`](019-lukas-wirth.md) | C4 | rust-analyzer maintainer | specforge-lsp feature completeness |
| 020 | Michael Woerister | [`020-michael-woerister.md`](020-michael-woerister.md) | C4 | rustc incremental compilation | IncrementalPipeline DAG invalidation |
| 021 | Niko Matsakis | [`021-niko-matsakis.md`](021-niko-matsakis.md) | C4 | Rust lang lead; salsa framework | query-based graph recompute design |
| 022 | Rebecca Stambler | [`022-rebecca-stambler.md`](022-rebecca-stambler.md) | C4 | gopls lead | LSP workspace indexing |
| 023 | Robert Findley | [`023-robert-findley.md`](023-robert-findley.md) | C4 | gopls co-lead | LSP workspace indexing |
| 024 | Muir Manders | [`024-muir-manders.md`](024-muir-manders.md) | C4 | gopls core | LSP workspace indexing |
| 025 | Sam McCall | [`025-sam-mccall.md`](025-sam-mccall.md) | C4 | clangd maintainer | LSP at scale |
| 026 | Mads Hartmann | [`026-mads-hartmann.md`](026-mads-hartmann.md) | C4 | bash-language-server creator | LSP for config-like DSLs |
| 027 | Nathan Sobo | [`027-nathan-sobo.md`](027-nathan-sobo.md) | C4 | Atom creator; Zed Wasm extensions | editor extension model, integrations/vscode |
| 028 | Eyal Kalderon | [`028-eyal-kalderon.md`](028-eyal-kalderon.md) | C4 | tower-lsp creator (direct dep) | crates/specforge-lsp |
| 029 | Esteban Küber | [`029-esteban-kuber.md`](029-esteban-kuber.md) | C4 | rustc diagnostics lead | Diagnostic model, ariadne, did-you-mean |
| 030 | Evan Czaplicki | [`030-evan-czaplicki.md`](030-evan-czaplicki.md) | C4 | Elm creator; compiler-error UX | `specforge check` output quality |
| 031 | Robert Tarjan | [`031-robert-tarjan.md`](031-robert-tarjan.md) | C5 | SCC & cycle algorithms | verified cycle checks: W003 imports, W061 references, E007 modules |
| 032 | Edsger W. Dijkstra | [`032-edsger-dijkstra.md`](032-edsger-dijkstra.md) | C5 | graph algorithms; weakest preconditions | graph algorithms, reasoning discipline |
| 033 | Peter Chen | [`033-peter-chen.md`](033-peter-chen.md) | C5 | Entity-Relationship model (1976) | specforge-emitter/src/model ERD renderers |
| 034 | bluss (Ulrik Sverdrup) | [`034-blake-sweeney.md`](034-blake-sweeney.md) | C5 | petgraph author (direct dep) | crates/specforge-graph — adopt-petgraph-vs-custom decision |
| 035 | Agustín Borgna | [`035-agustin-borgna.md`](035-agustin-borgna.md) | C5 | petgraph maintainer | crates/specforge-graph |
| 036 | Austin Wright | [`036-austin-wright.md`](036-austin-wright.md) | C6 | original JSON Schema author | schema/*.json draft 2020-12 |
| 037 | Henry Andrews | [`037-henry-andrews.md`](037-henry-andrews.md) | C6 | JSON Schema spec lead | Graph Protocol schema design |
| 038 | Ben Hutton | [`038-ben-hutton.md`](038-ben-hutton.md) | C6 | JSON Schema maintainer & governance | open-standard strategy, `specforge schema` |
| 039 | Lee Byron | [`039-lee-byron.md`](039-lee-byron.md) | C6 | GraphQL co-creator | one-schema-many-consumers moat, MCP tools |
| 040 | Kenton Varda | [`040-kenton-varda.md`](040-kenton-varda.md) | C6 | Protocol Buffers; Cap'n Proto | schema evolution, binary exports |
| 041 | David Tolnay | [`041-david-tolnay.md`](041-david-tolnay.md) | C6 | serde/thiserror/anyhow (direct deps) | serde models across all crates |
| 042 | Samuel Colvin | [`042-samuel-colvin.md`](042-samuel-colvin.md) | C6 | Pydantic v2 (Rust-core validation) | validation_engine.rs declarative rules |
| 043 | Colin McDonnell | [`043-colin-mcdonnell.md`](043-colin-mcdonnell.md) | C6 | Zod creator | TS-side schema consumption |
| 044 | James Munns | [`044-james-munns.md`](044-james-munns.md) | C6 | postcard binary format | binary report schema, wasm payloads |
| 045 | Steve Manuel | [`045-steve-manuel.md`](045-steve-manuel.md) | C7 | Extism creator (direct dep) | specforge-extism runtime.rs |
| 046 | Benjamin Eckel | [`046-benjamin-eckel.md`](046-benjamin-eckel.md) | C7 | Extism co-author | extism-convert, manifests, PDKs |
| 047 | Nick Fitzgerald | [`047-nick-fitzgerald.md`](047-nick-fitzgerald.md) | C7 | Wasmtime lead | AOT caching for CLI, engine pools |
| 048 | Alex Crichton | [`048-alex-crichton.md`](048-alex-crichton.md) | C7 | Wasmtime/wit-bindgen; early Rust/Cargo | Component Model for extension protocol |
| 049 | Dan Gohman | [`049-dan-gohman.md`](049-dan-gohman.md) | C7 | WASI lead | sandbox policy; wasi/unknown-unknown fix |
| 050 | Luke Wagner | [`050-luke-wagner.md`](050-luke-wagner.md) | C7 | Wasm co-designer; Component Model | handshake v1.0.0 protocol versioning |
| 051 | Till Schneidereit | [`051-till-schneidereit.md`](051-till-schneidereit.md) | C7 | Mozilla Wasm; Bytecode Alliance | runtime ecosystem positioning |
| 052 | Andreas Rossberg | [`052-andreas-rossberg.md`](052-andreas-rossberg.md) | C7 | Wasm formal semantics | RES-21 runtime decision, formal bridge |
| 053 | Matt Butcher | [`053-matt-butcher.md`](053-matt-butcher.md) | C7 | Fermyon Spin creator | warm engine pools, Wasm ergonomics |
| 054 | Kevin Hoffman | [`054-kevin-hoffman.md`](054-kevin-hoffman.md) | C7 | wasmCloud | capability-based host-fn allowlist |
| 055 | Syrus Akbary | [`055-syrus-akbary.md`](055-syrus-akbary.md) | C7 | Wasmer founder | competitive runtime landscape |
| 056 | Matt Klein | [`056-matt-klein.md`](056-matt-klein.md) | C7 | Envoy creator; extension architecture | extension surfaces (11 categories) at scale |
| 057 | Mitchell Hashimoto | [`057-mitchell-hashimoto.md`](057-mitchell-hashimoto.md) | C8 | Terraform/Vagrant; Ghostty | add/remove/publish, extension manifests |
| 058 | Tom Preston-Werner | [`058-tom-preston-werner.md`](058-tom-preston-werner.md) | C8 | SemVer spec author | handshake version compatibility |
| 059 | Isaac Schlueter | [`059-isaac-schlueter.md`](059-isaac-schlueter.md) | C8 | npm creator | registry UX, lockfile model |
| 060 | Carol Nichols | [`060-carol-nichols.md`](060-carol-nichols.md) | C8 | crates.io co-founder | registry-server storage/index |
| 061 | Yehuda Katz | [`061-yehuda-katz.md`](061-yehuda-katz.md) | C8 | Bundler; Cargo resolution design | peer-dependency resolution (E028) |
| 062 | D. Richard Hipp | [`062-richard-hipp.md`](062-richard-hipp.md) | C8 | SQLite creator | registry-server SQLite storage |
| 063 | Justin Cappos | [`063-justin-cappos.md`](063-justin-cappos.md) | C8 | TUF/in-toto | publish integrity (E032), update framework |
| 064 | Luke Hinds | [`064-luke-hinds.md`](064-luke-hinds.md) | C8 | Sigstore co-founder | extension artifact signing |
| 065 | David Soria Parra | [`065-david-soria-parra.md`](065-david-soria-parra.md) | C9 | MCP co-creator (Anthropic) | crates/specforge-mcp protocol |
| 066 | Justin Spahr-Summers | [`066-justin-spahr-summers.md`](066-justin-spahr-summers.md) | C9 | co-built MCP at Anthropic; now independent | MCP tool/resource/prompt design |
| 067 | Den Delimarsky | [`067-den-delimarsky.md`](067-den-delimarsky.md) | C9 | GitHub Spec Kit | spec-driven-development positioning, PRD-007 |
| 068 | Paul Gauthier | [`068-paul-gauthier.md`](068-paul-gauthier.md) | C9 | aider creator; repo maps | `export --format=context/brief`, token budgets |
| 069 | Simon Willison | [`069-simon-willison.md`](069-simon-willison.md) | C9 | LLM tooling practitioner-critic | agent-facing CLI ergonomics |
| 070 | Andrej Karpathy | [`070-andrej-karpathy.md`](070-andrej-karpathy.md) | C9 | "LLM OS"; structured context | AI-cost thesis, RES-18 token economics |
| 071 | Harrison Chase | [`071-harrison-chase.md`](071-harrison-chase.md) | C9 | LangChain | agent orchestration, MCP subscriptions |
| 072 | Jerry Liu | [`072-jerry-liu.md`](072-jerry-liu.md) | C9 | LlamaIndex | `query --depth`, retrieval budgets |
| 073 | Beyang Liu | [`073-beyang-liu.md`](073-beyang-liu.md) | C9 | Sourcegraph co-founder; now Amp Inc | graph as code-context substrate |
| 074 | Omar Khattab | [`074-omar-khattab.md`](074-omar-khattab.md) | C9 | DSPy creator | declarative pipelines over LM calls |
| 075 | Shreya Rajpal | [`075-shreya-rajpal.md`](075-shreya-rajpal.md) | C9 | Guardrails AI | validating agent output vs contracts |
| 076 | Jason Liu | [`076-jason-liu.md`](076-jason-liu.md) | C9 | Instructor; structured outputs | typed exports for agents |
| 077 | Walden Yan | [`077-walden-yan.md`](077-walden-yan.md) | C9 | Cognition; context engineering | context-first design thesis |
| 078 | C. A. R. Hoare | [`078-tony-hoare.md`](078-tony-hoare.md) | C10 | CSP; verifying-compiler challenge | formal `process`/`sync`, E034/E042 |
| 079 | Leslie Lamport | [`079-leslie-lamport.md`](079-leslie-lamport.md) | C10 | TLA+; safety/liveness | `property` kinds (safety/liveness/fairness) |
| 080 | Jean-Raymond Abrial | [`080-jean-raymond-abrial.md`](080-jean-raymond-abrial.md) | C10 | Z & B-Method | `refinement` entity, RES-25 |
| 081 | Bertrand Meyer | [`081-bertrand-meyer.md`](081-bertrand-meyer.md) | C10 | Design by Contract | `requires`/`ensures`, W036–W040 |
| 082 | Rustan Leino | [`082-rustan-leino.md`](082-rustan-leino.md) | C10 | Dafny/Spec#/Boogie | future `specforge analyze --prove` |
| 083 | Daniel Jackson | [`083-daniel-jackson.md`](083-daniel-jackson.md) | C10 | Alloy; "Software Abstractions" | impossible-by-construction checks |
| 084 | Gerard Holzmann | [`084-gerard-holzmann.md`](084-gerard-holzmann.md) | C10 | Spin/Promela | event-graph linting (sync blocks) |
| 085 | Edmund Clarke | [`085-edmund-clarke.md`](085-edmund-clarke.md) | C10 | model checking (Turing Award) | analyze-pass verification roadmap |
| 086 | Leonardo de Moura | [`086-leonardo-de-moura.md`](086-leonardo-de-moura.md) | C10 | Z3; Lean | SMT-backed condition checking |
| 087 | Xavier Leroy | [`087-xavier-leroy.md`](087-xavier-leroy.md) | C10 | CompCert verified compiler | TCB discipline (zero-entity-core guard) |
| 088 | Ralf Jung | [`088-ralf-jung.md`](088-ralf-jung.md) | C10 | RustBelt; Miri | Rust-core soundness, testing-as-oracle |
| 089 | Gernot Heiser | [`089-gernot-heiser.md`](089-gernot-heiser.md) | C10 | seL4 verified kernel | shipping verified components |
| 090 | Kent Beck | [`090-kent-beck.md`](090-kent-beck.md) | C11 | xUnit; TDD | `verify` kinds, specforge-test harness |
| 091 | Erich Gamma | [`091-erich-gamma.md`](091-erich-gamma.md) | C11 | JUnit; VS Code lead | integrations/vscode architecture |
| 092 | Dan North | [`092-dan-north.md`](092-dan-north.md) | C11 | BDD inventor | behavior contracts + verify statements |
| 093 | Aslak Hellesøy | [`093-aslak-hellesoy.md`](093-aslak-hellesoy.md) | C11 | Cucumber/Gherkin creator | `gherkin` field on behaviors |
| 094 | Gojko Adzic | [`094-gojko-adzic.md`](094-gojko-adzic.md) | C11 | "Specification by Example" | spec→test traceability, `specforge trace` |
| 095 | Holger Krekel | [`095-holger-krekel.md`](095-holger-krekel.md) | C11 | pytest creator | @specforge/pytest adapter |
| 096 | Bruno Oliveira | [`096-bruno-oliveira.md`](096-bruno-oliveira.md) | C11 | pytest lead maintainer | plugin/reporter architecture |
| 097 | Christoph Nakazawa | [`097-christoph-nakazawa.md`](097-christoph-nakazawa.md) | C11 | Jest creator | reporter-plugin pattern |
| 098 | Anthony Fu | [`098-anthony-fu.md`](098-anthony-fu.md) | C11 | Vitest creator | primary report adapter target |
| 099 | David L. Parnas | [`099-david-parnas.md`](099-david-parnas.md) | C12 | information hiding; A-7E tables | crate modularity, traceability chain |
| 100 | Harlan Mills | [`100-harlan-mills.md`](100-harlan-mills.md) | C12 | Cleanroom software engineering | correctness-by-specification philosophy |
| 101 | Ian Sommerville | [`101-ian-sommerville.md`](101-ian-sommerville.md) | C12 | requirements-engineering canon | behavior/invariant/feature chain |
| 102 | Axel van Lamsweerde | [`102-axel-van-lamsweerde.md`](102-axel-van-lamsweerde.md) | C12 | KAOS goal-oriented RE | feature→behavior linkage |
| 103 | Michael A. Jackson | [`103-michael-jackson.md`](103-michael-jackson.md) | C12 | Problem Frames | feature problem/solution framing |
| 104 | John McCarthy | [`104-john-mccarthy.md`](104-john-mccarthy.md) | C12 | formal business communication language | spec graph as shared formal vocabulary |
| 105 | Eric Evans | [`105-eric-evans.md`](105-eric-evans.md) | C12 | Domain-Driven Design | glossary.spec `term` entities |
| 106 | Michael Nygard | [`106-michael-nygard.md`](106-michael-nygard.md) | C12 | Architecture Decision Records | governance `decision` entities |
| 107 | Alberto Brandolini | [`107-alberto-brandolini.md`](107-alberto-brandolini.md) | C12 | Event Storming | event/produces/consumes discovery |
| 108 | Adam Dymitruk | [`108-adam-dymitruk.md`](108-adam-dymitruk.md) | C12 | Event Modeling | journey/event flow specs |
| 109 | John MacFarlane | [`109-john-macfarlane.md`](109-john-macfarlane.md) | C12 | CommonMark lead; pandoc | prose-vs-DSL boundary, markdown emitters |
| 110 | John Ellson | [`110-john-ellson.md`](110-john-ellson.md) | C13 | Graphviz/DOT co-creator | `emit --format=dot`, specforge-graph.dot |
| 111 | Emden Gansner | [`111-emden-gansner.md`](111-emden-gansner.md) | C13 | Graphviz layout algorithms | diagram layout quality |
| 112 | Stephen North | [`112-stephen-north.md`](112-stephen-north.md) | C13 | Graphviz project lead | DOT emitter fidelity |
| 113 | Knut Sveidqvist | [`113-knut-sveidqvist.md`](113-knut-sveidqvist.md) | C13 | Mermaid creator | mermaid erDiagram renderer |
| 114 | Arnaud Roques | [`114-arnaud-roques.md`](114-arnaud-roques.md) | C13 | PlantUML creator | future renderer extensions |
| 115 | Simon Brown | [`115-simon-brown.md`](115-simon-brown.md) | C13 | C4 model; Structurizr | `specforge model`, renderer mechanism |
| 116 | Sean McArthur | [`116-sean-mcarthur.md`](116-sean-mcarthur.md) | C14 | hyper/reqwest (direct deps) | specforge-registry HTTP client |
| 117 | Carl Lerche | [`117-carl-lerche.md`](117-carl-lerche.md) | C14 | Tokio co-creator | async runtime under lsp/mcp |
| 118 | Alice Ryhl | [`118-alice-ryhl.md`](118-alice-ryhl.md) | C14 | Tokio lead | async performance, LSP concurrency |
| 119 | David Pedersen | [`119-david-pedersen.md`](119-david-pedersen.md) | C14 | axum creator | specforge-registry-server handlers.rs |
| 120 | Joseph Birr-Pixton | [`120-joseph-birr-pixton.md`](120-joseph-birr-pixton.md) | C14 | rustls maintainer | registry client TLS |
| 121 | Félix Saparelli | [`121-felix-saparelli.md`](121-felix-saparelli.md) | C14 | notify maintainer | specforge-watch file events |
| 122 | Kevin Knapp | [`122-kevin-knapp.md`](122-kevin-knapp.md) | C14 | clap creator | specforge-cli 34 commands, completions |
| 123 | Armin Ronacher | [`123-armin-ronacher.md`](123-armin-ronacher.md) | C14 | insta; Flask; rye; AI-tooling DX | insta snapshot tests, CLI UX |
| 124 | Joshua Barretto | [`124-joshua-barretto.md`](124-joshua-barretto.md) | C14 | ariadne; chumsky | specforge-validator render.rs |
| 125 | Andrew Gallant | [`125-andrew-gallant.md`](125-andrew-gallant.md) | C14 | ripgrep; regex; walkdir | regex rule compilation, file discovery |

## Priority engagement order (if only 8)

Items 1, 2, and 5 below were closed by the W1-W9 audit-remediation pass (see
`.plugin/wave3-plan.md`, `.plugin/audit-triage.md`) and are kept here only as
historical context for the roster's "why this engineer" framing, not as open
work.

1. ~~**Extension-runtime bootstrap**~~ (Manuel/Eckel type, C7) — **done.**
   `.github/workflows/ci.yml`'s "Fresh-clone bootstrap" job runs
   `cargo gate bootstrap` from a clean checkout, which rebuilds every
   committed blob and compares it with the vendored bytes; the blobs are also git-tracked under
   `extensions/*/wasm/*.wasm` as a working fallback.
2. ~~**Incremental/LSP pipeline**~~ (Kladov/Woerister type, C4) — **done.**
   `specforge-lsp` now imports `specforge_watch::IncrementalPipeline`
   directly (`crates/specforge-lsp/src/{state,backend}.rs`) — one shared
   incremental pipeline, not two.
3. **MCP & agent DX** (Soria Parra/Delimarsky type, C9) — tool surface, token-budgeted exports,
   dialogue with the spec-driven-development ecosystem.
4. **Formal analyze roadmap** (Leino/Jackson type, C10) — turn `specforge analyze` passes into a
   staged path toward machine-checked conditions.
5. ~~**Registry trust**~~ (Cappos/Hinds type, C8) — **done.** `crates/specforge-registry/src/signing.rs`
   implements Ed25519 sign/verify with key persistence; the registry-server rejects unsigned
   publishes (`crates/specforge-registry-server/src/handlers.rs`, `tests/publish_signatures.rs`).
6. **Diagnostics UX** (Küber/Czaplicki type, C4) — rustc-grade codes, spans, suggestions everywhere.
7. **Graph Protocol standardization** (Hutton/Byron type, C6) — governance and conformance suites
   for the open schema.
8. **Traceability & process** (Parnas/Adzic type, C12/C11) — evidence chains from spec to test.
