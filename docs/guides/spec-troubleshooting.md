# SpecForge Troubleshooting

A symptom-first reference for the diagnostics you'll hit most while authoring `.spec` files.
Find the code or symptom, read the cause, apply the fix. For a guided introduction see the
**[tutorial](authoring-spec-files.md)**; for the complete diagnostic catalog see
**[entity-model.md](../entity-model.md)**.

> 💡 `specforge explain <CODE>` prints a description of any diagnostic. The compiler reports
> **all** diagnostics at once — fix them in batches and re-run.

---

## Reading a diagnostic

```
[E003] Error: unresolved reference 'Tsak' in entity 'create_task'
   ╭─[ tasks.spec ]
   │  types    [Tsak]
   │           ──┬─
   │             ╰── 'Tsak' does not resolve to any entity
```

- **Code** (`E003`) — look it up below or run `specforge explain E003`.
- **Severity** — `E` error (blocks), `W` warning (doesn't block), `I` info (advice).
- **Message** — what's wrong.
- **Span** — which file and line.

---

## Errors (must fix)

| Code | Symptom | Cause | Fix |
|------|---------|-------|-----|
| **E001** | a parse / syntax error | A `.spec` file could not be parsed (missing brace, unclosed string, invalid field syntax). | Fix the syntax at the reported span. |
| **E003** | `unresolved reference 'X' in entity 'Y'` | A reference list points at an entity that doesn't exist (often a typo or a missing `use`). | Correct the name, or `use` the file that defines it. Check you used an unquoted reference, not a string. |
| **E006** | `entity 'X' is missing required field 'F'` | A required field is absent (e.g. `constraint` without `description`, `feature` without `problem`, `persona`/`channel` without `description`, `journey` without `flow`, `term` without `definition`). | Add the required field. |
| **E022** | `reference 'X' in field 'F' of 'Y' targets a 'A', but this field expects 'B'` | A reference points to the wrong *kind* of entity (e.g. `term.see_also` pointing at a behavior — it only accepts terms). | Point the field at an entity of the expected kind. |
| **E024** | `unknown entity kind 'K' for entity 'X'` | You used a keyword from an extension that isn't installed. | `specforge add @specforge/<ext>`, or fix the keyword. |
| **W061** | reference cycle | A reference chain forms a cycle (A → B → A). | Break the cycle — references must form a DAG. |

---

## Warnings (likely problems)

These don't block compilation, but most indicate a missing edge or coverage gap. Use
`specforge check --strict` to treat them as errors in CI.

| Code | Symptom | Cause | Fix |
|------|---------|-------|-----|
| **W001** | `behavior 'X' does not implement any feature` | The behavior has no `features [...]` edge. | Add `features [some_feature]` (or accept it if intentional during early authoring). |
| **W002** | `type 'X' is not referenced by any behavior, port, or type` | Unreferenced type. | Reference it from a behavior/port, or remove it. |
| **W003** (software) | `invariant 'X' is not enforced by any behavior` | No behavior lists this invariant. | Add the invariant to an enforcing behavior's `invariants [...]`. |
| **W113** (import) | `circular import detected: a.spec -> b.spec` | A `use` chain forms a cycle. | Break the cycle, or extract shared entities into a third file both import. |
| **W005** | `port 'X' is not referenced by any behavior` | Unreferenced port. | Add it to a behavior's `ports [...]`, or remove it. |
| **W006** | `behavior 'X' has no category` | Missing `category` (agents use it for task routing). | Add `category command` / `query` / `handler` / etc. |
| **W007** | `event 'X' is not produced by any behavior` | Unreferenced event. | Add it to a behavior's `produces [...]`. |
| **W008** | `feature 'X' is not implemented by any behavior` | No behavior points up to the feature. | Add `features [X]` to an implementing behavior. |
| **W041 / W042 / W044** | `<feature/journey/module> 'X' has no incoming edges` | Product entity isn't referenced by anything upstream. | Reference it from a journey/milestone/deliverable, as appropriate. |
| **W045 / W092** | `<feature/release> dependency cycle detected` | Cyclic `depends_on`. | Break the cycle. |
| **W043 / W046** | `deliverable 'X' supports no journeys` / `contains no modules` | The deliverable lists no journeys / no modules. | Add `journeys [...]` / `modules [...]`. |
| **W049** | `milestone 'X' has no features — it may be empty` | The milestone has no `features` field (modules don't count). | Add `features [...]`. |
| **W051 / W052 / W121** | `failure_mode 'X' has invalid severity/occurrence/detection` | A FMEA score used a number or unknown word. | Use the enum words (severity: critical/high/medium/low; occurrence: certain/likely/occasional/unlikely/rare; detection: certain/likely/moderate/unlikely/undetectable). |
| **W077 / W079 / W085** | `<feature/milestone/deliverable> 'X' has invalid status` | A status value outside the allowed enum. | Use a valid status (feature: proposed/accepted/in_progress/done/deferred/deprecated; milestone: planned/in_progress/completed/blocked; deliverable: draft/in_progress/shipped/deprecated). |
| **W078** | `<entity> 'X' has invalid priority` | Priority outside the enum. | Use critical / high / medium / low. |
| **W080** | `deliverable 'X' has invalid artifact_type` | Unknown artifact type. | Use one of: cli, service, library, web_app, mobile_app, api, extension, documentation, package. |
| **W083 / W084** | `<persona/channel> 'X' has invalid status` | Status outside the enum. | Use active / deprecated. |
| **W093** | `release 'X' has invalid version format` | `version` is not valid semver. | Use a semver string like `1.0.0`, `2.1.0-rc.1`, or `1.0.0+build.5`. |
| **W095** | `feature 'X' has invalid effort` | Effort outside the enum. | Use xs / s / m / l / xl. |

---

## Info / advice

Info-level advice is always reported; `specforge check --severity info` shows only it. These
are suggestions, not problems.

| Code | Meaning |
|------|---------|
| **I010** | A `term` has no edges — it may be unreferenced. |
| **I046 / I047** | A `persona` / `channel` has no incoming journey edges. |
| **I059 / I060 / I066 / I069 / I070 / I089** | An entity has a `deferred`/`blocked`/`deprecated`/`recalled` status but no `reason`/`blockers` explaining it. |
| **I057** | A `blocked` milestone has no `depends_on`. |
| **I048 / I050 / I054 / I055 / I067 / I082 / I083** | A feature without acceptance criteria, a journey with `flow []`, without a persona or channels, a module without features, a release without deliverables or milestones. |
| **I053 / I086 / I087** | A milestone `target_date`, release `release_date` or milestone `start_date` not in YYYY-MM-DD shape. |
| **I061 / I062 / I068** | A deliverable `version` that isn't semver, a module `family` outside core/platform/extension/integration/advisory, a tag that isn't lowercase-hyphenated. |
| **I080 / I081** | A feature, milestone, deliverable or release without `owner`; a feature without `effort`. |

---

## The validation workflow

```bash
specforge check                  # errors, warnings and info-level advice
specforge check --strict         # promote warnings to errors (CI gate)
specforge check --severity error # show only errors (the exit code still counts everything)
specforge check --severity info  # show only the info-level advice
specforge check --format=json    # machine-readable diagnostics
specforge check --cache          # on success, record statuses in specforge-cache.json

specforge explain W008           # describe any diagnostic code
specforge trace <id>             # see an entity's full traceability chain
specforge query <id> --depth 2   # inspect an entity's neighborhood
specforge model                  # render the logical data model (what kinds/fields exist)
specforge format --check         # verify formatting (exit 1 if unformatted)
specforge format --check --format json  # the same verdict, as the document specforge.format answers
```

**Colour.** `check` and `analyze` colour-code diagnostics by severity (errors red,
warnings yellow, info blue) only when their output goes to a terminal. Piped or
redirected output, and `--format json`, is always plain. `NO_COLOR` set to
any non-empty value turns colour off. `CLICOLOR_FORCE` set to a non-empty value other
than `0` turns it on even when the output is not a terminal. When both are set,
`NO_COLOR` wins.

**Common workflow when stuck:**

1. `specforge check` — read every diagnostic, top to bottom.
2. Fix errors first (they block); then warnings, top down.
3. For an unreferenced warning, decide: add the missing edge, or delete the dead entity.
4. For an enum/required-field error, consult the [tutorial entity tour](authoring-spec-files.md#act-iii--the-entity-tour) or [quick reference](../quick-reference.md) for valid values.
5. Re-run until clean. Use `--strict` to confirm CI will pass.

---

**See also:** [tutorial](authoring-spec-files.md) · [cookbook](spec-cookbook.md) ·
[best practices](spec-best-practices.md) · [entity-model.md](../entity-model.md) (full
diagnostic catalog).

## Formatter on broken files

`specforge format` preserves unparsable regions verbatim instead of giving up:
the formatter tracks error regions from the parser's recovery and only
rewrites text it could parse. You can run it on a file with syntax errors —
the broken part comes out unchanged, the valid parts come out formatted.

`specforge format` exits 1 for such a file (W142 names the lines it kept), in every mode, as it
does for a file it cannot read; it still formats and writes the rest of the file. A check passes only
when every file is read and in canonical form.
