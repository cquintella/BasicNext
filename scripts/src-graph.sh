#!/usr/bin/env bash
# File-level component and call graph of the workspace, written as GML
# (default: target/src-graph.gml). Nodes are src/**/*.rs files with their
# items and SHA-256; edges are calls and type uses between files with a
# confidence attribute. The generator is the standalone crate
# scripts/src-graph (outside the workspace).
#
#   scripts/src-graph.sh [output.gml|.dot]   write the graph
#   scripts/src-graph.sh --crates [out.dot]   write aggregated crates-level DOT graph
#   scripts/src-graph.sh --audit              audit workspace architecture boundaries
#   scripts/src-graph.sh --callers <item>     query callers of a file or function
#   scripts/src-graph.sh --callees <file>     query callees of a file
#   scripts/src-graph.sh --cycles             detect cycles within crates
#   scripts/src-graph.sh --stats              display fan-in / fan-out metrics
#   scripts/src-graph.sh --unused-pub[=crate] list pub items no other crate
#                                             names (pub(crate) candidates)
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

usage() {
  sed -n '/^#   scripts\/src-graph.sh/,/^set -euo/p' "$0" | sed '$d; s/^# \{0,1\}//'
}

case "${1:-}" in
  -h|--help)
    usage
    exit 0
    ;;
  --check)
    run --check "$root" "${2:-"$root/target/src-graph.gml"}"
    ;;
  --crates)
    out=${2:-"$root/target/crates-graph.dot"}
    mkdir -p "$(dirname "$out")"
    run "$root" "$out" --crates
    echo "$out"
    ;;
  --*)
    run "$root" "$@"
    ;;
  *)
    out=${1:-"$root/target/src-graph.gml"}
    mkdir -p "$(dirname "$out")"
    run "$root" "$out"
    echo "$out"
    ;;
esac
