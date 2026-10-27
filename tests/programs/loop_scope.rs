// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Sibling loops that reuse their variable name run the same on both
//! backends (0.6.md, "Scopes and bindings").

use super::support::{TestDir, bni, compile_native, run, workspace_root};
use std::process::Command;
use std::time::Duration;

const PROCESS_TIMEOUT: Duration = Duration::from_secs(30);

const SIBLING_LOOPS_FIXTURE: &str = "tests/grammar/valid/for-variable-sibling-loops.bn";

/// Derived by hand from the fixture: 1+2+3 + 10+20+30 = 66; + 1+2 + 3+4 = 76;
/// "ab" + "cd"; + 1+2 (inside IF) + 1+2 = 82; + 1+2 = 85, then 85 + 100;
/// + 10+20 + 1000+1000 = 2115, then 2115 + 1, and "abcd" + "x".
const SIBLING_LOOPS_OUTPUT: &str = "66\n76\nabcd\n82\n185\n2116 abcdx\n";

#[test]
fn sibling_loop_variables_print_the_same_on_both_backends() {
    let source = workspace_root().join(SIBLING_LOOPS_FIXTURE);

    let interpreted =
        run(bni().arg("run").arg(&source), None, PROCESS_TIMEOUT).expect("run interpreter");
    assert!(
        interpreted.status.success(),
        "bni: {}",
        String::from_utf8_lossy(&interpreted.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&interpreted.stdout),
        SIBLING_LOOPS_OUTPUT
    );

    let directory = TestDir::new("loop-scope").expect("create directory");
    let artifact = compile_native(&source, &directory);
    let compiled =
        run(&mut Command::new(&artifact), None, PROCESS_TIMEOUT).expect("run compiled artifact");
    assert!(
        compiled.status.success(),
        "native: {}",
        String::from_utf8_lossy(&compiled.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&compiled.stdout),
        SIBLING_LOOPS_OUTPUT
    );
}
