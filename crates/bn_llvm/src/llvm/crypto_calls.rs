// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

// Calls to `BNCrypto` members. `Bytes` and keys travel as `bn_rt` table
// indices (`i64`), as BNLog resources do.
#![allow(clippy::wildcard_imports)]
#[path = "crypto_csprng_calls.rs"]
mod crypto_csprng_calls;
#[path = "crypto_encoding_calls.rs"]
mod crypto_encoding_calls;

use super::json_calls::emit_handle_index;
use super::*;
use crate::ir::{CastOp, ICmpCond, InstSink as _, LlvmInst as I, LlvmOperand as O, LlvmType as T};

/// How a member's `bn_rt` status becomes its result.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Out {
    /// `Bytes OR Error`: the status and the handle written through `ptr`.
    Handle,
    /// `BOOLEAN`: the status is 1 when the check holds.
    Verify,
    /// `Bytes`: the handle written through `ptr`, the status ignored.
    Bytes,
}

fn carries_type(ty: &Type, target: &str) -> bool {
    match ty {
        Type::Named(name)
        | Type::ImportedNamed { name, .. }
        | Type::ImportedTypeName { name, .. } => {
            name == target || name.ends_with(&format!(".{target}"))
        }
        Type::Alternative(alternatives) => alternatives.iter().any(|a| carries_type(a, target)),
        _ => false,
    }
}

