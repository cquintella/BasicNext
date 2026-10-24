// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use super::support::*;
use std::fs;
#[test]
fn filesystem_capability_reports_file_existence() {
    let source = "IMPORT HOST.FileSystem AS FS\nFUNCTION Start() AS VOID\nPRINT FS.Exists(\"Cargo.toml\"), FS.Exists(\"missing-file.bn\")\nEND FUNCTION\n";
    let (_, output) = run(source, "").expect("execute HOST.FileSystem");
    assert_eq!(output, "TRUE FALSE\n");
}

#[test]
fn file_is_test_and_identity_equality() {
    let source = "IMPORT HOST.FileSystem AS FS\nFUNCTION Nop() AS VOID\nEND FUNCTION\nFUNCTION Start() AS VOID\nLET missing AS FS.File OR Error = FS.Open(\"no-such-basicnext-r5-file\", FS.READ)\nIF missing IS Error THEN\nPRINT \"err\"\nEND IF\nLET file AS FS.File OR Error = FS.Open(\"Cargo.toml\", FS.READ)\nIF file IS FS.File THEN\nPRINT \"file\"\nLET alias AS FS.File OR Error = file\nIF alias = file THEN\nPRINT \"same\"\nEND IF\nRELEASE file\nEND IF\nIF missing IS Error THEN\nLET e AS Error = missing\nPRINT e.Code = FS.NOT_FOUND\nLET v AS VOID OR Error = Nop()\nLET both AS VOID OR Error = e\nPRINT v = both\nEND IF\nEND FUNCTION\n";
    // An `Error` has no default (error.md): the one compared is a real failure.
    let (_, output) = run(source, "").expect("IS FS.File and identity");
    assert_eq!(output, "err\nfile\nsame\nTRUE\nFALSE\n");
}

#[test]
fn filesystem_file_opens_reads_and_closes() {
    let source = "IMPORT HOST.FileSystem AS FS\nFUNCTION Start() AS VOID\nLET file AS FS.File OR Error = FS.Open(\"Cargo.toml\", FS.READ)\nIF file IS Error THEN\nPRINT file.Code\nELSE\nPRINT file.ReadAll()\nfile.Close()\nEND IF\nEND FUNCTION\n";
    let (_, output) = run(source, "").expect("execute FS.File");
    assert!(output.contains("[package]"));
}

#[test]
fn filesystem_file_delete_closes_and_rejects_reuse() {
    let source = "IMPORT HOST.FileSystem AS FS\nFUNCTION Start() AS VOID\nLET file AS FS.File OR Error = FS.Open(\"Cargo.toml\", FS.READ)\nIF file IS Error THEN\nPRINT file.Code\nELSE\nRELEASE file\nRELEASE file\nEND IF\nEND FUNCTION\n";
    let error = run(source, "").expect_err("second RELEASE must fail");
    assert_eq!(error.code, "DOUBLE_RELEASE");
}

#[test]
fn filesystem_file_reads_lines_and_bytes() {
    let source = "IMPORT HOST.FileSystem AS FS\nFUNCTION Start() AS VOID\nLET text AS FS.File OR Error = FS.Open(\"Cargo.toml\", FS.READ)\nIF text IS Error THEN\nPRINT text.Code\nELSE\nLET line AS STRING OR EOF OR Error = text.ReadLine()\nPRINT line\ntext.Close()\nEND IF\nLET binary AS FS.File OR Error = FS.Open(\"Cargo.toml\", FS.READ)\nLET buffer AS POINTER TO BYTE[] = NEW BYTE[8]\nIF binary IS Error THEN\nPRINT binary.Code\nELSE\nPRINT binary.ReadBytes(buffer), buffer[0]\nRELEASE binary\nEND IF\nEND FUNCTION\n";
    let (_, output) = run(source, "").expect("execute line and byte reads");
    assert!(output.starts_with("[workspace]\n"));
    assert!(output.contains('8'));
}

