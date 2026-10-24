// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use super::support::*;
use std::fs;
/// `Logger.New` with a file destination appends JSON Lines, and needs
/// `IMPORT HOST.FileSystem` (bnlog.md, "Making a logger").
#[test]
fn bnlog_file_logger_appends_json_lines_and_needs_the_filesystem_import() {
    let path = unique_temp("bnlog.jsonl");
    let program = |import: &str| {
        format!(
            "IMPORT BNLog AS Log\n{import}FUNCTION Start() AS VOID\nLET logger AS Log.Logger OR Error = Log.Logger.New(\"app\", \"JSON\", \"{}\")\nIF logger IS Error THEN\nPRINT logger.Code\nELSE\nLET event AS Log.Event\nevent.detail = \"x\"\nPRINT logger.Info(\"hello\", event) IS Error\nEND IF\nEND FUNCTION\n",
            bn_path(&path)
        )
    };
    let (_, refused) = run(&program(""), "").expect("execute BNLog without the import");
    assert_eq!(refused, "6\n");
    let (_, output) =
        run(&program("IMPORT HOST.FileSystem AS FS\n"), "").expect("execute BNLog file logger");
    let contents = fs::read_to_string(&path).expect("read JSON Lines output");
    let _ = fs::remove_file(path);
    assert_eq!(output, "FALSE\n");
    assert!(contents.contains("\"message\":\"hello\"") && contents.contains("\"detail\":\"x\""));
}

/// `"CONSOLE"` needs `IMPORT HOST.Console`; a record past the field-value
/// bound is refused and nothing is written (bnlog.md, "Records").
#[test]
fn bnlog_console_logger_needs_the_console_import_and_bounds_values() {
    let refused = r#"IMPORT BNLog AS Log
FUNCTION Start() AS VOID
LET logger AS Log.Logger OR Error = Log.Logger.New("app", "JSON", "CONSOLE")
IF logger IS Error THEN
PRINT logger.Code
END IF
END FUNCTION
"#;
    let (_, output) = run(refused, "").expect("execute BNLog without the console import");
    assert_eq!(output, "6\n");
    let bounded = r#"IMPORT BNLog AS Log
IMPORT HOST.Console AS CON
FUNCTION Start() AS VOID
LET logger AS Log.Logger OR Error = Log.Logger.New("app", "JSON", "CONSOLE")
IF logger IS Error THEN
RETURN
END IF
LET event AS Log.Event
LET long AS STRING = ""
FOR index AS INTEGER = 1 TO 4097
long = long + "x"
END FOR
event.detail = long
LET written AS VOID OR Error = logger.Info("too long", event)
IF written IS Error THEN
PRINT written.Code
END IF
END FUNCTION
"#;
    let (_, output) = run(bounded, "").expect("execute BNLog value bound");
    assert_eq!(output, "1\n");
}
