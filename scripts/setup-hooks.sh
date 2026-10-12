#!/usr/bin/env bash
# Use the tracked hooks in .githooks/ (once per clone; worktrees share the config).
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"
git config core.hooksPath .githooks
echo "core.hooksPath = $(git config core.hooksPath)"
