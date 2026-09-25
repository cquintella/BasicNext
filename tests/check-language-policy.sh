#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)

fail() {
  echo "language-policy gate: $*" >&2
  exit 1
}

command -v rg >/dev/null 2>&1 || fail "ripgrep (rg) is required"

mapfile_compatible_python_files=()
while IFS= read -r path; do
  mapfile_compatible_python_files+=("$path")
done < <(rg --files "$repo_root/tests" -g '*.py')

if (( ${#mapfile_compatible_python_files[@]} > 0 )); then
  printf '%s\n' "${mapfile_compatible_python_files[@]}" >&2
  fail "Python files are not allowed under tests/"
fi

if rg -n '\bpython(3)?\b.*(unittest|tests/[^[:space:]]*\.py)' \
  "$repo_root/.github/workflows"; then
  fail "active workflows must not invoke Python tests"
fi

if rg -n 'scripts/(differential_runner|support_matrix_report)\.py' \
  "$repo_root/.github" "$repo_root/tests" \
  -g '!check-language-policy.sh'; then
  fail "active tests and workflows must not reference migrated Python support scripts"
fi

echo "language-policy gate: OK"
