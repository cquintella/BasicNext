// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

// LoadPrivateKey on an encrypted PEM whose key-derivation cost is chosen by
// the attacker: the cost is checked before any derivation runs, so a hostile
// file is PARSE_ERROR instead of an abort (scrypt memory) or a hang (PBKDF2
// iterations).

use bn_stdlib::crypto::bn_load_private_pem;
use bn_types::error_codes::crypto::PARSE_ERROR;
use pkcs8::der::Encode as _;
use pkcs8::pkcs5::pbes2::{
    EncryptionScheme, Kdf, Parameters, Pbkdf2Params, Pbkdf2Prf, ScryptParams,
};

const SALT: [u8; 16] = [7; 16];
const IV: [u8; 16] = [9; 16];
/// Ciphertext is never reached: the check refuses the parameters first.
const CIPHERTEXT: [u8; 48] = [0; 48];
const PASSWORD: &str = "pw";

fn encrypted_pem(kdf: Kdf<'_>) -> String {
    let parameters = Parameters {
        kdf,
        encryption: EncryptionScheme::Aes256Cbc { iv: &IV },
    };
    let info = pkcs8::EncryptedPrivateKeyInfo {
        encryption_algorithm: pkcs8::pkcs5::EncryptionScheme::Pbes2(parameters),
        encrypted_data: &CIPHERTEXT,
    };
    let der = info.to_der().expect("encode the hostile key");
    pkcs8::der::pem::encode_string("ENCRYPTED PRIVATE KEY", pkcs8::LineEnding::LF, &der)
        .expect("PEM-encode the hostile key")
}

fn load_code(pem: &str) -> Result<(), i32> {
    bn_load_private_pem(pem, Some(PASSWORD))
        .map(|_| ())
        .map_err(|failure| failure.code())
}

#[test]
fn scrypt_cost_above_the_ceiling_is_a_parse_error() {
    let kdf = Kdf::Scrypt(ScryptParams {
        salt: &SALT,
        cost_parameter: 1_u64 << 40,
        block_size: 8,
        parallelization: 1,
        key_length: None,
    });
    assert_eq!(load_code(&encrypted_pem(kdf)), Err(PARSE_ERROR));
}

#[test]
fn scrypt_block_size_above_the_ceiling_is_a_parse_error() {
    let kdf = Kdf::Scrypt(ScryptParams {
        salt: &SALT,
        cost_parameter: 1_u64 << 14,
        block_size: 4096,
        parallelization: 1,
        key_length: None,
    });
    assert_eq!(load_code(&encrypted_pem(kdf)), Err(PARSE_ERROR));
}

#[test]
fn pbkdf2_iterations_above_the_ceiling_are_a_parse_error() {
    let kdf = Kdf::Pbkdf2(Pbkdf2Params {
        salt: &SALT,
        iteration_count: u32::MAX,
        key_length: None,
        prf: Pbkdf2Prf::HmacWithSha256,
    });
    assert_eq!(load_code(&encrypted_pem(kdf)), Err(PARSE_ERROR));
}
