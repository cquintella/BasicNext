#!/usr/bin/env bash
# Public files never cite private material (BDFL decision, 2026-10-29): no
# path into the private process repository or the unpublished docs, and no
# private ID (issue numbers, bucket names and items, audit and decision
# IDs). Prints one finding per matched line as `path<TAB>line<TAB>tier<TAB>text`;
# the caller (a Rust gate with a ratchet, or a process gate for the book)
# decides what fails.
#
#   tests/check-public-references.sh --root <tree>
#
# Reads the tracked files of <tree> when it is a git work tree, else every
# file but the history and the build output. Patterns may span lines (a
# citation wrapped in a comment). Fails closed: a missing or failing
# ripgrep is an error, never "no findings".
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

# Space between words of a wrapped comment: blanks, newlines, comment marks.
GAP='(?:\s|//[/!]?|#)+'
# Identifiers of private plans and findings: bucket items and sections,
# activity IDs, review and audit findings, decision and gate labels.
IDS='[A-Z]{1,2}\.S[0-9]+(\.[a-z])?|S[0-9]+\.[0-9a-z]+|P\.[0-9]+|P[0-9]{1,2}[a-z]?|D[0-9]+|[DG]-[0-9]+|D-[0-9]{3}-[0-9]+|H-D[0-9]+|CRY-[0-9]+|[A-Z]{2,5}-[A-Z]{2,5}-[0-9]+|R[0-9]+|B[0-9]{1,2}|[A-Z]\.[0-9]+(\.[A-Z0-9]+)*|AUDIT-[A-Z0-9-]+'

# Always private, wherever they appear.
ALWAYS=(
    '\bK-?[0-9]{1,4}\b'
    '\b[A-Z]\.K[0-9]+'
    '(?i)\b(agents|claude|gemini)\.md\b'
    '\b(known_issues|issues|decisions|STATUS)\.md\b'
    '(^|[^A-Za-z0-9_-])(todo|ongoing|done)/'
    '\bworkflow/'
    'bn/[d]ocs/'
    '\bdocs/(architecture|project|releases|governance|research|positioning|library)\b'
    '\bbucket-[0-9a-z][0-9a-z.-]*'
    '(?i)\bsprint[- ]?[0-9]+\b'
    '\bk[0-9]{2,3}\b'
    '(?i)\bknown\s+issues?\s+#?[0-9]+'
    '\bKI-[0-9]+\b'
    '\bG[0-9]+-D[0-9]+\b'
    '\bD-0[0-9]{2}\b'
    '\b[a-z0-9-]+-design\.md\b'
    '\bprocess-adjustments\b'
)
# Private when cited: a bucket named by version or name (the words may be
# wrapped), an ID in parentheses, or an ID after a citing word.
CITED=(
    "\\b[Bb]uckets?${GAP}[0-9]\\.[0-9][0-9A-Za-z.]*"
    "\\b[Bb]ucket${GAP}[a-z]+-[a-z][a-z-]*"
    "\\b[0-9]\\.[0-9]+\\.[0-9]+[a-z]?${GAP}bucket\\b"
    "\\([^)]{0,200}?\\b(${IDS})\\b[^)]{0,200}?\\)"
    "(^|//[/!]?|#)\\s*(${IDS})[,:]"
    "§[0-9][0-9.]*[^.\\n]{0,30}\\bbucket\\b"
    "(?i:\\b(audits?|findings?)\\b)[^.;:)]{0,40}?\\bA[0-9]{1,2}\\b"
    "\\bbucket\\b[^.\\n]{0,30}§[0-9]"
    "(?i:\\b(known\\s+issues?|issues?|decisions?|audits?|findings?|buckets?|sections?|items?|clusters?|gates?)\\b)[^.;:)]{0,40}?\\b(${IDS})\\b"
)

# `.gitignore` names private files to keep them out; that is not a citation.
# Without git (a copied tree), every file is read except build output: the
# target folders, and what a running build writes beside its source (the
# compile driver's `.<stem>.bnbuild-<pid>-<nanos>-<n>/` scratch folder and
# the `<product>.bnbuild.log` companion log), since tests build while the
# scan runs.
list_files() {
    if git -C "$root" rev-parse --show-prefix > /dev/null 2>&1; then
        git -C "$root" ls-files -z -- ':!:.gitignore'
    else
        (cd "$root" && find . -type f -not -path './.git/*' -not -path './target/*' \
            -not -path './fuzz/target/*' -not -path './fuzz/corpus/*' \
            -not -path './fuzz/artifacts/*' -not -path './.gitignore' \
            -not -path '*/.*.bnbuild-*/*' -not -name '*.bnbuild.log' -print0)
    fi
}

errors=$(mktemp "${TMPDIR:-/tmp}/check-public-references.XXXXXX")
trap 'rm -f "$errors"' EXIT

scan() {
    local tier=$1 pattern=$2
    # rg exits 1 when nothing matches; real errors go to $errors and stop
    # the scan below. Binary files are skipped.
    (cd "$root" && list_files | xargs -0 rg --multiline --no-heading --with-filename \
        --line-number --field-match-separator $'\t' -e "$pattern" -- 2>> "$errors" || true) \
        | while IFS=$'\t' read -r location number text; do
            [[ $number =~ ^[0-9]+$ ]] || continue
            printf '%s\t%s\t%s\t%s\n' "${location#./}" "$number" "$tier" "$text"
        done
}

# Public identifiers that look like private ones (a standard's section, an
# algorithm name): `path<TAB>literal<TAB>reason` lines in the tree's
# `tests/public_references.allow`. They exempt the cited tier only; an
# issue number or a private path is never exempt.
drop_allowed() {
    local allow="$root/tests/public_references.allow"
    if [[ ! -f $allow ]]; then
        cat
        return
    fi
    local path number tier text stripped matched keep pattern entry_path literal reason
    while IFS=$'\t' read -r path number tier text; do
        if [[ $tier == cited ]]; then
            # Remove each allowed literal, then test the rest of the line
            # again: an exemption covers its literal, not its line.
            stripped=$text
            matched=0
            while IFS=$'\t' read -r entry_path literal reason; do
                [[ $entry_path == \#* || -z $literal ]] && continue
                if [[ $entry_path == "$path" && $stripped == *"$literal"* ]]; then
                    stripped=${stripped//"$literal"/}
                    matched=1
                fi
            done < "$allow"
            if ((matched)); then
                keep=0
                for pattern in "${CITED[@]}"; do
                    printf '%s\n' "$stripped" | rg -q -e "$pattern" && keep=1
                done
                ((keep)) || continue
            fi
        fi
        printf '%s\t%s\t%s\t%s\n' "$path" "$number" "$tier" "$text"
    done
}

{
    for pattern in "${ALWAYS[@]}"; do scan always "$pattern"; done
    for pattern in "${CITED[@]}"; do scan cited "$pattern"; done
} | LC_ALL=C sort -t $'\t' -k1,1 -k2,2n -u -s | drop_allowed
if [[ -s $errors ]]; then
    echo "check-public-references: ripgrep failed:" >&2
    cat "$errors" >&2
    exit 2
fi
