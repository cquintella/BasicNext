// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! `bni` performance (bucket 0.6.10, cluster F): a timing table of the
//! `tests/bench` programs and the linear-time gates. Wall-clock time depends
//! on the machine, so these are ignored by the ordinary run; measure a
//! release interpreter:
//! `cargo build --release -p bni && BN_INTERPRETER=target/release/bni cargo test --release --test gates -- --ignored bni_performance`.

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
