#!/usr/bin/env bash
# Exercises .githooks/pre-commit in a throwaway clone: bad stages must be
# refused, clean ones (and a starter template) accepted. Slow the first time (the clone builds).
set -uo pipefail
ROOT=$(cd "$(dirname "$0")/.." && pwd)
TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT
git clone -q --shared "$ROOT" "$TMP/clone" || exit 1
cd "$TMP/clone" || exit 1
git config core.hooksPath "$ROOT/.githooks"
git config user.email test@example.com
git config user.name test
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$TMP/target}"
failures=0

expect() { # expect refused|accepted LABEL
    local want=$1 label=$2
    if git commit -q -m "test: $label" >"$TMP/out" 2>&1; then got=accepted; else got=refused; fi
    if [ "$got" = "$want" ]; then echo "ok   $label ($got)"; else
        echo "FAIL $label: wanted $want, got $got"; cat "$TMP/out"; failures=$((failures + 1))
    fi
    git reset -q --hard HEAD
}

touch crates/kernel/common/pending.snap.new
git add -f crates/kernel/common/pending.snap.new
expect refused "a staged .snap.new"

printf 'invariant   unformatted_one   "X"   {\n guarantee "g"\n}\n' >spec/zz_unformatted.spec
git add spec/zz_unformatted.spec
expect refused "an unformatted .spec"

f=$(grep -rl 'verify = "' crates/pipeline/registry/tests | head -1)
perl -0pi -e 's/verify = "/verify = "NOT DECLARED /' "$f"
git add "$f"
expect refused "a verify text the spec does not declare"

echo "// touched" >>crates/kernel/common/src/lib.rs
cargo fmt --all
git add crates/kernel/common/src/lib.rs
expect accepted "a clean stage"

printf '\nfn   unformatted_probe( )  {   }\n' >>extensions/governance/src/lib.rs
git add extensions/governance/src/lib.rs
expect refused "an unformatted guest"

echo "// a note" >>extensions/software/src/starter.spec
git add extensions/software/src/starter.spec
expect accepted "a staged starter template"

perl -pi -e 's#extras: \(\$\) => \[/\\s/, \$\.comment\]#extras: (\$) => [/\\s/, /;/, \$.comment]#' \
    crates/kernel/tree-sitter/grammar.js
git add crates/kernel/tree-sitter/grammar.js
expect refused "a stale grammar artifact"

[ "$failures" = 0 ]