#[test]
fn filesystem_file_writes_bytes_round_trip() {
    let file = unique_temp("sprint5.bn");
    let path = bn_path(&file);
    let source = format!(
        "IMPORT HOST.FileSystem AS FS\nFUNCTION Start() AS VOID\nLET out AS FS.File OR Error = FS.Open(\"{path}\", FS.WRITE)\nLET buffer AS POINTER TO BYTE[] = NEW BYTE[2]\nbuffer[0] = 65\nbuffer[1] = 66\nIF out IS Error THEN\nPRINT out.Code\nELSE\nLET result AS VOID OR Error = out.WriteBytes(buffer, 2)\nIF result IS Error THEN\nPRINT result.Code\nEND IF\nout.Close()\nEND IF\nLET input AS FS.File OR Error = FS.Open(\"{path}\", FS.READ)\nIF input IS Error THEN\nPRINT input.Code\nELSE\nLET read_buffer AS POINTER TO BYTE[] = NEW BYTE[2]\nPRINT input.ReadBytes(read_buffer), read_buffer[0], read_buffer[1]\nRELEASE input\nEND IF\nFS.DeleteFile(\"{path}\")\nEND FUNCTION\n"
    );
    let (_, output) = run(&source, "").expect("execute byte write");
    assert!(output.contains("2 65 66"));
}

#[test]
fn filesystem_write_bytes_on_a_read_only_file_returns_error() {
    let source = "IMPORT HOST.FileSystem AS FS\nFUNCTION Start() AS VOID\nLET file AS FS.File OR Error = FS.Open(\"Cargo.toml\", FS.READ)\nLET buffer AS POINTER TO BYTE[] = NEW BYTE[1]\nbuffer[0] = 65\nIF file IS Error THEN\nPRINT file.Code\nELSE\nLET written AS VOID OR Error = file.WriteBytes(buffer, 1)\nIF written IS Error THEN\nPRINT written.Code\nEND IF\nRELEASE file\nEND IF\nEND FUNCTION\n";
    let (_, output) = run(source, "").expect("WriteBytes I/O failure is an Error value");
    // FS.IO_FAILED, detected from the open mode so every host agrees.
    assert_eq!(output, "8\n");
}

#[test]
fn filesystem_file_rejects_byte_count_outside_buffer() {
    let file = unique_temp("sprint5-count.bn");
    let path = bn_path(&file);
    let source = format!(
        "IMPORT HOST.FileSystem AS FS\nFUNCTION Start() AS VOID\nLET out AS FS.File OR Error = FS.Open(\"{path}\", FS.WRITE)\nLET buffer AS POINTER TO BYTE[] = NEW BYTE[2]\nIF out IS Error THEN\nPRINT out.Code\nELSE\nout.WriteBytes(buffer, 3)\nEND IF\nEND FUNCTION\n"
    );
    let error = run(&source, "").expect_err("invalid byte count must fail");
    assert_eq!(error.code, "INDEX_OUT_OF_BOUNDS");
    let _ = fs::remove_file(&file);
}

#[test]
fn filesystem_file_reports_invalid_utf8_on_text_read() {
    let file = unique_temp("sprint5-utf8.bn");
    let path = bn_path(&file);
    fs::write(&file, [0xff, 0xfe]).expect("create invalid UTF-8 fixture");
    let source = format!(
        "IMPORT HOST.FileSystem AS FS\nFUNCTION Start() AS VOID\nLET file AS FS.File OR Error = FS.Open(\"{path}\", FS.READ)\nIF file IS Error THEN\nPRINT file.Code\nELSE\nLET text AS STRING OR Error = file.ReadAll()\nIF text IS Error THEN\nPRINT text.Code\nEND IF\nRELEASE file\nEND IF\nFS.DeleteFile(\"{path}\")\nEND FUNCTION\n"
    );
    let (_, output) = run(&source, "").expect("invalid UTF-8 is an Error value");
    assert_eq!(output, "7\n"); // FS.INVALID_UTF8 (host.md)
    let _ = fs::remove_file(&file);
}

#[test]
fn filesystem_failed_text_write_does_not_lock_out_binary_reads() {
    let source = "IMPORT HOST.FileSystem AS FS\nFUNCTION Start() AS VOID\nLET file AS FS.File OR Error = FS.Open(\"Cargo.toml\", FS.READ)\nIF file IS Error THEN\nPRINT file.Code\nELSE\nLET written AS VOID OR Error = file.Write(\"x\")\nLET buffer AS POINTER TO BYTE[] = NEW BYTE[8]\nPRINT file.ReadBytes(buffer)\nRELEASE file\nEND IF\nEND FUNCTION\n";
    let (_, output) = run(source, "").expect("failed text write must not change file family");
    assert_eq!(output, "8\n");
}

