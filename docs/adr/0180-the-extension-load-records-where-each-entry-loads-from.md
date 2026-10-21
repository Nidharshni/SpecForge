# The extension load records where each entry loads from

**Status:** accepted (2026-10-21, architecture round 19, plan 03). Amends [ADR 0028](0028-installed-extensions-are-one-module.md) (D3: each entry's outcome records its origin and whether it loaded; D9: a row's origin is the one the load recorded on its loading entry) and [ADR 0065](0065-every-extension-change-is-one-lock-plan-change.md) (D5: `Origin::of` is `Resolution::origin`, and the inventory reads the recorded origin).

## Context

- `Installed::load` decides, for every `specforge.json` entry, where its extension comes from (`Installed::resolve`) and
  whether it loaded, then records only the failure. `EnabledExtension::failure: None` means "loaded", "repeats an entry
  that was tried" and "nothing was loaded" alike.
- The extension inventory (read by the listing, doctor and remove) therefore resolved the entries a second time, over a
  second fetch of the builtins table, and re-walked the load order to find the entry that loaded an extension.
  `Declarations::loaded_by` re-parsed the entries and guessed a builtin's declaration from the declared name.
- The re-derivation drifted from the load twice (round 7 and round 8) and was fixed in ops each time. It cannot disagree
  today, because it is a pure function of the same inputs; each reader has to know to pass the same three.
- `Origin::of` also serves the lock plan's prediction, which resolves the entries a change leaves, so the rule is moved,
  not deleted.

## Decision

- **D1.** `Origin` and `Origin::source` live in `specforge-installed` beside `Resolution`, and `Resolution::origin(at)`
  answers an entry's origin: a builtin's name `Builtin`, a locked name `Installed { source }`, a `.wasm` file entry
  `File { path }`, anything else `Unknown`, a repeat its first's. `specforge_ops::extension::Origin` and `Origin::of` go.
- **D2.** `EnabledExtension` records `origin` and `loaded`. `origin` never depends on whether the entry loaded. `loaded`
  is true for exactly the entries whose slot loaded; a repeat is `loaded: false` with its first's origin. With no runtime
  every entry is `loaded: false` (`Installed::unloaded(entries, builtins)` replaces `Loaded::unloaded`).
- **D3.** The inventory reads the origin of the entry that loaded an extension, else of the first entry the load tried
  (`EnabledExtension::loader`), and the declaration the registry build holds under the name the load gave.
  `Declarations::loaded_by` takes a builtin's declaration only from an entry whose origin is `Builtin`, a file's only from
  an entry whose origin is `File`.
- **D4.** `EnabledExtension::file` is the origin's: the field goes, `file()` answers `Origin::File`'s path.

## Rejected

- **The declaration on the entry** (a clone, or an `Arc` shared with the registry build). The registry build reorders and
  edits declarations (E055); a copy is a second truth, an `Arc` changes the registry crate's types. The entry records
  *that* it loaded; the declaration stays the registry build's, under the name the load gave (unique by the admit rules).
- `Origin` staying in ops with a parallel enum on the entry: two enums of one fact.
- One `EntryLoad` enum replacing `failure` and `loaded`: 27 reads and 44 constructor sites, deferred, not rejected.
- Keeping the inventory's resolve.

## Consequences

- No output changes (pinned end to end by `each_source_is_where_the_load_took_the_extension`).
- A rootless (in-memory) view's rows read the recorded origin, not `Unknown` for a locked name; no surface lists one.
- Rust: `specforge-installed` gains `Origin`, `Resolution::origin`, `Installed::unloaded`, `EnabledExtension::{origin,
  loaded, loader, file()}` and loses `Loaded::unloaded` and the `file` field; `specforge-ops` loses `extension::Origin`;
  `specforge-project` re-exports `Origin`.

## Reopen when

A reader needs a fact of the load the entry does not carry.
