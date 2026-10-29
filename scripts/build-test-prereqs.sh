#!/usr/bin/env bash
# Test prerequisites: the one place that builds what tests run or link but
# `cargo test` does not rebuild — the `bni` and `bnc` executables and the
# static archives native builds link (`bn_rt`, `bn_stdlib`, `bn_web`).
# Every other build instruction (CI, the agent brief, `test.sh`,
# `test-battery.sh`) calls this file instead of repeating the package list.
#
#   scripts/build-test-prereqs.sh [cargo build options]   # build them
#   scripts/build-test-prereqs.sh --print-stamp           # print the tree stamp only
#   source scripts/build-test-prereqs.sh                  # define prepare_test_artifacts
#
# Sourced, `prepare_test_artifacts` builds them and exports their paths as
# BN_INTERPRETER, BN_COMPILER, BN_RT_LIB, BN_STDLIB_LIB and BN_WEB_LIB.
# Requires jq and git.
#
# Stamp: after a successful build, the stamp of the source tree the build
# started from is written to `test-prereqs.stamp` beside `bni`. A test that
# finds no stamp, or one that differs from `--print-stamp`, stops with
# "test prerequisites are stale or missing" (`tests/support/prerequisites.rs`).
# The stamp hashes HEAD, every staged and unstaged change, and every
# untracked file that is not ignored, so a stash, an edit or a new file all
# change it. It is computed before the build: an edit made while cargo runs
# leaves the binaries older than the stamp would claim, so it is not counted.

# The stamp and the root come from bash (`BASH_SOURCE`); another shell that
# sources this file would resolve the wrong root and fail without a word.
if [ -z "${BASH_VERSION:-}" ]; then
    echo "build-test-prereqs.sh: bash is required (bash scripts/build-test-prereqs.sh)" >&2
    return 2 2>/dev/null || exit 2
fi

TEST_PREREQS_STAMP_FILE=test-prereqs.stamp

test_prereqs_root() {
    (cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
}

# True when the workspace root is the top of a git work tree. A source copy
# without git (a release tarball, the Windows test host's copy) has no tree
# to stamp; neither does a copy unpacked inside another repository.
test_prereqs_in_git() {
    local root prefix
    root=$(test_prereqs_root) || return 1
    prefix=$(git -C "$root" rev-parse --show-prefix 2> /dev/null) || return 1
    [[ -z $prefix ]]
}

# Prints the stamp of the source tree under the workspace root.
test_prereqs_stamp() {
    local root
    root=$(test_prereqs_root) || return "$?"
    git -C "$root" rev-parse --verify --quiet HEAD > /dev/null || return "$?"
    {
        git -C "$root" rev-parse HEAD
        git -C "$root" diff HEAD --binary
        git -C "$root" ls-files -z --others --exclude-standard \
            | LC_ALL=C sort -z \
            | while IFS= read -r -d '' path; do
                printf '%s\0' "$path"
                if [[ -L "$root/$path" ]]; then
                    readlink "$root/$path"
                elif [[ -f "$root/$path" ]]; then
                    cat -- "$root/$path"
                fi
            done
    } | git hash-object --stdin
}

prepare_test_artifacts() {
    local artifacts interpreter compiler runtime stdlib web stamp
    jq --version > /dev/null || return "$?"
    stamp=
    if test_prereqs_in_git; then
        stamp=$(test_prereqs_stamp) || return "$?"
    else
        echo "build-test-prereqs.sh: not a git work tree; no stamp is written, so" \
            "tests need every override (scripts/test.sh sets them)" >&2
    fi
    artifacts=$(cargo build --locked -p bni -p bnc -p bn_rt -p bn_stdlib -p bn_web \
        --message-format=json-render-diagnostics "$@") || return "$?"
    interpreter=$(printf '%s\n' "$artifacts" | jq -bser '
        [.[] | select(.reason == "compiler-artifact" and .target.name == "bni"
            and .executable != null) | .executable]
        | if length == 1 then .[0] else empty end') || return "$?"
    compiler=$(printf '%s\n' "$artifacts" | jq -bser '
        [.[] | select(.reason == "compiler-artifact" and .target.name == "bnc"
            and .executable != null) | .executable]
        | if length == 1 then .[0] else empty end') || return "$?"
    runtime=$(printf '%s\n' "$artifacts" | jq -bser '
        [.[] | select(.reason == "compiler-artifact" and .target.name == "bn_rt")
        | .filenames[] | select(endswith(".a") or endswith(".lib"))]
        | if length == 1 then .[0] else empty end') || return "$?"
    stdlib=$(printf '%s\n' "$artifacts" | jq -bser '
        [.[] | select(.reason == "compiler-artifact" and .target.name == "bn_stdlib")
        | .filenames[] | select(endswith(".a") or endswith(".lib"))]
        | if length == 1 then .[0] else empty end') || return "$?"
    web=$(printf '%s\n' "$artifacts" | jq -bser '
        [.[] | select(.reason == "compiler-artifact" and .target.name == "bn_web")
        | .filenames[] | select(endswith(".a") or endswith(".lib"))]
        | if length == 1 then .[0] else empty end') || return "$?"
    test -x "$interpreter" && test -x "$compiler" \
        && test -f "$runtime" && test -f "$stdlib" && test -f "$web" || return 1
    local stamp_path
    stamp_path="$(dirname "$interpreter")/$TEST_PREREQS_STAMP_FILE"
    if [[ -n $stamp ]]; then
        printf '%s\n' "$stamp" > "$stamp_path" || return "$?"
    else
        rm -f "$stamp_path"
    fi
    export BN_INTERPRETER="$interpreter" BN_COMPILER="$compiler"
    export BN_RT_LIB="$runtime" BN_STDLIB_LIB="$stdlib" BN_WEB_LIB="$web"
}

# Run as a command (not sourced): build from the workspace root.
if [[ ${BASH_SOURCE[0]} == "$0" ]]; then
    set -euo pipefail
    cd "$(dirname "${BASH_SOURCE[0]}")/.."
    if [[ ${1:-} == --print-stamp ]]; then
        test_prereqs_stamp
    else
        prepare_test_artifacts "$@"
    fi
fi
