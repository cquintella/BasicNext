#!/usr/bin/env bash
#   tests/check-forbidden-deps.sh            # the working tree
#   tests/check-forbidden-deps.sh --cached   # the index (pre-commit hook)
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)

# --cached checks what a commit would hold: the index is exported to a
# scratch tree and this gate, as staged, runs there against the staged
# manifests and lock (its repo_root is the snapshot).
if [[ ${1:-} == --cached ]]; then
  snapshot=$(mktemp -d "${TMPDIR:-/tmp}/bn-forbidden-deps-index.XXXXXX")
  trap 'rm -rf "$snapshot"' EXIT
  git -C "$repo_root" checkout-index --all --prefix="$snapshot/"
  status=0
  (cd "$snapshot" && CARGO_TARGET_DIR="$repo_root/target" bash tests/check-forbidden-deps.sh) \
    || status=$?
  exit "$status"
fi
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
: > "$fixture/allowlist"
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

for probe in 'use bn_frontend::ast::Program;' 'use crate::ast::Program;' 'use crate::module_graph::load;' 'let m = crate::ir::lower_graph(&g, &models);'; do
  cat > "$fixture/src/runtime/illegal.rs" <<RS
$probe
RS
  if "$checker" --root "$fixture" --allowlist "$fixture/allowlist" >/dev/null 2>&1; then
    echo "checker accepted a seeded frontend edge: $probe" >&2
    exit 1
  fi
done
echo "frontend-module spelling and re-lowering negatives passed"

mkdir -p "$fixture/crates/bn_interp/src"
for probe in 'use crate::libraries::web::execute_callback;' 'let s = crate::web::ServerState::new();' 'use tokio::net::TcpListener;' 'use crate::hosts::net::NetProvider;'; do
  cat > "$fixture/crates/bn_interp/src/illegal.rs" <<RS
$probe
RS
  if "$checker" --root "$fixture" --allowlist "$fixture/allowlist" >/dev/null 2>&1; then
    echo "checker accepted a seeded bn_interp→capability edge: $probe" >&2
    exit 1
  fi
done
rm -rf "$fixture/crates"
echo "bn_interp→capability negatives passed"

rm -f "$fixture/src/runtime/illegal.rs"
printf 'src/runtime/gone.rs:1:use crate::semantic::Type;\n' > "$fixture/allowlist"
if "$checker" --root "$fixture" --allowlist "$fixture/allowlist" >/dev/null 2>&1; then
  echo "checker accepted a stale allowlist record (missing file)" >&2
  exit 1
fi
mkdir -p "$fixture/src/runtime" && printf 'fn drifted() {}\n' > "$fixture/src/runtime/drift.rs"
printf 'src/runtime/drift.rs:1:use crate::semantic::Type;\n' > "$fixture/allowlist"
if "$checker" --root "$fixture" --allowlist "$fixture/allowlist" >/dev/null 2>&1; then
  echo "checker accepted a stale allowlist record (line changed)" >&2
  exit 1
fi
: > "$fixture/allowlist"
echo "allowlist rot-detection negatives passed"

mkdir -p "$fixture/src/semantic"
cat > "$fixture/src/semantic/illegal.rs" <<'RS'
use crate::net::Endpoint;
RS

if "$checker" --root "$fixture" --allowlist "$fixture/allowlist" >/dev/null 2>&1; then
  echo "checker accepted a seeded semantic-to-host implementation edge" >&2
  exit 1
fi

echo "semantic host-implementation boundary check passed"


# W5 / GC-DEP: the public IR model names no frontend type. Fails closed if
# the model moves, so the check cannot pass vacuously.
ir_model="$repo_root/crates/bn_ir/src/model.rs"
if [[ ! -f $ir_model ]]; then
  echo "IR model not found at ${ir_model#"$repo_root/"}" >&2
  exit 1
fi
if rg -n 'bn_frontend|module_graph::ModuleId|semantic::' "$ir_model" >/dev/null; then
  echo "IR model still depends on frontend identity definitions" >&2
  exit 1
fi

echo "IR model dependency check passed"

# E.S1.a / AC9: bnc and bn_rt must not link or depend on external BN* library stacks.
# One `cargo tree` per package, from this tree's manifest and lock: it must
# succeed (an error is a failure, never "no such dependency"), and
# `--locked` never rewrites Cargo.lock.
dependencies_of() {
  (cd "$repo_root" && cargo tree --locked -p "$1" --prefix none --format '{p}') \
    | sed 's/ .*//' | sort -u
}
bnc_dependencies=$(dependencies_of bnc) || { echo "cargo tree failed for bnc" >&2; exit 1; }
runtime_dependencies=$(dependencies_of bn_rt) || { echo "cargo tree failed for bn_rt" >&2; exit 1; }
for forbidden in tokio hyper rustls rusqlite libffi libloading bn_core_json bn_stdlib bn_web; do
  if grep -qx "$forbidden" <<< "$bnc_dependencies"; then
    echo "bnc unexpectedly depends on optional library crate: $forbidden" >&2
    exit 1
  fi
  if grep -qx "$forbidden" <<< "$runtime_dependencies"; then
    echo "bn_rt unexpectedly depends on optional library crate: $forbidden" >&2
    exit 1
  fi
done
if grep -qx serde_json <<< "$runtime_dependencies"; then
  echo "bn_rt unexpectedly depends on serde_json" >&2
  exit 1
fi
# Positive sanity check: the listing holds a real dependency.
if ! grep -qx bn_frontend <<< "$bnc_dependencies"; then
  echo "cargo tree listing sanity check failed for bn_frontend" >&2
  exit 1
fi
echo "bnc web-runtime dependency gate (AC9) passed"
