#!/usr/bin/env bash
# Lists merged PRs whose head commit main does not contain: a PR stacked on a
# branch that had already merged lands there and never reaches main.
# Exit 1 when any is found. UNLANDED_MAIN_REF overrides the ref checked (testing).
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"
git fetch -q origin
MAIN=${UNLANDED_MAIN_REF:-origin/main}
found=0
while IFS=$'\t' read -r number base head title; do
    [ -n "$number" ] || continue
    # an unknown head (deleted and gc'd) cannot be judged here
    git cat-file -e "$head^{commit}" 2>/dev/null || continue
    if ! git merge-base --is-ancestor "$head" "$MAIN"; then
        echo "#$number '$title' merged into $base but its head ${head:0:8} is not in $MAIN"
        [ "$base" = main ] || echo "    fix: gh pr create --base main --head $base"
        found=1
    fi
done < <(gh pr list --state merged --limit 100 \
    --json number,baseRefName,headRefOid,title \
    --jq '.[] | [.number, .baseRefName, .headRefOid, .title] | @tsv')
exit $found
