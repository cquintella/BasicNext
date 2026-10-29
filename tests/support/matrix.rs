// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Conformance matrices (`ownership_matrix`, `alternatives_matrix`,
//! `value_matrix`): each cell is one BN program run on `bni` and `bnc` and
//! checked against the output the specification gives, with a ratchet list
//! of known failures per backend.

use std::{
    num::NonZero,
    sync::atomic::{AtomicUsize, Ordering},
    time::Duration,
};

use super::{ProcessOutput, TestDir, bnc, bni, run};

/// One program of a matrix: its name, its source and the exact output the
/// rules give.
pub struct Cell {
    pub name: String,
    pub source: String,
    pub expected: String,
}

/// Cells a backend does not run as the rules define, each with what it does
/// instead (a diagnostic code, or `WRONG`). A ratchet: remove a cell when it
/// passes; never add one to make a test green.
pub type KnownFailures = &'static [(&'static str, &'static str)];

const CELL_TIMEOUT: Duration = Duration::from_secs(60);

/// `Ok` when the program ended normally with the expected output; otherwise
/// the code of the first `error[...]` it reported, or `WRONG`.
pub fn outcome(output: &ProcessOutput, expected: &str) -> Result<(), String> {
    let stdout = String::from_utf8_lossy(&output.stdout);
    if output.status.success() && stdout == expected {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    let code = stderr
        .lines()
        .find_map(|line| line.split_once("error[").map(|(_, code)| code))
        .and_then(|code| code.split_once(']'))
        .map(|(code, _)| code.to_owned());
    Err(code.unwrap_or_else(|| "WRONG".to_owned()))
}

/// The outcome of `cell` on `bni` and on `bnc` (a failed build counts as the
/// native outcome).
fn run_on_both(cell: &Cell, directory: &TestDir, index: usize) -> [Result<(), String>; 2] {
    let path = directory.join(format!("cell{index}.bn"));
    std::fs::write(&path, &cell.source).expect("write cell");
    let mut interpreted = run(bni().arg("run").arg(&path), None, CELL_TIMEOUT).expect("run bni");
    let interpreted_result = outcome(&interpreted, &cell.expected);
    interpreted.accept_expected_status();
    let artifact = directory.join(format!("cell{index}{}", std::env::consts::EXE_SUFFIX));
    let mut built = run(
        bnc().arg(&path).arg("-o").arg(&artifact),
        None,
        CELL_TIMEOUT,
    )
    .expect("run bnc");
    let compiled_result = if built.status.success() {
        let mut compiled = run(
            &mut std::process::Command::new(&artifact),
            None,
            CELL_TIMEOUT,
        )
        .expect("run compiled cell");
        let result = outcome(&compiled, &cell.expected);
        compiled.accept_expected_status();
        result
    } else {
        let result = outcome(&built, &cell.expected);
        built.accept_expected_status();
        result
    };
    [interpreted_result, compiled_result]
}

/// The cells whose name contains the value of `filter_variable`, each source
/// printed to study it by hand; every cell when the variable is unset.
pub fn selected(cells: Vec<Cell>, filter_variable: &str) -> Vec<Cell> {
    let Ok(only) = std::env::var(filter_variable) else {
        return cells;
    };
    cells
        .into_iter()
        .filter(|cell| cell.name.contains(only.as_str()))
        .inspect(|cell| {
            eprintln!(
                "== {}\n{}-- expected\n{}",
                cell.name, cell.source, cell.expected
            );
        })
        .collect()
}

/// Runs every cell on both backends, on every core, and fails with one line
/// per cell that breaks the ratchet: an unlisted failure, a listed cell that
/// now passes, or a listed cell that fails for another reason.
pub fn run_matrix(
    cells: &[Cell],
    directory: &TestDir,
    interpreter_failures: KnownFailures,
    compiler_failures: KnownFailures,
) {
    let workers = std::thread::available_parallelism().map_or(1, NonZero::get);
    let next = AtomicUsize::new(0);
    let mut results = std::thread::scope(|scope| {
        let handles = (0..workers)
            .map(|_| {
                scope.spawn(|| {
                    let mut done = Vec::new();
                    loop {
                        let index = next.fetch_add(1, Ordering::Relaxed);
                        let Some(cell) = cells.get(index) else {
                            return done;
                        };
                        done.push((index, run_on_both(cell, directory, index)));
                    }
                })
            })
            .collect::<Vec<_>>();
        handles
            .into_iter()
            .flat_map(|handle| handle.join().expect("matrix worker"))
            .collect::<Vec<_>>()
    });
    results.sort_by_key(|(index, _)| *index);
    let mut report = Vec::new();
    // Every failing cell, so a changed list can be pasted from the output.
    let mut failing = Vec::new();
    for (index, outcomes) in results {
        let cell = cells[index].name.as_str();
        for ((backend, known), result) in
            [("bni", interpreter_failures), ("bnc", compiler_failures)]
                .into_iter()
                .zip(outcomes)
        {
            if let Err(reason) = &result {
                failing.push(format!("{backend}    (\"{cell}\", \"{reason}\"),"));
            }
            let listed = known.iter().find(|(listed, _)| *listed == cell);
            match (result, listed) {
                (Ok(()), Some(_)) => report.push(format!(
                    "{backend} `{cell}` now passes: remove it from its known-failure list"
                )),
                (Err(reason), None) => report.push(format!("{backend} `{cell}` fails: {reason}")),
                (Err(reason), Some((_, listed_reason))) if reason != *listed_reason => {
                    report.push(format!(
                        "{backend} `{cell}` fails differently: {reason}, listed {listed_reason}"
                    ));
                }
                _ => {}
            }
        }
    }
    assert!(
        report.is_empty(),
        "{}\n\nfailing cells:\n{}",
        report.join("\n"),
        failing.join("\n")
    );
}

const SANITIZED_TIMEOUT: Duration = Duration::from_secs(120);
const ADDRESS_SANITIZER_CLANG: &str =
    "#!/bin/sh\nexec clang -fsanitize=address -fno-omit-frame-pointer \"$@\"\n";
const WITHOUT_LEAK_DETECTION: &str = "detect_leaks=0";
const WITH_LEAK_DETECTION: &str = "detect_leaks=1";

/// Builds every cell natively with `AddressSanitizer` (through the
/// `[toolchain]` clang of `bnc`'s `config.toml`) and runs it. Returns one
/// line per cell that does not build, reports a sanitizer error (a double
/// release, a use after release, a read out of bounds) or prints other than
/// its expected output. Leak detection stays off: what a `STATIC` holds
/// lives until the process ends, by the language rules.
#[cfg(unix)]
pub fn sanitizer_failures(cells: &[Cell], label: &str) -> Vec<String> {
    sanitized_failures(cells, label, WITHOUT_LEAK_DETECTION)
}

/// As [`sanitizer_failures`], with leak detection on: a cell also fails when
/// a heap block is unreachable at exit. Only for cells that keep nothing in
/// a `STATIC`.
#[cfg(unix)]
pub fn leak_failures(cells: &[Cell], label: &str) -> Vec<String> {
    sanitized_failures(cells, label, WITH_LEAK_DETECTION)
}

#[cfg(unix)]
fn sanitized_failures(cells: &[Cell], label: &str, sanitizer_options: &str) -> Vec<String> {
    use std::os::unix::fs::PermissionsExt;
    let directory = TestDir::new(label).expect("create test directory");
    let wrapper = directory.join("asan-clang");
    std::fs::write(&wrapper, ADDRESS_SANITIZER_CLANG).expect("write clang wrapper");
    std::fs::set_permissions(&wrapper, std::fs::Permissions::from_mode(0o755))
        .expect("make the wrapper executable");
    std::fs::write(
        directory.join("config.toml"),
        format!("[toolchain]\nclang = \"{}\"\n", wrapper.display()),
    )
    .expect("write config.toml");
    let mut failures = Vec::new();
    for (index, cell) in cells.iter().enumerate() {
        let path = directory.join(format!("cell{index}.bn"));
        std::fs::write(&path, &cell.source).expect("write cell");
        let artifact = directory.join(format!("asan{index}"));
        let mut built = run(
            bnc()
                .current_dir(directory.path())
                .arg(&path)
                .arg("-o")
                .arg(&artifact),
            None,
            SANITIZED_TIMEOUT,
        )
        .expect("run bnc");
        built.accept_expected_status();
        if !built.status.success() {
            let stderr = String::from_utf8_lossy(&built.stderr);
            failures.push(format!("{}: build: {stderr}", cell.name));
            continue;
        }
        let mut ran = run(
            std::process::Command::new(&artifact).env("ASAN_OPTIONS", sanitizer_options),
            None,
            SANITIZED_TIMEOUT,
        )
        .expect("run the sanitized program");
        ran.accept_expected_status();
        let stderr = String::from_utf8_lossy(&ran.stderr);
        if !ran.status.success()
            || stderr.contains("AddressSanitizer")
            || stderr.contains("LeakSanitizer")
            || String::from_utf8_lossy(&ran.stdout) != cell.expected
        {
            failures.push(format!("{}: {stderr}", cell.name));
        }
    }
    failures
}
