// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! `BNCompress` obeys the file execution policy (bncompress.md, "File access
//! and execution policy"): under `BN_FS_POLICY=deny` and under a sandbox,
//! `ZipArchive.Open`, `Create` and `AddFile` return `POLICY_DENIED` and touch
//! nothing, the same on both backends (bucket 0.6.11, S6.5).

use super::support::{ProcessOutput, TestDir, bnc, bni, run};
use std::fs;
use std::path::Path;
use std::process::Command;
use std::time::Duration;

const PROCESS_TIMEOUT: Duration = Duration::from_secs(60);

/// Opens `HOST.Args[3]`, creates `HOST.Args[1]` and adds `HOST.Args[2]` to it,
/// printing whether each denial is `POLICY_DENIED`.
const PROGRAM: &str = "IMPORT BNCompress AS Z

FUNCTION Start() AS VOID
    LET opened AS Z.ZipArchive OR Error = Z.ZipArchive.Open(HOST.Args[3])
    IF opened IS Error THEN
        PRINT \"open\", opened.Code = Z.POLICY_DENIED
    ELSE
        PRINT \"open allowed\"
    END IF
    LET archive AS Z.ZipArchive OR Error = Z.ZipArchive.Create(HOST.Args[1])
    IF archive IS Error THEN
        PRINT \"create\", archive.Code = Z.POLICY_DENIED
        RETURN
    END IF
    LET added AS VOID OR Error = archive.AddFile(\"s.txt\", HOST.Args[2])
    IF added IS Error THEN
        PRINT \"add\", added.Code = Z.POLICY_DENIED
    ELSE
        PRINT \"add allowed\"
    END IF
    LET closed AS VOID OR Error = archive.Close()
    IF closed IS Error THEN
        PRINT \"close failed\"
    END IF
END FUNCTION
";

struct Paths<'a> {
    archive: &'a Path,
    source: &'a Path,
    existing: &'a Path,
}

impl Paths<'_> {
    fn arguments(&self) -> [&str; 3] {
        [
            self.archive.to_str().expect("archive path"),
            self.source.to_str().expect("source path"),
            self.existing.to_str().expect("existing path"),
        ]
    }
}

