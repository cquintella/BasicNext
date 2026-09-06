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

declare -a backend_paths=(
  src/runtime_impl.rs src/runtime src/heap.rs src/dispatch.rs src/dispatch
  src/net.rs src/net src/http.rs src/web.rs src/web src/web_state.rs
  src/dataframe.rs src/llvm.rs src/llvm crates/bn_rt/src
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
    "$@" 2>/dev/null || true)
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
  done < <(rg -n --no-heading --glob '*.rs' 'execute_with_host' "${frontend_paths[@]}" 2>/dev/null || true)
fi

# W5 freeze: the public IR model currently carries semantic and module-graph
# types as an explicitly versioned debt. Any changed import line must be
# reviewed and added deliberately; new symbols on an existing line also fail.
ir_model="$repo_root/src/ir/model.rs"
if [[ -f "$ir_model" ]]; then
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
    "$ir_model" 2>/dev/null || true)
fi

if ((found != 0)); then
  echo "forbidden dependency check failed" >&2
  exit 1
fi
echo "forbidden dependency check passed"
