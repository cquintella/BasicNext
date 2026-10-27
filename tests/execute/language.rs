// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use super::support::*;
use bn_interp::{DebugDecision, execute_with_host_debug_control};
use std::{fs, io::Cursor, process::Command};
#[test]
fn executes_calls_loops_and_checked_arithmetic() {
    let source = "FUNCTION Factorial(value AS INTEGER) AS INTEGER\nIF value = 0 THEN\nRETURN 1\nELSE\nRETURN value * Factorial(value - 1)\nEND IF\nEND FUNCTION\nFUNCTION Start() AS INTEGER\nLET total AS INTEGER = Factorial(5)\nLET quotient AS INTEGER = -5 DIV 3\nLET remainder AS INTEGER = -5 % 3\nPRINT total, quotient, remainder\nRETURN 7\nEND FUNCTION\n";
    let (code, output) = run(source, "").expect("execute source");
    assert_eq!(code, 7);
    assert_eq!(output, "120 -2 1\n");
}

#[test]
fn debug_hook_reports_instruction_spans_through_interpreter_pipeline() {
    let path = unique_temp("debug.bn");
    fs::write(
        &path,
        "FUNCTION Start() AS VOID\nPRINT \"hooked\"\nEND FUNCTION\n",
    )
    .expect("write debug source");
    let mut events = Vec::new();
    run_loaded_debug(
        path.to_str().expect("utf-8 path"),
        &fixed(vec!["debug.bn".into()], 0, 0),
        &mut events,
    )
    .expect("execute with debug hook");
    let _ = fs::remove_file(path);
    assert!(
        events
            .iter()
            .any(|(function, line)| function == "Start" && *line == 2)
    );
}

#[test]
fn debug_control_can_terminate_before_user_instruction() {
    let path = unique_temp("debug-control.bn");
    fs::write(
        &path,
        "FUNCTION Start() AS INTEGER\nPRINT \"must not run\"\nRETURN 0\nEND FUNCTION\n",
    )
    .unwrap();
    let graph = load(&path).unwrap();
    let models = analyze_modules(&graph).unwrap();
    let module = lower_graph(&graph, &models).unwrap();
    let mut input = Cursor::new(Vec::<u8>::new());
    let mut output = Vec::new();
    let mut control =
        |_function: &str, _depth: usize, _span: bn_source::Span, _view: &bn_interp::DebugView| {
            DebugDecision::Terminate
        };
    let error = execute_with_host_debug_control(
        &module,
        &mut input,
        &mut output,
        &fixed(Vec::new(), 0, 0),
        &mut control,
    )
    .expect_err("debugger termination");
    assert_eq!(error.code, "DEBUG_TERMINATED");
    assert!(output.is_empty());
    let _ = fs::remove_file(path);
}

/// Bucket 0.6.10 F.1 lock: a pause inside a hot counted loop sees the
/// frame's bindings with the values of that iteration, after the dense
/// frame slots replaced the per-call maps (F.2).
#[test]
fn debug_view_inside_a_hot_loop_shows_the_iteration_values() {
    const LOOP_BODY_LINE: usize = 4;
    const PAUSED_ITERATION: &str = "Integer(500, Int32)";
    let path = unique_temp("debug-hot-loop.bn");
    fs::write(
        &path,
        "FUNCTION Start() AS VOID\nLET total AS INTEGER = 0\nFOR i AS INTEGER = 1 TO 1000\ntotal = total + i\nEND FOR\nPRINT total\nEND FUNCTION\n",
    )
    .unwrap();
    let graph = load(&path).unwrap();
    let models = analyze_modules(&graph).unwrap();
    let module = lower_graph(&graph, &models).unwrap();
    let mut input = Cursor::new(Vec::<u8>::new());
    let mut output = Vec::new();
    let mut paused = None;
    let mut control =
        |_function: &str, _depth: usize, span: bn_source::Span, view: &bn_interp::DebugView| {
            // The view names bindings by symbol id, so the lock reads their values.
            let mut bindings = view
                .variables
                .iter()
                .filter(|variable| variable.name.starts_with("symbol#"))
                .map(|variable| variable.value.clone())
                .collect::<Vec<_>>();
            bindings.sort();
            if span.start.line == LOOP_BODY_LINE
                && bindings.iter().any(|value| value == PAUSED_ITERATION)
            {
                paused = Some(bindings);
                return DebugDecision::Terminate;
            }
            DebugDecision::Continue
        };
    let error = execute_with_host_debug_control(
        &module,
        &mut input,
        &mut output,
        &fixed(Vec::new(), 0, 0),
        &mut control,
    )
    .expect_err("debugger termination");
    let _ = fs::remove_file(path);
    assert_eq!(error.code, "DEBUG_TERMINATED");
    // total holds 1 + … + 499 when the body of iteration 500 starts.
    assert_eq!(
        paused,
        Some(vec![
            "Integer(124750, Int32)".to_string(),
            PAUSED_ITERATION.to_string()
        ])
    );
    assert!(output.is_empty());
}

