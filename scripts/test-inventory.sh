#!/usr/bin/env bash
set -euo pipefail

[[ $# == 3 ]] || exit 2
artifact_manifest=$1
cell_inventory=$2
destination=$3
script_directory=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
cd "$script_directory/.."
scratch=$(mktemp -d)
trap 'rm -rf "$scratch"' EXIT
jq -bse 'last | .reason == "build-finished" and .success == true' \
    "$artifact_manifest" > /dev/null
cargo metadata --locked --offline --no-deps --format-version=1 > "$scratch/packages.json"
invalid_cells=$(jq -ber '.exit_codes.invalid_cells' "$script_directory/test-inventory.json")
invalid_targets=$(jq -ber '.exit_codes.invalid_targets' "$script_directory/test-inventory.json")
jq -bse --slurpfile configuration "$script_directory/test-inventory.json" '
    last as $completion | .[:-1] as $cells
    | $completion.kind == "complete"
    and ($cells | group_by(.old_id) | all(length == 1))
    and (($cells | group_by(.domain) | map({domain: .[0].domain, count: length}))
        == $completion.counts)
    and (($completion.counts | map(.domain)) ==
        ([$configuration[0].matrices[].target, "arc_differential"] | sort))
' "$cell_inventory" > /dev/null || exit "$invalid_cells"
jq -bse --slurpfile metadata "$scratch/packages.json" '
    $metadata[0] as $workspace
    | ([$workspace.packages[]
        | select(.id as $id | $workspace.workspace_members | index($id))
        | . as $package | .targets[] | select(.test)
        | [$package.id, .name, .kind]] | sort) as $expected
    | ([.[] | select(.reason == "compiler-artifact" and .profile.test
        and .executable != null) | [.package_id, .target.name, .target.kind]] | sort)
        == $expected
' "$artifact_manifest" > /dev/null || exit "$invalid_targets"

jq -br --slurpfile metadata "$scratch/packages.json" '
    select(.reason == "compiler-artifact" and .profile.test and .executable != null)
    | . as $artifact
    | $metadata[0].packages[] | select(.id == $artifact.package_id)
    | [.name, $artifact.target.name, $artifact.target.kind[0],
        (.manifest_path | sub("[/\\\\]Cargo.toml$"; "")), $artifact.executable] | @tsv
' "$artifact_manifest" > "$scratch/artifacts.tsv"

while IFS=$'\t' read -r package target kind directory executable; do
    (cd "$directory" && "$executable" --list --format terse) > "$scratch/all.txt"
    (cd "$directory" && "$executable" --list --ignored --format terse) > "$scratch/ignored.txt"
    jq -bRn --rawfile ignored "$scratch/ignored.txt" \
        --slurpfile configuration "$script_directory/test-inventory.json" \
        --arg package "$package" --arg target "$target" --arg kind "$kind" '
        inputs | select(endswith(": test")) | rtrimstr(": test") as $name
        | $configuration[0] as $config
        | ($package + "::" + $kind + "/" + $target + "::" + $name) as $identity
        | ($config.added_tests | index($identity) != null) as $added
        | (if $package == "bn" then $config.targets[$target] else "crate" end) as $harness
        | select($harness != null)
        | {old_id: $identity, domain: $target, harness: $harness,
            oracle: (if $added then "diagnostic" else "observed" end),
            new_target: (if $harness == "crate" then $target else $harness end),
            new_id: (if $harness == "crate" then $identity else "" end),
            status: (if $harness == "crate" then "migrated" else "open" end),
            reason: (if $added then $config.added
                elif $harness == "crate" then $config.retained else $config.pending end),
            ignored: ($ignored | split("\n") | index($name + ": test") != null)}
    ' < "$scratch/all.txt" >> "$scratch/rows.jsonl"
    if [[ $package == bn ]]; then
        jq -be --arg target "$target" '.targets | has($target)' \
            "$script_directory/test-inventory.json" > /dev/null
    fi
done < "$scratch/artifacts.tsv"

rg --files tests/grammar -g '*.bn' | LC_ALL=C sort | jq -bRn \
    --slurpfile configuration "$script_directory/test-inventory.json" '
    inputs | gsub("\\\\"; "/")
    | {old_id: ("fixture::" + .), domain: "grammar", harness: "programs",
        oracle: "observed", new_target: "programs", new_id: "", status: "open",
        reason: $configuration[0].pending, ignored: false}
' >> "$scratch/rows.jsonl"
jq -bc 'select(.kind != "complete")' "$cell_inventory" >> "$scratch/rows.jsonl"
jq -bse 'length > 0 and (group_by(.old_id) | all(length == 1))' \
    "$scratch/rows.jsonl" > /dev/null
jq -bsr '
    ["old_id", "domain", "harness", "oracle", "new_target", "new_id", "status", "reason", "ignored"],
    (sort_by(.old_id)[] | [.old_id, .domain, .harness, .oracle, .new_target,
        .new_id, .status, .reason, .ignored]) | @csv
' "$scratch/rows.jsonl" > "$scratch/ledger.csv"
mv "$scratch/ledger.csv" "$destination"
