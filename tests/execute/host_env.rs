// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use super::support::*;
#[test]
fn host_num_procs_reports_a_positive_logical_processor_count() {
    let source = r#"FUNCTION Start() AS VOID
LET count AS INTEGER OR Error = HOST.NumProcs()
IF count IS Error THEN
PRINT "error"
ELSE
PRINT count > 0
END IF
END FUNCTION
"#;
    let (_, output) = run(source, "").expect("execute HOST.NumProcs");
    assert_eq!(output, "TRUE\n");
}

#[test]
fn host_random_seed_is_deterministic_and_bounded() {
    let source = "IMPORT HOST.Random AS R\nFUNCTION Start() AS VOID\nR.Seed(1)\nPRINT R.Random(), R.Random()\nEND FUNCTION\n";
    let (_, output) = run(source, "").expect("execute HOST.Random");
    assert_eq!(output, "0.28083505005035947 0.14023887919672862\n");
}

#[test]
fn host_args_exposes_injected_arguments() {
    let source = r"
FUNCTION Start() AS VOID
    PRINT LEN(HOST.Args)
    PRINT HOST.Args[0]
    PRINT HOST.Args[1]
END FUNCTION
";
    let host = fixed(vec!["prog.bn".into(), "ação".into()], 0, 0);
    let (code, output) = run_with_host(source, "", &host).expect("execute HOST.Args");
    assert_eq!(code, 0);
    assert_eq!(output, "2\nprog.bn\nação\n");
}

#[test]
fn host_args_exposes_count_and_immutable_strings() {
    let source = r"
FUNCTION Start() AS VOID
    PRINT LEN(HOST.Args)
    PRINT HOST.Args[0]
    PRINT HOST.Args[1]
END FUNCTION
";
    let host = fixed(vec!["/tmp/program.bn".into(), "ação".into()], 0, 0);
    let (code, output) = run_with_host(source, "", &host).expect("execute HOST.Args");
    assert_eq!(code, 0);
    assert_eq!(output, "2\n/tmp/program.bn\nação\n");
}

#[test]
fn host_clock_uses_injected_providers() {
    let source = r"
IMPORT HOST.Clock AS Clock

FUNCTION Start() AS VOID
    PRINT Clock.Now()
    PRINT Clock.Timer()
END FUNCTION
";
    let host = fixed(vec!["runtime.bn".into()], 1_000, 42);
    let (code, output) = run_with_host(source, "", &host).expect("execute HOST.Clock");
    assert_eq!(code, 0);
    assert_eq!(output, "1000\n42\n");
}

#[test]
fn host_argument_out_of_range_is_index_error() {
    let source = r"
FUNCTION Start() AS VOID
    PRINT HOST.Args[1]
END FUNCTION
";
    let error = run(source, "").expect_err("missing argument must fail");
    assert_eq!(error.code, "INDEX_OUT_OF_BOUNDS");
}
