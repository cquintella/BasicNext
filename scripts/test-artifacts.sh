#!/usr/bin/env bash

prepare_test_artifacts() {
    local artifacts interpreter compiler runtime stdlib web
    jq --version > /dev/null || return "$?"
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
    export BN_INTERPRETER="$interpreter" BN_COMPILER="$compiler"
    export BN_RT_LIB="$runtime" BN_STDLIB_LIB="$stdlib" BN_WEB_LIB="$web"
}
