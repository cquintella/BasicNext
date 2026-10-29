#!/usr/bin/env bash
# Public files never cite private material (BDFL decision, 2026-10-29): no
# path into the private process repository or the unpublished docs, and no
# private ID (issue numbers, bucket items, audit IDs). Prints one finding
# per line as `path:line:tier:text`; the caller (a Rust gate with a
# ratchet, or a process gate for the book) decides what fails.
#
#   tests/check-public-references.sh --root <tree>
#
# Reads the tracked files of <tree> when it is a git work tree, else every
# file. Fails closed without ripgrep.
set -euo pipefail

root=
if (($# == 2)) && [[ $1 == --root ]]; then
    root=$(cd "$2" && pwd)
else
    echo "usage: $0 --root <tree>" >&2
    exit 2
fi
command -v rg > /dev/null 2>&1 || {
    echo "check-public-references: ripgrep (rg) is required" >&2
    exit 2
}

# Always private, wherever they appear.
ALWAYS=(
    '\bK[0-9]{1,3}\b'
    '\b[A-Z]\.K[0-9]+'
    '\b(AGENTS|CLAUDE|GEMINI)\.md\b'
    '(^|[^A-Za-z0-9_./-])(todo|ongoing|done|workflow)/'
    'bn/docs/'
    '(^|[^A-Za-z0-9_./-])docs/(architecture|project|releases|governance|research|positioning|library)\b'
)
# Private only when cited: in parentheses, or after a citing word.
CITED_IDS='([A-Z]{1,2}\.S[0-9]+(\.[a-z])?|S[0-9]+\.[a-z]|P\.[0-9]+|A[0-9]{2}|[DG]-[0-9]+|R[0-9]|[Ss]print [0-9]+)'
CITED=(
    '\b[Bb]uckets? [0-9]\.[0-9][0-9A-Za-z.]*'
    '\b[Bb]ucket [a-z]+-[a-z][a-z-]*'
    "\\([^)]*\\b${CITED_IDS}\\b[^)]*\\)"
    "\\b([Kk]nown issues?|[Ii]ssues?|[Dd]ecisions?|[Aa]udits?|[Bb]uckets?|[Ss]ections?|[Ii]tems?)\\b[^.;:]{0,40}\\b${CITED_IDS}\\b"
)

# `.gitignore` names private files to keep them out; that is not a citation.
NOT_SCANNED='^(\./)?\.gitignore$'

files() {
    if git -C "$root" rev-parse --show-prefix > /dev/null 2>&1; then
        git -C "$root" ls-files
    else
        # Without git (a source copy), every file but the history.
        (cd "$root" && find . -type f -not -path './.git/*')
    fi | grep -Ev "$NOT_SCANNED" | tr '\n' '\0'
}

scan() {
    local tier=$1 pattern=$2
    (cd "$root" && files | xargs -0 rg --no-heading --with-filename --line-number \
        --pcre2 -e "$pattern" -- 2> /dev/null || true) \
        | while IFS= read -r hit; do
            local location=${hit%%:*} rest=${hit#*:}
            # A binary file is reported without a line number; it is not text.
            [[ ${rest%%:*} =~ ^[0-9]+$ ]] || continue
            printf '%s:%s:%s:%s\n' "${location#./}" "${rest%%:*}" "$tier" "${rest#*:}"
        done
}

{
    for pattern in "${ALWAYS[@]}"; do scan always "$pattern"; done
    for pattern in "${CITED[@]}"; do scan cited "$pattern"; done
} | LC_ALL=C sort -t: -k1,1 -k2,2n -u
