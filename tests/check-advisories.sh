#!/usr/bin/env bash
# Every package in Cargo.lock is free of unaccepted security advisories:
# the advisories policy in deny.toml ([advisories]). Fails closed when cargo-deny
# is missing, so the check cannot pass by not being checked.
#   tests/check-advisories.sh
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$repo_root"

if ! command -v cargo-deny >/dev/null 2>&1; then
  echo "check-advisories: cargo-deny is not installed (cargo install --locked cargo-deny)" >&2
  exit 1
fi
cargo deny --locked check advisories
