#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)
workflow_dir="$repo_root/.github/workflows"

fail() {
  echo "supply-chain gate: $*" >&2
  exit 1
}

command -v rg >/dev/null 2>&1 || fail "ripgrep (rg) is required"
workflows=()
while IFS= read -r workflow_path; do
  workflows+=("$workflow_path")
done < <(rg --files "$workflow_dir" -g '*.yml' -g '*.yaml')
(( ${#workflows[@]} > 0 )) || fail "no GitHub Actions workflows found"

while IFS= read -r use_line; do
  [[ "$use_line" =~ uses:[[:space:]]+[^[:space:]@]+@[0-9a-f]{40}([[:space:]]*\#.*)?$ ]] \
    || fail "GitHub Action is not pinned to a full commit SHA: $use_line"
done < <(rg '^[[:space:]]*-[[:space:]]+uses:' "${workflows[@]}")

if rg -n 'apt\.llvm\.org/llvm\.sh|curl[^|]*\|[[:space:]]*(ba)?sh|wget[^|]*\|[[:space:]]*(ba)?sh' \
  "${workflows[@]}" "$repo_root/README.md" "$repo_root/scripts/install.sh"; then
  fail "remote content must not be executed directly by a shell"
fi

if rg -n 'runs-on:[[:space:]]+ubuntu-latest' "${workflows[@]}"; then
  fail "Ubuntu runners must use an explicit release label"
fi

rg -q 'Fingerprint: 6084 F3CF 814B 57C1 CF12 EFD5 15CF 4D18 AF4F 7421' "${workflows[@]}" \
  || fail "LLVM repository key fingerprint is not pinned"

for installer in install.sh install.ps1; do
  rg -q "cp scripts/$installer dist/" "${workflows[@]}" \
    || fail "$installer is not staged before release checksums"
done

rg -q 'basicnext-source-\$\{tag\}\.tar\.gz' "${workflows[@]}" \
  || fail "a versioned source payload is not staged before release checksums"
rg -q 'basicnext-source-\$\{tag\}\.zip' "${workflows[@]}" \
  || fail "a versioned Windows ZIP payload is not staged before release checksums"
rg -q 'basicnext-source-\$tag\.tar\.gz' "$repo_root/scripts/install.sh" \
  || fail "the Unix installer does not use the checksummed source payload"
rg -q 'basicnext-source-\$Tag\.zip' "$repo_root/scripts/install.ps1" \
  || fail "the Windows installer does not use the checksummed ZIP payload"
rg -q 'Expand-Archive' "$repo_root/scripts/install.ps1" \
  || fail "the Windows installer does not extract its payload with PowerShell"
if rg -n '(^|[^[:alnum:]_])(curl(\.exe)?|tar\.exe|tar[[:space:]]+-)' "$repo_root/scripts/install.ps1"; then
  fail "the Windows installer must use native PowerShell download and archive commands"
fi
rg -q 'Invoke-RestMethod' "$repo_root/scripts/install.ps1" \
  || fail "the Windows installer does not resolve the latest release with PowerShell"
rg -q 'tests/install_windows.ps1' "${workflows[@]}" \
  || fail "the Windows release bootstrap integration test is not run by CI"

rg -q 'SHA256SUMS' "$repo_root/README.md" \
  || fail "README does not document release checksum verification"
rg -q 'sha256sum|shasum' "$repo_root/README.md" \
  || fail "README does not show a checksum verification command"

[[ -f "$repo_root/SECURITY.md" ]] || fail "SECURITY.md is missing"

echo "supply-chain gate: OK"
