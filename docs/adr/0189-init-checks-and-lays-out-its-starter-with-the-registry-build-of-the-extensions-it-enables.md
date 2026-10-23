# Init checks and lays out its starter with the registry build of the extensions it enables

**Status:** accepted (2026-10-21, architecture round 20, plan 06). Extended by [ADR 0226](0226-init-writes-its-starters-values-through-the-value-writer.md) (D4: the starter's values are filled by the value writer and the laid-out starter is held to a parse before it is written). Amends [ADR 0134](0134-add-and-init-ask-their-lock-plan-through-the-names-module.md) D1 (a local file's answer carries its declaration, not just its starter template) and [ADR 0147](0147-a-value-is-written-in-its-fields-one-form.md) D6 (init lays its starter out with the forms of the registry build of the extensions it enables, read before the project exists).

`init` wrote a starter its own first `specforge check` could refuse. It checked the starter's spec ID by length only,
before it knew which kinds it enables, so a directory called `release` with `@specforge/product` got `spec "release"`
and an E013 on the first check while init exited 0. It also laid the starter out with no field forms, where
`format --check` uses the project's: latent today (the builtin templates are canonical), a W159 for any template that
quotes an `identifier` value. The entity-ID rule was copied by hand into `rename` and into init, and the copies had
drifted twice.

## Decision

- **D1. The entity-ID rule is `specforge_registry::refused_id`**, beside E013 and E014: E014 (length) then E013
  (reserved word) over a kind registry. `authoring::entity_id::validate_id` adds the grammar's identifier for an ID
  written as a token (`rename`, the typed `create`).
- **D2. Init builds the registry build of the declarations of the extensions it enables**, in `specforge.json` order
  (builtins from the binary, local files as `names` read them), before anything is written: the build its first `check`
  loads.
- **D3. The starter's spec ID is held to `refused_id` over that build's kinds**, whatever is enabled (the structural
  keywords included). A refused ID is `invalid_name` about the name, suggesting `--name <id>_spec`; nothing is written.
- **D4. The starter's template is the first declared one in that order.** It is laid out with the configuration
  `format`'s `Configs` reads for its directory and that build's field forms, handed to the formatter directly: a place
  lookup cannot find a project that is not on disk yet.

## Rejected

- Exempting the spec block from E013 and E014: it is a node ID in the entity namespace (W060 fires beside a `behavior`
  of the same ID).
- Asking `validate_id` of the spec ID: it would refuse `1demo`, which checks clean.
- Suffixing a reserved ID (`release_spec`): a second branch, and the starter's title would no longer be the project's name.
- Exposing the lock plan's declarations cache: a local install's declaration is not kept there.
- Writing the project, then loading it: init validates everything before any file is written.

## Consequences

- A bad name is refused after the extensions are read; a request with a bad name and a bad extension is refused about
  the extension.
- One extra declaration read per enabled builtin.

## What would reopen it

A starter that needs the compile (its references or rules), not just kinds and forms; a spec block taken out of the
entity namespace.
