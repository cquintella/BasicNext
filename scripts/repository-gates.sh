#!/usr/bin/env bash
# The one list of repository gates that read the real tree: CI's `quality`
# job and `scripts/test-battery.sh --gates` both run this file, so a gate
# that only CI ran (check-shared-cores.sh, 2026-10-29) cannot reappear.
# Each gate fails closed (a missing `rg` is a failure).
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."

REPOSITORY_GATES=(
    tests/check-forbidden-deps.sh
    tests/check-licenses.sh
    tests/check-shared-cores.sh
    tests/check-value-layout.sh
)

for gate in "${REPOSITORY_GATES[@]}"; do
    echo "== $gate"
    bash "$gate"
done
echo "== repository gates passed"
