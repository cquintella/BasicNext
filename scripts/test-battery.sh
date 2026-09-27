#!/usr/bin/env bash
# Sprint-close test battery: one build, then every workspace test binary run
# directly — longest first, a bounded pool of concurrent binaries — followed
# by the doc-tests. Never overlaps two `cargo test` processes (the
# 2026-09-17 low-memory incident); the only concurrency is between already
# compiled test executables. Timings are recorded per binary so the next
# run orders by measured duration.
#
# Concurrency (JOBS): the executable suites (bni/bnc `cli`, `parity`,
# `runtime`) compile helpers with rustc/clang and run them at once. On macOS
# every freshly built executable goes through a Gatekeeper scan before its
# first exec; with two such suites overlapping the scan queue stalls
# children for minutes and HOST.Exec tests fail with EXEC_TIMEOUT (code 9).
# Measured 2026-09-22: JOBS=3 → parity 1413 s, JOBS=2 → runtime 164 s and
# one timeout, JOBS=1 → clean. Keep JOBS=1 unless the suites stop spawning.
#
#   scripts/test-battery.sh            # sequential binaries, --test-threads=4 each
#   JOBS=2 THREADS=2 scripts/test-battery.sh
#   DOCTESTS=1 scripts/test-battery.sh # also run `cargo test --doc` (rustdoc
#                                      # recompiles every crate: ~9 min for a
#                                      # workspace that has no doc-tests today)
#
# Exit status: 0 when every binary and the doc-tests pass.
set -u
cd "$(dirname "$0")/.."

JOBS=${JOBS:-1}   # see the note on concurrency below
THREADS=${THREADS:-4}
out=$PWD/target/test-battery      # absolute: binaries run from their package dir
timings=$PWD/target/test-timings.txt
mkdir -p "$out"
rm -f "$out"/*.log "$out"/*.status
touch "$timings"

command -v jq >/dev/null || { echo "test-battery: jq is required" >&2; exit 2; }

# 1. Build every test target once (no concurrent compilation later).
echo "== building test targets"
manifest=$(cargo test --workspace --no-run --message-format=json 2>"$out/build.log") || {
  cat "$out/build.log" >&2; exit 2; }

# name<TAB>executable<TAB>cwd (cargo runs a test binary from its package dir)
binaries=$(printf '%s\n' "$manifest" | jq -r '
  select(.reason=="compiler-artifact" and .profile.test==true and .executable!=null)
  | "\(.manifest_path|sub("/Cargo.toml$";"")|sub("^.*/";""))-\(.target.name)-\(.target.kind[0])\t\(.executable)\t\(.manifest_path|sub("/Cargo.toml$";""))"')

# 2. Order: measured duration descending; never-measured binaries first
#    (unknown cost is treated as expensive).
ordered=$(printf '%s\n' "$binaries" | while IFS=$'\t' read -r name exe dir; do
  t=$(awk -v n="$name" '$1==n {print $2}' "$timings" | tail -1)
  printf '%s\t%s\t%s\t%s\n' "${t:-999999}" "$name" "$exe" "$dir"
done | sort -t$'\t' -k1,1nr)

total=$(printf '%s\n' "$ordered" | grep -c .)
echo "== running $total test binaries, pool=$JOBS, threads=$THREADS"

run_one() { # name exe dir
  local name=$1 exe=$2 dir=$3 start end
  start=$(date +%s)
  (cd "$dir" && "$exe" --test-threads="$THREADS" >"$out/$name.log" 2>&1)
  echo $? >"$out/$name.status"
  end=$(date +%s)
  printf '%s\t%s\n' "$name" "$((end - start))" >>"$timings.new"
  printf '   %4ss  %s  %s\n' "$((end - start))" \
    "$([[ $(cat "$out/$name.status") == 0 ]] && echo ok || echo FAIL)" "$name"
}

: >"$timings.new"
while IFS=$'\t' read -r _ name exe dir; do
  [[ -n $name ]] || continue
  while (( $(jobs -rp | wc -l) >= JOBS )); do sleep 0.5; done
  run_one "$name" "$exe" "$dir" &
done <<<"$ordered"
wait

# Keep the latest timing per binary.
sort -t$'\t' -k1,1 -u "$timings.new" >"$timings" && rm -f "$timings.new"

# 3. Doc-tests, opt-in (rustdoc compiles them itself; run alone, after the pool).
doc_status=0
if [[ ${DOCTESTS:-0} == 1 ]]; then
  echo "== doc-tests"
  cargo test --workspace --doc >"$out/doctests.log" 2>&1
  doc_status=$?
fi

# 4. Summary.
failed=0
for status in "$out"/*.status; do
  [[ $(cat "$status") == 0 ]] || { failed=$((failed + 1)); echo "FAILED: $(basename "${status%.status}")  (see $out/$(basename "${status%.status}").log)"; }
done
passed=$(grep -hE '^test result' "$out"/*.log | awk '{p+=$4} END {print p+0}')
failed_tests=$(grep -hE '^test result' "$out"/*.log | awk '{f+=$6} END {print f+0}')
echo "== tests: passed=$passed failed=$failed_tests binaries_failed=$failed doc-tests=$([[ ${DOCTESTS:-0} == 1 ]] && { [[ $doc_status == 0 ]] && echo ok || echo FAIL; } || echo skipped)"
[[ $failed == 0 && $doc_status == 0 ]]