pub(super) fn lower_bncrypto_call(
    text: &mut crate::ir::LlvmModule,
    analysis: &LoweringAnalysis<'_>,
    destination: ValueId,
    member: &str,
    arguments: &[ValueId],
) {
    if crypto_encoding_calls::try_lower_encoding_call(
        text,
        analysis,
        destination,
        member,
        arguments,
    ) {
        return;
    }
    if crypto_csprng_calls::try_lower_csprng_call(text, analysis, destination, member, arguments) {
        return;
    }
    let dest = destination.0;
    let v = |id: ValueId| O::reg(format!("v{}", id.0));
    let cryh = || O::reg(format!("cryh{dest}"));
    match member {
        "SHA256" | "SHA512" => {
            let symbol = format!("bn_rt_crypto_{}", member.to_ascii_lowercase());
            let args = vec![(T::Ptr, v(arguments[0]))];
            text.assign(format!("v{dest}"), I::call(T::Ptr, &symbol, args));
            return;
        }
        "FromText" => {
            let args = vec![(T::Ptr, v(arguments[0]))];
            let call = I::call(T::I64, "bn_rt_crypto_bytes_from_text", args);
            text.assign(format!("cryh{dest}"), call);
            text.assign(
                format!("v{dest}"),
                I::cast(CastOp::IntToPtr, T::I64, cryh(), T::Ptr),
            );
            return;
        }
        "NewSecret" | "SecretFromText" => {
            let args = vec![(T::Ptr, v(arguments[0]))];
            let call = I::call(T::I64, "bn_rt_crypto_secret_from_text", args);
            text.assign(format!("cryh{dest}"), call);
            text.assign(
                format!("v{dest}"),
                I::cast(CastOp::IntToPtr, T::I64, cryh(), T::Ptr),
            );
            return;
        }
        "ToBytes" => {
            emit_handle_index(text, analysis, &format!("cryh{dest}"), arguments[0]);
            let args = vec![(T::I64, cryh())];
            let call = I::call(T::I64, "bn_rt_crypto_secret_to_bytes", args);
            text.assign(format!("cryb{dest}"), call);
            text.assign(
                format!("v{dest}"),
                I::cast(
                    CastOp::IntToPtr,
                    T::I64,
                    O::reg(format!("cryb{dest}")),
                    T::Ptr,
                ),
            );
            return;
        }
        "ConstantTimeEquals" => {
            emit_handle_index(text, analysis, &format!("crya{dest}"), arguments[0]);
            emit_handle_index(text, analysis, &format!("cryb{dest}"), arguments[1]);
            let is_secret = analysis
                .values
                .get(&arguments[0])
                .is_some_and(|ty| match ty {
                    Type::Named(name)
                    | Type::ImportedNamed { name, .. }
                    | Type::ImportedTypeName { name, .. } => {
                        name == "Secret" || name.ends_with(".Secret")
                    }
                    _ => false,
                });
            let sym = if is_secret {
                "bn_rt_crypto_secret_constant_time_equals"
            } else {
                "bn_rt_crypto_bytes_constant_time_equals"
            };
            let args = vec![
                (T::I64, O::reg(format!("crya{dest}"))),
                (T::I64, O::reg(format!("cryb{dest}"))),
            ];
            let call = I::call(T::I32, sym, args);
            text.assign(format!("cryrc{dest}"), call);
            text.assign(
                format!("v{dest}"),
                I::icmp(
                    ICmpCond::Eq,
                    T::I32,
                    O::reg(format!("cryrc{dest}")),
                    O::int(1),
                ),
            );
            return;
        }
        "Length" | "Algorithm" | "Zeroize" => {
            emit_handle_index(text, analysis, &format!("cryh{dest}"), arguments[0]);
            let args = vec![(T::I64, cryh())];
            if member == "Algorithm" {
                let is_secret_key = analysis
                    .values
                    .get(&arguments[0])
                    .is_some_and(|ty| carries_type(ty, "SecretKey"));
                let is_pub = analysis
                    .values
                    .get(&arguments[0])
                    .is_some_and(|ty| carries_type(ty, "PublicKey"));
                let is_priv = analysis
                    .values
                    .get(&arguments[0])
                    .is_some_and(|ty| carries_type(ty, "PrivateKey"));
                if is_secret_key {
                    let call = I::call(T::Ptr, "bn_rt_crypto_secret_key_algorithm", args);
                    text.assign(format!("v{dest}"), call);
                } else if is_pub {
                    let call = I::call(T::I32, "bn_rt_crypto_public_key_algorithm", args);
                    text.assign(format!("v{dest}"), call);
                } else if is_priv {
                    let call = I::call(T::I32, "bn_rt_crypto_private_key_algorithm", args);
                    text.assign(format!("v{dest}"), call);
                } else {
                    let call = I::call(T::I32, "bn_rt_crypto_key_algorithm", args);
                    text.assign(format!("v{dest}"), call);
                }
            } else if member == "Zeroize" {
                let is_secret_key = analysis
                    .values
                    .get(&arguments[0])
                    .is_some_and(|ty| carries_type(ty, "SecretKey"));
                let is_priv = analysis
                    .values
                    .get(&arguments[0])
                    .is_some_and(|ty| carries_type(ty, "PrivateKey"));
                let is_key = analysis
                    .values
                    .get(&arguments[0])
                    .is_some_and(|ty| carries_type(ty, "Key"));
                let is_secret = analysis
                    .values
                    .get(&arguments[0])
                    .is_some_and(|ty| carries_type(ty, "Secret"));
                let symbol = if is_priv {
                    "bn_rt_crypto_private_key_zeroize"
                } else if is_secret_key {
                    "bn_rt_crypto_secret_key_zeroize"
                } else if is_key {
                    "bn_rt_crypto_key_zeroize"
                } else if is_secret {
                    "bn_rt_crypto_secret_zeroize"
                } else {
                    "bn_rt_crypto_bytes_zeroize"
                };
                let call = I::call(T::I32, symbol, args);
                text.assign(format!("cryrc{dest}"), call);
            } else {
                let is_secret_key = analysis
                    .values
                    .get(&arguments[0])
                    .is_some_and(|ty| carries_type(ty, "SecretKey"));
                let is_key = analysis
                    .values
                    .get(&arguments[0])
                    .is_some_and(|ty| carries_type(ty, "Key"));
                let is_secret = analysis
                    .values
                    .get(&arguments[0])
                    .is_some_and(|ty| carries_type(ty, "Secret"));
                let sym = if is_secret_key {
                    "bn_rt_crypto_secret_key_length"
                } else if is_key {
                    "bn_rt_crypto_key_length"
                } else if is_secret {
                    "bn_rt_crypto_secret_length"
                } else {
                    "bn_rt_crypto_bytes_length"
                };
                let call = I::call(T::I64, sym, args);
                text.assign(format!("cryl{dest}"), call);
                let length = O::reg(format!("cryl{dest}"));
                text.assign(
                    format!("v{dest}"),
                    I::cast(CastOp::Trunc, T::I64, length, T::I32),
                );
            }
            return;
        }
        "NewKeyPair" => {
            text.assign(
                format!("kpptr{dest}"),
                I::call(
                    T::Ptr,
                    "calloc",
                    vec![(T::I64, O::int(1)), (T::I64, O::int(32))],
                ),
            );
            let kpptr = O::reg(format!("kpptr{dest}"));
            text.assign(
                format!("kppub_{dest}"),
                I::gep(T::I8, kpptr.clone(), vec![(T::I32, O::int(16))]),
            );
            let pub_slot = O::reg(format!("kppub_{dest}"));
            text.assign(
                format!("kppriv_{dest}"),
                I::gep(T::I8, kpptr.clone(), vec![(T::I32, O::int(24))]),
            );
            let priv_slot = O::reg(format!("kppriv_{dest}"));
            let args = vec![
                (T::I32, O::int(30)),
                (T::Ptr, pub_slot),
                (T::Ptr, priv_slot),
            ];
            text.assign(
                format!("kprc{dest}"),
                I::call(T::I32, "bn_rt_crypto_keypair_generate", args),
            );
            let rc = O::reg(format!("kprc{dest}"));
            text.assign(
                format!("kppayload{dest}"),
                I::cast(CastOp::PtrToInt, T::Ptr, kpptr.clone(), T::I64),
            );
            let kppayload = O::reg(format!("kppayload{dest}"));
            super::host_results::emit_status_result(
                text,
                destination,
                &rc,
                None,
                &kpptr,
                &kppayload,
            );
            return;
        }
        "PublicKey" => {
            emit_handle_index(text, analysis, &format!("cryh{dest}"), arguments[0]);
            let args = vec![(T::I64, cryh())];
            let call = I::call(T::I64, "bn_rt_crypto_private_key_public_key", args);
            text.assign(format!("crypub{dest}"), call);
            text.assign(
                format!("v{dest}"),
                I::cast(
                    CastOp::IntToPtr,
                    T::I64,
                    O::reg(format!("crypub{dest}")),
                    T::Ptr,
                ),
            );
            return;
        }
        "NewSecretKey" | "NewSecretKeyWith" => {
            let args = vec![
                (T::I64, O::int(256)),
                (T::Ptr, O::null()),
                (T::Ptr, O::null()),
            ];
            let call = I::call(T::I64, "bn_rt_crypto_secret_key_generate", args);
            text.assign(format!("cryh{dest}"), call);
            text.assign(
                format!("v{dest}"),
                I::cast(CastOp::IntToPtr, T::I64, cryh(), T::Ptr),
            );
            return;
        }
        "SecretKeyFromBytes" => {
            emit_handle_index(text, analysis, &format!("cryh{dest}"), arguments[0]);
            let args = vec![
                (T::I64, cryh()),
                (T::Ptr, v(arguments[1])),
                (T::Ptr, O::null()),
            ];
            let call = I::call(T::I64, "bn_rt_crypto_secret_key_from_bytes", args);
            text.assign(format!("crys{dest}"), call);
            text.assign(
                format!("v{dest}"),
                I::cast(
                    CastOp::IntToPtr,
                    T::I64,
                    O::reg(format!("crys{dest}")),
                    T::Ptr,
                ),
            );
            return;
        }
        "WrapKey" => {
            emit_handle_index(text, analysis, &format!("crya{dest}"), arguments[0]);
            emit_handle_index(text, analysis, &format!("cryb{dest}"), arguments[1]);
            let args = vec![
                (T::I64, O::reg(format!("crya{dest}"))),
                (T::I64, O::reg(format!("cryb{dest}"))),
            ];
            let call = I::call(T::I64, "bn_rt_crypto_secret_key_wrap", args);
            text.assign(format!("cryw{dest}"), call);
            text.assign(
                format!("v{dest}"),
                I::cast(
                    CastOp::IntToPtr,
                    T::I64,
                    O::reg(format!("cryw{dest}")),
                    T::Ptr,
                ),
            );
            return;
        }
        "UnwrapKey" | "UnwrapKeyWith" => {
            emit_handle_index(text, analysis, &format!("crya{dest}"), arguments[0]);
            emit_handle_index(text, analysis, &format!("cryb{dest}"), arguments[1]);
            let args = vec![
                (T::I64, O::reg(format!("crya{dest}"))),
                (T::I64, O::reg(format!("cryb{dest}"))),
                (T::I64, O::int(256)),
                (T::Ptr, O::null()),
                (T::Ptr, O::null()),
            ];
            let call = I::call(T::I64, "bn_rt_crypto_secret_key_unwrap", args);
            text.assign(format!("cryu{dest}"), call);
            text.assign(
                format!("v{dest}"),
                I::cast(
                    CastOp::IntToPtr,
                    T::I64,
                    O::reg(format!("cryu{dest}")),
                    T::Ptr,
                ),
            );
            return;
        }
        "Id" | "Purpose" => {
            emit_handle_index(text, analysis, &format!("cryh{dest}"), arguments[0]);
            let args = vec![(T::I64, cryh())];
            let is_pub = analysis
                .values
                .get(&arguments[0])
                .is_some_and(|ty| carries_type(ty, "PublicKey"));
            let is_priv = analysis
                .values
                .get(&arguments[0])
                .is_some_and(|ty| carries_type(ty, "PrivateKey"));
            let sym = if is_pub {
                "bn_rt_crypto_public_key_id"
            } else if is_priv {
                "bn_rt_crypto_private_key_id"
            } else if member == "Id" {
                "bn_rt_crypto_secret_key_id"
            } else {
                "bn_rt_crypto_secret_key_purpose"
            };
            let call = I::call(T::Ptr, sym, args);
            text.assign(format!("v{dest}"), call);
            return;
        }
        "Bits" => {
            emit_handle_index(text, analysis, &format!("cryh{dest}"), arguments[0]);
            let args = vec![(T::I64, cryh())];
            let call = I::call(T::I64, "bn_rt_crypto_secret_key_bits", args);
            text.assign(format!("cryb{dest}"), call);
            text.assign(
                format!("v{dest}"),
                I::cast(CastOp::Trunc, T::I64, O::reg(format!("cryb{dest}")), T::I32),
            );
            return;
        }
        "ExportSecret" => {
            emit_handle_index(text, analysis, &format!("cryh{dest}"), arguments[0]);
            let fmt_op = if arguments.len() > 1 {
                v(arguments[1])
            } else {
                O::null()
            };
            let args = vec![(T::I64, cryh()), (T::Ptr, fmt_op)];
            let call = I::call(T::I64, "bn_rt_crypto_secret_key_export", args);
            text.assign(format!("crye{dest}"), call);
            text.assign(
                format!("v{dest}"),
                I::cast(
                    CastOp::IntToPtr,
                    T::I64,
                    O::reg(format!("crye{dest}")),
                    T::Ptr,
                ),
            );
            return;
        }
        "Encrypt" => {
            emit_handle_index(text, analysis, &format!("crya{dest}"), arguments[0]);
            emit_handle_index(text, analysis, &format!("cryb{dest}"), arguments[1]);
            let args = vec![
                (T::I64, O::reg(format!("crya{dest}"))),
                (T::I64, O::reg(format!("cryb{dest}"))),
            ];
            let call = I::call(T::I64, "bn_rt_crypto_encrypt", args);
            text.assign(format!("cryenc{dest}"), call);
            text.assign(
                format!("v{dest}"),
                I::cast(
                    CastOp::IntToPtr,
                    T::I64,
                    O::reg(format!("cryenc{dest}")),
                    T::Ptr,
                ),
            );
            return;
        }
        "Decrypt" => {
            emit_handle_index(text, analysis, &format!("crya{dest}"), arguments[0]);
            emit_handle_index(text, analysis, &format!("cryb{dest}"), arguments[1]);
            let args = vec![
                (T::I64, O::reg(format!("crya{dest}"))),
                (T::I64, O::reg(format!("cryb{dest}"))),
            ];
            let call = I::call(T::I64, "bn_rt_crypto_decrypt", args);
            text.assign(format!("crydec{dest}"), call);
            text.assign(
                format!("v{dest}"),
                I::cast(
                    CastOp::IntToPtr,
                    T::I64,
                    O::reg(format!("crydec{dest}")),
                    T::Ptr,
                ),
            );
            return;
        }
        "EncryptText" => {
            emit_handle_index(text, analysis, &format!("cryh{dest}"), arguments[0]);
            let args = vec![(T::I64, cryh()), (T::Ptr, v(arguments[1]))];
            let call = I::call(T::Ptr, "bn_rt_crypto_encrypt_text", args);
            text.assign(format!("v{dest}"), call);
            return;
        }
        "DecryptText" => {
            emit_handle_index(text, analysis, &format!("cryh{dest}"), arguments[0]);
            let args = vec![(T::I64, cryh()), (T::Ptr, v(arguments[1]))];
            let call = I::call(T::Ptr, "bn_rt_crypto_decrypt_text", args);
            text.assign(format!("v{dest}"), call);
            return;
        }
        "EncryptFile" | "DecryptFile" => {
            emit_handle_index(text, analysis, &format!("cryh{dest}"), arguments[0]);
            let symbol = if member == "EncryptFile" {
                "bn_rt_crypto_encrypt_file"
            } else {
                "bn_rt_crypto_decrypt_file"
            };
            let args = vec![
                (T::I64, cryh()),
                (T::Ptr, v(arguments[1])),
                (T::Ptr, v(arguments[2])),
            ];
            text.assign(format!("cryrc{dest}"), I::call(T::I32, symbol, args));
            emit_void_result(text, destination, &O::reg(format!("cryrc{dest}")));
            return;
        }
        "Hash" => {
            emit_handle_index(text, analysis, &format!("cryh{dest}"), arguments[0]);
            let args = vec![(T::I64, cryh()), (T::I32, O::int(1))];
            let call = I::call(T::I64, "bn_rt_crypto_hash_handle", args);
            text.assign(format!("cryd{dest}"), call);
            text.assign(
                format!("v{dest}"),
                I::cast(
                    CastOp::IntToPtr,
                    T::I64,
                    O::reg(format!("cryd{dest}")),
                    T::Ptr,
                ),
            );
            return;
        }
        "HashText" => {
            let args = vec![(T::Ptr, v(arguments[0]))];
            let call = I::call(T::Ptr, "bn_rt_crypto_hash_text", args);
            text.assign(format!("v{dest}"), call);
            return;
        }
        "HashFile" => {
            let args = vec![(T::Ptr, v(arguments[0])), (T::I32, O::int(1))];
            lower_status_call(
                text,
                destination,
                "bn_rt_crypto_hash_file",
                args,
                Out::Handle,
            );
            return;
        }
        "HashWith" => {
            emit_handle_index(text, analysis, &format!("cryh{dest}"), arguments[0]);
            let args = vec![(T::I64, cryh()), (T::Ptr, v(arguments[1]))];
            lower_status_call(
                text,
                destination,
                "bn_rt_crypto_hash_with",
                args,
                Out::Handle,
            );
            return;
        }
        "Mac" => {
            emit_handle_index(text, analysis, &format!("cryk{dest}"), arguments[0]);
            emit_handle_index(text, analysis, &format!("cryd{dest}"), arguments[1]);
            let args = vec![
                (T::I64, O::reg(format!("cryk{dest}"))),
                (T::I64, O::reg(format!("cryd{dest}"))),
            ];
            let call = I::call(T::I64, "bn_rt_crypto_mac_handle", args);
            text.assign(format!("crym{dest}"), call);
            text.assign(
                format!("v{dest}"),
                I::cast(
                    CastOp::IntToPtr,
                    T::I64,
                    O::reg(format!("crym{dest}")),
                    T::Ptr,
                ),
            );
            return;
        }
        "MacWith" => {
            emit_handle_index(text, analysis, &format!("cryk{dest}"), arguments[0]);
            emit_handle_index(text, analysis, &format!("cryd{dest}"), arguments[1]);
            let args = vec![
                (T::I64, O::reg(format!("cryk{dest}"))),
                (T::I64, O::reg(format!("cryd{dest}"))),
                (T::Ptr, v(arguments[2])),
            ];
            lower_status_call(
                text,
                destination,
                "bn_rt_crypto_mac_with",
                args,
                Out::Handle,
            );
            return;
        }
        "VerifyMac" => {
            emit_handle_index(text, analysis, &format!("cryk{dest}"), arguments[0]);
            emit_handle_index(text, analysis, &format!("cryd{dest}"), arguments[1]);
            emit_handle_index(text, analysis, &format!("cryt{dest}"), arguments[2]);
            let args = vec![
                (T::I64, O::reg(format!("cryk{dest}"))),
                (T::I64, O::reg(format!("cryd{dest}"))),
                (T::I64, O::reg(format!("cryt{dest}"))),
            ];
            lower_status_call(
                text,
                destination,
                "bn_rt_crypto_mac_verify_handle",
                args,
                Out::Verify,
            );
            return;
        }
        "HashPassword" => {
            let args = vec![(T::Ptr, v(arguments[0]))];
            let call = I::call(T::Ptr, "bn_rt_crypto_hash_password", args);
            text.assign(format!("v{dest}"), call);
            return;
        }
        "VerifyPassword" => {
            let args = vec![(T::Ptr, v(arguments[0])), (T::Ptr, v(arguments[1]))];
            let call = I::call(T::I32, "bn_rt_crypto_verify_password", args);
            text.assign(format!("cryrc{dest}"), call);
            text.assign(
                format!("v{dest}"),
                I::icmp(
                    ICmpCond::Eq,
                    T::I32,
                    O::reg(format!("cryrc{dest}")),
                    O::int(1),
                ),
            );
            return;
        }
        "Hkdf" => {
            emit_handle_index(text, analysis, &format!("cryikm{dest}"), arguments[0]);
            emit_handle_index(text, analysis, &format!("crysalt{dest}"), arguments[1]);
            emit_handle_index(text, analysis, &format!("cryinfo{dest}"), arguments[2]);
            let len_ty = analysis
                .values
                .get(&arguments[3])
                .and_then(llvm_type)
                .unwrap_or(T::I64);
            let len_op = if len_ty == T::I64 {
                v(arguments[3])
            } else {
                let slot = format!("crylen{dest}");
                text.assign(
                    &slot,
                    I::cast(CastOp::SExt, len_ty, v(arguments[3]), T::I64),
                );
                O::reg(slot)
            };
            let args = vec![
                (T::I64, O::reg(format!("cryikm{dest}"))),
                (T::I64, O::reg(format!("crysalt{dest}"))),
                (T::I64, O::reg(format!("cryinfo{dest}"))),
                (T::I64, len_op),
            ];
            lower_status_call(text, destination, "bn_rt_crypto_hkdf", args, Out::Handle);
            return;
        }
        "New" | "NewWith" => {
            let is_key = analysis
                .values
                .get(&arguments[0])
                .is_some_and(|t| carries_type(t, "SecretKey") || carries_type(t, "Key"));
            if is_key {
                emit_handle_index(text, analysis, &format!("cryk{dest}"), arguments[0]);
                if member == "NewWith" {
                    let args = vec![
                        (T::I64, O::reg(format!("cryk{dest}"))),
                        (T::Ptr, v(arguments[1])),
                    ];
                    lower_status_call(
                        text,
                        destination,
                        "bn_rt_crypto_mac_stream_new_with",
                        args,
                        Out::Handle,
                    );
                    return;
                }
                let alg_op = if arguments.len() > 1 {
                    let ty = analysis
                        .values
                        .get(&arguments[1])
                        .and_then(llvm_type)
                        .unwrap_or(T::I32);
                    if ty == T::I32 {
                        v(arguments[1])
                    } else {
                        let slot = format!("cryalg{dest}");
                        text.assign(&slot, I::cast(CastOp::Trunc, ty, v(arguments[1]), T::I32));
                        O::reg(slot)
                    }
                } else {
                    O::int(11) // default MAC_HMAC_SHA256
                };
                let args = vec![(T::I64, O::reg(format!("cryk{dest}"))), (T::I32, alg_op)];
                lower_status_call(
                    text,
                    destination,
                    "bn_rt_crypto_mac_stream_new",
                    args,
                    Out::Handle,
                );
                return;
            }
            let ty = analysis
                .values
                .get(&arguments[0])
                .and_then(llvm_type)
                .unwrap_or(T::I32);
            let arg_op = if ty == T::I32 {
                v(arguments[0])
            } else {
                let slot = format!("cryalg{dest}");
                text.assign(&slot, I::cast(CastOp::Trunc, ty, v(arguments[0]), T::I32));
                O::reg(slot)
            };
            let args = vec![(T::I32, arg_op)];
            lower_status_call(
                text,
                destination,
                "bn_rt_crypto_hasher_new",
                args,
                Out::Handle,
            );
            return;
        }
        "Update" => {
            emit_handle_index(text, analysis, &format!("cryh{dest}"), arguments[0]);
            emit_handle_index(text, analysis, &format!("cryd{dest}"), arguments[1]);
            let is_mac = analysis
                .values
                .get(&arguments[0])
                .is_some_and(|t| carries_type(t, "MacStream"));
            let sym = if is_mac {
                "bn_rt_crypto_mac_stream_update_handle"
            } else {
                "bn_rt_crypto_hasher_update_handle"
            };
            let args = vec![(T::I64, cryh()), (T::I64, O::reg(format!("cryd{dest}")))];
            text.assign(format!("cryrc{dest}"), I::call(T::I32, sym, args));
            return;
        }
        "Finish" => {
            emit_handle_index(text, analysis, &format!("cryh{dest}"), arguments[0]);
            let is_mac = analysis
                .values
                .get(&arguments[0])
                .is_some_and(|t| carries_type(t, "MacStream"));
            let sym = if is_mac {
                "bn_rt_crypto_mac_stream_finish"
            } else {
                "bn_rt_crypto_hasher_finish"
            };
            let args = vec![(T::I64, cryh())];
            lower_status_call(text, destination, sym, args, Out::Bytes);
            return;
        }
        "Verify" => {
            emit_handle_index(text, analysis, &format!("crys{dest}"), arguments[0]);
            emit_handle_index(text, analysis, &format!("cryt{dest}"), arguments[1]);
            let args = vec![
                (T::I64, O::reg(format!("crys{dest}"))),
                (T::I64, O::reg(format!("cryt{dest}"))),
            ];
            lower_status_call(
                text,
                destination,
                "bn_rt_crypto_mac_stream_verify",
                args,
                Out::Verify,
            );
            return;
        }
        "GenerateNonce" | "GenerateKey" => {
            let ty = analysis
                .values
                .get(&arguments[0])
                .and_then(llvm_type)
                .unwrap_or(T::I32);
            let arg_op = if ty == T::I32 {
                v(arguments[0])
            } else {
                let slot = format!("cryalg{dest}");
                text.assign(&slot, I::cast(CastOp::Trunc, ty, v(arguments[0]), T::I32));
                O::reg(slot)
            };
            let args = vec![(T::I32, arg_op)];
            let sym = if member == "GenerateNonce" {
                "bn_rt_crypto_generate_nonce"
            } else {
                "bn_rt_crypto_generate_key"
            };
            lower_status_call(text, destination, sym, args, Out::Handle);
            return;
        }
        "ImportKey" => {
            emit_handle_index(text, analysis, &format!("cryh{dest}"), arguments[0]);
            let ty = analysis
                .values
                .get(&arguments[1])
                .and_then(llvm_type)
                .unwrap_or(T::I32);
            let arg1_op = if ty == T::I32 {
                v(arguments[1])
            } else {
                let slot = format!("cryalg{dest}");
                text.assign(&slot, I::cast(CastOp::Trunc, ty, v(arguments[1]), T::I32));
                O::reg(slot)
            };
            let args = vec![(T::I64, cryh()), (T::I32, arg1_op)];
            lower_status_call(
                text,
                destination,
                "bn_rt_crypto_import_key",
                args,
                Out::Handle,
            );
            return;
        }
        _ => {}
    }
    let variant = i64::from(member.starts_with("Ecdsa") || member.ends_with("ChaCha20"));
    // (symbol, selector, leading handle operands, result)
    let (symbol, selector, handles, out) = match member {
        "SealAesGcm" | "SealChaCha20" => ("seal", true, 4, Out::Handle),
        "OpenAesGcm" | "OpenChaCha20" => ("open", true, 4, Out::Handle),
        "Slice" => ("slice", false, 1, Out::Handle),
        "MlKemKeypair" => ("kem_keypair", false, arguments.len(), Out::Handle),
        "MlKemEncapsulate" => ("kem_encapsulate", false, arguments.len(), Out::Handle),
        "MlKemDecapsulate" => ("kem_decapsulate", false, arguments.len(), Out::Handle),
        "MlDsaKeypair" => ("dsa_keypair", false, arguments.len(), Out::Handle),
        "MlDsaSign" => ("dsa_sign", false, arguments.len(), Out::Handle),
        "MlDsaVerify" => ("dsa_verify", false, arguments.len(), Out::Verify),
        "Ed25519PublicKey" | "EcdsaP256PublicKey" => ("public_key", true, 1, Out::Handle),
        "Ed25519Sign" | "EcdsaP256Sign" => ("sign", true, 2, Out::Handle),
        "Ed25519Verify" | "EcdsaP256Verify" => ("verify", true, 3, Out::Verify),
        "HmacSha256" => ("hmac", false, 2, Out::Bytes),
        "VerifyHmacSha256" => ("hmac_verify", false, 3, Out::Verify),
        "Argon2id" => ("argon2id", false, 2, Out::Handle),
        other => unreachable!("unsupported BNCrypto member reached emission: {other}"),
    };
    let mut args = Vec::new();
    if selector {
        args.push((T::I32, O::int(variant)));
    }
    // `Slice` names its one handle `%cryh`; the integer operands after the
    // handles (bounds, Argon2id costs) widen to `i64`.
    let (handle_slot, int_prefix) = if member == "Slice" {
        (None, "crybound")
    } else {
        (Some("cryarg"), "crycost")
    };
    for (index, operand) in arguments[..handles].iter().enumerate() {
        let slot = handle_slot.map_or_else(
            || format!("cryh{dest}"),
            |prefix| format!("{prefix}{dest}_{index}"),
        );
        emit_handle_index(text, analysis, &slot, *operand);
        args.push((T::I64, O::reg(slot)));
    }
    for (index, operand) in arguments[handles..].iter().enumerate() {
        let ty = analysis
            .values
            .get(operand)
            .and_then(llvm_type)
            .unwrap_or(T::I64);
        if ty == T::I64 {
            args.push((T::I64, v(*operand)));
        } else {
            let slot = format!("{int_prefix}{dest}_{index}");
            text.assign(&slot, I::cast(CastOp::SExt, ty, v(*operand), T::I64));
            args.push((T::I64, O::reg(slot)));
        }
    }
    lower_status_call(
        text,
        destination,
        &format!("bn_rt_crypto_{symbol}"),
        args,
        out,
    );
}

