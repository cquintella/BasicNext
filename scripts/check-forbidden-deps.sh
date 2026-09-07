#!/usr/bin/env bash
set -euo pipefail

usage() {
  echo "usage: $0 [--root ROOT] [--allowlist FILE]" >&2
}

repo_root=$(pwd -P)
allowlist=""
while (($# > 0)); do
  case "$1" in
    --root)
      [[ $# -ge 2 ]] || { usage; exit 2; }
      repo_root=$(cd "$2" && pwd -P)
      shift 2
      ;;
    --allowlist)
      [[ $# -ge 2 ]] || { usage; exit 2; }
      allowlist=$2
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      usage
      exit 2
      ;;
  esac
done

if [[ -z "$allowlist" ]]; then
  allowlist="$repo_root/scripts/forbidden-deps.allowlist"
fi
[[ -f "$allowlist" ]] || { echo "missing allowlist: $allowlist" >&2; exit 2; }

# Fail closed: without ripgrep the scans are empty and look like a clean tree.
if ! command -v rg >/dev/null 2>&1; then
  echo "ripgrep (rg) is required for forbidden-dependency checks" >&2
  exit 2
fi

declare -a backend_paths=(
  src/runtime_impl.rs src/runtime src/heap.rs src/dispatch.rs src/dispatch
  src/net.rs src/net src/http.rs src/web.rs src/web src/web_state.rs
  src/dataframe.rs src/llvm.rs src/llvm crates/bn_rt/src crates/bn_runtime/src
  crates/bn_value/src crates/bn_llvm/src
)

found=0
check_matches() {
  local rule=$1
  shift
  local path rel line text record
  while IFS=: read -r path line text; do
    [[ -n "$path" ]] || continue
    rel=${path#"$repo_root/"}
    record="$rel:$line:$text"
    if ! grep -Fqx -- "$record" "$allowlist"; then
      printf 'forbidden dependency (%s): %s\n' "$rule" "$record" >&2
      found=1
    fi
  done < <(rg -n --with-filename --no-heading --glob '*.rs' \
    -e '(^|[^[:alnum:]_])(crate::|use[[:space:]]+)(parser|lexer|semantic)::' \
    -e '(^|[^[:alnum:]_])semantic::' \
    -e '(^|[^[:alnum:]_])super::(parser|lexer|semantic)::' \
    -e 'use[[:space:]]+(crate|super)::\{[^}]*\b(parser|lexer|semantic)\b' \
    "$@" || true)
}

existing_paths=()
for path in "${backend_paths[@]}"; do
  [[ -e "$repo_root/$path" ]] && existing_paths+=("$repo_root/$path")
done
if ((${#existing_paths[@]} > 0)); then
  check_matches "backend→frontend" "${existing_paths[@]}"
fi

frontend_paths=()
for path in src/lexer.rs src/token.rs src/parser.rs src/parser src/ast.rs src/source.rs \
  src/module_graph.rs src/semantic.rs src/semantic src/keyword_registry.rs src/ir/lowering.rs \
  src/ir/lowering_callable.rs src/ir/builder; do
  [[ -e "$repo_root/$path" ]] && frontend_paths+=("$repo_root/$path")
done
if ((${#frontend_paths[@]} > 0)); then
  while IFS=: read -r path line text; do
    [[ -n "$path" ]] || continue
    rel=${path#"$repo_root/"}
    record="$rel:$line:$text"
    if ! grep -Fqx -- "$record" "$allowlist"; then
      printf 'forbidden dependency (frontend→runtime): %s\n' "$record" >&2
      found=1
    fi
  done < <(rg -n --no-heading --glob '*.rs' 'execute_with_host' "${frontend_paths[@]}" || true)
fi

# Frontend semantic analysis may consume the HOST specification, but never a
# host implementation. Keep the implementation boundary explicit until the
# dedicated crates are extracted.
if ((${#frontend_paths[@]} > 0)); then
  while IFS=: read -r path line text; do
    [[ -n "$path" ]] || continue
    rel=${path#"$repo_root/"}
    record="$rel:$line:$text"
    if ! grep -Fqx -- "$record" "$allowlist"; then
      printf 'forbidden dependency (frontend→host implementation): %s\n' "$record" >&2
      found=1
    fi
  done < <(rg -n --no-heading --glob '*.rs' \
    -e '(^|[^[:alnum:]_])(crate::|use[[:space:]]+)(net|http|web|web_state|tls|dispatch)::' \
    -e 'use[[:space:]]+(crate|super)::\{[^}]*\b(net|http|web|web_state|tls|dispatch)\b' \
    "${frontend_paths[@]}" || true)
fi

# W5 freeze: inspect the current extracted public IR model (and retain the
# legacy path while the compatibility facade is still present). Any semantic
# or module-graph import in either model is a contract violation.
ir_models=()
for candidate in "$repo_root/crates/bn_ir/src/model.rs" "$repo_root/src/ir/model.rs"; do
  [[ -f "$candidate" ]] && ir_models+=("$candidate")
done
if ((${#ir_models[@]} > 0)); then
  for ir_model in "${ir_models[@]}"; do
    while IFS=: read -r path line text; do
      [[ -n "$path" ]] || continue
      rel=${path#"$repo_root/"}
      record="$rel:$line:$text"
      if ! grep -Fqx -- "$record" "$allowlist"; then
        printf 'forbidden dependency (public IR model W5): %s\n' "$record" >&2
        found=1
      fi
    done < <(rg -n --with-filename --no-heading --glob '*.rs' \
      -e '(^|[^[:alnum:]_])semantic::' \
      -e '(^|[^[:alnum:]_])module_graph::' \
      "$ir_model" || true)
  done
fi

# Section 6 / Activity 6.5 guards:
# 1. Ensure zero path-shims from crates into src/
while IFS=: read -r file_path line_num match_text; do
  [[ -n "$file_path" ]] || continue
  rel=${file_path#"$repo_root/"}
  printf 'forbidden path-shim from crates into src (Activity 6.5): %s:%s:%s\n' "$rel" "$line_num" "$match_text" >&2
  found=1
done < <(grep -rnE '#\[path\s*=\s*".*(\.\./)+src/' "$repo_root/crates" 2>/dev/null || true)

# 2. Ensure banned orphan paths never return to src/
banned_orphans=(
  src/lexer.rs
  src/token.rs
  src/parser.rs
  src/parser
  src/ast.rs
  src/source.rs
  src/host_spec.rs
  src/host_spec
  src/module_graph.rs
  src/semantic.rs
  src/semantic
  src/keyword_registry.rs
  src/ir/lowering.rs
  src/ir/lowering_callable.rs
  src/ir/builder
  src/ir/model.rs
  src/ir/validate.rs
)
for orphan in "${banned_orphans[@]}"; do
  if [[ -e "$repo_root/$orphan" ]]; then
    printf 'banned orphan path returned to src/ (Activity 6.5): %s\n' "$orphan" >&2
    found=1
  fi
done

if ((found != 0)); then
  echo "forbidden dependency check failed" >&2
  exit 1
fi
echo "forbidden dependency check passed"
