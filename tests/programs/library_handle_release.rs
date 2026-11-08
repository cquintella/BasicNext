// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! `BNJson`, `BNData`, `BNSqlite` and `BNCompress` handles are counted: the
//! end of a binding releases the handle without `RELEASE`, a copy keeps its own
//! count, and a second release of an emptied binding does nothing. A compiled
//! loop of 100,000 scopes stays under a fixed resident ceiling.

use super::support::{TestDir, bni, compile_native, run, workspace_root};
use std::fs;
use std::process::Command;
use std::time::Duration;

const PROCESS_TIMEOUT: Duration = Duration::from_secs(180);
const SCOPES: u32 = 100_000;
const PAYLOAD_BYTES: usize = 4096;
const RESIDENT_CEILING_BYTES: i64 = 64 * 1024 * 1024;

const KEPT_PROGRAM: &str = "tests/programs/library_handle_kept.bn";
const KEPT_OUTPUT: &str = "hello\n0\n0\n4\n";

#[test]
fn library_handles_end_with_their_binding_on_both_backends() {
    let source = workspace_root().join(KEPT_PROGRAM);
    let interpreted = run(bni().arg("run").arg(&source), None, PROCESS_TIMEOUT).expect("run bni");
    assert!(
        interpreted.status.success(),
        "{}",
        String::from_utf8_lossy(&interpreted.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&interpreted.stdout), KEPT_OUTPUT);
    let directory = TestDir::new("library-handle-kept").expect("directory");
    let artifact = compile_native(&source, &directory);
    let compiled = run(&mut Command::new(&artifact), None, PROCESS_TIMEOUT).expect("run artifact");
    assert!(
        compiled.status.success(),
        "{}",
        String::from_utf8_lossy(&compiled.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&compiled.stdout), KEPT_OUTPUT);
}

#[cfg(unix)]
#[test]
fn a_compiled_loop_of_library_handles_keeps_memory_flat() {
    let payload = "a".repeat(PAYLOAD_BYTES);
    let cases = [
        (
            "json",
            "IMPORT BNJson AS Json\n",
            "FUNCTION Once(text AS STRING) AS INTEGER\n    \
             LET doc AS Json.Json = Json.Json.Object()\n    \
             LET s AS VOID OR Error = Json.Json.SetString(doc, \"key\", text)\n    \
             RETURN 1\nEND FUNCTION\n",
            i64::from(SCOPES),
            true,
        ),
        (
            "data",
            "IMPORT BNData AS Data\n",
            "FUNCTION Once(text AS STRING) AS INTEGER\n    \
             LET df AS Data.DataFrame = NEW Data.DataFrame()\n    \
             RETURN df.RowCount()\nEND FUNCTION\n",
            0,
            false,
        ),
        (
            "sqlite",
            "IMPORT BNData AS Data\nIMPORT BNSqlite AS Sqlite\n",
            "FUNCTION Once(text AS STRING) AS INTEGER\n    \
             LET conn AS Sqlite.Connection OR Error = Sqlite.Open(\":memory:\")\n    \
             IF conn IS Error THEN\n        STOP 1\n    END IF\n    \
             RETURN 1\nEND FUNCTION\n",
            i64::from(SCOPES),
            false,
        ),
        (
            "compress-bytes",
            "IMPORT BNCompress AS Compress\n",
            "FUNCTION Once(text AS STRING) AS INTEGER\n    \
             LET value AS Compress.Bytes = Compress.FromText(text)\n    \
             RETURN value.Length()\nEND FUNCTION\n",
            i64::from(SCOPES) * i64::try_from(PAYLOAD_BYTES).expect("fits i64"),
            true,
        ),
    ];
    for (name, imports, body, expected, uses_payload) in cases {
        let directory = TestDir::new(&format!("library-handle-{name}")).expect("directory");
        let source = directory.join("loop.bn");
        let text = if uses_payload { payload.as_str() } else { "" };
        fs::write(&source, loop_program(imports, body, text, expected)).expect("source");
        let artifact = compile_native(&source, &directory);
        let mut command = Command::new(&artifact);
        command.stdout(std::process::Stdio::null());
        let (status, peak) = super::resident::peak_resident_bytes(&mut command);
        assert_eq!(status, 0, "{name} exited with {status}");
        assert!(
            peak < RESIDENT_CEILING_BYTES,
            "{name} peak resident {peak} bytes after {SCOPES} scopes"
        );
    }
}

fn loop_program(imports: &str, body: &str, text: &str, expected: i64) -> String {
    format!(
        "{imports}\
         {body}\
         FUNCTION Start() AS VOID\n    \
         LET text AS STRING = \"{text}\"\n    \
         LET total AS INTEGER = 0\n    \
         FOR i AS INTEGER = 1 TO {SCOPES}\n        \
         total = total + Once(text)\n    \
         END FOR\n    \
         IF total <> {expected} THEN\n        \
         STOP 2\n    \
         END IF\n\
         END FUNCTION\n"
    )
}
