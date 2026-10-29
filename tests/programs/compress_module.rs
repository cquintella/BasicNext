// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! The `BNCompress` tour (`tests/modules/bncompress/main.bn`) prints the same
//! lines on both backends: buffers, Deflate, Gzip, and a ZIP written, read
//! and extracted (bncompress.md; bucket 0.6.11, S2.6 and S6.7, finding A16).

use super::support::{TestDir, bnc, bni, run};
use std::fs;
use std::process::Command;
use std::time::Duration;

const PROCESS_TIMEOUT: Duration = Duration::from_secs(60);

const EXPECTED: &str = "Length: 30
Hex: 48656c6c6f2c204261736963204e65787420436f6d7072657373696f6e21
ToText: Hello, Basic Next Compression!
Deflated length: 32
Inflated: Hello, Basic Next Compression!
Gzip length: 50
Gunzipped: Hello, Basic Next Compression!
DeflateText roundtrip: Fast convenience string compression
GzipText roundtrip: Fast gzip string convenience
Archive created and closed successfully
Entry count: 3
Entry 0: mimetype
Entry 1: chapter1.txt
Entry 2: data.bin
Extracted mimetype: application/epub+zip
Extracted chapter1: Chapter 1 content inside ZIP archive.
Extracted data.bin: Binary data content
Bad hex properly rejected as Error
Nonexistent zip properly rejected as Error
BNCompress tests completed
";

#[test]
fn bncompress_tour_matches_on_both_backends() {
    let source = fs::read_to_string("tests/modules/bncompress/main.bn").expect("read tour");
    // The tour writes its archive in the working directory: one per backend.
    let interpreted_dir = TestDir::new("compress-tour-bni").expect("create directory");
    let program = interpreted_dir.join("main.bn");
    fs::write(&program, &source).expect("write tour");
    let interpreted = run(
        bni()
            .arg("run")
            .arg(&program)
            .current_dir(interpreted_dir.path()),
        None,
        PROCESS_TIMEOUT,
    )
    .expect("run interpreter");

    let compiled_dir = TestDir::new("compress-tour-bnc").expect("create directory");
    let program = compiled_dir.join("main.bn");
    fs::write(&program, &source).expect("write tour");
    let artifact = compiled_dir.join(format!("tour{}", std::env::consts::EXE_SUFFIX));
    let built = bnc()
        .arg(&program)
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
        Command::new(&artifact).current_dir(compiled_dir.path()),
        None,
        PROCESS_TIMEOUT,
    )
    .expect("run compiled artifact");
    assert_eq!(
        (
            String::from_utf8_lossy(&interpreted.stdout).into_owned(),
            String::from_utf8_lossy(&compiled.stdout).into_owned()
        ),
        (EXPECTED.to_string(), EXPECTED.to_string())
    );
}
