# A session takes a change from disk only as changes

**Status:** accepted (2026-10-15, round 10, plan 09). Amends ADR 0046 D1 and finishes ADR 0073 D1.
*Amended (2026-10-17, round 14, plan 10): D5.*

`ProjectSession::update(SourceChange::Disk(keys))` was public, but its only production caller was
`ProjectSession::apply`. 58 test call sites used it and skipped the step every surface goes through: turning paths
into changes by comparing stamps (`ProjectSession::changes`, ADR 0073). That door also re-read a key whose file did
not change and re-ran every check. Three tests depended on that, one paired it with a check mode production never
uses on disk, and others asserted the fields of an `Update` that no surface makes (watch prints nothing for a batch
that reaches only an excluded file, and `apply` returns `None`).

## Decision

- **D1.** A session takes a change from disk only as `Changes` (`changes`, `stale`) through `apply` or
  `ensure_fresh`. The key door is a private method (`reread`).
- **D2.** Buffers go in through `hold`, `hold_with` and `release`. `SourceChange`, `update` and `update_with` are
  gone.
- **D3.** Tests take the surfaces' doors. `specforge_project::testing::disk_changed` names keys for `changes` plus
  `apply`, and a test that names an untouched file gets `None`, as a surface does. The tests that relied on the old
  door take the one that gives what they test: the cache file as a check input, `reload_environment`, a buffer with
  the typing fast path.
- **D4.** The LSP stand-in stays. ADR 0023's rejection of a shared read snapshot stands. Its premise is restated:
  the stand-in copies five things (the graph, the environment handle, the source texts, the source keys and the
  memo's snapshot), not one, and the graph clone is deep. Its reopen condition (a second reader of a session during
  an update) is unmet.
  *ADR 0108: the stand-in is a copy of the compiled project (`CompiledProject: Clone`); the decision and its reopen
  condition stand.*
- **D5.** A session makes every `Changes` it applies. `Changes`'s fields are the crate's, and the readers (watch's
  event labels, the LSP's touched files, tests) read them through `sources()`, `environment()` and
  `check_inputs()`. A test takes `Changes` from `changes`, `stale`, `ensure_fresh` or `testing::disk_changed`.
  `apply` asks no root: a detached session makes no changes (ADR 0073 D7). It still leaves out every buffer held
  when it runs, since a `Changes` is a value that may outlive a `hold`.

## Rejected

- A `testing` re-export of the key door: it keeps the door tests use and surfaces do not.
- Widening `Changes` or `Update` with the labels watch and the LSP print: they read the public result of `changes`
  once each, which is the interface working as designed.
- A `testing`-only `Changes` constructor: the key door's reason above.
- A shared read snapshot with a copy-on-write graph: one reader, and no measurement that the copy now costs more
  than ADR 0023's 0.76 ms.

## What would reopen it

A surface that must re-read a file whose stamp did not change (a "touch" command). A second reader of a session
during an update (ADR 0023's condition).
