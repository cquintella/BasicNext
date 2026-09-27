#!/usr/bin/env bash
# Construction gate for bucket 0.5.2.1b: keep runtime records positional,
# field references resolved, and LLVM layout metadata-driven. Fails closed.
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)
if (($# == 2)) && [[ $1 == --root ]]; then
  repo_root=$(cd "$2" && pwd -P)
elif (($# != 0)); then
  echo "usage: $0 [--root ROOT]" >&2
  exit 2
fi

command -v rg >/dev/null 2>&1 || {
  echo "check-value-layout: ripgrep (rg) is required; refusing to pass without a scan" >&2
  exit 2
}

scan_root() {
  local root=$1
  local value="$root/crates/bn_value/src/lib.rs"
  local value_manifest="$root/crates/bn_value/Cargo.toml"
  local ir_model="$root/crates/bn_ir/src/model.rs"
  local llvm_layout="$root/crates/bn_llvm/src/llvm/layout.rs"
  local runtime="$root/crates/bn_interp/src"

  for required in "$value" "$value_manifest" "$ir_model" "$llvm_layout" "$runtime"; do
    [[ -e $required ]] || {
      echo "check-value-layout: required source is missing: ${required#"$root/"}" >&2
      return 1
    }
  done

  if rg -n 'HashMap[[:space:]]*<[[:space:]]*String[[:space:]]*,[[:space:]]*Value[[:space:]]*>' \
    "$value" "$runtime"; then
    echo "check-value-layout: string-keyed runtime record storage returned" >&2
    return 1
  fi
  rg -q 'fields:[[:space:]]*Box<\[Value\]>' "$value" || {
    echo "check-value-layout: RecordValue no longer owns boxed positional fields" >&2
    return 1
  }
  rg -q 'Record[[:space:]]*\{[[:space:]]*record:[[:space:]]*RecordValue[[:space:]]*\}' "$value" || {
    echo "check-value-layout: Value::Record no longer uses RecordValue" >&2
    return 1
  }
  rg -q 'field:[[:space:]]*Option<FieldRef>' "$ir_model" || {
    echo "check-value-layout: member instructions lost resolved FieldRef storage" >&2
    return 1
  }
  rg -q 'fields:[[:space:]]*Option<Vec<FieldRef>>' "$ir_model" || {
    echo "check-value-layout: field-path instructions lost resolved FieldRefs" >&2
    return 1
  }
  if rg -n 'Instruction::(FieldInit|Default)' "$llvm_layout"; then
    echo "check-value-layout: LLVM inferred layout by scanning initialization instructions" >&2
    return 1
  fi
  if rg -n 'bn_ir|bn_frontend' "$value_manifest" "$value"; then
    echo "check-value-layout: bn_value depends on IR or frontend" >&2
    return 1
  fi
}

scan_root "$repo_root"

# Prove the forbidden-shape scan is live using a disposable source snapshot.
fixture=$(mktemp -d "${TMPDIR:-/tmp}/bn-value-layout.XXXXXX")
trap 'rm -rf "$fixture"' EXIT
mkdir -p "$fixture/crates/bn_value/src" "$fixture/crates/bn_value" \
  "$fixture/crates/bn_ir/src" "$fixture/crates/bn_llvm/src/llvm" \
  "$fixture/crates/bn_interp/src"
cp "$repo_root/crates/bn_value/src/lib.rs" "$fixture/crates/bn_value/src/lib.rs"
cp "$repo_root/crates/bn_value/Cargo.toml" "$fixture/crates/bn_value/Cargo.toml"
cp "$repo_root/crates/bn_ir/src/model.rs" "$fixture/crates/bn_ir/src/model.rs"
cp "$repo_root/crates/bn_llvm/src/llvm/layout.rs" "$fixture/crates/bn_llvm/src/llvm/layout.rs"
cp "$repo_root/crates/bn_interp/src/lib.rs" "$fixture/crates/bn_interp/src/lib.rs"
printf '\ntype ForbiddenRecord = HashMap<String, Value>;\n' \
  >> "$fixture/crates/bn_value/src/lib.rs"
if scan_root "$fixture" >/dev/null 2>&1; then
  echo "check-value-layout: accepted seeded string-keyed record storage" >&2
  exit 1
fi

echo "value/layout construction gate passed (including seeded negative fixture)"
