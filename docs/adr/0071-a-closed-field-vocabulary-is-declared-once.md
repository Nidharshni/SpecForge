# A closed field vocabulary is declared once

**Status:** accepted (2026-10-13, architecture round 9, plan 01). Amends ADR 0011 (the product surface's closed
enums), ADR 0020 D5 (its example) and ADR 0034 D4 (what a declared default means).

The builtins declared every closed set of field values (a feature's `status`, a failure mode's `severity`, a
property's `property_type`) as a `string` field and copied the values by hand into the field's description, a
`one_of` warning rule and its message, the list filter, the filter's help, the lifecycle pass's defaults and the
diagnostic catalog. The copies drifted: persona `technical_level`, channel `interaction_model` and release `status`
had closed list filters and no rule, so `check` admitted values the lists refused, and this repository's own spec
held one. The host already read a closed vocabulary (`enum`: E061, completion, the schema's `enum`, the guide's
example), and no builtin used it.

## Decisions

- **D1. A closed vocabulary is one constant.** `specforge_extension_sdk::Vocabulary` holds the values in order and the
  value an absent field counts as; its `const fn`s check it when it is evaluated, and `member` names a value so a
  misspelt one does not compile. `FieldBuilder::vocabulary` declares the field an `enum` of it, with the absent value
  as `default_value`; it is the only way to declare an enum (`field_type(Enum)` panics; `enum_values` and
  `default_value` left the builder). The guest wire is unchanged.
- **D2. A value outside a vocabulary is E061.** The per-vocabulary warnings (W050, W051, W052, W077, W078, W079, W080,
  W083, W084, W085, W095, W121, W125) are retired to E061. A warning-level enum would need a second admission rule
  and would break ADR 0064's "a value E061 admits is a value the published schema admits".
- **D3. A declared `default_value` is what an absent value counts as.** The host shows it (schema, model, guide,
  hover) and never writes it into the graph. An enum without values, or a default outside its values, is W021.
- **D4. The extension's readers read the constant**: list filters (`one_of` its values, the absent value matching an
  entity without the field), their help, sorts, lifecycle passes, rollups and conditional rules' values.
- **D5. `{allowed}` is a `one_of` constraint's values** on every check that has one.
- **D6. An open set of standard values is not an enum**: the field is a string, and an info rule names the standard
  values through `{allowed}` (product's I062 over `ModuleFamily`).
- **D7. Product's `technical_level`, `interaction_model` and release `status` are closed.** Governance's
  `constraint.priority` is governance's own vocabulary; no builtin rule names a `target_extension` any more.

## Rejected

- **Keeping the warnings with derived rules**: one constant, but the host's enum features would never reach the
  builtins, and the published schema would still type the fields as free strings.
- **Guest-local constants without an SDK type**: three extensions would repeat the type, and the enum declaration
  could disagree with the constant.
- **Deriving the list filters from the declaration at run time**: the declaration is being built when the commands are
  declared.
- **FMEA ratings as integers** (governance's old types): 41 failure modes, the docs and the declaration use the words.

## Consequences

- User-visible: `check` exits 1 on a value outside a builtin vocabulary (E061, naming the closest value) where it
  warned or said nothing; `specforge explain W077` (and the twelve others) says it is retired to E061; the schema,
  MCP `specforge.schema`, the published JSON Schema and the model list these fields' values and the status defaults;
  the editor completes them; the guide's fields carry `values` and `default`; the product list commands' help no
  longer retypes the values.
- The schema cache sees `FieldTypeChanged` (string → enum) on the builtin vocabulary fields: one major bump of a
  project's cached schema version.
- Product declares 50 rules, governance none, formal 12.

## What would reopen it

- A vocabulary a project extends with its own values: the vocabulary would then be read from configuration, and the
  enum declared per project.
- Status transitions (W087–W091, W094): they would attach to the vocabulary that defines the states.
