// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Behaviour locks for the bni performance work (bucket 0.6.10 F.1): the
//! runtime errors and ownership results an interpreter rewrite could change,
//! held equal on both backends. The value-semantics locks are the
//! conformance cell `tests/conformance/interp/value-semantics.bn`; the
//! stack budget is `tests/runtime-traps/stack-overflow.bn`.

use super::support::{ProcessOutput, TestDir, bnc, bni, run};
use std::fs;
use std::process::Command;
use std::time::Duration;

const PROCESS_TIMEOUT: Duration = Duration::from_secs(30);

/// A program, the diagnostic code it stops with (`None` when it runs to the
/// end), and what it prints before.
struct Lock {
    name: &'static str,
    program: &'static str,
    code: Option<&'static str>,
    stdout: &'static str,
}

const BOX_CLASS: &str = "CLASS Box\n    PUBLIC Value AS INTEGER\n\n    PUBLIC FUNCTION CONSTRUCTOR()\n    END FUNCTION\nEND CLASS\n\n";

const LOCKS: &[Lock] = &[
    Lock {
        name: "step-zero",
        program: "    LET s AS INTEGER = 0\n    FOR i AS INTEGER = 1 TO 3 STEP s\n        PRINT i\n    END FOR\n",
        code: Some("INVALID_FOR_STEP"),
        stdout: "",
    },
    Lock {
        name: "int32-max-plus-one",
        program: "    LET a AS INT32 = 2147483647\n    PRINT a + 1\n",
        code: Some("NUMERIC_OVERFLOW"),
        stdout: "",
    },
    Lock {
        name: "int64-min-minus-one",
        program: "    LET a AS INT64 = -9223372036854775807 - 1\n    PRINT a - 1\n",
        code: Some("NUMERIC_OVERFLOW"),
        stdout: "",
    },
    // An integer constant with no destination is INT32 arithmetic (0.6.md,
    // "Integer constants without a destination"; bucket 0.6.11 S5.2).
    Lock {
        name: "int32-constant-plus-one",
        program: "    PRINT 2147483647 + 1\n",
        code: Some("NUMERIC_OVERFLOW"),
        stdout: "",
    },
    Lock {
        name: "int32-constant-power",
        program: "    PRINT 10 ** 30\n",
        code: Some("NUMERIC_OVERFLOW"),
        stdout: "",
    },
    Lock {
        name: "int32-constant-comparison",
        program: "    PRINT 1\n    PRINT 10 ** 30 > 1\n",
        code: Some("NUMERIC_OVERFLOW"),
        stdout: "1\n",
    },
    Lock {
        name: "constant-with-a-destination",
        program: "    LET x AS INT64 = 2 ** 40\n    PRINT x, -2147483648, 2 ** 10, NOT 5, 1 SHL 3\n",
        code: None,
        stdout: "1099511627776 -2147483648 1024 -6 8\n",
    },
    Lock {
        name: "int32-constant-quotient",
        program: "    PRINT 1\n    PRINT 2147483647 * 2 DIV 2\n",
        code: Some("NUMERIC_OVERFLOW"),
        stdout: "1\n",
    },
    Lock {
        name: "int32-constant-bitwise",
        program: "    PRINT 1\n    PRINT 2147483647 * 2 XOR 1\n",
        code: Some("NUMERIC_OVERFLOW"),
        stdout: "1\n",
    },
    Lock {
        name: "int32-constant-division-operand",
        program: "    PRINT 1\n    PRINT 2147483647 * 2 / 2\n",
        code: Some("NUMERIC_OVERFLOW"),
        stdout: "1\n",
    },
    // An integer constant takes a floating-point operand's type (0.6.md,
    // "Numeric operands"; BDFL decision, 2026-10-28).
    Lock {
        name: "constant-takes-the-float-type",
        program: "    PRINT 2 * 3 + 1.5, 2 + 2, 2 + 2.0, 2.0 + 2, -2 + 2, -(2 * 3) * 0.5\n",
        code: None,
        stdout: "7.5 4 4.0 4.0 0 -3.0\n",
    },
    // The destination checks the exact value of the whole constant, so an
    // intermediate past it does not matter.
    Lock {
        name: "folded-quotient-with-a-destination",
        program: "    LET q AS INT64 = 2 ** 70 DIV 2 ** 10\n    LET h AS INT32 = 3000000000 DIV 2\n    LET m AS BYTE = 255 AND 15\n    PRINT q, h, m, (-5) DIV 3, -7 % 2\n",
        code: None,
        stdout: "1152921504606846976 1500000000 15 -2 1\n",
    },
    // Only the exact uppercase spelling is reserved (keywords.md, bucket
    // 0.6.11 S5.4).
    Lock {
        name: "mixed-case-reserved-spellings",
        program: "    LET New AS INTEGER = 1\n    LET while AS INTEGER = 2\n    LET Date AS INTEGER = 3\n    PRINT New + while + Date\n",
        code: None,
        stdout: "6\n",
    },
    Lock {
        name: "div-by-zero",
        program: "    LET z AS INTEGER = 0\n    PRINT 7 DIV z\n",
        code: Some("DIVISION_BY_ZERO"),
        stdout: "",
    },
    Lock {
        name: "remainder-by-zero",
        program: "    LET z AS INTEGER = 0\n    PRINT 7 % z\n",
        code: Some("DIVISION_BY_ZERO"),
        stdout: "",
    },
    Lock {
        name: "negative-runtime-exponent",
        program: "    LET e AS INTEGER = -1\n    PRINT 2 ** e\n",
        code: Some("INVALID_EXPONENT"),
        stdout: "",
    },
    Lock {
        name: "index-equal-to-len",
        program: "    LET v AS INTEGER[3] = [1, 2, 3]\n    LET k AS INTEGER = LEN(v)\n    PRINT v[k]\n",
        code: Some("INDEX_OUT_OF_BOUNDS"),
        stdout: "",
    },
    Lock {
        name: "ascii-string-index-equal-to-len",
        program: "    LET s AS STRING = \"ab\"\n    PRINT s[2]\n",
        code: Some("INDEX_OUT_OF_BOUNDS"),
        stdout: "",
    },
    Lock {
        name: "accented-string-index-equal-to-len",
        program: "    LET s AS STRING = \"áb\"\n    PRINT s[2]\n",
        code: Some("INDEX_OUT_OF_BOUNDS"),
        stdout: "",
    },
];