#[test]
fn filesystem_rejects_directory_open_and_missing_delete() {
    let source = "IMPORT HOST.FileSystem AS FS\nFUNCTION Start() AS VOID\nLET file AS FS.File OR Error = FS.Open(\".\", FS.READ)\nIF file IS Error THEN\nPRINT \"open\", file.Code\nELSE\nPRINT \"opened\"\nRELEASE file\nEND IF\nLET removed AS VOID OR Error = FS.DeleteFile(\"/tmp/basicnext-sprint5-missing.bn\")\nIF removed IS Error THEN\nPRINT \"del\", removed.Code\nEND IF\nEND FUNCTION\n";
    let (_, output) = run(source, "").expect("unsupported paths return Error values");
    assert_eq!(output, "open 4\ndel 2\n"); // FS.IS_DIRECTORY, FS.NOT_FOUND
}

#[test]
fn filesystem_import_without_use_still_requires_the_capability() {
    let source =
        "IMPORT HOST.FileSystem AS FS\nFUNCTION Start() AS VOID\nPRINT \"ran\"\nEND FUNCTION\n";
    let error = run_with_host(
        source,
        "",
        &fixed(vec!["runtime.bn".into()], 0, 0).without_filesystem(),
    )
    .expect_err("import-only FileSystem must fail before Start");
    assert_eq!(error.code, "HOST_CAPABILITY_UNAVAILABLE");
}

#[test]
fn filesystem_eof_locks_the_text_family() {
    let file = unique_temp("r8-empty-text.bn");
    let path = bn_path(&file);
    let _ = fs::remove_file(&file);
    let source = format!(
        "IMPORT HOST.FileSystem AS FS\nFUNCTION Start() AS VOID\nLET out AS FS.File OR Error = FS.Open(\"{path}\", FS.WRITE)\nIF out IS Error THEN\nPRINT out.Code\nELSE\nout.Close()\nRELEASE out\nEND IF\nLET text AS FS.File OR Error = FS.Open(\"{path}\", FS.READ)\nIF text IS Error THEN\nPRINT text.Code\nELSE\nLET line AS STRING OR EOF OR Error = text.ReadLine()\nIF line IS EOF THEN\nPRINT \"eof\"\nEND IF\nLET buffer AS POINTER TO BYTE[] = NEW BYTE[1]\nLET bytes AS INTEGER OR EOF OR Error = text.ReadBytes(buffer)\nIF bytes IS Error THEN\nPRINT \"blocked\"\nEND IF\nRELEASE buffer\nRELEASE text\nEND IF\nFS.DeleteFile(\"{path}\")\nEND FUNCTION\n"
    );
    let (_, output) = run(&source, "").expect("ReadLine EOF locks text family");
    assert_eq!(output, "eof\nblocked\n");
}

#[test]
fn filesystem_eof_locks_the_binary_family() {
    let file = unique_temp("r8-empty-binary.bn");
    let path = bn_path(&file);
    let _ = fs::remove_file(&file);
    let source = format!(
        "IMPORT HOST.FileSystem AS FS\nFUNCTION Start() AS VOID\nLET out AS FS.File OR Error = FS.Open(\"{path}\", FS.WRITE)\nIF out IS Error THEN\nPRINT out.Code\nELSE\nout.Close()\nRELEASE out\nEND IF\nLET binary AS FS.File OR Error = FS.Open(\"{path}\", FS.READ)\nIF binary IS Error THEN\nPRINT binary.Code\nELSE\nLET buffer AS POINTER TO BYTE[] = NEW BYTE[1]\nLET bytes AS INTEGER OR EOF OR Error = binary.ReadBytes(buffer)\nIF bytes IS EOF THEN\nPRINT \"eof\"\nEND IF\nLET line AS STRING OR EOF OR Error = binary.ReadLine()\nIF line IS Error THEN\nPRINT \"blocked\"\nEND IF\nRELEASE buffer\nRELEASE binary\nEND IF\nFS.DeleteFile(\"{path}\")\nEND FUNCTION\n"
    );
    let (_, output) = run(&source, "").expect("ReadBytes EOF locks binary family");
    assert_eq!(output, "eof\nblocked\n");
}

