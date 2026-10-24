#!/usr/bin/env bash
set -euo pipefail

[[ $# == 2 ]] || exit 2
artifact_manifest=$1
destination=$2
script_directory=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
cd "$script_directory/.."
scratch=$(mktemp -d)
trap 'rm -rf "$scratch"' EXIT
jq -bse 'last | .reason == "build-finished" and .success == true' \
    "$artifact_manifest" > /dev/null
jq -br '.matrices[] | [.target, .test, .filter] | @tsv' \
    "$script_directory/test-inventory.json" > "$scratch/matrices.tsv"

while IFS=$'\t' read -r target test_name filter; do
    executable=$(jq -bser --arg target "$target" --slurpfile configuration "$script_directory/test-inventory.json" '
        ($configuration[0].targets[$target] // $target) as $binary
        | [.[] | select(.reason == "compiler-artifact" and .profile.test
            and .target.name == $binary and .executable != null) | .executable]
        | if length == 1 then .[0] else empty end
    ' "$artifact_manifest")
    if env "$filter=" "$executable" --exact "$test_name" --nocapture \
        --format terse --test-threads=1 > "$scratch/$target.log" 2>&1; then
        :
    else
        result=$?
        cat "$scratch/$target.log" >&2
        exit "$result"
    fi
    rg '^test result: ok\. 1 passed;' "$scratch/$target.log" > /dev/null
    sed -n 's/^== //p' "$scratch/$target.log" > "$scratch/names.txt"
    test -s "$scratch/names.txt"
    jq -bRn --arg target "$target" \
        --slurpfile configuration "$script_directory/test-inventory.json" '
        inputs | {old_id: ("cell::bn::test/" + $target + "::" + .),
            domain: $target, harness: "values", oracle: "observed",
            new_target: "values", new_id: "", status: "open",
            reason: $configuration[0].pending, ignored: false}
    ' < "$scratch/names.txt" >> "$scratch/cells.jsonl"
done < "$scratch/matrices.tsv"

arc_file=$([[ -f tests/values/arc_differential.rs ]] && echo tests/values/arc_differential.rs || echo tests/arc_differential.rs)
seed_count=$(sed -n 's/^const SEEDS: u64 = \([0-9][0-9]*\);$/\1/p' "$arc_file")
jq -ben --argjson count "$seed_count" '$count > 0' > /dev/null
jq -bcn --argjson count "$seed_count" \
    --slurpfile configuration "$script_directory/test-inventory.json" '
    range(0; $count) | {old_id: ("cell::bn::test/arc_differential::seed" + tostring),
        domain: "arc_differential", harness: "values", oracle: "observed",
        new_target: "values", new_id: "", status: "open",
        reason: $configuration[0].pending, ignored: false}
' >> "$scratch/cells.jsonl"
jq -bse 'length > 0 and (group_by(.old_id) | all(length == 1))' \
    "$scratch/cells.jsonl" > /dev/null
jq -bcs '{kind: "complete", counts: (group_by(.domain)
    | map({domain: .[0].domain, count: length}))}' \
    "$scratch/cells.jsonl" > "$scratch/completion.jsonl"
cat "$scratch/completion.jsonl" >> "$scratch/cells.jsonl"
mv "$scratch/cells.jsonl" "$destination"
