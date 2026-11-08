// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! `BNCrypto` handles are counted: the end of a binding releases the handle
//! without `RELEASE`, a copy keeps its own count, and a second release of an
//! emptied binding does nothing. A compiled loop of 100,000 scopes stays
//! under a fixed resident ceiling (one kept handle per scope is about a
//! hundred megabytes).

use super::support::{TestDir, bni, compile_native, run, workspace_root};
use std::fs;
use std::process::Command;
use std::time::Duration;

const PROCESS_TIMEOUT: Duration = Duration::from_secs(180);
const SCOPES: u32 = 100_000;
const PAYLOAD_BYTES: usize = 4096;
/// A correct run stays near the runtime's baseline (a few megabytes). One
/// kept buffer per scope is 400 MB; one kept key or hasher is about 100 MB.
const RESIDENT_CEILING_BYTES: i64 = 64 * 1024 * 1024;

const KEPT_PROGRAM: &str = "tests/programs/crypto_handle_kept.bn";
const KEPT_OUTPUT: &str = "30\n30\n";

#[test]
fn crypto_handles_end_with_their_binding_on_both_backends() {
    let source = workspace_root().join(KEPT_PROGRAM);
    let interpreted = run(bni().arg("run").arg(&source), None, PROCESS_TIMEOUT).expect("run bni");
    assert!(
        interpreted.status.success(),
        "{}",
        String::from_utf8_lossy(&interpreted.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&interpreted.stdout), KEPT_OUTPUT);
    let directory = TestDir::new("crypto-handle-kept").expect("directory");
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
fn a_compiled_loop_of_crypto_handles_keeps_memory_flat() {
    let payload = "a".repeat(PAYLOAD_BYTES);
    for (name, body, expected, uses_payload) in crypto_cases() {
        let directory = TestDir::new(&format!("crypto-handle-{name}")).expect("directory");
        let source = directory.join("loop.bn");
        let text = if uses_payload { payload.as_str() } else { "" };
        fs::write(&source, loop_program(&body, text, expected)).expect("source");
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

fn crypto_cases() -> [(&'static str, String, i64, bool); 9] {
    let payload_len = i64::try_from(PAYLOAD_BYTES).expect("fits i64");
    [
        (
            "bytes",
            "FUNCTION Once(text AS STRING) AS INTEGER\n    \
             LET value AS Crypto.Bytes = Crypto.FromText(text)\n    \
             RETURN value.Length()\nEND FUNCTION\n"
                .to_string(),
            i64::from(SCOPES) * payload_len,
            true,
        ),
        (
            "secret",
            "FUNCTION Once(text AS STRING) AS INTEGER\n    \
             LET value AS Crypto.Secret = Crypto.NewSecret(text)\n    \
             RETURN 1\nEND FUNCTION\n"
                .to_string(),
            i64::from(SCOPES),
            true,
        ),
        (
            "secret-key",
            "FUNCTION Once(text AS STRING) AS INTEGER\n    \
             LET value AS Crypto.SecretKey = Crypto.SecretKeyFromBytes(Crypto.FromText(text), \"mac\")\n    \
             RETURN 1\nEND FUNCTION\n"
                .to_string(),
            i64::from(SCOPES),
            true,
        ),
        (
            "key",
            "FUNCTION Once(text AS STRING) AS INTEGER\n    \
             LET value AS Crypto.Key OR Error = Crypto.GenerateKey(Crypto.ALG_AES_GCM_256)\n    \
             IF value IS Error THEN\n        STOP 1\n    END IF\n    \
             RETURN value.Length()\nEND FUNCTION\n"
                .to_string(),
            i64::from(SCOPES) * 32,
            false,
        ),
        (
            "public-key",
            "FUNCTION Once(text AS STRING) AS INTEGER\n    \
             LET kp AS Crypto.KeyPair OR Error = Crypto.NewKeyPair()\n    \
             IF kp IS Error THEN\n        STOP 1\n    END IF\n    \
             LET pub AS Crypto.PublicKey = kp.PublicKey\n    \
             RETURN pub.Algorithm()\nEND FUNCTION\n"
                .to_string(),
            i64::from(SCOPES) * 30,
            false,
        ),
        (
            "private-key",
            "FUNCTION Once(text AS STRING) AS INTEGER\n    \
             LET kp AS Crypto.KeyPair OR Error = Crypto.NewKeyPair()\n    \
             IF kp IS Error THEN\n        STOP 1\n    END IF\n    \
             LET priv AS Crypto.PrivateKey = kp.PrivateKey\n    \
             RETURN priv.Algorithm()\nEND FUNCTION\n"
                .to_string(),
            i64::from(SCOPES) * 30,
            false,
        ),
        (
            "hasher",
            "FUNCTION Once(text AS STRING) AS INTEGER\n    \
             LET value AS Crypto.Hasher OR Error = Crypto.Hasher.New(Crypto.Alg.SHA256)\n    \
             IF value IS Error THEN\n        STOP 1\n    END IF\n    \
             RETURN 1\nEND FUNCTION\n"
                .to_string(),
            i64::from(SCOPES),
            false,
        ),
        (
            "mac-stream",
            "FUNCTION Once(text AS STRING) AS INTEGER\n    \
             LET value AS Crypto.MacStream OR Error = Crypto.MacStream.New(Crypto.NewSecretKey())\n    \
             IF value IS Error THEN\n        STOP 1\n    END IF\n    \
             RETURN 1\nEND FUNCTION\n"
                .to_string(),
            i64::from(SCOPES),
            false,
        ),
        (
            "keypair",
            "FUNCTION Once(text AS STRING) AS INTEGER\n    \
             LET value AS Crypto.KeyPair OR Error = Crypto.NewKeyPair()\n    \
             IF value IS Error THEN\n        STOP 1\n    END IF\n    \
             RETURN value.PublicKey.Algorithm()\nEND FUNCTION\n"
                .to_string(),
            i64::from(SCOPES) * 30,
            false,
        ),
    ]
}

fn loop_program(body: &str, text: &str, expected: i64) -> String {
    format!(
        "IMPORT BNCrypto AS Crypto\n\
         {body}\n\
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
