# Tracing Rust tests to spec entities

This guide links a Rust project's tests to the entities they prove, so
`specforge analyze coverage` can tell which `verify` obligations are backed by
a passing test, which by a failing one, and which by nothing.

It takes three pieces:

- **`@specforge/testing`** gives kinds like `behavior` and `invariant` their
  `verify` obligations.
- **`specforge-test`** provides the `#[specforge_test]` attribute and records
  every annotated test's result. It's optional: plain `#[test]` functions
  can be linked by their names instead (see
  [Without the attribute](#without-the-attribute)).
- **`@specforge/cargo-test`** is the runner extension: `specforge collect`
  runs `cargo test` for it and maps the results onto the spec
  ([ADR 0002](../adr/0002-test-runner-extensions.md)).

## 1. Enable the extensions

In a project whose `Cargo.toml` sits at the root, `specforge init` enables all
three when you ask for `@specforge/software`:

```bash
specforge init --extensions @specforge/software
```

In an existing project:

```bash
specforge add @specforge/cargo-test   # also enables @specforge/testing
```

## 2. Declare the obligations

```spec
behavior create_user "Create User" {
  contract "..."
  verify unit "rejects a duplicate email"
  verify unit "stores a hashed password"
}
```

## 3. Annotate the tests

Add the crate as a dev-dependency. It isn't published to crates.io yet, so
take it from the SpecForge repository:

```toml
[dev-dependencies]
specforge-test = { git = "https://github.com/leaderiop/SpecForge" }
```

Then annotate each test with the entity it proves and, optionally, the
obligation:

```rust
use specforge_test::prelude::*;

#[specforge_test(behavior = "create_user", verify = "rejects a duplicate email")]
fn rejects_a_duplicate_email() {
    // ...
}
```

**Don't also write `#[test]`.** The attribute registers the test itself.

- A `#[test]` *below* `#[specforge_test]` is a compile error that points at
  the line to delete.
- A `#[test]` *above* it can't be seen by the macro (the compiler expands it
  first), so the test would be registered twice. The second run fails with
  "is registered twice: remove its #[test]".

The attribute takes any entity kind as its first argument (`behavior`,
`invariant`, `type`, `constraint`, ...) and the obligation's text as
`verify`. It works with the attributes you already use:

| Arrangement | What happens |
|---|---|
| `#[specforge_test(...)]` alone | Registered as a test. |
| two `#[specforge_test(...)]` on one function | One test, one recorded result per attribute: a test can prove several entities. |
| `#[specforge_test(...)]` above `#[tokio::test]`, `#[rstest]` or `#[test_case]` | That attribute registers the test; the macro only records it. |
| with `#[should_panic]` | A panic records `pass`; no panic records `fail`. |
| with `#[ignore]` | Ignored as usual: nothing runs and nothing is recorded. `cargo test -- --ignored` (or `--include-ignored`) runs it and records its result. |

### Without the attribute

Plain `#[test]` functions can be linked by their names, with no dependency
at all. `specforge collect` reads the test results `cargo test` prints and
links a test the attribute didn't record when:

- **its name is `<entity_id>__<obligation>`** (two underscores):

  ```rust
  #[test]
  #[allow(non_snake_case)]
  fn create_user__rejects_a_duplicate_email() { /* ... */ }
  ```

- **or it sits in a module named after the entity** (the innermost one wins),
  which suits generated tests such as proptest's:

  ```rust
  mod create_user {
      #[test]
      fn rejects_a_duplicate_email() { /* ... */ }
  }
  ```

The part after the entity proves the obligation whose slug it is: the
obligation's text lowercased, with spaces as underscores, `<`, `>`, `<=` and
`>=` as `lt`, `gt`, `lte` and `gte`, and other punctuation dropped
(`"p99 < 200ms"` becomes `p99_lt_200ms`). A name that matches no obligation
(`create_user__works`) links the test to the entity without proving a
specific obligation, so A015 still lists what's unproven. `collect` says how
many tests it linked this way, and a test named `a__b__c` when both `a` and
`a__b` are entities is W137 and stays unlinked.

The attribute still wins where both apply, and it's the sturdier choice:
a renamed obligation shows up as A016 instead of the test quietly no longer
proving it.

## 4. Collect and analyze

```bash
specforge collect            # runs `cargo test --workspace --no-fail-fast`, records results
specforge analyze coverage   # reads specforge-report.json
```

The first `specforge collect` shows the command and asks you to approve it.
The approval is remembered per project and command, in
`~/.specforge/collector-consent.json` (never in the repository). Failing tests
don't make `collect` fail: they're recorded, and `analyze` reports them
(A014).

`analyze` then lists, per entity, the obligations no passing test names
(A015). A test proves an obligation only by naming its exact text in
`verify = "..."`; a test that names an obligation the entity doesn't declare,
usually a typo or a reworded statement, is A016.

- **Generated files:** `specforge-report.json` and `.specforge/` are
  generated. `specforge init` adds them to `.gitignore`.
- **CI:** there's no terminal to ask, so pass `--yes`, or run `cargo test`
  yourself and then `specforge collect --no-run` to record the report it wrote.
  To keep tests linked by name, save the output where `collect` keeps it:
  `cargo test --workspace --no-fail-fast | tee target/specforge/cargo-test.stdout.txt`
  (after `mkdir -p target/specforge`), or pass the saved output with
  `specforge collect --report out.txt`.
- **Custom target directory:** `collect` sets `SPECFORGE_REPORT`, and
  `specforge-test` writes its per-binary reports there, wherever
  `CARGO_TARGET_DIR` points. A plain `cargo test` writes them to
  `specforge/` in the target directory (`$CARGO_TARGET_DIR/specforge/`);
  record them with `specforge collect --report "$CARGO_TARGET_DIR/specforge"`.
- **Another command** (for example `cargo nextest run`): run it yourself, then
  `specforge collect --no-run`. The reports land in `specforge/` in the
  target directory, `target/specforge/` by default (with a custom one, use
  `--report` as above).
  Under nextest, which runs each test in its own process, every test writes
  its own report, and a run replaces the previous run's reports. Linking by
  name reads `cargo test`'s output format, so it doesn't apply to nextest
  runs.

## Troubleshooting

- **Stray `<crate>/target/specforge/` directories in the source tree:** older
  versions wrote a report into the working directory when the target
  directory had no `target` path component. This lists the leftovers (none
  holds cargo's `CACHEDIR.TAG`); they are gitignored and safe to delete:
  ```sh
  find crates integrations xtask extensions examples -maxdepth 3 -type d -name target -prune \
    -exec sh -c 'test -e "$1/CACHEDIR.TAG" || echo "$1"' _ {} \;
  ```

- **`W115 ... reported tests for unknown entity 'x'`:** a test names an entity
  no spec declares, usually after a rename. Fix the annotation.
- **`A016 tests name obligation(s) ... does not declare`:** the test's
  `verify` text differs from the spec's statement. Make them match exactly.
- **`E059 ... needs approval for this project`:** you ran `collect` without a terminal.
  Use `--yes` or `--no-run`.
- **`E045 ... produced no report`:** the tests didn't build, so `cargo test`
  printed no results. The runner's output above the error says why.
- **A test named by convention isn't linked:** check the entity ID's
  spelling and the double underscore; a test the name can't link is left
  out without a warning.
