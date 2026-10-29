// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! The 0.6.12 `HOST.FileSystem` operations under every execution policy
//! (language/0.6/host.md, "Directories, private files, copy and move";
//! bucket 0.6.12, FS-TEST-1): the same codes on `bni` and `bnc`, nothing
//! written outside a `--write-root`, no link followed, a root never written,
//! relative paths checked after a change of directory, and owner-only modes.
//! Unix only: the fixture plants symbolic links.

#![cfg(unix)]

use super::support::{ProcessOutput, TestDir, bnc, bni, run};
use std::fs;
use std::os::unix::fs::{PermissionsExt as _, symlink};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

const PROCESS_TIMEOUT: Duration = Duration::from_secs(60);
const PRIVATE_DIRECTORY: u32 = 0o700;
const PRIVATE_FILE: u32 = 0o600;
const SEED_MODE: u32 = 0o644;
const PERMISSION_BITS: u32 = 0o777;

/// Writes inside `HOST.Args[1]` and outside it in `HOST.Args[2]`, printing
/// the code of each call (0 for success).
const PROGRAM: &str = "IMPORT HOST.FileSystem AS FS

FUNCTION Code(result AS VOID OR Error) AS INTEGER
    IF result IS Error THEN
        RETURN result.Code
    END IF
    RETURN 0
END FUNCTION

