// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use super::support::*;
use std::fs;
#[test]
fn sqlite_in_memory_basic_lifecycle() {
    let source = r#"IMPORT BNData AS Data
IMPORT BNSqlite AS Sqlite

FUNCTION Start() AS VOID
LET db AS Sqlite.Connection OR Error = Sqlite.Open(":memory:")
IF db IS Error THEN
    PRINT "open error", db.Code
    RETURN
END IF
LET res AS VOID OR Error = db.Exec("CREATE TABLE items (id INT, name STRING, val FLOAT);")
IF res IS Error THEN
    PRINT "create error", res.Code
    RETURN
END IF
res = db.Exec("INSERT INTO items (id, name, val) VALUES (42, 'item42', 3.14);")
IF res IS Error THEN
    PRINT "insert error", res.Code
    RETURN
END IF
PRINT db.Changes(), db.LastInsertRowId()
LET df AS Data.DataFrame OR Error = db.Query("SELECT id, name, val FROM items;")
IF df IS Error THEN
    PRINT "query error", df.Code
    RETURN
END IF
PRINT df.RowCount(), df.ColumnCount()
PRINT df.GetInteger(0, "id"), df.GetString(0, "name"), df.GetFloat(0, "val")
res = db.Close()
IF res IS Error THEN
    PRINT "close error", res.Code
END IF
END FUNCTION
"#;
    let (_, output) = run(source, "").expect("execute sqlite in-memory lifecycle");
    assert_eq!(output, "1 1\n1 3\n42 item42 3.14\n");
}

#[test]
fn sqlite_misuse_and_closed_errors() {
    let source = r#"IMPORT BNData AS Data
IMPORT BNSqlite AS Sqlite

FUNCTION Start() AS VOID
LET db AS Sqlite.Connection OR Error = Sqlite.Open(":memory:")
IF db IS Error THEN
    RETURN
END IF
LET misuse_exec AS VOID OR Error = db.Exec("SELECT 1;")
IF misuse_exec IS Error THEN
    PRINT "exec-misuse", misuse_exec.Code, misuse_exec.Operation
END IF
LET misuse_query AS Data.DataFrame OR Error = db.Query("CREATE TABLE x (id INT);")
IF misuse_query IS Error THEN
    PRINT "query-misuse", misuse_query.Code, misuse_query.Operation
END IF
LET close_res AS VOID OR Error = db.Close()
IF close_res IS Error THEN
    PRINT "close error", close_res.Code
END IF
LET double_close AS VOID OR Error = db.Close()
IF double_close IS Error THEN
    PRINT "double-close", double_close.Code
END IF
LET after_close AS VOID OR Error = db.Exec("CREATE TABLE y (id INT);")
IF after_close IS Error THEN
    PRINT "after-close", after_close.Code
END IF
END FUNCTION
"#;
    let (_, output) = run(source, "").expect("execute sqlite misuse and closed errors");
    assert_eq!(
        output,
        "exec-misuse 12 BNSqlite.Connection.Exec\nquery-misuse 12 BNSqlite.Connection.Query\ndouble-close 13\nafter-close 13\n"
    );
}

#[test]
fn sqlite_transactions_and_rollback() {
    let source = r#"IMPORT BNData AS Data
IMPORT BNSqlite AS Sqlite

FUNCTION Start() AS VOID
LET db AS Sqlite.Connection OR Error = Sqlite.Open(":memory:")
IF db IS Error THEN
    RETURN
END IF
LET res AS VOID OR Error = db.Exec("CREATE TABLE tx_test (val INT);")
res = db.Begin()
res = db.Exec("INSERT INTO tx_test VALUES (10);")
res = db.Rollback()
LET df1 AS Data.DataFrame OR Error = db.Query("SELECT val FROM tx_test;")
IF NOT (df1 IS Error) THEN
    PRINT "after-rollback", df1.RowCount()
END IF
res = db.Begin()
res = db.Exec("INSERT INTO tx_test VALUES (20);")
res = db.Commit()
LET df2 AS Data.DataFrame OR Error = db.Query("SELECT val FROM tx_test;")
IF NOT (df2 IS Error) THEN
    PRINT "after-commit", df2.RowCount(), df2.GetInteger(0, "val")
END IF
db.Close()
END FUNCTION
"#;
    let (_, output) = run(source, "").expect("execute sqlite transactions");
    assert_eq!(output, "after-rollback 0\nafter-commit 1 20\n");
}

#[test]
fn sqlite_open_modes() {
    let temp_db = unique_temp("modes.db");
    let temp_path = bn_path(&temp_db);

    let source = format!(
        r#"IMPORT BNData AS Data
IMPORT BNSqlite AS Sqlite

FUNCTION Start() AS VOID
LET missing AS Sqlite.Connection OR Error = Sqlite.OpenExisting("{temp_path}")
IF missing IS Error THEN
    PRINT "missing", missing.Code
END IF
LET db AS Sqlite.Connection OR Error = Sqlite.Open("{temp_path}")
IF db IS Error THEN
    PRINT "open error", db.Code
    RETURN
END IF
LET res AS VOID OR Error = db.Exec("CREATE TABLE t (id INT, val STRING);")
res = db.Exec("INSERT INTO t VALUES (1, 'readonly-test');")
db.Close()

LET ro AS Sqlite.Connection OR Error = Sqlite.OpenReadOnly("{temp_path}")
IF ro IS Error THEN
    PRINT "ro open error", ro.Code
    RETURN
END IF
LET df AS Data.DataFrame OR Error = ro.Query("SELECT id, val FROM t;")
IF NOT (df IS Error) THEN
    PRINT "read ok", df.RowCount(), df.GetString(0, "val")
END IF
LET write_res AS VOID OR Error = ro.Exec("INSERT INTO t VALUES (2, 'fail');")
IF write_res IS Error THEN
    PRINT "write fail", write_res.Code
END IF
ro.Close()
END FUNCTION
"#
    );

    let (_, output) = run(&source, "").expect("execute sqlite open modes");
    let _ = fs::remove_file(&temp_db);
    assert_eq!(output, "missing 1\nread ok 1 readonly-test\nwrite fail 7\n");
}