#[test]
fn filesystem_close_flushes_written_bytes_to_disk() {
    let file = unique_temp("r8-close-flush.bn");
    let path = bn_path(&file);
    let _ = fs::remove_file(&file);
    let source = format!(
        "IMPORT HOST.FileSystem AS FS\nFUNCTION Start() AS VOID\nLET out AS FS.File OR Error = FS.Open(\"{path}\", FS.WRITE)\nIF out IS Error THEN\nPRINT out.Code\nELSE\nLET written AS VOID OR Error = out.Write(\"flushed\")\nIF written IS Error THEN\nPRINT written.Code\nELSE\nLET closed AS VOID OR Error = out.Close()\nIF closed IS Error THEN\nPRINT closed.Code\nEND IF\nEND IF\nRELEASE out\nEND IF\nLET input AS FS.File OR Error = FS.Open(\"{path}\", FS.READ)\nIF input IS Error THEN\nPRINT input.Code\nELSE\nPRINT input.ReadAll()\nRELEASE input\nEND IF\nFS.DeleteFile(\"{path}\")\nEND FUNCTION\n"
    );
    let (_, output) = run(&source, "").expect("Close flushes before release");
    assert_eq!(output, "flushed\n");
}

#[test]
fn filesystem_new_file_starts_closed() {
    let source = "IMPORT HOST.FileSystem AS FS\nFUNCTION Start() AS VOID\nLET file AS FS.File = NEW FS.File()\nLET closed AS VOID OR Error = file.Close()\nIF closed IS Error THEN\nPRINT closed.Code\nEND IF\nLET text AS STRING OR Error = file.ReadAll()\nIF text IS Error THEN\nPRINT text.Code\nEND IF\nRELEASE file\nEND FUNCTION\n";
    let (_, output) = run(source, "").expect("NEW FS.File creates a closed handle");
    assert_eq!(output, "5\n"); // FS.CLOSED
}

#[test]
fn filesystem_open_returns_error_for_unknown_mode() {
    let source = "IMPORT HOST.FileSystem AS FS\nFUNCTION Start() AS VOID\nLET mode AS INTEGER = 99\nLET file AS FS.File OR Error = FS.Open(\"Cargo.toml\", mode)\nIF file IS Error THEN\nPRINT file.Code\nEND IF\nEND FUNCTION\n";
    let (_, output) = run(source, "").expect("computed unknown mode returns Error");
    assert_eq!(output, "1\n");
}

#[test]
fn filesystem_capability_is_checked_before_start() {
    let source = "IMPORT HOST.FileSystem AS FS\nFUNCTION Start() AS VOID\nPRINT FS.Exists(\"Cargo.toml\")\nEND FUNCTION\n";
    let error = run_with_host(
        source,
        "",
        &fixed(vec!["runtime.bn".into()], 0, 0).without_filesystem(),
    )
    .expect_err("missing filesystem capability must fail before Start");
    assert_eq!(error.code, "HOST_CAPABILITY_UNAVAILABLE");
}

#[test]
fn local_class_file_is_not_a_host_file() {
    let source = "CLASS File\nPUBLIC name AS STRING = \"user\"\nPUBLIC FUNCTION CONSTRUCTOR()\nEND FUNCTION\nEND CLASS\nFUNCTION Start() AS VOID\nLET f AS File = NEW File()\nPRINT f.name\nRELEASE f\nEND FUNCTION\n";
    let (_, output) = run(source, "").expect("local CLASS File");
    assert_eq!(output, "user\n");
}

#[test]
fn imported_class_file_is_not_a_host_file() {
    let (_, output) =
        run_path("tests/modules/user-file-class/main.bn").expect("imported CLASS File");
    assert_eq!(output, "7\n");
}

#[test]
fn error_return_narrows_file_for_the_rest_of_the_block() {
    let (_, output) =
        run_path("tests/grammar/valid/error-return-narrow.bn").expect("narrow after RETURN");
    assert!(output.starts_with('['), "{output}");
}