FUNCTION Start() AS VOID
    LET inside AS STRING = HOST.Args[1]
    LET outside AS STRING = HOST.Args[2]
    PRINT \"create\", Code(FS.CreateDirectory(inside + \"/made\")), Code(FS.CreateDirectory(outside + \"/made\"))
    LET created AS FS.File OR Error = FS.OpenNew(outside + \"/new.txt\")
    IF created IS Error THEN
        PRINT \"open new\", created.Code
    ELSE
        PRINT \"open new\", Code(created.Close())
    END IF
    PRINT \"copy\", Code(FS.Copy(inside + \"/seed.txt\", outside + \"/copy.txt\"))
    PRINT \"move\", Code(FS.Move(inside + \"/seed.txt\", outside + \"/moved.txt\"))
    PRINT \"through a link\", Code(FS.CreateDirectory(inside + \"/link/deeper\"))
    PRINT \"dangling\", Code(FS.CreateDirectory(inside + \"/dangling\"))
    PRINT \"remove the root\", Code(FS.RemoveDirectory(inside))
    LET root AS BOOLEAN OR Error = FS.IsDirectory(inside)
    IF root IS Error THEN
        PRINT \"root is directory\", root.Code
    ELSE
        PRINT \"root is directory\", root
    END IF
    PRINT \"enter\", Code(FS.SetCurrentDirectory(inside))
    PRINT \"escape\", Code(FS.CreateDirectory(\"../escape\"))
END FUNCTION
";

const UNRESTRICTED: &str = "create 0 0\nopen new 0\ncopy 0\nmove 0\nthrough a link 0\n\
                            dangling 12\nremove the root 11\nroot is directory TRUE\nenter 0\n\
                            escape 0\n";
const SANDBOX: &str = "create 0 9\nopen new 9\ncopy 9\nmove 9\nthrough a link 9\ndangling 12\n\
                       remove the root 9\nroot is directory TRUE\nenter 0\nescape 9\n";
const READ_ONLY: &str = "create 9 9\nopen new 9\ncopy 9\nmove 9\nthrough a link 9\ndangling 9\n\
                         remove the root 9\nroot is directory TRUE\nenter 0\nescape 9\n";
const DENY: &str = "create 9 9\nopen new 9\ncopy 9\nmove 9\nthrough a link 9\ndangling 9\n\
                    remove the root 9\nroot is directory 9\nenter 9\nescape 9\n";

/// One policy: the flags of `bni run` and `bnc`, and `BN_FS_POLICY`.
struct Policy {
    name: &'static str,
    sandbox: bool,
    environment: Option<&'static str>,
    expected: &'static str,
}

const POLICIES: [Policy; 4] = [
    Policy {
        name: "unrestricted",
        sandbox: false,
        environment: None,
        expected: UNRESTRICTED,
    },
    Policy {
        name: "sandbox",
        sandbox: true,
        environment: None,
        expected: SANDBOX,
    },
    Policy {
        name: "read-only",
        sandbox: false,
        environment: Some("read-only"),
        expected: READ_ONLY,
    },
    Policy {
        name: "deny",
        sandbox: false,
        environment: Some("deny"),
        expected: DENY,
    },
];

/// `base/inside` (the write root) with a seed file, a link to `outside`,
/// and a dangling link into it; `base/outside` empty.
struct Layout {
    directory: TestDir,
    inside: PathBuf,
    outside: PathBuf,
}

impl Layout {
    fn new(label: &str) -> Self {
        let directory = TestDir::new(label).expect("create directory");
        let base = directory.path().canonicalize().expect("canonical base");
        let inside = base.join("inside");
        let outside = base.join("outside");
        fs::create_dir_all(&inside).expect("create inside");
        fs::create_dir_all(&outside).expect("create outside");
        let seed = inside.join("seed.txt");
        fs::write(&seed, "seed").expect("write seed");
        fs::set_permissions(&seed, fs::Permissions::from_mode(SEED_MODE)).expect("chmod seed");
        symlink(&outside, inside.join("link")).expect("link");
        symlink(outside.join("target"), inside.join("dangling")).expect("dangling link");
        Self {
            directory,
            inside,
            outside,
        }
    }

    fn arguments(&self) -> [&str; 2] {
        [
            self.inside.to_str().expect("inside path"),
            self.outside.to_str().expect("outside path"),
        ]
    }

    fn sandbox_flags(&self) -> Vec<String> {
        let inside = self.inside.to_str().expect("inside path").to_owned();
        vec![
            "--sandbox".into(),
            "--read-root".into(),
            inside.clone(),
            "--write-root".into(),
            inside,
        ]
    }

    fn outside_entries(&self) -> Vec<String> {
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

fn mode(path: &Path) -> u32 {
    fs::metadata(path).expect("metadata").permissions().mode() & PERMISSION_BITS
}

fn stdout(output: &ProcessOutput, context: &str) -> String {
    assert!(
        output.status.success() && !output.timed_out,
        "{context}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn interpreted(policy: &Policy, layout: &Layout, source: &Path) -> String {
    let mut command = bni();
    command.current_dir(layout.directory.path()).arg("run");
    if policy.sandbox {
        command.args(layout.sandbox_flags());
    }
    command.arg(source).arg("--").args(layout.arguments());
    if let Some(value) = policy.environment {
        command.env("BN_FS_POLICY", value);
    }
    let output = run(&mut command, None, PROCESS_TIMEOUT).expect("run bni");
    stdout(&output, &format!("bni {}", policy.name))
}

fn compiled(policy: &Policy, layout: &Layout, source: &Path) -> String {
    let artifact = layout
        .directory
        .join(format!("program{}", std::env::consts::EXE_SUFFIX));
    let mut compiler = bnc();
    if policy.sandbox {
        compiler.args(layout.sandbox_flags());
    }
    let built = compiler
        .arg(source)
        .arg("-o")
        .arg(&artifact)
        .output()
        .expect("run bnc");
    assert!(
        built.status.success(),
        "bnc {}: {}",
        policy.name,
        String::from_utf8_lossy(&built.stderr)
    );
    let mut command = Command::new(&artifact);
    command
        .current_dir(layout.directory.path())
        .args(layout.arguments());
    if let Some(value) = policy.environment {
        command.env("BN_FS_POLICY", value);
    }
    let output = run(&mut command, None, PROCESS_TIMEOUT).expect("run artifact");
    stdout(&output, &format!("bnc {}", policy.name))
}

/// What a run may leave in `outside`: everything when no policy confines
/// writes, nothing otherwise.
fn check_layout(policy: &Policy, layout: &Layout, backend: &str) {
    let context = format!("{backend} {}", policy.name);
    if policy.expected != UNRESTRICTED {
        assert!(
            layout.outside_entries().is_empty(),
            "{context}: {:?}",
            layout.outside_entries()
        );
        assert!(layout.inside.join("seed.txt").exists(), "{context}");
        return;
    }
    assert_eq!(
        layout.outside_entries(),
        ["copy.txt", "deeper", "made", "moved.txt", "new.txt"],
        "{context}"
    );
    for directory in [layout.inside.join("made"), layout.outside.join("made")] {
        assert_eq!(mode(&directory), PRIVATE_DIRECTORY, "{context}");
    }
    for file in ["new.txt", "copy.txt"] {
        assert_eq!(
            mode(&layout.outside.join(file)),
            PRIVATE_FILE,
            "{context}: {file}"
        );
    }
    assert_eq!(
        mode(&layout.outside.join("moved.txt")),
        SEED_MODE,
        "{context}"
    );
    assert!(layout.directory.join("escape").is_dir(), "{context}");
}

#[test]
fn directory_operations_obey_every_policy_on_both_backends() {
    let sources = TestDir::new("fs-directories-source").expect("create directory");
    let source = sources.join("program.bn");
    fs::write(&source, PROGRAM).expect("write program");
    for policy in &POLICIES {
        let layout = Layout::new(&format!("fs-directories-bni-{}", policy.name));
        assert_eq!(
            interpreted(policy, &layout, &source),
            policy.expected,
            "bni {}",
            policy.name
        );
        check_layout(policy, &layout, "bni");

        let layout = Layout::new(&format!("fs-directories-bnc-{}", policy.name));
        assert_eq!(
            compiled(policy, &layout, &source),
            policy.expected,
            "bnc {}",
            policy.name
        );
        check_layout(policy, &layout, "bnc");
    }
}
