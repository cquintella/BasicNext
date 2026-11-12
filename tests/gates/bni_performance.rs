// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! `bni` performance: a timing table of the
//! `tests/bench` programs and the linear-time gates. Wall-clock time depends
//! on the machine, so these are ignored by the ordinary run; measure a
//! release interpreter:
//! `bash scripts/test.sh --release --test gates -- --ignored bni_performance`
//! (it builds the release prerequisites and sets every override).

use super::support::{bni, run, workspace_root};
use std::time::{Duration, Instant};

const RUNS: usize = 5;

const PROCESS_TIMEOUT: Duration = Duration::from_secs(120);

/// Time ratio allowed when the input grows four times: linear work stays
/// near 4, quadratic work reaches 16.
const LINEAR_RATIO_CEILING: f64 = 6.0;

const TABLE: [&str; 6] = [
    "tests/bench/for_counted.bn",
    "tests/bench/while_loop.bn",
    "tests/bench/vector_read_4000.bn",
    "tests/bench/vector_read_16000.bn",
    "tests/bench/user_call_100000.bn",
    "tests/bench/user_call_400000.bn",
];

/// Median wall-clock time of `RUNS` runs of `program`, in milliseconds.
fn median_milliseconds(program: &str) -> f64 {
    let source = workspace_root().join(program);
    let mut times = (0..RUNS)
        .map(|_| {
            let started = Instant::now();
            let output =
                run(bni().arg("run").arg(&source), None, PROCESS_TIMEOUT).expect("run bni");
            assert!(output.status.success(), "{program} failed");
            started.elapsed().as_secs_f64() * 1000.0
        })
        .collect::<Vec<_>>();
    times.sort_by(f64::total_cmp);
    times[RUNS / 2]
}

#[test]
#[ignore = "wall-clock measurement; run in release (module doc)"]
fn bni_performance_table() {
    for program in TABLE {
        println!("{program}\t{:.1} ms", median_milliseconds(program));
    }
}

fn assert_linear(small: &str, large: &str) {
    let ratio = median_milliseconds(large) / median_milliseconds(small);
    assert!(
        ratio <= LINEAR_RATIO_CEILING,
        "{large} / {small} = {ratio:.1}, above {LINEAR_RATIO_CEILING} for 4x the input"
    );
}

#[test]
#[ignore = "wall-clock measurement; run in release (module doc)"]
fn bni_performance_vector_read_is_linear() {
    assert_linear(
        "tests/bench/vector_read_4000.bn",
        "tests/bench/vector_read_16000.bn",
    );
}

#[test]
#[ignore = "wall-clock measurement; run in release (module doc)"]
fn bni_performance_user_call_is_linear() {
    assert_linear(
        "tests/bench/user_call_100000.bn",
        "tests/bench/user_call_400000.bn",
    );
}

/// The per-iteration program of the instruction-loop budget.
const LOOP_ADD: &str = "tests/bench/loop_add.bn";
/// Median wall-clock budget of `LOOP_ADD` on the reference host (an Apple
/// M-series laptop, release build): 5 000 000 iterations at 300 ns each.
/// At `ae7c41d` it took 1 390 ms; with the observer-free loop, 1 210 ms.
const LOOP_ADD_BUDGET_MS: f64 = 1500.0;

#[test]
#[ignore = "wall-clock measurement; run in release (module doc)"]
fn bni_performance_loop_add_stays_within_budget() {
    let median = median_milliseconds(LOOP_ADD);
    assert!(
        median <= LOOP_ADD_BUDGET_MS,
        "{LOOP_ADD}: {median:.1} ms, budget {LOOP_ADD_BUDGET_MS} ms"
    );
}

/// The watchers a run may have; the loop over a block's instructions reads
/// none of them, so a plain run pays nothing for the debugger and the tracer.
const OBSERVERS: [&str; 4] = [
    "self.debug_hook",
    "self.emitter",
    "self.cancel_flag",
    "self.debug_control",
];
const INSTRUCTION_LOOP: &str = "crates/bn_interp/src/executor/part1.rs";

/// The text of the loop over a block's instructions in `source`.
fn instruction_loop(source: &str) -> Option<&str> {
    let start = source.find("for (index, instruction) in current.instructions")?;
    let end = start + source[start..].find("match &current.terminator")?;
    Some(&source[start..end])
}

#[test]
fn bni_performance_instruction_loop_reads_no_observer() {
    let source = std::fs::read_to_string(workspace_root().join(INSTRUCTION_LOOP))
        .expect("read the instruction loop");
    let body = instruction_loop(&source).expect("the instruction loop");
    let read = OBSERVERS
        .iter()
        .filter(|observer| body.contains(**observer))
        .collect::<Vec<_>>();
    assert!(read.is_empty(), "the instruction loop reads {read:?}");
}
