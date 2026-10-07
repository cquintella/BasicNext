#!/usr/bin/env bash
# File-level component and call graph of the workspace, written as GML
# (default: target/src-graph.gml). Nodes are src/**/*.rs files with their
# items and SHA-256; edges are calls between files with a confidence
# attribute. The generator is the standalone crate scripts/src-graph
# (outside the workspace).
#
#   scripts/src-graph.sh [output.gml|.dot]   write the graph
#   scripts/src-graph.sh --crates [out.dot]   write aggregated crates-level DOT graph
#   scripts/src-graph.sh --audit              audit workspace architecture boundaries
#   scripts/src-graph.sh --callers <item>     query callers of a file or function
#   scripts/src-graph.sh --callees <file>     query callees of a file
#   scripts/src-graph.sh --cycles             detect cycles within crates
#   scripts/src-graph.sh --stats              display fan-in / fan-out metrics
#   scripts/src-graph.sh --check [graph.gml]  exit 0 if the graph matches the
#                                             sources and the generator; else
#                                             list changed/added/removed and
#                                             exit 1
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
run() {
  CARGO_TARGET_DIR="$root/target/src-graph-tool" cargo run --quiet --release \
    --manifest-path "$root/scripts/src-graph/Cargo.toml" -- "$@"
}

if [[ "${1:-}" == "--check" ]]; then
  run --check "$root" "${2:-"$root/target/src-graph.gml"}"
  exit
fi

if [[ "${1:-}" == "--audit" ]]; then
  if [[ -n "${2:-}" ]]; then
    mkdir -p "$(dirname "$2")"
    run --audit "$root" "$2"
  else
    run --audit "$root"
  fi
  exit
fi

if [[ "${1:-}" == "--crates" ]]; then
  out=${2:-"$root/target/crates-graph.dot"}
  mkdir -p "$(dirname "$out")"
  run --crates "$root" "$out"
  echo "$out"
  exit
fi

if [[ "${1:-}" == "--callers" ]]; then
  run "$root" --callers "$2"
  exit
fi

if [[ "${1:-}" == "--callees" ]]; then
  run "$root" --callees "$2"
  exit
fi

if [[ "${1:-}" == "--cycles" ]]; then
  run "$root" --cycles
  exit
fi

if [[ "${1:-}" == "--stats" ]]; then
  run "$root" --stats
  exit
fi

out=${1:-"$root/target/src-graph.gml"}
mkdir -p "$(dirname "$out")"
run "$root" "$out"
echo "$out"
