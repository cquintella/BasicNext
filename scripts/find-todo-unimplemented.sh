#!/usr/bin/env bash
# Author: Carlos Quintella
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at http://mozilla.org/MPL/2.0/.

# Search for todo! and unimplemented! macros in Rust source files.
set -euo pipefail

usage() {
  echo "usage: $0 [--root <directory>]" >&2
}

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd -P)"
repo_root="$(cd "${script_dir}/.." && pwd -P)"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --root)
      [[ $# -ge 2 ]] || { usage; exit 2; }
      repo_root="$(cd "$2" && pwd -P)"
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

if command -v rg >/dev/null 2>&1; then
  rg -n "(todo!|unimplemented!)" \
    --glob "*.rs" \
    --glob "!target/**" \
    --glob "!.git/**" \
    "${repo_root}" || true
else
  find "${repo_root}" \
    -name "target" -prune -o \
    -name ".git" -prune -o \
    -name "*.rs" -exec grep -En "(todo!|unimplemented!)" {} + || true
fi
