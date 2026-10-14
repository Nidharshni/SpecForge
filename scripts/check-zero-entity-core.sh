#!/usr/bin/env bash
set -euo pipefail

# Verify that core crate source directories contain no hardcoded entity kind names.
# Extension code (builtins/) is excluded — it SHOULD reference domain vocabulary.
# Test code is excluded: `tests/` directories, `#[cfg(test)] mod name { ... }`
# blocks, and files declared by `#[cfg(test)] mod name;`. Comments and lines
# marked `// zero-entity-core: exempt` are excluded too.
#
# "type", "property", "channel", "event" are excluded because they collide
# with JSON Schema keywords, LSP protocol terms, and MCP event contexts.

cd "$(dirname "$0")/.."

ENTITY_PATTERN='"behavior"|"feature"|"invariant"|"port"|"decision"|"constraint"|"failure_mode"|"journey"|"deliverable"|"milestone"|"module"|"term"|"persona"|"release"|"condition"|"axiom"|"refinement"|"process"'

CORE_DIRS=(
  crates/specforge-parser/src
  crates/specforge-emitter/src
  crates/specforge-graph/src
  crates/specforge-lsp/src
  crates/specforge-mcp/src
  crates/specforge-coverage/src
  crates/specforge-ops/src
  crates/specforge-ops-registry/src
  crates/specforge-registry-wire/src
  crates/specforge-installed/src
  crates/specforge-package/src
  crates/specforge-shape-macros/src
)

# Files that `#[cfg(test)] mod name;` declares, relative to the declaring file:
# `dir/lib.rs|main.rs|mod.rs` -> `dir/name.rs` or `dir/name/`, `dir/foo.rs` -> `dir/foo/name...`.
test_module_paths() {
  local file="$1"
  awk '
    /^[[:space:]]*#\[cfg\(test\)\]/ { pending = 1; next }
    pending && /^[[:space:]]*#\[/ { next }
    pending {
      if (match($0, /^[[:space:]]*(pub(\([a-z]+\))?[[:space:]]+)?mod[[:space:]]+[A-Za-z0-9_]+[[:space:]]*;/)) {
        line = $0
        sub(/^[[:space:]]*(pub(\([a-z]+\))?[[:space:]]+)?mod[[:space:]]+/, "", line)
        sub(/[[:space:]]*;.*/, "", line)
        print line
      }
      pending = 0
    }
  ' "$file" | while read -r name; do
    local dir base
    dir=$(dirname "$file")
    base=$(basename "$file" .rs)
    case "$base" in
      lib | main | mod) ;;
      *) dir="$dir/$base" ;;
    esac
    echo "$dir/$name.rs"
    echo "$dir/$name/"
  done
}

# Print `file:line:text` for every line outside `#[cfg(test)] mod name { ... }` blocks.
non_test_lines() {
  awk '
    FNR == 1 { pending = 0; skipping = 0; depth = 0 }
    skipping {
      depth += gsub(/\{/, "{") - gsub(/\}/, "}")
      if (depth <= 0) skipping = 0
      next
    }
    /^[[:space:]]*#\[cfg\(test\)\]/ { pending = 1; next }
    pending && /^[[:space:]]*#\[/ { next }
    pending && /^[[:space:]]*(pub(\([a-z]+\))?[[:space:]]+)?mod[[:space:]]+[A-Za-z0-9_]+[[:space:]]*\{/ {
      pending = 0
      depth = gsub(/\{/, "{") - gsub(/\}/, "}")
      skipping = depth > 0
      next
    }
    { pending = 0; print FILENAME ":" FNR ":" $0 }
  ' "$@"
}

EXIT_CODE=0

for dir in "${CORE_DIRS[@]}"; do
  if [ ! -d "$dir" ]; then
    echo "ERROR: $dir is not a directory: update CORE_DIRS when a core crate moves"
    EXIT_CODE=1
    continue
  fi

  files=$(find "$dir" -name '*.rs' -not -path '*/tests/*' -not -path '*/builtins/*' | sort)
  excluded=$(for f in $files; do test_module_paths "$f"; done)
  kept=()
  for f in $files; do
    skip=0
    while IFS= read -r ex; do
      [ -z "$ex" ] && continue
      case "$ex" in
        */) case "$f" in "$ex"*) skip=1 ;; esac ;;
        *) [ "$f" = "$ex" ] && skip=1 ;;
      esac
    done <<<"$excluded"
    [ "$skip" -eq 0 ] && kept+=("$f")
  done
  [ "${#kept[@]}" -eq 0 ] && continue

  hits=$(non_test_lines "${kept[@]}" \
    | grep -E "$ENTITY_PATTERN" \
    | grep -v '// zero-entity-core: exempt' \
    | grep -Ev '^[^:]+:[0-9]+:[[:space:]]*//' \
    || true)

  if [ -n "$hits" ]; then
    echo "ERROR: Hardcoded entity kind names found in $dir:"
    echo "$hits"
    echo
    EXIT_CODE=1
  fi
done

if [ "$EXIT_CODE" -eq 0 ]; then
  echo "OK: No hardcoded entity kind names in core crate sources."
fi

exit "$EXIT_CODE"
