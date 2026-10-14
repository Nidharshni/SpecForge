# First-Attempt Accuracy Eval (C1-07)

The north-star metric — *first-attempt accuracy rises from ~30% to 70–85%
when the agent works from a validated spec graph* — is a claim until it is
measured. This directory is the minimal apparatus that makes the number
falsifiable.

## Design

- **Corpus**: `tasks.json` — N questions/edits over two fixtures
  (`examples/todo-app` and this repo), each with `expected_entities` the
  agent's first answer must reference and optional `forbidden_entities` it
  must not.
- **Conditions**: every task runs twice with the same model and prompt:
  1. `without-graph` — repo files only.
  2. `with-graph` — the compiled graph exported into context
     (`specforge export --format=context` pasted or attached via the MCP
     server).
- **Scoring**: `score.py` checks containment of expected ids in the first
  delivered output. First attempt only — no retry loops.

## Runbook

1. Pick the model and fix it for the whole eval (record it in the run file).
2. For each task in `tasks.json`, run the agent under each condition and
   save its first delivered answer:

   ```
   evaluation/runs/without-graph.json   { "<task-id>": { "output": "..." }, ... }
   evaluation/runs/with-graph.json      { "<task-id>": { "output": "..." }, ... }
   ```

3. Score:

   ```
   python3 evaluation/score.py --runs evaluation/runs/without-graph.json
   python3 evaluation/score.py --runs evaluation/runs/with-graph.json
   ```

4. Record both accuracies (and the model + date) in `runs/RESULTS.md`.

## Status

Unrecorded. The claim in `docs/vision/north-star.md` stays an unverified
hypothesis until a labeled pair of run files covers all tasks — which is
exactly the point: the number is now checkable by anyone with an API key
and an afternoon.