/// `BNMath` failures on both backends: `Gcd` / `Lcm` results that do not fit `INT64` (math.md, "Integer
/// divisors"): `Gcd(MIN_INT64, 0)` is `2^63`, and `Lcm(2^62, 3)` is
/// `3 * 2^62`; `Covariance` / `Correlation` / `Dot` of vectors of different lengths.
const BNMATH_LOCKS: &[Lock] = &[
    Lock {
        name: "gcd-of-min-int64",
        program: "    LET lowest AS INT64 = -9223372036854775807 - 1\n    PRINT Math.Gcd(lowest, 0)\n",
        code: Some("NUMERIC_OVERFLOW"),
        stdout: "",
    },
    Lock {
        name: "lcm-past-max-int64",
        program: "    LET large AS INT64 = 4611686018427387904\n    PRINT Math.Lcm(large, 3)\n",
        code: Some("NUMERIC_OVERFLOW"),
        stdout: "",
    },
    Lock {
        name: "covariance-length-mismatch",
        program: "    LET a AS FLOAT[2] = [1.0, 2.0]\n    LET b AS FLOAT[3] = [1.0, 2.0, 3.0]\n    PRINT Math.Covariance(a, b)\n",
        code: Some("VECTOR_LENGTH_MISMATCH"),
        stdout: "",
    },
    Lock {
        name: "correlation-length-mismatch",
        program: "    LET a AS FLOAT[2] = [1.0, 2.0]\n    LET b AS FLOAT[3] = [1.0, 2.0, 3.0]\n    PRINT Math.Correlation(a, b)\n",
        code: Some("VECTOR_LENGTH_MISMATCH"),
        stdout: "",
    },
    Lock {
        name: "dot-length-mismatch",
        program: "    LET a AS FLOAT[2] = [1.0, 2.0]\n    LET b AS FLOAT[3] = [1.0, 2.0, 3.0]\n    PRINT Math.Dot(a, b)\n",
        code: Some("VECTOR_LENGTH_MISMATCH"),
        stdout: "",
    },
];

