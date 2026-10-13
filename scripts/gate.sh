#!/usr/bin/env bash
# Local verification gate. CI (.github/workflows/ci.yml) stays the source of
# truth; this only makes the local loop fast.
#
#   scripts/gate.sh          quick: only what the working tree changed
#   scripts/gate.sh full     everything, once per phase
#   scripts/gate.sh baseline lower gate/baseline.json to today's
#                            warning counts (it never raises one)
#
# Full gate: nextest (not `cargo test`) for the workspace, doctests only for
# crates that have one, `specforge collect --no-run` + `analyze` on the
# reports the tests just wrote, the extensions' native tests (their linked
# tests report into target/specforge beside the workspace's, so the dogfood
# records them: a bare `specforge collect` runs only `cargo test
# --workspace`), their wasm and native clippy in one shared target dir,
# workspace clippy, fmt, blob freshness, and
# `specforge check` on every spec corpus. It uses target/debug/specforge (no
# release build). Light steps run alongside the heavy ones; the two heavy
# compiles (nextest build, clippy) never overlap.
#
# `specforge check` runs on each corpus's project, so its extensions load
# (`.` is this repository's own project; `check` run anywhere inside a project
# checks that project). Any error fails. Warnings are ratcheted per code against
# gate/baseline.json: a count above its baseline fails, and so does
# one below it until the baseline is lowered in the same commit.
set -uo pipefail

MODE=${1:-quick}
ROOT=$(cd "$(dirname "$0")/.." && pwd)
cd "$ROOT"
LOGS=target/gate
EXT_TARGET="$ROOT/target/ext"
SPECFORGE=target/debug/specforge
# `format` walks every .spec under a path, so it takes the spec
# directories; `check` takes the projects (their specforge.json).
CORPORA=(spec integrations/rust/spec examples/todo-app examples/shop)
PROJECTS=(. integrations/rust/spec examples/todo-app examples/shop)
BASELINE=gate/baseline.json
mkdir -p "$LOGS"
rm -f "$LOGS"/*.status

# step NAME CMD... : run CMD, log to target/gate/NAME.log, record status+time.
step() {
    local name=$1; shift
    local start=$SECONDS
    "$@" >"$LOGS/$name.log" 2>&1 </dev/null
    local rc=$?
    echo "$rc $((SECONDS - start))" >"$LOGS/$name.status"
    return $rc
}

# Crates with at least one runnable doctest (a ``` fence that isn't
# text/ignore/json/...): rustdoc costs ~15 s per crate even with none.
doctest_packages() {
    local dir
    for dir in crates/* integrations/rust/*; do
        [ -f "$dir/Cargo.toml" ] && [ -d "$dir/src" ] || continue
        if grep -rhE '^\s*//[/!]\s*```' "$dir/src" | awk '
            { sub(/^[ \t]*\/\/[\/!][ \t]*```/, ""); lang = $0 }
            open { open = 0; next }
            { open = 1; if (lang ~ /^(rust|compile_fail|should_panic|no_run)?[ \t]*$/) found = 1 }
            END { exit !found }'; then
            sed -n 's/^name *= *"\(.*\)"/\1/p' "$dir/Cargo.toml" | head -1
        fi
    done
}

extensions_check() {
    local ext status=0
    for ext in "$@"; do
        echo "== $ext"
        # Linked extension tests report beside the workspace's, where the
        # dogfood `collect --no-run` reads them.
        CARGO_TARGET_DIR="$EXT_TARGET" SPECFORGE_REPORT="$ROOT/target/specforge" \
            cargo test -q --manifest-path "extensions/$ext/Cargo.toml" || status=1
        CARGO_TARGET_DIR="$EXT_TARGET" cargo clippy -q --manifest-path "extensions/$ext/Cargo.toml" \
            --target wasm32-wasip2 -- -D warnings || status=1
        CARGO_TARGET_DIR="$EXT_TARGET" cargo clippy -q --manifest-path "extensions/$ext/Cargo.toml" \
            --all-targets -- -D warnings || status=1
    done
    return $status
}

# Same checks as CI: rustfmt on the workspace (extension crates are
# outside it) and the spec corpora through `specforge format --check`.
fmt_check() {
    cargo fmt --all -- --check || return 1
    cargo run -q -p specforge-cli -- format --check "${CORPORA[@]}"
}

