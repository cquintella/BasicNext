#!/usr/bin/env bash
# Builds a Basic Next program into an executable optimized for this machine.
#
# A thin wrapper over `bnc`: it asks for the highest optimization level and
# the instructions of the host CPU (`bnc --opt 3 --cpu native`, which passes
# `-O3` and `-march=native` / `-mcpu=native` to clang), then removes the
# symbol table with `llvm-strip` (or the system `strip`). Linking, the
# runtime library and the system libraries of each platform stay in `bnc`.
#
# The executable runs on this machine and on machines with the same CPU
# features; for a portable executable use plain `bnc` (`--cpu generic`).
# No option here changes a program's results: `bnc` never enables fast-math.
#
# Usage:
#   scripts/build-native.sh [-o <output>] [--keep-symbols] [bnc options] <entry.bn>
#
#   -o, --output <file>  executable to write (default: entry name, no .bn)
#   --keep-symbols       do not strip the executable
#   bnc options          passed through (for example --module-path <dir>);
#                        -g keeps the symbols, as a debug build needs them
#
# Environment:
#   BNC          the bnc to run (default: bnc on PATH, else this checkout's
#                target/release/bnc or target/debug/bnc)
#   LLVM_STRIP   the strip tool (default: llvm-strip on PATH, else strip)

set -euo pipefail

usage() {
    sed -n '/^# Usage:/,/^# Environment:/p' "$0" | sed '$d; s/^# \{0,1\}//'
}

fail() {
    echo "build-native: $*" >&2
    exit 1
}

script_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)

find_bnc() {
    if [[ -n "${BNC:-}" ]]; then
        echo "$BNC"
    elif command -v bnc >/dev/null 2>&1; then
        command -v bnc
    else
        local profile
        for profile in release debug; do
            if [[ -x "$script_dir/../target/$profile/bnc" ]]; then
                echo "$script_dir/../target/$profile/bnc"
                return
            fi
        done
        fail "bnc not found: install Basic Next (scripts/install.sh) or set BNC"
    fi
}

find_strip() {
    if [[ -n "${LLVM_STRIP:-}" ]]; then
        echo "$LLVM_STRIP"
    elif command -v llvm-strip >/dev/null 2>&1; then
        command -v llvm-strip
    elif command -v strip >/dev/null 2>&1; then
        command -v strip
    fi
}

output=""
entry=""
keep_symbols=0
bnc_options=()
while [[ $# -gt 0 ]]; do
    case "$1" in
        -h | --help)
            usage
            exit 0
            ;;
        -o | --output)
            [[ $# -ge 2 ]] || fail "$1 expects a file name"
            output="$2"
            shift 2
            ;;
        --keep-symbols)
            keep_symbols=1
            shift
            ;;
        --opt | --cpu | --target | --emit)
            fail "$1 is fixed by this script (--opt 3 --cpu native, native executable)"
            ;;
        -g | --debug)
            keep_symbols=1
            bnc_options+=("$1")
            shift
            ;;
        *.bn)
            [[ -z "$entry" ]] || fail "more than one entry file: $entry and $1"
            entry="$1"
            shift
            ;;
        *)
            bnc_options+=("$1")
            shift
            ;;
    esac
done

[[ -n "$entry" ]] || { usage >&2; exit 2; }
[[ -f "$entry" ]] || fail "entry file not found: $entry"
if [[ -z "$output" ]]; then
    output=$(basename "$entry" .bn)
fi

bnc=$(find_bnc)
"$bnc" --help 2>/dev/null | grep -q -- "--cpu" ||
    fail "$bnc ($("$bnc" --version 2>/dev/null)) has no --cpu option; it needs Basic Next 0.6.5 or later (set BNC to choose another)"
"$bnc" --opt 3 --cpu native ${bnc_options[@]+"${bnc_options[@]}"} -o "$output" "$entry"

if [[ "$keep_symbols" -eq 0 ]]; then
    strip_tool=$(find_strip)
    [[ -n "$strip_tool" ]] || fail "no strip tool (llvm-strip or strip); use --keep-symbols"
    if [[ "$(uname -s)" == "Darwin" && "$(basename "$strip_tool")" == "strip" ]]; then
        # Apple's strip: -x removes local symbols, keeping what dyld needs.
        "$strip_tool" -x "$output"
    else
        "$strip_tool" --strip-all "$output"
    fi
fi

size=$(wc -c <"$output" | tr -d ' ')
echo "$output: $size bytes, optimized for $(uname -m) ($(uname -s))"