/// Ownership locks need a CLASS before `Start`.
const OWNERSHIP_LOCKS: &[Lock] = &[
    Lock {
        name: "read-after-release",
        program: "FUNCTION Start() AS VOID\n    LET b AS Box = NEW Box()\n    b.Value = 1\n    RELEASE b\n    PRINT b.Value\nEND FUNCTION\n",
        code: Some("USE_AFTER_RELEASE"),
        stdout: "",
    },
    Lock {
        name: "weak-in-callee",
        program: "FUNCTION Peek(b AS Box) AS BOOLEAN\n    LET w AS WEAK Box OR NULL = b\n    RETURN w IS NULL\nEND FUNCTION\n\nFUNCTION Start() AS VOID\n    LET b AS Box = NEW Box()\n    b.Value = 7\n    PRINT Peek(b), b.Value\n    RELEASE b\nEND FUNCTION\n",
        code: None,
        stdout: "FALSE 7\n",
    },
];

fn interpreted(source: &std::path::Path) -> ProcessOutput {
    run(bni().arg("run").arg(source), None, PROCESS_TIMEOUT).expect("run interpreter")
}

fn compiled(directory: &TestDir, source: &std::path::Path, name: &str) -> ProcessOutput {
    let artifact = directory.join(format!("{name}{}", std::env::consts::EXE_SUFFIX));
    let built = bnc()
        .arg(source)
        .arg("-o")
        .arg(&artifact)
        .output()
        .expect("run bnc");
    assert!(
        built.status.success(),
        "{name}: bnc: {}",
        String::from_utf8_lossy(&built.stderr)
    );
    run(&mut Command::new(&artifact), None, PROCESS_TIMEOUT).expect("run compiled artifact")
}

fn assert_lock(lock: &Lock, backend: &str, output: &mut ProcessOutput) {
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        lock.stdout,
        "{} on {backend}: stdout",
        lock.name
    );
    match lock.code {
        Some(code) => {
            output.accept_expected_status();
            assert!(
                !output.status.success(),
                "{} on {backend}: should stop",
                lock.name
            );
            assert!(
                stderr.contains(&format!("error[{code}]")),
                "{} on {backend}: expected {code}, got {stderr}",
                lock.name
            );
        }
        None => assert!(
            output.status.success(),
            "{} on {backend}: {stderr}",
            lock.name
        ),
    }
}

fn check_locks(prefix: &str, locks: &[Lock], wrap: fn(&str) -> String) {
    let directory = TestDir::new(prefix).expect("create directory");
    for lock in locks {
        let source = directory.join(format!("{}.bn", lock.name));
        fs::write(&source, wrap(lock.program)).expect("write source");
        assert_lock(lock, "bni", &mut interpreted(&source));
        assert_lock(lock, "bnc", &mut compiled(&directory, &source, lock.name));
    }
}

#[test]
fn runtime_errors_keep_their_codes_on_both_backends() {
    check_locks("runtime-locks", LOCKS, |body| {
        format!("FUNCTION Start() AS VOID\n{body}END FUNCTION\n")
    });
}

#[test]
fn release_and_weak_keep_their_results_on_both_backends() {
    check_locks("ownership-locks", OWNERSHIP_LOCKS, |program| {
        format!("{BOX_CLASS}{program}")
    });
}

#[test]
fn bnmath_failures_stop_on_both_backends() {
    check_locks("bnmath-locks", BNMATH_LOCKS, |body| {
        format!("IMPORT BNMath AS Math\n\nFUNCTION Start() AS VOID\n{body}END FUNCTION\n")
    });
}
