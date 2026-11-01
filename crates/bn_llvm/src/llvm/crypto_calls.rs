// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

// Calls to `BNCrypto` members. `Bytes` and keys travel as `bn_rt` table
// indices (`i64`), as BNLog resources do.
#![allow(clippy::wildcard_imports)]
use super::json_calls::emit_handle_index;
use super::*;
use crate::ir::{CastOp, ICmpCond, InstSink as _, LlvmInst as I, LlvmOperand as O, LlvmType as T};

/// How a member's `bn_rt` status becomes its result.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Out {
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
        "Length" | "ToHex" | "Algorithm" | "Zeroize" => {
            emit_handle_index(text, analysis, &format!("cryh{dest}"), arguments[0]);
            let args = vec![(T::I64, cryh())];
            if member == "ToHex" {
                let call = I::call(T::Ptr, "bn_rt_crypto_bytes_to_hex", args);
                text.assign(format!("v{dest}"), call);
            } else if member == "Algorithm" {
                let is_secret_key = analysis
                    .values
                    .get(&arguments[0])
                    .is_some_and(|ty| carries_type(ty, "SecretKey"));
                if is_secret_key {
                    let call = I::call(T::Ptr, "bn_rt_crypto_secret_key_algorithm", args);
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
                let is_key = analysis
                    .values
                    .get(&arguments[0])
                    .is_some_and(|ty| carries_type(ty, "Key"));
                let is_secret = analysis
                    .values
                    .get(&arguments[0])
                    .is_some_and(|ty| carries_type(ty, "Secret"));
                let symbol = if is_secret_key {
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
            let sym = if member == "Id" {
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
        "New" => {
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
            let args = vec![(T::I64, cryh()), (T::I64, O::reg(format!("cryd{dest}")))];
            text.assign(
                format!("cryrc{dest}"),
                I::call(T::I32, "bn_rt_crypto_hasher_update_handle", args),
            );
            return;
        }
        "Finish" => {
            emit_handle_index(text, analysis, &format!("cryh{dest}"), arguments[0]);
            let args = vec![(T::I64, cryh())];
            lower_status_call(
                text,
                destination,
                "bn_rt_crypto_hasher_finish",
                args,
                Out::Bytes,
            );
            return;
        }
        "RandomBytes" => {
            let ty = analysis
                .values
                .get(&arguments[0])
                .and_then(llvm_type)
                .unwrap_or(T::I64);
            let arg_op = if ty == T::I64 {
                v(arguments[0])
            } else {
                let slot = format!("crybound{dest}");
                text.assign(&slot, I::cast(CastOp::SExt, ty, v(arguments[0]), T::I64));
                O::reg(slot)
            };
            let args = vec![(T::I64, arg_op)];
            lower_status_call(
                text,
                destination,
                "bn_rt_crypto_random_bytes",
                args,
                Out::Handle,
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
        "FromHex" => {
            let args = vec![(T::Ptr, v(arguments[0]))];
            lower_status_call(
                text,
                destination,
                "bn_rt_crypto_bytes_from_hex",
                args,
                Out::Handle,
            );
            return;
        }
        "RandomUuidV4" | "UuidV7" => {
            let sym = if member == "RandomUuidV4" {
                "bn_rt_crypto_random_uuid_v4"
            } else {
                "bn_rt_crypto_uuid_v7"
            };
            text.assign(format!("v{dest}"), I::call(T::Ptr, sym, Vec::new()));
            return;
        }
        "RandomInt" => {
            let mut args = Vec::new();
            for (index, operand) in arguments.iter().enumerate() {
                let ty = analysis
                    .values
                    .get(operand)
                    .and_then(llvm_type)
                    .unwrap_or(T::I64);
                let op = if ty == T::I64 {
                    v(*operand)
                } else {
                    let slot = format!("cryint{dest}_{index}");
                    text.assign(&slot, I::cast(CastOp::SExt, ty, v(*operand), T::I64));
                    O::reg(slot)
                };
                args.push((T::I64, op));
            }
            let slot = O::reg(format!("cryout{dest}"));
            text.assign(format!("cryout{dest}"), I::alloca(T::I64));
            text.emit(I::store(T::I64, O::int(0), slot.clone()));
            args.push((T::Ptr, slot));
            text.assign(
                format!("cryrc{dest}"),
                I::call(T::I32, "bn_rt_crypto_random_int", args),
            );
            text.assign(
                format!("cryval{dest}"),
                I::load(T::I64, O::reg(format!("cryout{dest}"))),
            );
            emit_status_result(
                text,
                destination,
                &O::reg(format!("cryrc{dest}")),
                None,
                &O::null(),
                &O::reg(format!("cryval{dest}")),
            );
            return;
        }
        "RandomToken" => {
            let ty = analysis
                .values
                .get(&arguments[0])
                .and_then(llvm_type)
                .unwrap_or(T::I64);
            let arg_op = if ty == T::I64 {
                v(arguments[0])
            } else {
                let slot = format!("crybound{dest}");
                text.assign(&slot, I::cast(CastOp::SExt, ty, v(arguments[0]), T::I64));
                O::reg(slot)
            };
            let slot = O::reg(format!("cryout{dest}"));
            text.assign(format!("cryout{dest}"), I::alloca(T::Ptr));
            text.emit(I::store(T::Ptr, O::null(), slot.clone()));
            let args = vec![(T::I64, arg_op), (T::Ptr, slot)];
            text.assign(
                format!("cryrc{dest}"),
                I::call(T::I32, "bn_rt_crypto_random_token", args),
            );
            text.assign(
                format!("crystr{dest}"),
                I::load(T::Ptr, O::reg(format!("cryout{dest}"))),
            );
            emit_status_result(
                text,
                destination,
                &O::reg(format!("cryrc{dest}")),
                None,
                &O::reg(format!("crystr{dest}")),
                &O::int(0),
            );
            return;
        }
        "RandomPassword" => {
            let slot = O::reg(format!("cryout{dest}"));
            text.assign(format!("cryout{dest}"), I::alloca(T::Ptr));
            text.emit(I::store(T::Ptr, O::null(), slot.clone()));
            let args = vec![(T::Ptr, v(arguments[0])), (T::Ptr, slot)];
            text.assign(
                format!("cryrc{dest}"),
                I::call(T::I32, "bn_rt_crypto_random_password", args),
            );
            text.assign(
                format!("crystr{dest}"),
                I::load(T::Ptr, O::reg(format!("cryout{dest}"))),
            );
            emit_status_result(
                text,
                destination,
                &O::reg(format!("cryrc{dest}")),
                None,
                &O::reg(format!("crystr{dest}")),
                &O::int(0),
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

fn lower_status_call(
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
