// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

// Encoding codecs and non-cryptographic checksum calls for `BNCrypto`.
#![allow(clippy::wildcard_imports)]
use super::crypto_calls::{Out, lower_status_call};
use super::json_calls::emit_handle_index;
use super::*;
use crate::ir::{ICmpCond, InstSink as _, LlvmInst as I, LlvmOperand as O, LlvmType as T};

pub(super) fn try_lower_encoding_call(
    text: &mut crate::ir::LlvmModule,
    analysis: &LoweringAnalysis<'_>,
    destination: ValueId,
    member: &str,
    arguments: &[ValueId],
) -> bool {
    let dest = destination.0;
    let v = |id: ValueId| O::reg(format!("v{}", id.0));
    let cryh = || O::reg(format!("cryh{dest}"));

    match member {
        "ToHex" => {
            emit_handle_index(text, analysis, &format!("cryh{dest}"), arguments[0]);
            let args = vec![(T::I64, cryh())];
            let call = I::call(T::Ptr, "bn_rt_crypto_bytes_to_hex", args);
            text.assign(format!("v{dest}"), call);
            true
        }
        "ToBase64" => {
            emit_handle_index(text, analysis, &format!("cryh{dest}"), arguments[0]);
            let args = vec![(T::I64, cryh())];
            let call = I::call(T::Ptr, "bn_rt_crypto_bytes_to_base64", args);
            text.assign(format!("v{dest}"), call);
            true
        }
        "ToBase64Url" => {
            emit_handle_index(text, analysis, &format!("cryh{dest}"), arguments[0]);
            let args = vec![(T::I64, cryh())];
            let call = I::call(T::Ptr, "bn_rt_crypto_bytes_to_base64url", args);
            text.assign(format!("v{dest}"), call);
            true
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
            true
        }
        "FromBase64" | "FromBase64Url" | "FromPem" => {
            let sym = match member {
                "FromBase64" => "bn_rt_crypto_bytes_from_base64",
                "FromBase64Url" => "bn_rt_crypto_bytes_from_base64url",
                _ => "bn_rt_crypto_from_pem",
            };
            let args = vec![(T::Ptr, v(arguments[0]))];
            lower_status_call(text, destination, sym, args, Out::Handle);
            true
        }
        "ToText" => {
            emit_handle_index(text, analysis, &format!("cryh{dest}"), arguments[0]);
            let slot = O::reg(format!("cryout{dest}"));
            text.assign(format!("cryout{dest}"), I::alloca(T::Ptr));
            text.emit(I::store(T::Ptr, O::null(), slot.clone()));
            let args = vec![(T::I64, O::reg(format!("cryh{dest}"))), (T::Ptr, slot)];
            text.assign(
                format!("cryrc{dest}"),
                I::call(T::I32, "bn_rt_crypto_bytes_to_text", args),
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
        "Concat" => {
            emit_handle_index(text, analysis, &format!("cryha{dest}"), arguments[0]);
            emit_handle_index(text, analysis, &format!("cryhb{dest}"), arguments[1]);
            let args = vec![
                (T::I64, O::reg(format!("cryha{dest}"))),
                (T::I64, O::reg(format!("cryhb{dest}"))),
            ];
            lower_status_call(
                text,
                destination,
                "bn_rt_crypto_bytes_concat",
                args,
                Out::Bytes,
            );
            true
        }
        "ToPem" => {
            emit_handle_index(text, analysis, &format!("cryh{dest}"), arguments[1]);
            let args = vec![
                (T::Ptr, v(arguments[0])),
                (T::I64, O::reg(format!("cryh{dest}"))),
            ];
            let call = I::call(T::Ptr, "bn_rt_crypto_to_pem", args);
            text.assign(format!("v{dest}"), call);
            true
        }
        "Crc32" | "Crc32c" | "Adler32" | "XxHash64" => {
            emit_handle_index(text, analysis, &format!("cryh{dest}"), arguments[0]);
            let args = vec![(T::I64, O::reg(format!("cryh{dest}")))];
            let sym = match member {
                "Crc32" => "bn_rt_crypto_crc32",
                "Crc32c" => "bn_rt_crypto_crc32c",
                "Adler32" => "bn_rt_crypto_adler32",
                _ => "bn_rt_crypto_xxhash64",
            };
            let call = I::call(T::I64, sym, args);
            text.assign(format!("v{dest}"), call);
            true
        }
        "VerifyFileChecksum" => {
            let args = vec![(T::Ptr, v(arguments[0])), (T::Ptr, v(arguments[1]))];
            let call = I::call(T::I32, "bn_rt_crypto_verify_file_checksum", args);
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
            true
        }
        _ => false,
    }
}
