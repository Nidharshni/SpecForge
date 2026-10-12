# Setup diagnostics carry their part; doctor reads no code table

**Status:** accepted (2026-10-12). Amends ADR 0012 D11 (the environment's order), ADR 0028's 2026-10-11
amendment (doctor's load findings), ADR 0029 D3a (doctor's verdict), ADR 0024 D7 (failure kinds).

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
- **S5. A failure's kind is stated where it is raised.** `OpErrorKind::of_diagnostic` repeated the
  pattern for failure kinds: a code it did not list was `internal_error`, which every trust refusal, an
  unpublished version, an empty package and an unusable credential were. The table, `OpError::diagnostic`,
  `From<Diagnostic> for OpError`, `ErrorCode::for_diagnostic` and `McpError::from_diagnostic` are gone: an
  operation raises `OpError::coded(kind, code, ..)`; a lower crate's diagnostic crosses with
  `OpError::of(kind, diagnostic)`; a `RegistryError` is mapped by variant (`specforge_ops_registry::failure`)
  and a `PublishError` by its two cases; the trust check's refusals (R-TRUST-001..006) are
  `permission_denied` (amends ADR 0024 D7).

## Rejected

- Completing doctor's tables: the next setup code is dropped again.
- Exposing the build's four stage lists: a stage is not a part (E039 shares the surfaces' list with E055).
- Folding I002 into "load": every project without extensions would have a load finding.
- Doctor's verdict = check's: doctor would fail on source errors, which are not setup.
- A failure kind per catalog entry: about seventy source codes are never an operation's failure, and one
  code (E027) is raised with different kinds.

## Consequences

- `specforge doctor` and `specforge.doctor` list every setup diagnostic; `findings[].about` gains
  `structural_only`, `providers`, `declaration`, `registry`, `rule`, `surface`; E026 is a `conflict` with its
  `keyword`; `status` is `error`, `warn` or `info`; load findings come in entry order.
- `check` lists an entry's W153/W138 right after that entry, before the next entry's diagnostics.
- MCP: R-TRUST-001..006 are `permission_denied` (R-TRUST-004 was `schema_mismatch`), R006 and R-RES-002
  `extension_not_found`, unusable credentials (R010, R011, R012, R-AUTH-020/021) `precondition_failed`, a
  declaration refused at publish `invalid_input`, a missing local file `file_not_found`.
- The catalog test no longer counts core entries; the `catalog!` macro is the guarantee.

**What would reopen it.** A diagnostic about two parts at once (then a part set), or a reader that needs a
part finer than these (then a sub-part, never a code table).
