// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! The `BNMath.Pi` attribute and the `DegreeToRad` / `RadToDegree` methods
//! (math.md, "Angle conversions";
//! BDFL decision, 2026-10-26) print the same on both backends.

use super::support::{TestDir, bnc, bni, run};
use std::fs;
use std::process::Command;
use std::time::Duration;

const PROCESS_TIMEOUT: Duration = Duration::from_secs(30);

const ANGLES_PROGRAM: &str = "IMPORT BNMath AS Math\n\nFUNCTION Start() AS VOID\n    PRINT Math.Pi, Math.Pi * 1.0\n    PRINT Math.DegreeToRad(180.0), Math.DegreeToRad(90.0), Math.DegreeToRad(45.0)\n    PRINT Math.DegreeToRad(360.0), Math.DegreeToRad(-90.0), Math.DegreeToRad(30.0)\n    PRINT Math.DegreeToRad(0.0), Math.DegreeToRad(-0.0)\n    PRINT Math.RadToDegree(Math.Pi), Math.RadToDegree(Math.Pi / 2.0), Math.RadToDegree(1.0)\n    PRINT Math.RadToDegree(-Math.Pi), Math.RadToDegree(0.0)\n    PRINT Math.DegreeToRad(NAN), Math.DegreeToRad(INF), Math.RadToDegree(-INF)\n    LET straight AS FLOAT32 = 180.0\n    LET radians AS FLOAT32 = Math.DegreeToRad(straight)\n    PRINT radians\nEND FUNCTION\n";

/// IEEE 754 binary64 values of Rust `to_radians` / `to_degrees`, which
/// multiply by the same precomputed ratios, printed as the shortest decimal
/// that reads back (math.md; independent probe, bucket 0.6.10 E.1b).
const ANGLES_OUTPUT: &str = "3.141592653589793 3.141592653589793\n\
3.141592653589793 1.5707963267948966 0.7853981633974483\n\
6.283185307179586 -1.5707963267948966 0.5235987755982988\n\
0.0 -0.0\n\
180.0 90.0 57.29577951308232\n\
-180.0 0.0\n\
NAN INF -INF\n\
3.1415927\n";

#[test]
fn pi_and_angle_conversions_print_the_same_on_both_backends() {
    let directory = TestDir::new("bnmath-angles").expect("create directory");
    let source = directory.join("program.bn");
    fs::write(&source, ANGLES_PROGRAM).expect("write source");
    let interpreted =
        run(bni().arg("run").arg(&source), None, PROCESS_TIMEOUT).expect("run interpreter");
    let artifact = directory.join(format!("program{}", std::env::consts::EXE_SUFFIX));
    let built = bnc()
        .arg(&source)
        .arg("-o")
        .arg(&artifact)
        .output()
        .expect("run bnc");
    let native = if built.status.success() {
        let compiled = run(&mut Command::new(&artifact), None, PROCESS_TIMEOUT)
            .expect("run compiled artifact");
        String::from_utf8_lossy(&compiled.stdout).into_owned()
    } else {
        format!(
            "bnc build failed: {}",
            String::from_utf8_lossy(&built.stderr)
        )
    };
    let interpreted = if interpreted.status.success() {
        String::from_utf8_lossy(&interpreted.stdout).into_owned()
    } else {
        format!(
            "bni failed: {}",
            String::from_utf8_lossy(&interpreted.stderr)
        )
    };
    assert_eq!(
        (interpreted.as_str(), native.as_str()),
        (ANGLES_OUTPUT, ANGLES_OUTPUT)
    );
}