#[test]
fn multi_binding_let_evaluates_left_to_right() {
    let source = "FUNCTION Start() AS VOID\nLET first, second AS INTEGER = 1, 2\nPRINT first, second\nEND FUNCTION\n";
    let (_, output) = run(source, "").expect("execute multi-binding LET");
    assert_eq!(output, "1 2\n");
}

#[test]
fn single_line_if_executes_one_branch() {
    let source = "FUNCTION Start() AS VOID\nLET flag AS BOOLEAN = TRUE\nIF flag THEN PRINT \"yes\" ELSE PRINT \"no\"\nEND FUNCTION\n";
    let (_, output) = run(source, "").expect("execute single-line IF");
    assert_eq!(output, "yes\n");
}

#[test]
fn boolean_and_or_short_circuit() {
    let source = "FUNCTION Unexpected() AS BOOLEAN\nPRINT \"unexpected\"\nRETURN TRUE\nEND FUNCTION\nFUNCTION Start() AS VOID\nIF FALSE AND Unexpected() THEN\nPRINT \"bad\"\nEND IF\nIF TRUE OR Unexpected() THEN\nPRINT \"ok\"\nEND IF\nEND FUNCTION\n";
    let (_, output) = run(source, "").expect("execute source");
    assert_eq!(output, "ok\n");
}

#[test]
fn converts_timestamps_to_utc_components() {
    let source = "IMPORT BNMath AS Math\nFUNCTION Start() AS VOID\nPRINT Math.TOHOUR(0 AS TIMESTAMP), Math.TOWEEKDAY(0 AS TIMESTAMP)\nPRINT Math.TOHOUR(-1 AS TIMESTAMP), Math.TOWEEKDAY(-1 AS TIMESTAMP)\nEND FUNCTION\n";
    let (_, output) = run(source, "").expect("execute timestamp conversion");
    assert_eq!(output, "0 4\n23 3\n");
}

#[test]
fn stop_propagates_through_function_calls() {
    let source = "FUNCTION Halt() AS VOID\nSTOP 23\nEND FUNCTION\nFUNCTION Start() AS VOID\nHalt()\nEND FUNCTION\n";
    let (code, output) = run(source, "").expect("execute source");
    assert_eq!(code, 23);
    assert!(output.is_empty());
}

#[test]
fn executes_imported_function_through_alias() {
    let (code, output) =
        run_path("tests/modules/user-alias/main.bn").expect("execute imported Soma");
    assert_eq!(code, 0);
    assert_eq!(output, "3\n");
}

#[test]
fn executes_cls_and_beep_through_host_console() {
    let source = include_str!("../grammar/valid/cls-and-beep.bn");
    let (code, output) = run(source, "").expect("execute CLS and BEEP");
    assert_eq!(code, 0);
    assert_eq!(output, "\u{1b}[2J\u{1b}[H\u{7}ok\n");
}

