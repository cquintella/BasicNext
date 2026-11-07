// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! `BNCrypto.Policy`: the per-call policy an options record carries
//! (`language/0.6/bncrypto.md`, "Per-Call Cryptographic Policy"). One decision
//! per algorithm family, shared by the interpreter and the native runtime.

use crate::crypto_asymmetric::KEY_ALG_X25519;
use crate::crypto_error::CryptoFailure;
use crate::crypto_hash::{HASH_BLAKE2B, HASH_BLAKE3};
use crate::crypto_mac::MAC_BLAKE3_KEYED;

/// The policy of one call. `allow_legacy` is reserved: no legacy algorithm is
/// available in this version, so it changes nothing.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CryptoPolicy {
    pub allow_legacy: bool,
    pub fips_only: bool,
}

/// Hash algorithms that are not FIPS-approved (FIPS 180-4 and 202 list SHA-2
/// and SHA-3 only).
const NON_FIPS_HASHES: [i32; 2] = [HASH_BLAKE2B, HASH_BLAKE3];
/// MAC algorithms that are not FIPS-approved (HMAC with SHA-2 is).
const NON_FIPS_MACS: [i32; 1] = [MAC_BLAKE3_KEYED];
/// Key-pair algorithms that are not FIPS-approved (Ed25519 and P-256 are,
/// FIPS 186-5; X25519 key agreement is not in SP 800-56A Rev. 3).
const NON_FIPS_KEY_PAIRS: [i32; 1] = [KEY_ALG_X25519];

impl CryptoPolicy {
    fn admits(
        self,
        alg: i32,
        refused: &[i32],
        operation: &'static str,
    ) -> Result<(), CryptoFailure> {
        if self.fips_only && refused.contains(&alg) {
            return Err(CryptoFailure::UnsupportedAlgorithm { operation });
        }
        Ok(())
    }

    /// `HashWith`, `Hasher.NewWith`: `fipsOnly` refuses `BLAKE2b` and `BLAKE3`.
    ///
    /// # Errors
    /// `UNSUPPORTED_ALGORITHM` when the policy refuses `alg`.
    pub fn admits_hash(self, alg: i32) -> Result<(), CryptoFailure> {
        self.admits(alg, &NON_FIPS_HASHES, "BNCrypto.HashWith")
    }

    /// `MacWith`, `MacStream.NewWith`: `fipsOnly` refuses keyed `BLAKE3`.
    ///
    /// # Errors
    /// `UNSUPPORTED_ALGORITHM` when the policy refuses `alg`.
    pub fn admits_mac(self, alg: i32) -> Result<(), CryptoFailure> {
        self.admits(alg, &NON_FIPS_MACS, "BNCrypto.MacWith")
    }

    /// `NewKeyPairWith`: `fipsOnly` refuses X25519.
    ///
    /// # Errors
    /// `UNSUPPORTED_ALGORITHM` when the policy refuses `alg`.
    pub fn admits_key_pair(self, alg: i32) -> Result<(), CryptoFailure> {
        self.admits(alg, &NON_FIPS_KEY_PAIRS, "BNCrypto.NewKeyPairWith")
    }
}

/// Native layout of a `Policy` field inside an options record: an inline
/// STRUCT with its own object header, then `allowLegacy` and `fipsOnly`
/// as one byte each.
const OBJECT_HEADER_BYTES: usize = 16;
const POLICY_ALLOW_LEGACY_OFFSET: usize = OBJECT_HEADER_BYTES;
const POLICY_FIPS_ONLY_OFFSET: usize = OBJECT_HEADER_BYTES + 1;

/// Offset of `policy` in each options record: after the header and `alg`
/// (`INTEGER` and `INT32` are 4 bytes), the inline STRUCT aligned to 8.
const EARLIER_FIELDS_BYTES: usize = 8;
pub(crate) const HASH_OPTS_POLICY_OFFSET: usize = OBJECT_HEADER_BYTES + EARLIER_FIELDS_BYTES;
pub(crate) const MAC_OPTS_POLICY_OFFSET: usize = OBJECT_HEADER_BYTES + EARLIER_FIELDS_BYTES;
pub(crate) const KEY_PAIR_OPTS_POLICY_OFFSET: usize = OBJECT_HEADER_BYTES + EARLIER_FIELDS_BYTES;

/// Reads the policy of a native options record; the default policy when the
/// record pointer is null.
///
/// # Safety
/// `opts` is null or points to an options record whose `policy` field is at
/// `policy_offset`, as the backend lays it out.
#[allow(unsafe_code)]
pub(crate) unsafe fn read_policy(opts: *const u8, policy_offset: usize) -> CryptoPolicy {
    if opts.is_null() {
        return CryptoPolicy::default();
    }
    CryptoPolicy {
        // SAFETY: the caller guarantees the record layout; the byte is inside the
        // inline `Policy` STRUCT.
        allow_legacy: unsafe { read_option_byte(opts, policy_offset + POLICY_ALLOW_LEGACY_OFFSET) }
            != 0,
        // SAFETY: as above.
        fips_only: unsafe { read_option_byte(opts, policy_offset + POLICY_FIPS_ONLY_OFFSET) } != 0,
    }
}

/// One byte of a native options record.
///
/// # Safety
/// `opts` points to an options record with a readable byte at `offset`.
#[allow(unsafe_code)]
pub(crate) unsafe fn read_option_byte(opts: *const u8, offset: usize) -> u8 {
    // SAFETY: `offset` is inside the record (the caller's contract).
    let field = unsafe { opts.add(offset) };
    // SAFETY: one readable byte at `field`.
    unsafe { field.read() }
}

/// An `INT32` field of a native options record, in the host's byte order (the
/// backend stores it natively).
///
/// # Safety
/// `opts` is non-null and points to an options record with four readable bytes
/// at `offset`, as the backend lays it out.
#[allow(unsafe_code)]
pub(crate) unsafe fn read_option_i32(opts: *const u8, offset: usize) -> i32 {
    // SAFETY: `offset` is inside the record (the caller's contract).
    let field = unsafe { opts.add(offset) };
    // SAFETY: four readable bytes at `field`; `read_unaligned` needs no alignment.
    unsafe { field.cast::<i32>().read_unaligned() }
}
