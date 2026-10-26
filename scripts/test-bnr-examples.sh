#!/usr/bin/env bash
# Author: Carlos Quintella
# Injects Basic Next example programs line-by-line into `bnr` (interactive REPL)
# and monitors output and parity against `bni run`.

set -u

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"

BNR_BIN="${ROOT_DIR}/target/debug/bnr"
BNI_BIN="${ROOT_DIR}/target/debug/bni"

# Colors
CLR_RESET="\033[0m"
CLR_BOLD="\033[1m"
CLR_GREEN="\033[32m"
CLR_RED="\033[31m"
CLR_YELLOW="\033[33m"
CLR_CYAN="\033[36m"
CLR_GRAY="\033[90m"

# Options
DELAY=0
STEP=0
VERBOSE=0
VERIFY_PARITY=1
TARGET_FILES=()

usage() {
    cat <<EOF
Usage: $0 [options] [example1.bn example2.bn ...]

Options:
  -d, --delay <sec>    Delay in seconds between sending each line (e.g. 0.05 or 0.1)
  -s, --step           Wait for [ENTER] after injecting each line
  -v, --verbose        Show every line as it is injected into bnr
      --no-parity      Do not compare bnr output against bni run
  -h, --help           Show this help text

Examples:
  $0 examples/hello.bn
  $0 --verbose --delay 0.05 examples/arithmetic.bn
  $0 --step examples/factorial.bn
  $0                   # Runs all standalone examples
EOF
    exit 0
}

# Parse options
while [[ $# -gt 0 ]]; do
    case "$1" in
        -d|--delay)
            DELAY="$2"
            shift 2
            ;;
        -s|--step)
            STEP=1
            shift
            ;;
        -v|--verbose)
            VERBOSE=1
            shift
            ;;
        --no-parity)
            VERIFY_PARITY=0
            shift
            ;;
        -h|--help)
            usage
            ;;
        *)
            TARGET_FILES+=("$1")
            shift
            ;;
    esac
done

# Ensure binaries are built
if [[ ! -x "$BNR_BIN" || ! -x "$BNI_BIN" ]]; then
    printf "${CLR_CYAN}==> Building bnr and bni...${CLR_RESET}\n"
    (cd "$ROOT_DIR" && cargo build -p bnr -p bni) || {
        printf "${CLR_RED}Failed to build bnr/bni${CLR_RESET}\n" >&2
        exit 1
    }
fi