pub(super) fn lower_status_call(
    text: &mut crate::ir::LlvmModule,
    destination: ValueId,
    symbol: &str,
    mut args: Vec<(T, O)>,
    out: Out,
) {
    let dest = destination.0;
    let rc = O::reg(format!("cryrc{dest}"));
    if out == Out::Verify {
        text.assign(format!("cryrc{dest}"), I::call(T::I32, symbol, args));
        text.assign(
            format!("v{dest}"),
            I::icmp(ICmpCond::Eq, T::I32, rc, O::int(1)),
        );
        return;
    }
    let slot = O::reg(format!("cryout{dest}"));
    text.assign(format!("cryout{dest}"), I::alloca(T::I64));
    args.push((T::Ptr, slot.clone()));
    text.assign(format!("cryrc{dest}"), I::call(T::I32, symbol, args));
    text.assign(format!("cryhandle{dest}"), I::load(T::I64, slot));
    if out == Out::Bytes {
        let handle = O::reg(format!("cryhandle{dest}"));
        text.assign(
            format!("v{dest}"),
            I::cast(CastOp::IntToPtr, T::I64, handle, T::Ptr),
        );
    } else {
        emit_handle_result(
            text,
            destination,
            &O::reg(format!("cryrc{dest}")),
            &O::reg(format!("cryhandle{dest}")),
        );
    }
}