# spec_checks [--write]: check every project; fail on any error and on any
# warning count that differs from its baseline. --write lowers the baseline
# to today's counts, and refuses when a count rose or an error remains.
spec_checks() {
    python3 - "$SPECFORGE" "$BASELINE" "${1:-}" "${PROJECTS[@]}" <<'PY'
import collections, json, subprocess, sys
specforge, baseline_path, write = sys.argv[1], sys.argv[2], sys.argv[3] == "--write"
baseline = json.load(open(baseline_path))
counts, failed = {}, False
for project in sys.argv[4:]:
    out = subprocess.run([specforge, "check", "--path", project, "--format", "json"],
                         capture_output=True, text=True)
    try:
        diagnostics = json.loads(out.stdout)
    except ValueError:
        print(f"{project}: `specforge check` printed no JSON\n{out.stdout}{out.stderr}")
        failed = True
        continue
    errors = [d for d in diagnostics if d["severity"] == "Error"]
    warnings = collections.Counter(d["code"] for d in diagnostics if d["severity"] == "Warning")
    counts[project] = dict(sorted(warnings.items()))
    print(f"{project}: {len(errors)} errors, {sum(warnings.values())} warnings")
    for d in errors:
        span = d.get("span") or {}
        print(f"  {d['code']} {span.get('file')}:{span.get('start_line')}: {d['message']}")
    failed |= bool(errors)
    allowed = baseline.get(project, {})
    for code in sorted(set(warnings) | set(allowed)):
        now, was = warnings.get(code, 0), allowed.get(code, 0)
        if now > was:
            print(f"  {code} rose {was} -> {now}: fix the new warnings")
            failed = True
        elif now < was and not write:
            print(f"  {code} fell {was} -> {now}: lower it in {baseline_path}"
                  " in the same commit (scripts/gate.sh baseline)")
            failed = True
if write:
    if failed:
        print(f"{baseline_path} not written: fix the errors and risen counts first")
        sys.exit(1)
    with open(baseline_path, "w") as f:
        json.dump({p: c for p, c in counts.items() if c}, f, indent=2)
        f.write("\n")
    print(f"wrote {baseline_path}")
sys.exit(1 if failed else 0)
PY
}

# Dogfood: record what the nextest run just proved, then require no
# unknown entities (W115), no failing proof (A014), no undeclared
# obligation (A016).
dogfood() {
    "$SPECFORGE" collect --no-run --format json >"$LOGS/collect.json" || return 1
    if grep -q '"W115"' "$LOGS/collect.json"; then
        echo "W115 in collect:"; grep -A2 '"W115"' "$LOGS/collect.json"; return 1
    fi
    "$SPECFORGE" analyze coverage --format json >"$LOGS/analyze.json"
    python3 - "$LOGS/analyze.json" <<'PY'
import json, sys
doc = json.load(open(sys.argv[1]))
bad = [f for p in doc["passes"] for f in p["findings"] if f["code"] in ("A014", "A016")]
for f in bad:
    print(f["code"], f["message"])
funnel = next(p for p in doc["passes"] if p["pass"].endswith(":coverage"))["summary"]["test_results"]
print("obligations proven:", funnel and funnel.get("obligations_proven"))
sys.exit(1 if bad else 0)
PY
}

report() {
    local failed=0 f name rc secs
    printf '\n%-14s %6s  %s\n' STEP TIME RESULT
    for f in "$LOGS"/*.status; do
        [ -e "$f" ] || continue
        name=$(basename "$f" .status)
        read -r rc secs <"$f"
        if [ "$rc" = 0 ]; then
            printf '%-14s %5ss  ok\n' "$name" "$secs"
        else
            printf '%-14s %5ss  FAILED (see %s/%s.log)\n' "$name" "$secs" "$LOGS" "$name"
            [ "$name" = builtins ] && echo "                         re-vendor: cargo run -p xtask --bin revendor"
            failed=1
        fi
    done
    printf '%-14s %5ss\n' total "$SECONDS"
    return $failed
}

run_full() {
    # Light, independent of the workspace build: in the background.
    step fmt fmt_check &
    step builtins cargo run -q -p xtask --bin build-builtins -- --check &
    local exts=()
    for m in extensions/*/Cargo.toml; do exts+=("$(basename "$(dirname "$m")")"); done
    step extensions extensions_check "${exts[@]}" &

    # Heavy compile #1: every test binary plus the CLI.
    step build cargo nextest run --workspace --no-run --cargo-quiet || { wait; report; return 1; }
    # Tests mostly wait on I/O once built, so clippy (heavy compile #2)
    # runs alongside them; doctests and spec checks are light.
    step nextest cargo nextest run --workspace --profile gate &
    step clippy cargo clippy -q --workspace --all-targets -- -D warnings &
    local docs
    docs=$(doctest_packages | sed 's/^/-p /' | tr '\n' ' ')
    if [ -n "$docs" ]; then
        # shellcheck disable=SC2086
        step doctests cargo test -q --doc $docs &
    fi
    step specs spec_checks &
    wait
    if [ "$(cut -d' ' -f1 "$LOGS/nextest.status")" = 0 ]; then
        step dogfood dogfood
    fi
    report
}

