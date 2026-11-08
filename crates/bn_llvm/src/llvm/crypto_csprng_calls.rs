// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

// CSPRNG calls for `BNCrypto`.
#![allow(clippy::wildcard_imports)]
use super::crypto_calls::{Out, lower_status_call};
use super::*;
use crate::ir::{CastOp, InstSink as _, LlvmInst as I, LlvmOperand as O, LlvmType as T};

pub(super) fn try_lower_csprng_call(
    text: &mut crate::ir::LlvmModule,
    analysis: &LoweringAnalysis<'_>,
    destination: ValueId,
    member: &str,
    arguments: &[ValueId],
) -> bool {
    let dest = destination.0;
    let v = |id: ValueId| O::reg(format!("v{}", id.0));

    match member {
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
            true
        }
        "RandomUuidV4" | "UuidV7" => {
            let sym = if member == "RandomUuidV4" {
                "bn_rt_crypto_random_uuid_v4"
            } else {
                "bn_rt_crypto_uuid_v7"
            };
            text.assign(format!("v{dest}"), I::call(T::Ptr, sym, Vec::new()));
            true
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
            true
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
            true
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
            true
        }
        _ => false,
    }
}
