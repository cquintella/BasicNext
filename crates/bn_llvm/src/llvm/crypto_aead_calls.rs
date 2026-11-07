// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

// Sealed envelopes and the one-shot MAC of `BNCrypto`. Each member is
// `T OR Error`: the `bn_rt` status says whether it failed, so a wrong key
// or a tampered envelope is an `Error`, never a handle the program reads on.
#![allow(clippy::wildcard_imports)]
use super::super::json_calls::emit_handle_index;
use super::super::*;
use super::crypto_asymmetric_calls::emit_string_status;
use super::{Out, lower_status_call};
use crate::ir::{CastOp, InstSink as _, LlvmInst as I, LlvmOperand as O, LlvmType as T};
use crate::layout::{class_layout_fields, field_byte_offset, struct_owner};

/// `AeadOptions` fields in declaration order (`bncrypto.md`, "Options & Streaming").
const AEAD_OPTS_ALG: usize = 0;
const AEAD_OPTS_AAD: usize = 1;
const AEAD_OPTS_INCLUDE_KEY_ID: usize = 2;

pub(super) fn try_lower_aead_call(
    text: &mut crate::ir::LlvmModule,
    module: &Module,
    analysis: &LoweringAnalysis<'_>,
    destination: ValueId,
    member: &str,
    arguments: &[ValueId],
) -> bool {
    let dest = destination.0;
    let handle = |text: &mut crate::ir::LlvmModule, name: &str, value: ValueId| {
        emit_handle_index(text, analysis, &format!("{name}{dest}"), value);
        (T::I64, O::reg(format!("{name}{dest}")))
    };
    match member {
        "Encrypt" | "Decrypt" => {
            let symbol = if member == "Encrypt" {
                "bn_rt_crypto_encrypt_status"
            } else {
                "bn_rt_crypto_decrypt_status"
            };
            let args = vec![
                handle(text, "crya", arguments[0]),
                handle(text, "cryb", arguments[1]),
            ];
            lower_status_call(text, destination, symbol, args, Out::Handle);
            true
        }
        "EncryptText" | "DecryptText" => {
            let symbol = if member == "EncryptText" {
                "bn_rt_crypto_encrypt_text_status"
            } else {
                "bn_rt_crypto_decrypt_text_status"
            };
            let args = vec![
                handle(text, "cryh", arguments[0]),
                (T::Ptr, O::reg(format!("v{}", arguments[1].0))),
            ];
            emit_string_status(text, destination, symbol, args);
            true
        }
        // `Mac` is `MacWith` with no options record: HMAC-SHA256, default policy.
        "Mac" => {
            let args = vec![
                handle(text, "cryk", arguments[0]),
                handle(text, "cryd", arguments[1]),
                (T::Ptr, O::null()),
            ];
            lower_status_call(
                text,
                destination,
                "bn_rt_crypto_mac_with",
                args,
                Out::Handle,
            );
            true
        }
        "EncryptWith" | "DecryptWith" => {
            let mut args = vec![
                handle(text, "crya", arguments[0]),
                handle(text, "cryb", arguments[1]),
            ];
            let fields = options_record_fields(text, module, analysis, dest, arguments[2]);
            args.push(fields[AEAD_OPTS_ALG].clone());
            args.push(fields[AEAD_OPTS_AAD].clone());
            let symbol = if member == "EncryptWith" {
                args.push(fields[AEAD_OPTS_INCLUDE_KEY_ID].clone());
                "bn_rt_crypto_encrypt_with"
            } else {
                "bn_rt_crypto_decrypt_with"
            };
            lower_status_call(text, destination, symbol, args, Out::Handle);
            true
        }
        "HashPasswordWith" | "NeedsRehash" => {
            let mut args = vec![(T::Ptr, O::reg(format!("v{}", arguments[0].0)))];
            args.extend(options_record_fields(
                text,
                module,
                analysis,
                dest,
                arguments[1],
            ));
            if member == "HashPasswordWith" {
                emit_string_status(text, destination, "bn_rt_crypto_hash_password_with", args);
            } else {
                lower_status_call(
                    text,
                    destination,
                    "bn_rt_crypto_needs_rehash",
                    args,
                    Out::Verify,
                );
            }
            true
        }
        "DeriveKey" => {
            let args = vec![
                (T::Ptr, O::reg(format!("v{}", arguments[0].0))),
                handle(text, "crys", arguments[1]),
            ];
            lower_status_call(
                text,
                destination,
                "bn_rt_crypto_derive_key",
                args,
                Out::Handle,
            );
            true
        }
        "DeriveSubkey" => {
            let args = vec![
                handle(text, "crym", arguments[0]),
                (T::Ptr, O::reg(format!("v{}", arguments[1].0))),
            ];
            lower_status_call(
                text,
                destination,
                "bn_rt_crypto_derive_subkey",
                args,
                Out::Handle,
            );
            true
        }
        _ => false,
    }
}

/// The fields of the options record `value`, in declaration order, as the C
/// ABI takes them: an `INTEGER` as `i32`, a `BOOLEAN` as `i32` 0 or 1, a
/// `bn_rt` handle (`Bytes`) as `i64`. Offsets come from the backend's own
/// layout, as `Log.Event` fields do.
pub(super) fn options_record_fields(
    text: &mut crate::ir::LlvmModule,
    module: &Module,
    analysis: &LoweringAnalysis<'_>,
    dest: u32,
    value: ValueId,
) -> Vec<(T, O)> {
    let owner = analysis
        .values
        .get(&value)
        .and_then(|ty| struct_owner(module, ty))
        .expect("validated options record argument");
    let mut fields = Vec::new();
    for (index, field) in class_layout_fields(module, &owner).into_iter().enumerate() {
        let offset = field_byte_offset(module, &field.reference).expect("validated field");
        let at = vec![(T::I32, O::uint(u64::from(offset)))];
        let pointer = format!("optsptr{dest}_{index}");
        let loaded = format!("opts{dest}_{index}");
        let wide = format!("optswide{dest}_{index}");
        text.assign(&pointer, I::gep(T::I8, O::reg(format!("v{}", value.0)), at));
        match llvm_type(&field.ty).expect("validated options field type") {
            T::I1 => {
                text.assign(&loaded, I::load(T::I1, O::reg(pointer)));
                text.assign(&wide, I::cast(CastOp::ZExt, T::I1, O::reg(loaded), T::I32));
                fields.push((T::I32, O::reg(wide)));
            }
            T::Ptr => {
                text.assign(&loaded, I::load(T::Ptr, O::reg(pointer)));
                text.assign(
                    &wide,
                    I::cast(CastOp::PtrToInt, T::Ptr, O::reg(loaded), T::I64),
                );
                fields.push((T::I64, O::reg(wide)));
            }
            scalar => {
                text.assign(&loaded, I::load(scalar.clone(), O::reg(pointer)));
                fields.push((scalar, O::reg(loaded)));
            }
        }
    }
    fields
}