run_quick() {
    local changed pkgs=() exts=() specs=0 builtins=0 links=0 file dir name
    changed=$( { git diff --name-only HEAD; git ls-files --others --exclude-standard; } | sort -u)
    [ -n "$changed" ] || { echo "nothing changed"; return 0; }
    for file in $changed; do
        case $file in
            crates/*|integrations/rust/specforge-*|xtask/*)
                dir=$(echo "$file" | cut -d/ -f1-2)
                [[ $file == integrations/* ]] && dir=$(echo "$file" | cut -d/ -f1-3)
                [[ $file == xtask/* ]] && dir=xtask
                name=$(sed -n 's/^name *= *"\(.*\)"/\1/p' "$dir/Cargo.toml" 2>/dev/null | head -1)
                [ -n "$name" ] && pkgs+=("$name")
                [[ $dir == crates/specforge-extension-sdk* ]] && builtins=1
                ;;
            extensions/*)
                exts+=("$(echo "$file" | cut -d/ -f2)"); builtins=1 ;;
        esac
        case $file in
            *.spec|spec/*|examples/*|integrations/rust/spec/*) specs=1 ;;
            # the extensions' rules and the config decide what check reports
            specforge.json|scripts/gate*|extensions/*|crates/specforge-emitter/*) specs=1 ;;
        esac
        # a test's verify text must match its spec entity (A016, statically)
        case $file in
            *.spec|spec/*) links=1 ;;
            *.rs) if [[ $file == tests/* || $file == */tests/* ]] || grep -q 'verify *=' "$file" 2>/dev/null; then links=1; fi ;;
        esac
    done
    # Deduplicate (no mapfile: macOS ships bash 3.2).
    local unique=()
    while IFS= read -r name; do [ -n "$name" ] && unique+=("$name"); done \
        < <(printf '%s\n' "${pkgs[@]+"${pkgs[@]}"}" | sort -u)
    pkgs=("${unique[@]+"${unique[@]}"}")
    unique=()
    while IFS= read -r name; do [ -n "$name" ] && unique+=("$name"); done \
        < <(printf '%s\n' "${exts[@]+"${exts[@]}"}" | sort -u)
    exts=("${unique[@]+"${unique[@]}"}")
    echo "crates: ${pkgs[*]:-none}; extensions: ${exts[*]:-none}; specs: $specs"

    step fmt fmt_check &
    [ "$links" = 1 ] && step links cargo run -q -p xtask --bin check-links &
    [ "$builtins" = 1 ] && step builtins cargo run -q -p xtask --bin build-builtins -- --check &
    [ ${#exts[@]} -gt 0 ] && step extensions extensions_check "${exts[@]}" &
    if [ ${#pkgs[@]} -gt 0 ]; then
        # Build and lint the whole workspace (cached, and the same feature
        # set as the full gate: `-p` would rebuild dependencies with other
        # features), but run only the tests of the changed crates and of
        # the crates that depend on them.
        local filter
        filter=$(printf 'rdeps(=%s) | ' "${pkgs[@]}")
        step nextest cargo nextest run --workspace --profile gate -E "${filter% | }"
        step clippy cargo clippy -q --workspace --all-targets -- -D warnings
        if printf '%s\n' "${pkgs[@]}" | grep -qxF -f <(doctest_packages); then
            # shellcheck disable=SC2046
            step doctests cargo test -q --doc $(printf '%s\n' "${pkgs[@]}" | grep -xF -f <(doctest_packages) | sed 's/^/-p /')
        fi
    fi
    if [ "$specs" = 1 ]; then
        cargo build -q -p specforge-cli && step specs spec_checks
    fi
    wait
    report
}

case $MODE in
    full) run_full ;;
    quick) run_quick ;;
    baseline) cargo build -q -p specforge-cli && spec_checks --write ;;
    *) echo "usage: $0 [quick|full|baseline]" >&2; exit 2 ;;
esac
