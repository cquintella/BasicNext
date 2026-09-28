#!/usr/bin/env bash
# File-level component and call graph of the workspace, written as GML
# (default: target/src-graph.gml). Nodes are src/**/*.rs files with their
# items and SHA-256; edges are calls between files with a confidence
# attribute. The generator is the standalone crate scripts/src-graph
# (outside the workspace).
#
#   scripts/src-graph.sh [output.gml]         write the graph
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
out=${1:-"$root/target/src-graph.gml"}
mkdir -p "$(dirname "$out")"
run "$root" "$out"
echo "$out"