#[test]
fn console_tty_calls_fail_only_when_executed() {
    let skipped = "IMPORT HOST.Console AS CON\nFUNCTION Start() AS VOID\nIF FALSE THEN\nCON.PrintAt(1, 1, \"x\")\nEND IF\nEND FUNCTION\n";
    run(skipped, "").expect("unexecuted TTY call must be valid");
    let executed =
        "IMPORT HOST.Console AS CON\nFUNCTION Start() AS VOID\nCON.NumCols()\nEND FUNCTION\n";
    let path =
        std::env::temp_dir().join(format!("basicnext-console-pipe-{}.bn", std::process::id()));
    fs::write(&path, executed).expect("write piped console fixture");
    // The interpreter executable from the workspace target directory (this
    // package has no binary; `cargo build -p bni` first).
    let bni = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join(if cfg!(debug_assertions) {
            "debug"
        } else {
            "release"
        })
        .join(format!("bni{}", std::env::consts::EXE_SUFFIX));
    assert!(bni.is_file(), "bni must be built first: cargo build -p bni");
    let output = Command::new(bni)
        .args(["run", path.to_str().expect("UTF-8 fixture path")])
        .output()
        .expect("run with captured standard output");
    let _ = fs::remove_file(path);
    assert_eq!(output.status.code(), Some(1));
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("HOST_CAPABILITY_UNAVAILABLE"),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// Bucket 0.5.1d §1.1: a library provider registered from outside the crate
/// answers a library callee and re-enters the core through `CoreContext`.
#[test]
fn library_provider_seam_routes_calls_and_reenters_the_core() {
    use bn_interp::provider::{CoreContext, Provider, Providers};
    use std::sync::Arc;

    struct Fake;
    impl Provider for Fake {
        fn call(
            &mut self,
            core: &mut dyn CoreContext,
            member: &str,
            _arguments: Vec<bn_value::Value>,
            span: bn_source::Span,
        ) -> Result<bn_value::Value, bn_diag::Diagnostic> {
            assert_eq!(member, "ABS");
            let helper = core.call_function("Helper", Vec::new(), span)?;
            let bn_value::Value::Integer(seven, kind) = helper else {
                panic!("Helper returns INTEGER");
            };
            Ok(bn_value::Value::Integer(seven * 6, kind))
        }
    }
    let mut libraries = Providers::default();
    libraries.register("BNMath", Arc::new(|| Box::new(Fake)));
    let host = fixed(vec!["runtime.bn".into()], 0, 0).with_libraries(libraries);
    let source = "IMPORT BNMath AS M\nFUNCTION Helper() AS INTEGER\n    RETURN 7\nEND FUNCTION\nFUNCTION Start() AS VOID\n    PRINT M.ABS(-1)\nEND FUNCTION\n";
    let (code, output) = run_with_host(source, "", &host).expect("fake provider answers");
    assert_eq!(code, 0);
    assert_eq!(output, "42\n");

    // No provider registered under the library: unavailable, never a language error.
    let bare = fixed(vec!["runtime.bn".into()], 0, 0).with_libraries(Providers::default());
    let source = "IMPORT BNMath AS M\nFUNCTION Start() AS VOID\n    LET v AS FLOAT = M.SQRT(4.0)\n    PRINT v\nEND FUNCTION\n";
    let error = run_with_host(source, "", &bare).expect_err("unregistered library is an error");
    assert_eq!(error.code, "LIBRARY_PROVIDER_UNAVAILABLE");
    assert!(
        error.message.contains("BNMath provider unavailable"),
        "{}",
        error.message
    );
}

#[test]
fn rfc3339_and_civil_values_round_trip_through_utc() {
    let source = r#"
IMPORT BNMath AS Math
FUNCTION Start() AS VOID
    LET epoch AS TIMESTAMP = Timestamp.Parse("1970-01-01T00:00:00.000Z")
    LET offset AS TIMESTAMP = Timestamp.Parse("1970-01-01T00:00:00.000-03:00")
    LET date AS DATE = Date.Parse("2026-08-22")
    LET time AS TIME = Time.Parse("10:20:30.000")
    LET zone AS TIMEZONE = TimeZone.Parse("America/Sao_Paulo")
    LET combined AS TIMESTAMP = Math.TOTIMESTAMP(date, time)
    PRINT epoch, offset, Timestamp.Format(epoch)
    PRINT date, time, zone
    PRINT Math.TODATE(0 AS TIMESTAMP), Math.TOTIME(0 AS TIMESTAMP), SIZEOF(date), SIZEOF(time)
    PRINT combined, Date.Parse("1970-01-01") = Math.TODATE(0 AS TIMESTAMP)
END FUNCTION
"#;
    let (code, output) = run(source, "").expect("execute temporal values");
    assert_eq!(code, 0);
    assert_eq!(
        output,
        "0 10800000 1970-01-01T00:00:00.000Z\n2026-08-22 10:20:30.000 America/Sao_Paulo\n1970-01-01 00:00:00.000 4 4\n1787394030000 TRUE\n"
    );
}

#[test]
fn timestamp_parse_accepts_rfc3339_fraction_widths() {
    let source = r#"
FUNCTION Start() AS VOID
    PRINT Timestamp.Format(Timestamp.Parse("2020-01-01T00:00:00.5Z"))
    PRINT Timestamp.Format(Timestamp.Parse("2020-01-01T00:00:00.05Z"))
    PRINT Timestamp.Format(Timestamp.Parse("2020-01-01T00:00:00.0050Z"))
END FUNCTION
"#;
    let (code, output) = run(source, "").expect("parse RFC 3339 fractions");
    assert_eq!(code, 0);
    assert_eq!(
        output,
        "2020-01-01T00:00:00.500Z\n2020-01-01T00:00:00.050Z\n2020-01-01T00:00:00.005Z\n"
    );
}

#[test]
fn language_tour_executes_through_ir() {
    let (code, output) = run_path("examples/language-tour.bn").expect("execute language tour");
    assert_eq!(code, 0);
    assert!(output.contains("Basic Next"));
    assert!(output.contains("counter deleted"));
}

#[test]
fn shortest_path_example_runs() {
    let (code, output) = run_path("examples/shortest_path.bn").expect("execute shortest path");
    assert_eq!(code, 0);
    assert_eq!(output, "6\n");
}

#[test]
fn temporal_parse_rejects_invalid_civil_and_rfc3339_forms() {
    let error = run(
        r#"FUNCTION Start() AS VOID
PRINT Date.Parse("2026-02-30")
END FUNCTION
"#,
        "",
    )
    .expect_err("invalid DATE");
    assert_eq!(error.code, "INVALID_DATE");
    let error = run(
        r#"FUNCTION Start() AS VOID
PRINT Time.Parse("24:00:00.000")
END FUNCTION
"#,
        "",
    )
    .expect_err("invalid TIME");
    assert_eq!(error.code, "INVALID_TIME");
    let error = run(
        r#"FUNCTION Start() AS VOID
PRINT Timestamp.Parse("2026-08-22T10:20:30.0001Z")
END FUNCTION
"#,
        "",
    )
    .expect_err("excess TIMESTAMP precision");
    assert_eq!(error.code, "PARSE_ERROR");
    let error = run(
        r#"FUNCTION Start() AS VOID
PRINT TimeZone.Parse("-03:00")
END FUNCTION
"#,
        "",
    )
    .expect_err("offset is not a TIMEZONE");
    assert_eq!(error.code, "INVALID_TIMEZONE");
}

#[test]
fn stop_releases_the_locals_of_every_running_function() {
    // 0.6.md, "`STOP`" (BDFL rule A, 2026-10-06): the function that stops,
    // then each caller; in each, temporaries, then scopes innermost first.
    let source = r#"
CLASS Tag
    PUBLIC name AS STRING

    PUBLIC FUNCTION CONSTRUCTOR(name AS STRING)
        SELF.name = name
    END FUNCTION

    PUBLIC FUNCTION DESTRUCTOR()
        PRINT "gone " + SELF.name
    END FUNCTION
END CLASS

FUNCTION Check(tag AS Tag) AS INTEGER
    LET inner AS Tag = NEW Tag("inner")
    PRINT "checking " + tag.name
    STOP 4
END FUNCTION

FUNCTION Use(tag AS Tag, n AS INTEGER) AS VOID
    PRINT "not reached"
END FUNCTION

FUNCTION Start() AS VOID
    LET outer AS Tag = NEW Tag("outer")
    IF outer.name = "outer" THEN
        LET block AS Tag = NEW Tag("block")
        Use(NEW Tag("temporary"), Check(outer))
    END IF
    PRINT "not reached"
END FUNCTION
"#;
    let (code, output) = run(source, "").expect("STOP inside a callee");
    assert_eq!(
        (code, output.as_str()),
        (
            4,
            "checking outer\ngone inner\ngone temporary\ngone block\ngone outer\n"
        )
    );
}

#[test]
fn timezone_conversion_and_dst_round_trip() {
    let source = r#"
FUNCTION Start() AS VOID
    LET utc AS TIMEZONE = TimeZone.Parse("UTC")
    LET sp AS TIMEZONE = TimeZone.Parse("America/Sao_Paulo")
    LET lon AS TIMEZONE = TimeZone.Parse("Europe/London")
    LET ny AS TIMEZONE = TimeZone.Parse("America/New_York")
    LET tyo AS TIMEZONE = TimeZone.Parse("Asia/Tokyo")

    LET tsWinter AS TIMESTAMP = Timestamp.Parse("2026-01-15T12:00:00.000Z")
    LET tsSummer AS TIMESTAMP = Timestamp.Parse("2026-07-15T12:00:00.000Z")

    PRINT TimeZone.Offset(utc, tsWinter), TimeZone.IsDST(utc, tsWinter)
    PRINT TimeZone.Offset(sp, tsWinter), TimeZone.IsDST(sp, tsWinter)
    PRINT TimeZone.Offset(tyo, tsWinter), TimeZone.IsDST(tyo, tsWinter)
    PRINT TimeZone.Offset(lon, tsWinter), TimeZone.IsDST(lon, tsWinter)
    PRINT TimeZone.Offset(lon, tsSummer), TimeZone.IsDST(lon, tsSummer)
    PRINT TimeZone.Offset(ny, tsWinter), TimeZone.IsDST(ny, tsWinter)
    PRINT TimeZone.Offset(ny, tsSummer), TimeZone.IsDST(ny, tsSummer)

    PRINT Timestamp.AtZone(tsSummer, sp)
    PRINT Timestamp.AtZone(tsSummer, lon)
    PRINT Timestamp.AtZone(tsSummer, ny)
    PRINT Timestamp.AtZone(tsSummer, tyo)

    LET localDate AS DATE = Date.Parse("2026-07-15")
    LET localTime AS TIME = Time.Parse("09:00:00.000")
    LET backTs AS TIMESTAMP = Timestamp.FromLocal(localDate, localTime, sp)
    PRINT backTs = tsSummer
END FUNCTION
"#;
    let (code, output) = run(source, "").expect("execute timezone conversions");
    assert_eq!(code, 0);
    assert_eq!(
        output,
        "0 FALSE\n\
         -10800 FALSE\n\
         32400 FALSE\n\
         0 FALSE\n\
         3600 TRUE\n\
         -18000 FALSE\n\
         -14400 TRUE\n\
         2026-07-15T09:00:00.000-03:00\n\
         2026-07-15T13:00:00.000+01:00\n\
         2026-07-15T08:00:00.000-04:00\n\
         2026-07-15T21:00:00.000+09:00\n\
         TRUE\n"
    );
}
