// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

// Signatures and PKCS#8 PEM for `BNCrypto` key handles.
#![allow(clippy::wildcard_imports)]
use super::crypto_calls::{Out, lower_status_call};
use super::json_calls::emit_handle_index;
use super::*;
use crate::ir::{CastOp, InstSink as _, LlvmInst as I, LlvmOperand as O, LlvmType as T};

pub(super) fn try_lower_asymmetric_call(
    text: &mut crate::ir::LlvmModule,
    analysis: &LoweringAnalysis<'_>,
    destination: ValueId,
    member: &str,
    arguments: &[ValueId],
) -> bool {
    let dest = destination.0;
    let v = |id: ValueId| O::reg(format!("v{}", id.0));
    match member {
        "Sign" => {
            emit_handle_index(text, analysis, &format!("cryk{dest}"), arguments[0]);
            emit_handle_index(text, analysis, &format!("crym{dest}"), arguments[1]);
            lower_status_call(
                text,
                destination,
                "bn_rt_crypto_private_sign",
                vec![
                    (T::I64, O::reg(format!("cryk{dest}"))),
                    (T::I64, O::reg(format!("crym{dest}"))),
                ],
                Out::Handle,
            );
            true
        }
        "Verify" if arguments.len() == 3 => {
            emit_handle_index(text, analysis, &format!("cryk{dest}"), arguments[0]);
            emit_handle_index(text, analysis, &format!("crym{dest}"), arguments[1]);
            emit_handle_index(text, analysis, &format!("crys{dest}"), arguments[2]);
            lower_status_call(
                text,
                destination,
                "bn_rt_crypto_public_verify",
                vec![
                    (T::I64, O::reg(format!("cryk{dest}"))),
                    (T::I64, O::reg(format!("crym{dest}"))),
                    (T::I64, O::reg(format!("crys{dest}"))),
                ],
                Out::Verify,
            );
            true
        }
        "SignFile" => {
            emit_handle_index(text, analysis, &format!("cryk{dest}"), arguments[0]);
            lower_status_call(
                text,
                destination,
                "bn_rt_crypto_sign_file",
                vec![
                    (T::I64, O::reg(format!("cryk{dest}"))),
                    (T::Ptr, v(arguments[1])),
                ],
                Out::Handle,
            );
            true
        }
        "VerifyFile" => {
            emit_handle_index(text, analysis, &format!("cryk{dest}"), arguments[0]);
            emit_handle_index(text, analysis, &format!("crys{dest}"), arguments[2]);
            lower_status_call(
                text,
                destination,
                "bn_rt_crypto_verify_file",
                vec![
                    (T::I64, O::reg(format!("cryk{dest}"))),
                    (T::Ptr, v(arguments[1])),
                    (T::I64, O::reg(format!("crys{dest}"))),
                ],
                Out::Verify,
            );
            true
        }
        "ExportPem" if arguments.len() == 1 => {
            emit_handle_index(text, analysis, &format!("cryk{dest}"), arguments[0]);
            emit_string_status(
                text,
                destination,
                "bn_rt_crypto_public_export_pem",
                vec![(T::I64, O::reg(format!("cryk{dest}")))],
            );
            true
        }
        "ExportPem" => {
            emit_handle_index(text, analysis, &format!("cryk{dest}"), arguments[0]);
            let format = i32_argument(text, analysis, &format!("cryfmt{dest}"), arguments[1]);
            emit_string_status(
                text,
                destination,
                "bn_rt_crypto_private_export_pem",
                vec![
                    (T::I64, O::reg(format!("cryk{dest}"))),
                    (T::I32, format),
                    (T::Ptr, v(arguments[2])),
                ],
            );
            true
        }
        "ExportPemUnencrypted" => {
            emit_handle_index(text, analysis, &format!("cryk{dest}"), arguments[0]);
            let format = i32_argument(text, analysis, &format!("cryfmt{dest}"), arguments[1]);
            emit_string_status(
                text,
                destination,
                "bn_rt_crypto_private_export_pem_plain",
                vec![(T::I64, O::reg(format!("cryk{dest}"))), (T::I32, format)],
            );
            true
        }
        "Fingerprint" => {
            emit_handle_index(text, analysis, &format!("cryk{dest}"), arguments[0]);
            let alg = i32_argument(text, analysis, &format!("cryalg{dest}"), arguments[1]);
            emit_string_status(
                text,
                destination,
                "bn_rt_crypto_public_fingerprint",
                vec![(T::I64, O::reg(format!("cryk{dest}"))), (T::I32, alg)],
            );
            true
        }
        "LoadPrivateKey" => {
            lower_status_call(
                text,
                destination,
                "bn_rt_crypto_load_private_key",
                vec![(T::Ptr, v(arguments[0])), (T::Ptr, v(arguments[1]))],
                Out::Handle,
            );
            true
        }
        "LoadPublicKey" => {
            lower_status_call(
                text,
                destination,
                "bn_rt_crypto_load_public_key",
                vec![(T::Ptr, v(arguments[0]))],
                Out::Handle,
            );
            true
        }
        _ => false,
    }
}

fn i32_argument(
    text: &mut crate::ir::LlvmModule,
    analysis: &LoweringAnalysis<'_>,
    slot: &str,
    value: ValueId,
) -> O {
    let ty = analysis
        .values
        .get(&value)
        .and_then(llvm_type)
        .unwrap_or(T::I64);
    let raw = O::reg(format!("v{}", value.0));
    if ty == T::I32 {
        raw
    } else {
        text.assign(slot, I::cast(CastOp::Trunc, ty, raw, T::I32));
        O::reg(slot)
    }
}

fn emit_string_status(
    text: &mut crate::ir::LlvmModule,
    destination: ValueId,
    symbol: &str,
    mut args: Vec<(T, O)>,
) {
    let dest = destination.0;
    let slot = O::reg(format!("cryout{dest}"));
    text.assign(format!("cryout{dest}"), I::alloca(T::Ptr));
    text.emit(I::store(T::Ptr, O::null(), slot.clone()));
    args.push((T::Ptr, slot));
    text.assign(format!("cryrc{dest}"), I::call(T::I32, symbol, args));
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
}
