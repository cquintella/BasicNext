#!/usr/bin/env bash
# Every package in Cargo.lock has a license the project allows: the SPDX
# list in deny.toml ([licenses] allow). Fails closed when cargo-deny is
# missing, so the policy cannot pass by not being checked.
#   tests/check-licenses.sh
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$repo_root"

if ! command -v cargo-deny >/dev/null 2>&1; then
  echo "check-licenses: cargo-deny is not installed (cargo install --locked cargo-deny)" >&2
  exit 1
fi
cargo deny --locked check licenses
