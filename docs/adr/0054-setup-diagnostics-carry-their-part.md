# Setup diagnostics carry their part; doctor reads no code table

**Status:** accepted (2026-10-12). Amends ADR 0012 D11 (the environment's order), ADR 0028's 2026-10-11
amendment (doctor's load findings), ADR 0029 D3a (doctor's verdict). The failure-kind half (S5, below) is
pending.

`specforge doctor` decided what a setup problem is from four code tables (CONFLICT, SHADOWING, CONFIG,
PEER). A diagnostic outside them was dropped: two extensions declaring one command (E039), an MCP tool
schema that is not an object (E055) and a malformed `ext_short` (E030) failed `check` while doctor said
`ok: true` with no finding, and twelve setup warnings never appeared. The environment knew which part of
the setup each diagnostic was about and flattened it; the tables guessed it back from codes. Ten commits in
twelve days edited them.

## Decisions

- **S1. A setup diagnostic says what it is about where it is raised.** The registry build tags each
  diagnostic with a `BuildPart` (declaration, peers, registries, rules, surfaces, collision); the extension
  load keeps an entry's diagnostics on the entry (`EnabledExtension::warnings`, `LoadFailure`) and orders
  them (`Installed::load_diagnostics`); providers tag W118 and E057. `Environment::setup_diagnostics()`
  returns every diagnostic `check` reports before any source with its `SetupPart`, in `check`'s order;
  `diagnostics()` is that list without the parts.
- **S2. Collision is a part.** E026, W018, W023, E039 and E057 are about two contributions claiming one
  name; doctor's `conflicts` is that part.
- **S3. An entry's load diagnostics come together.** Entry by entry: its failure, or W149 then W153/W138;
  the lock's E033 once before the first entry it left unloaded (amends ADR 0012 D11).
- **S4. Doctor makes one finding per setup diagnostic**, at its severity (error, warn, info), about its
  part, plus its own disk checks (config file, lock, binaries) and E013 read from the report by its one
  code. A module-health load failure and the lock's E033 are their disk findings when there is a root. Its
  verdict, no error-level finding, therefore fails whenever `check` fails on a setup diagnostic.
- **S5. A failure's kind is stated where it is raised (pending).** `OpErrorKind::of_diagnostic` repeats the
  pattern for failure kinds: a code it does not list is `internal_error`, which every trust refusal, an
  unpublished version, an empty package and an unusable credential are. The decision is to delete the table
  and state the kind at each raise site (`OpError::coded`) and each boundary a lower crate's diagnostic
  crosses (`OpError::of`). It is not built yet: it changes constructors in files other plans still edit, and
  lands last. Until it does, ADR 0024 D7 stands as written.

## Rejected

- Completing doctor's tables: the next setup code is dropped again.
- Exposing the build's four stage lists: a stage is not a part (E039 shares the surfaces' list with E055).
- Folding I002 into "load": every project without extensions would have a load finding.
- Doctor's verdict = check's: doctor would fail on source errors, which are not setup.

## Consequences

- `specforge doctor` and `specforge.doctor` list every setup diagnostic; `findings[].about` gains
  `structural_only`, `providers`, `declaration`, `registry`, `rule`, `surface`; E026 is a `conflict` with its
  `keyword`; `status` is `error`, `warn` or `info`; load findings come in entry order.
- `check` lists an entry's W153/W138 right after that entry, before the next entry's diagnostics.
- The catalog test no longer counts core entries; the `catalog!` macro is the guarantee.

**What would reopen it.** A diagnostic about two parts at once (then a part set), or a reader that needs a
part finer than these (then a sub-part, never a code table).
