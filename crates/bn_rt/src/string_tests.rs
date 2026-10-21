// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Re-executes one string test in a child process so a trap or a live-count
//! check is not mixed with the rest of the suite.

use std::process::Output;

pub(super) fn run_exact_child(name: &str, child_env: &str) -> Output {
    std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", name])
        .env(child_env, "1")
        .output()
        .expect("spawn string test child")
}
