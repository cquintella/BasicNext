#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
checker="$repo_root/scripts/check-forbidden-deps.sh"

if [[ ! -x "$checker" ]]; then
  echo "checker is missing or not executable: $checker" >&2
  exit 1
fi

"$checker" --root "$repo_root"

fixture=$(mktemp -d "${TMPDIR:-/tmp}/bn-forbidden-deps.XXXXXX")
trap 'rm -rf "$fixture"' EXIT
mkdir -p "$fixture/src/runtime"
cp "$repo_root/scripts/forbidden-deps.allowlist" "$fixture/allowlist"
cat > "$fixture/src/runtime/illegal.rs" <<'RS'
use crate::{semantic::Type};

pub fn illegal() -> Type {
    Type::Null
}
RS

if "$checker" --root "$fixture" --allowlist "$fixture/allowlist" >/dev/null 2>&1; then
  echo "checker accepted a seeded illegal dependency" >&2
  exit 1
fi

echo "forbidden dependency checker baseline and negative fixture passed"