fn stdout(output: &ProcessOutput) -> String {
    assert!(
        output.status.success() && !output.timed_out,
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn interpreted(source: &Path, policy: &[&str], deny: bool, paths: &Paths<'_>) -> String {
    let mut command = bni();
    command
        .arg("run")
        .args(policy)
        .arg(source)
        .arg("--")
        .args(paths.arguments());
    if deny {
        command.env("BN_FS_POLICY", "deny");
    }
    stdout(&run(&mut command, None, PROCESS_TIMEOUT).expect("run bni"))
}

fn compiled(
    directory: &TestDir,
    source: &Path,
    policy: &[&str],
    deny: bool,
    paths: &Paths<'_>,
) -> String {
    let artifact = directory.join(format!("program{}", std::env::consts::EXE_SUFFIX));
    let built = bnc()
        .args(policy)
        .arg(source)
        .arg("-o")
        .arg(&artifact)
        .output()
        .expect("run bnc");
    assert!(
        built.status.success(),
        "bnc: {}",
        String::from_utf8_lossy(&built.stderr)
    );
    let mut command = Command::new(&artifact);
    command.args(paths.arguments());
    if deny {
        command.env("BN_FS_POLICY", "deny");
    }
    stdout(&run(&mut command, None, PROCESS_TIMEOUT).expect("run artifact"))
}

#[test]
fn bncompress_obeys_the_file_policy_on_both_backends() {
    let directory = TestDir::new("compress-policy").expect("create directory");
    let source = directory.join("program.bn");
    fs::write(&source, PROGRAM).expect("write source");
    let secret = directory.join("secret.txt");
    fs::write(&secret, "secret").expect("write secret");
    let writable = directory.join("writable");
    fs::create_dir_all(&writable).expect("create writable root");
    let archive = writable.join("out.zip");
    let paths = Paths {
        archive: &archive,
        source: &secret,
        existing: &directory.join("missing.zip"),
    };

    let denied = "open TRUE\ncreate TRUE\n";
    assert_eq!(interpreted(&source, &[], true, &paths), denied);
    assert!(!archive.exists(), "bni: a denied Create wrote the archive");
    assert_eq!(compiled(&directory, &source, &[], true, &paths), denied);
    assert!(!archive.exists(), "bnc: a denied Create wrote the archive");

    let root = writable.to_str().expect("root path");
    let sandbox = ["--sandbox", "--write-root", root];
    let confined = "open TRUE\nadd TRUE\n";
    assert_eq!(interpreted(&source, &sandbox, false, &paths), confined);
    fs::remove_file(&archive).expect("bni sandbox created the archive in its root");
    assert_eq!(
        compiled(&directory, &source, &sandbox, false, &paths),
        confined
    );
    assert!(
        archive.exists(),
        "bnc sandbox created the archive in its root"
    );
}

/// Extracts `HOST.Args[1]` through three symlinks a `--write-root` holds,
/// each pointing outside it: `ExtractFile` to a dangling link, `ExtractAll`
/// through a linked directory, and `Create` at a dangling link. Each is
/// `POLICY_DENIED` (bucket 0.6.11, CMP-SEC-1).
const SYMLINK_PROGRAM: &str = "IMPORT BNCompress AS Z

FUNCTION Start() AS VOID
    LET source AS Z.ZipArchive OR Error = Z.ZipArchive.Open(HOST.Args[1])
    IF source IS Error THEN
        PRINT \"open failed\"
        RETURN
    END IF
    LET file AS VOID OR Error = source.ExtractFile(\"a.txt\", HOST.Args[2])
    IF file IS Error THEN
        PRINT \"file\", file.Code = Z.POLICY_DENIED
    END IF
    LET all AS VOID OR Error = source.ExtractAll(HOST.Args[3])
    IF all IS Error THEN
        PRINT \"all\", all.Code = Z.POLICY_DENIED
    END IF
    LET created AS Z.ZipArchive OR Error = Z.ZipArchive.Create(HOST.Args[4])
    IF created IS Error THEN
        PRINT \"create\", created.Code = Z.POLICY_DENIED
    END IF
END FUNCTION
";

const ARCHIVE_PROGRAM: &str = "IMPORT BNCompress AS Z

FUNCTION Start() AS VOID
    LET archive AS Z.ZipArchive OR Error = Z.ZipArchive.Create(HOST.Args[1])
    IF archive IS Error THEN
        RETURN
    END IF
    LET first AS VOID OR Error = archive.AddText(\"a.txt\", \"alpha\")
    LET second AS VOID OR Error = archive.AddText(\"sub/b.txt\", \"beta\")
    LET closed AS VOID OR Error = archive.Close()
    PRINT first IS Error, second IS Error, closed IS Error
END FUNCTION
";

#[cfg(unix)]
struct Traps {
    root: std::path::PathBuf,
    outside: std::path::PathBuf,
}

#[cfg(unix)]
impl Traps {
    /// Empties both directories and plants the three links.
    fn plant(&self) {
        use std::os::unix::fs::symlink;
        for directory in [&self.root, &self.outside] {
            let _ = fs::remove_dir_all(directory);
            fs::create_dir_all(directory).expect("create directory");
        }
        symlink(self.outside.join("target.txt"), self.root.join("link.txt")).expect("file link");
        symlink(&self.outside, self.root.join("sub")).expect("directory link");
        symlink(
            self.outside.join("created.zip"),
            self.root.join("archive.zip"),
        )
        .expect("archive link");
    }

    fn arguments<'a>(&'a self, source: &'a Path) -> Vec<std::path::PathBuf> {
        vec![
            source.to_path_buf(),
            self.root.join("link.txt"),
            self.root.clone(),
            self.root.join("archive.zip"),
        ]
    }

    fn escaped(&self) -> Vec<String> {
        let mut names: Vec<_> = fs::read_dir(&self.outside)
            .expect("read outside")
            .map(|entry| {
                entry
                    .expect("entry")
                    .file_name()
                    .to_string_lossy()
                    .into_owned()
            })
            .collect();
        names.sort();
        names
    }
}

#[cfg(unix)]
#[test]
fn bncompress_never_writes_through_a_symlink_on_both_backends() {
    let directory = TestDir::new("compress-symlink").expect("create directory");
    let inputs = directory.join("in");
    fs::create_dir_all(&inputs).expect("create inputs");
    let archive = inputs.join("source.zip");
    let setup = directory.join("setup.bn");
    fs::write(&setup, ARCHIVE_PROGRAM).expect("write setup");
    let made = run(
        bni().arg("run").arg(&setup).arg("--").arg(&archive),
        None,
        PROCESS_TIMEOUT,
    )
    .expect("run setup");
    assert_eq!(stdout(&made), "FALSE FALSE FALSE\n");

    let source = directory.join("attack.bn");
    fs::write(&source, SYMLINK_PROGRAM).expect("write source");
    let traps = Traps {
        root: directory.join("out"),
        outside: directory.join("outside"),
    };
    let policy = [
        "--sandbox",
        "--read-root",
        inputs.to_str().expect("inputs path"),
        "--write-root",
        traps.root.to_str().expect("root path"),
    ];
    let expected = "file TRUE\nall TRUE\ncreate TRUE\n";

    traps.plant();
    let interpreted = run(
        bni()
            .arg("run")
            .args(policy)
            .arg(&source)
            .arg("--")
            .args(traps.arguments(&archive)),
        None,
        PROCESS_TIMEOUT,
    )
    .expect("run bni");
    assert_eq!(stdout(&interpreted), expected);
    assert!(
        traps.escaped().is_empty(),
        "bni wrote {:?}",
        traps.escaped()
    );

    traps.plant();
    let artifact = directory.join(format!("program{}", std::env::consts::EXE_SUFFIX));
    let built = bnc()
        .args(policy)
        .arg(&source)
        .arg("-o")
        .arg(&artifact)
        .output()
        .expect("run bnc");
    assert!(
        built.status.success(),
        "bnc: {}",
        String::from_utf8_lossy(&built.stderr)
    );
    let compiled = run(
        Command::new(&artifact).args(traps.arguments(&archive)),
        None,
        PROCESS_TIMEOUT,
    )
    .expect("run artifact");
    assert_eq!(stdout(&compiled), expected);
    assert!(
        traps.escaped().is_empty(),
        "bnc wrote {:?}",
        traps.escaped()
    );
}