# Determine file list
if [[ ${#TARGET_FILES[@]} -eq 0 ]]; then
    # Standalone computational examples suitable for non-interactive REPL
    CANDIDATES=(
        "examples/hello.bn"
        "examples/arithmetic.bn"
        "examples/factorial.bn"
        "examples/control-flow.bn"
        "examples/conversions.bn"
        "examples/boyer-moore.bn"
        "examples/kmp.bn"
        "examples/rabin-karp.bn"
        "examples/shortest_path.bn"
        "examples/objects.bn"
        "examples/temperature-factories.bn"
        "examples/clock.bn"
        "examples/div.bn"
        "examples/global.bn"
        "examples/variables.bn"
        "examples/type_test.bn"
    )
    for c in "${CANDIDATES[@]}"; do
        if [[ -f "${ROOT_DIR}/${c}" ]]; then
            TARGET_FILES+=("${c}")
        fi
    done
fi

printf "${CLR_BOLD}${CLR_CYAN}======================================================${CLR_RESET}\n"
printf "${CLR_BOLD} Basic Next REPL (bnr) Line-by-Line Injection Suite${CLR_RESET}\n"
printf "${CLR_BOLD}${CLR_CYAN}======================================================${CLR_RESET}\n"
printf "Target:  %s\n" "$BNR_BIN"
printf "Files:   %d example(s)\n" "${#TARGET_FILES[@]}"
if [[ "$DELAY" != "0" ]]; then
    printf "Pace:    %s delay between lines\n" "$DELAY"
fi
if [[ "$STEP" -eq 1 ]]; then
    printf "Mode:    Step-by-step (press ENTER after each line)\n"
fi
printf "\n"

TOTAL=0
PASSED=0
FAILED=0
SKIPPED=0

for rel_path in "${TARGET_FILES[@]}"; do
    file_path="${ROOT_DIR}/${rel_path}"
    if [[ ! -f "$file_path" ]]; then
        file_path="$rel_path"
    fi
    if [[ ! -f "$file_path" ]]; then
        printf "${CLR_RED}File not found: %s${CLR_RESET}\n" "$rel_path"
        ((FAILED++))
        continue
    fi

    base_name="$(basename "$file_path")"
    TOTAL=$((TOTAL + 1))
    printf "${CLR_BOLD}[%d/%d] Testing %s${CLR_RESET} (%s)\n" "$TOTAL" "${#TARGET_FILES[@]}" "$base_name" "$rel_path"

    # Feed line-by-line
    feed_lines() {
        local has_start_decl=0
        while IFS= read -r line || [[ -n "$line" ]]; do
            if [[ "$line" =~ ^[[:space:]]*FUNCTION[[:space:]]+Start\( ]]; then
                has_start_decl=1
            fi
            if [[ "$VERBOSE" -eq 1 ]]; then
                printf "${CLR_GRAY}--> %s${CLR_RESET}\n" "$line" >&2
            fi
            printf "%s\n" "$line"
            if [[ "$STEP" -eq 1 ]]; then
                read -r -u 3 _dummy </dev/tty || true
            elif (( $(awk -v d="$DELAY" 'BEGIN { print (d > 0) }') )); then
                sleep "$DELAY"
            fi
        done < "$file_path" 3<&0
        # If the program defined FUNCTION Start(), invoke it to run the entry point
        if [[ "$has_start_decl" -eq 1 ]]; then
            if [[ "$VERBOSE" -eq 1 ]]; then
                printf "${CLR_GRAY}--> Start()${CLR_RESET}\n" >&2
            fi
            printf "Start()\n"
        fi
    }

    bnr_out_file="$(mktemp /tmp/bnr_test_out.XXXXXX)"
    bnr_err_file="$(mktemp /tmp/bnr_test_err.XXXXXX)"

    feed_lines | "$BNR_BIN" > "$bnr_out_file" 2> "$bnr_err_file"
    bnr_status=$?

    bnr_output="$(cat "$bnr_out_file")"
    bnr_error="$(cat "$bnr_err_file")"
    rm -f "$bnr_out_file" "$bnr_err_file"

    if [[ $bnr_status -ne 0 || -n "$bnr_error" ]]; then
        # Check if error is just diagnostic warning or actual failure
        if [[ $bnr_status -ne 0 || "$bnr_error" == *"error["* ]]; then
            printf "  ${CLR_RED}FAILED (exit %d)${CLR_RESET}\n" "$bnr_status"
            if [[ -n "$bnr_error" ]]; then
                printf "%s\n" "$bnr_error" | sed 's/^/    /'
            fi
            if [[ -n "$bnr_output" ]]; then
                printf "  Stdout:\n%s\n" "$(printf "%s\n" "$bnr_output" | sed 's/^/    /')"
            fi
            FAILED=$((FAILED + 1))
            continue
        fi
    fi

    # Parity check against bni run
    if [[ "$VERIFY_PARITY" -eq 1 ]]; then
        bni_out_file="$(mktemp /tmp/bni_test_out.XXXXXX)"
        bni_err_file="$(mktemp /tmp/bni_test_err.XXXXXX)"

        (cd "$ROOT_DIR" && "$BNI_BIN" run "$file_path") > "$bni_out_file" 2> "$bni_err_file"
        bni_status=$?
        bni_output="$(cat "$bni_out_file")"
        rm -f "$bni_out_file" "$bni_err_file"

        if [[ $bni_status -ne 0 ]]; then
            printf "  ${CLR_YELLOW}SKIPPED (bni run returned non-zero exit %d)${CLR_RESET}\n" "$bni_status"
            SKIPPED=$((SKIPPED + 1))
            continue
        fi

        # Compare outputs
        trimmed_bnr="$(echo -e "$bnr_output" | sed -e 's/[[:space:]]*$//')"
        trimmed_bni="$(echo -e "$bni_output" | sed -e 's/[[:space:]]*$//')"

        # Normalize non-deterministic outputs like wall-clock time
        if [[ "$base_name" == "clock.bn" ]]; then
            trimmed_bnr="$(echo "$trimmed_bnr" | sed -E 's/Hora:  [0-9]{2}:[0-9]{2}:[0-9]{2}\.[0-9]+/Hora:  <TIME>/')"
            trimmed_bni="$(echo "$trimmed_bni" | sed -E 's/Hora:  [0-9]{2}:[0-9]{2}:[0-9]{2}\.[0-9]+/Hora:  <TIME>/')"
        fi

        # If Start() returns an integer, bnr displays the turn's expression result (e.g. "0 : INT32")
        # while bni uses it as the process exit code. Normalize for parity comparison:
        if [[ "$trimmed_bnr" =~ ^(.*)[[:space:]]+[0-9]+[[:space:]]*:[[:space:]]*INT32$ ]]; then
            normalized_bnr="${BASH_REMATCH[1]}"
            if [[ "$normalized_bnr" == "$trimmed_bni" ]]; then
                trimmed_bnr="$trimmed_bni"
            fi
        fi

        if [[ "$trimmed_bnr" == "$trimmed_bni" ]]; then
            printf "  ${CLR_GREEN}✓ PASS (parity matches bni exactly)${CLR_RESET}\n"
            if [[ "$VERBOSE" -eq 1 && -n "$trimmed_bnr" ]]; then
                printf "  Output:\n%s\n" "$(printf "%s\n" "$trimmed_bnr" | sed 's/^/    /')"
            fi
            PASSED=$((PASSED + 1))
        else
            printf "  ${CLR_RED}PARITY MISMATCH${CLR_RESET}\n"
            printf "  --- bnr output ---\n%s\n" "$(printf "%s\n" "$trimmed_bnr" | sed 's/^/    /')"
            printf "  --- bni output ---\n%s\n" "$(printf "%s\n" "$trimmed_bni" | sed 's/^/    /')"
            FAILED=$((FAILED + 1))
        fi
    else
        printf "  ${CLR_GREEN}✓ PASS (bnr executed cleanly)${CLR_RESET}\n"
        if [[ "$VERBOSE" -eq 1 && -n "$bnr_output" ]]; then
            printf "  Output:\n%s\n" "$(printf "%s\n" "$bnr_output" | sed 's/^/    /')"
        fi
        PASSED=$((PASSED + 1))
    fi

done

printf "\n${CLR_BOLD}${CLR_CYAN}------------------------------------------------------${CLR_RESET}\n"
printf "${CLR_BOLD}Summary: %d total, ${CLR_GREEN}%d passed${CLR_RESET}, ${CLR_RED}%d failed${CLR_RESET}, ${CLR_YELLOW}%d skipped${CLR_RESET}\n" \
    "$TOTAL" "$PASSED" "$FAILED" "$SKIPPED"
printf "${CLR_BOLD}${CLR_CYAN}------------------------------------------------------${CLR_RESET}\n"

if [[ $FAILED -gt 0 ]]; then
    exit 1
fi
exit 0
