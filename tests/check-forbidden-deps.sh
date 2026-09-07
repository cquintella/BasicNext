#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
checker="$repo_root/scripts/check-forbidden-deps.sh"

if [[ ! -x "$checker" ]]; then
  echo "checker is missing or not executable: $checker" >&2
  exit 1
fi

if ! command -v rg >/dev/null 2>&1; then
  echo "ripgrep (rg) is required for forbidden-dependency checks" >&2
  exit 2
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

mkdir -p "$fixture/src/semantic"
cat > "$fixture/src/semantic/illegal.rs" <<'RS'
use crate::net::Endpoint;
RS

if "$checker" --root "$fixture" --allowlist "$fixture/allowlist" >/dev/null 2>&1; then
  echo "checker accepted a seeded semantic-to-host implementation edge" >&2
  exit 1
fi

echo "semantic host-implementation boundary check passed"

if rg -n '^use crate::runtime::Value;' "$repo_root/src/dataframe.rs" >/dev/null; then
  echo "dataframe still depends on runtime::Value" >&2
  exit 1
fi

echo "dataframe/runtime cycle check passed"

if [[ -f "$repo_root/src/ir/model.rs" ]] && rg -n \
  'module_graph::ModuleId|semantic::\{[^}]*\bSymbolId\b|semantic::SymbolId' \
  "$repo_root/src/ir/model.rs" >/dev/null; then
  echo "IR model still depends on frontend identity definitions" >&2
  exit 1
fi

echo "IR model dependency check passed"
