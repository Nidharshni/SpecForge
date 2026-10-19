# A lock plan writes the specforge.json entries it judged

**Status:** accepted (2026-10-16, architecture round 13, plan 04). Amends [ADR 0028](0028-installed-extensions-are-one-module.md)
D4 (the last step) and [ADR 0065](0065-every-extension-change-is-one-lock-plan-change.md) D1 (`Checked::finish`). Builds on
[ADR 0128](0128-one-project-change-over-held-files.md), whose project change stages the file this decision writes.

A lock plan judged the project peer set on one model of `specforge.json` (the entries it opened with, minus what a
`Disabled` drops, plus what `enable` appended) and its commit wrote another: `config::enable`, `remove_extension` and
`remove_entry` re-read the file by name and re-applied their own copies of those predicates. The copies agree; the
defect is the time between the two reads. Reproduced with the shared binary:

- R2: a `specforge remove` judged on `["@specforge/software"]` while a hand edit made the file
  `["@specforge/software", "@specforge/formal"]` wrote `["@specforge/formal"]`, a set the peer rule refuses (E027).
- R4: a `specforge remove` over a `specforge.json` deleted mid-run answered `removed` and wrote nothing, because a
  missing file was "nothing to drop".

A served MCP session is brought up to date before every call, so an edit between calls is judged (not reproduced);
the window is inside one call or one CLI run.

## Decision

- **D1. One rule.** The `extensions` items a change leaves are the items the plan opened with, without those a
  `Disabled` drops, then the appended entries (`Opened::items_after`). The peer judgement, the dry run, the per-ask
  `appended`, the count and the write all read it. `config.rs` reads and writes `specforge.json` and edits nothing.
- **D2. The plan opens with the config as read.** add and update hand it the `ConfigRead` they read once; remove the
  compile's; init the document it creates. The commit writes that document with the items the change leaves, keeping
  every other key and every non-string item, and writes nothing when the items are unchanged.
- **D3. The file must still hold it.** Before anything is written, a commit reads `specforge.json` through its
  `Held` and refuses `file_changed` (Conflict), writing nothing, unless it parses to the document the plan opened
  with (absent if it opened none). A dry run is not checked. The same rule as rename (`e625990a`), on the parsed
  document because the edit is made on it. The code is `specforge_ops::FILE_CHANGED`, the one code for "a file an
  operation planned on changed", which the project change and rename already answer.

## Rejected

- Comparing bytes: the environment keeps the parsed config only. Comparing only `extensions` and merging: a smaller
  twin of the edit this removes. Overwriting without a check: a hand edit is lost silently. Writing only the string
  items: it drops an E069 item on every add and remove.

## Consequences

- `specforge add`, `remove`, `init` and their MCP tools refuse `file_changed` (MCP `conflict`) when `specforge.json`
  changed while they ran, where they wrote a merge or reported `removed` over a deleted file. A refused plan pins
  nothing.
- `Asked.appended` no longer answers "another writer added it".
- `specforge_ops::config` loses `enable`, `remove_extension`, `remove_entry`, `has_extension`, `edit_extensions`,
  `Enabled` and `entry_name`. `rename::CHANGED` becomes `specforge_ops::FILE_CHANGED`.
- The lock is not checked this way: remove writes the compile's lock back whole by design.

## What would reopen it

A `specforge.json` key the peer rule reads besides `extensions` (a concurrent edit to it is still refused, but the
rule grows); an editor that holds `specforge.json` open (then the buffer, not the disk, is what the file holds).

**Amended by ADR 0138 (2026-10-17):** D1: the per-ask `appended` is gone; an ask is answered `Changes` or `Unchanged`.

**Amended by [ADR 0155](0155-a-project-change-words-its-own-failures.md) (2026-10-19):** D3: the refusal is worded by the project change ("cannot <operation>: specforge.json changed since it was read").
