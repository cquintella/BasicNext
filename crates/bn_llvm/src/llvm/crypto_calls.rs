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
use crate::layout::typed_llvm;

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

pub(super) fn lower_bncrypto_call(
    text: &mut String,
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
        "Length" | "ToHex" => {
            emit_handle_index(text, analysis, &format!("cryh{dest}"), arguments[0]);
            let args = vec![(T::I64, cryh())];
            if member == "ToHex" {
                let call = I::call(T::Ptr, "bn_rt_crypto_bytes_to_hex", args);
                text.assign(format!("v{dest}"), call);
            } else {
                let call = I::call(T::I64, "bn_rt_crypto_bytes_length", args);
                text.assign(format!("cryl{dest}"), call);
                let length = O::reg(format!("cryl{dest}"));
                text.assign(
                    format!("v{dest}"),
                    I::cast(CastOp::Trunc, T::I64, length, T::I32),
                );
            }
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
            .unwrap_or("i64");
        if ty == "i64" {
            args.push((T::I64, v(*operand)));
        } else {
            let slot = format!("{int_prefix}{dest}_{index}");
            text.assign(
                &slot,
                I::cast(CastOp::SExt, typed_llvm(ty), v(*operand), T::I64),
            );
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
    text: &mut String,
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
            format!("%cryrc{dest}"),
            format!("%cryhandle{dest}"),
        );
    }
}
