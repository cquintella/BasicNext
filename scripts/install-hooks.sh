#!/usr/bin/env bash
# Installs the repository's git hooks into this clone (worktrees share them).
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."
hooks=$(git rev-parse --git-path hooks)
mkdir -p "$hooks"
install -m 0755 scripts/pre-commit "$hooks/pre-commit"
echo "installed pre-commit into $hooks"
