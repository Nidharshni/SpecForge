# Round tracking

An architecture round's progress lives in one file, `.planning/architecture-<date>/tracking.json`. It feeds the
round's progress page. Plan writers produce its parts, implementers leave it alone, and the coordinator keeps it
current.

## The plan's block

A plan ends with exactly one fenced `json` block:

```json
{
  "plan": "01", "slug": "…", "title": "…", "card": 1, "strength": "Strong", "lane": "S", "wave": 1,
  "adr": "0177", "amends": ["0101"], "depends_on": [], "blocks": ["06"],
  "tickets": [
    { "id": "T1", "title": "<the commit title>", "kind": "pin", "crates": ["specforge-ops"],
      "files": ["…"], "test": "…", "command": "cargo nextest run …", "defects": ["01-D1"], "breaking": false }
  ],
  "defects": [
    { "id": "01-D1", "summary": "…", "evidence": "path:line", "status": "REPRODUCED", "pin": "T1", "fix": "T4" }
  ],
  "decisions": [ { "id": "U1", "question": "…", "options": ["…"], "recommendation": "…" } ],
  "metrics": { "sites_before": 12, "sites_after": 1, "files_touched": 18, "est_lines_added": 220,
               "est_lines_removed": 310, "new_tests": 9, "churn_commits": 6, "evidence_refs": 54 }
}
```

- `kind` is one of `pin`, `fix`, `refactor`, `feat`, `perf`, `test`, `docs`, `adr`, `chore`.
- `title` is the exact commit subject: the coordinator matches commits to tickets by it.
- A ticket that lands only under one answer carries `"only_if": "U1=b"`.
- `decisions` holds only the user's `U` decisions. A choice the plan settles itself stays in its Decisions prose.
- Defect `status` is `REPRODUCED`, `UNVERIFIED` or `NOT REPRODUCED`, and its id is prefixed with the plan number.
- `sites_before`/`sites_after` count the duplicated rule the plan collapses, by the grep its Evidence names.
  `evidence_refs` counts the distinct `path:line` citations in that section.

## The round's file

The coordinator gathers the blocks into `tracking.json` under the round's fields: `round`, `name`, `date`, `base`,
`branch`, `phase`, `steps` (`[{id, done}]` for base, explore, plan, implement, merge, revendor, gate, pr), `waves`,
`merge_order`, `adrs` (`taken`, `released`), `reconciliations` and `plans`. Each ticket gains `status` (`planned`,
`skipped` for an `only_if` its answer excludes, `done`) and `sha`; each defect gains `state` (`open`, `fixed`); each
decision gains `answer`.

After each plan lands, the coordinator marks its tickets `done` by commit subject, marks the defects its fix tickets
closed `fixed`, and regenerates the page.

## Tools

The planning folder `../SpecForge-planning/tools/` holds them. Copy them into the round's directory:

- `sync.py` marks tickets `done` with their sha by matching the subjects of `git log <base>..<branch>` (both from
  `tracking.json`, so it works from any checkout), and names the ones it could not match.
- `progress.py` writes `progress-data.js` from `tracking.json`. `progress.html` loads that file from disk (no server).
  It shows the stepper, KPIs, the wave graph, per-plan bars, charts, the burn-down and filterable tables.

The loop is: edit `tracking.json` or run `sync.py`, run `python3 progress.py`, then reload `progress.html`.
