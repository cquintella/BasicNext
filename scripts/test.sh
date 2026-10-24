#!/usr/bin/env bash
set -euo pipefail

script_directory=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
if [[ ${1:-} == --help || ${1:-} == -h ]]; then
    cat "$script_directory/test-help.txt"
    exit 0
fi

build_options=()
arguments=("$@")
while [[ $# -gt 0 ]]; do
    case "$1" in
        --) break ;;
        --manifest-path|--profile|--target|--target-dir|--features|-F|--jobs|-j|--config)
            [[ $# -ge 2 ]] || exit 2
            build_options+=("$1" "$2")
            shift 2
            ;;
        --manifest-path=*|--profile=*|--target=*|--target-dir=*|--features=*|--jobs=*|--config=*|-F?*|-j?*|--release|--all-features|--no-default-features|--offline|--frozen|--locked)
            build_options+=("$1")
            shift
            ;;
        *) shift ;;
    esac
done

cd "$script_directory/.."
source "$script_directory/test-artifacts.sh"
prepare_test_artifacts "${build_options[@]+"${build_options[@]}"}"
exec cargo test --locked "${arguments[@]+"${arguments[@]}"}"
