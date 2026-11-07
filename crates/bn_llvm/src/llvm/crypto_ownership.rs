// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Counted `BNCrypto` handles. The IR emits `Retain` and `Release`; this
//! module calls the runtime table, never `bn_rt_arc_retain`. Handle 0 is the
//! empty binding and the error arm of `T OR Error`.

use super::{Ownership, emit_ownership, fresh};
use crate::ir::{CastOp, ICmpCond, InstSink as _, LlvmInst, LlvmOperand, LlvmType};
use bn_ir::{Module, ModuleId};
use bn_types::Type;

/// Whether `ty` is a counted crypto handle, a `KeyPair`, or an alternative
/// that holds one.
pub(super) fn holds_counted_crypto(module: &Module, ty: &Type) -> bool {
    match ty {
        Type::Alternative(members) => members
            .iter()
            .any(|member| holds_counted_crypto(module, member)),
        other => provider_name(module, other).is_some_and(counted_name),
    }
}

/// Applies `op` when `ty` is a counted handle or a `KeyPair OR Error` record.
/// An exact `KeyPair` falls through so the struct walk releases its fields
/// without freeing the slot (that slot is the function's own storage).
pub(super) fn emit_if_counted(
    text: &mut crate::ir::LlvmModule,
    module: &Module,
    op: Ownership,
    ty: &Type,
    operand: &LlvmOperand,
    state: &mut crate::EmissionState,
) -> bool {
    if let Some((retain, release)) = handle_symbols(module, ty) {
        let symbol = match op {
            Ownership::Retain => retain,
            Ownership::Release => release,
        };
        emit_handle(text, ty, operand, symbol, state);
        return true;
    }
    if let Some(record) = keypair_member(module, ty) {
        emit_keypair_alternative(text, module, op, &record, operand, state);
        return true;
    }
    false
}

fn counted_name(name: &str) -> bool {
    bn_ir::names::is_bncrypto_counted_handle(name) || bn_ir::names::is_bncrypto_counted_record(name)
}

fn provider_name<'a>(module: &Module, ty: &'a Type) -> Option<&'a str> {
    match ty {
        Type::ImportedNamed {
            module: id, name, ..
        } if module.bncrypto_providers.contains(&ModuleId::from(*id)) => Some(name.as_str()),
        // A field of a library STRUCT is `Named("#<id>.PublicKey")`. The
        // layout qualifier rewrites the module's own types in place, so the
        // walk of a `KeyPair` never sees `ImportedNamed`.
        Type::Named(qualified) => qualified_provider_name(module, qualified),
        _ => None,
    }
}

/// `#<id>.Name` when `<id>` is a `BNCrypto` provider this program imports.
fn qualified_provider_name<'a>(module: &Module, qualified: &'a str) -> Option<&'a str> {
    let (id_text, name) = qualified.strip_prefix('#')?.split_once('.')?;
    let id = id_text.parse::<u32>().ok()?;
    module
        .bncrypto_providers
        .contains(&ModuleId(id))
        .then_some(name)
}

fn symbols_of(name: &str) -> Option<(&'static str, &'static str)> {
    match name {
        "Bytes" => Some(("bn_rt_crypto_bytes_retain", "bn_rt_crypto_bytes_release")),
        "Key" => Some(("bn_rt_crypto_key_retain", "bn_rt_crypto_key_release")),
        "Secret" => Some(("bn_rt_crypto_secret_retain", "bn_rt_crypto_secret_release")),
        "SecretKey" => Some((
            "bn_rt_crypto_secret_key_retain",
            "bn_rt_crypto_secret_key_release",
        )),
        "PublicKey" => Some((
            "bn_rt_crypto_public_key_retain",
            "bn_rt_crypto_public_key_release",
        )),
        "PrivateKey" => Some((
            "bn_rt_crypto_private_key_retain",
            "bn_rt_crypto_private_key_release",
        )),
        "Hasher" => Some(("bn_rt_crypto_hasher_retain", "bn_rt_crypto_hasher_release")),
        "MacStream" => Some((
            "bn_rt_crypto_mac_stream_retain",
            "bn_rt_crypto_mac_stream_release",
        )),
        _ => None,
    }
}

/// The retain and release symbols of a counted handle, or of the handle
/// member of `T OR Error`.
fn handle_symbols(module: &Module, ty: &Type) -> Option<(&'static str, &'static str)> {
    match ty {
        Type::Alternative(members) => members
            .iter()
            .find_map(|member| provider_name(module, member).and_then(symbols_of)),
        other => provider_name(module, other).and_then(symbols_of),
    }
}

/// The `KeyPair` member of a `KeyPair OR Error` alternative.
fn keypair_member(module: &Module, ty: &Type) -> Option<Type> {
    let Type::Alternative(members) = ty else {
        return None;
    };
    members
        .iter()
        .find(|member| provider_name(module, member).is_some_and(|name| name == "KeyPair"))
        .cloned()
}

fn emit_handle(
    text: &mut crate::ir::LlvmModule,
    ty: &Type,
    operand: &LlvmOperand,
    symbol: &str,
    state: &mut crate::EmissionState,
) {
    let tag = fresh(state, "cryown");
    let handle = if matches!(ty, Type::Alternative(_)) {
        let union = crate::layout::handle_result_ty();
        text.assign(
            format!("{tag}err"),
            LlvmInst::extract(union.clone(), operand.clone(), 0),
        );
        text.assign(
            format!("{tag}raw"),
            LlvmInst::extract(union, operand.clone(), 2),
        );
        text.assign(
            format!("{tag}handle"),
            LlvmInst::select(
                LlvmOperand::reg(format!("{tag}err")),
                LlvmType::I64,
                LlvmOperand::int(0),
                LlvmOperand::reg(format!("{tag}raw")),
            ),
        );
        LlvmOperand::reg(format!("{tag}handle"))
    } else {
        text.assign(
            format!("{tag}handle"),
            LlvmInst::cast(
                CastOp::PtrToInt,
                LlvmType::Ptr,
                operand.clone(),
                LlvmType::I64,
            ),
        );
        LlvmOperand::reg(format!("{tag}handle"))
    };
    text.assign(
        format!("{tag}rc"),
        LlvmInst::call(LlvmType::I32, symbol, vec![(LlvmType::I64, handle)]),
    );
}

/// Releases or retains the keys in a `KeyPair OR Error` and, on release,
/// frees the record the constructor allocated. An error still frees that
/// record; it holds no keys. A null pointer (the binding was already taken)
/// does nothing.
fn emit_keypair_alternative(
    text: &mut crate::ir::LlvmModule,
    module: &Module,
    op: Ownership,
    record: &Type,
    operand: &LlvmOperand,
    state: &mut crate::EmissionState,
) {
    let tag = fresh(state, "kprec");
    let union = crate::layout::handle_result_ty();
    let done = format!("{tag}done");
    let check_ptr = format!("{tag}ptr");
    let walk = format!("{tag}walk");
    text.assign(
        format!("{tag}err"),
        LlvmInst::extract(union.clone(), operand.clone(), 0),
    );
    text.assign(
        format!("{tag}rec"),
        LlvmInst::extract(union, operand.clone(), 1),
    );
    text.emit(LlvmInst::CondBr {
        cond: LlvmOperand::reg(format!("{tag}err")),
        true_dest: format!("{tag}free"),
        false_dest: check_ptr.clone(),
    });
    state.control_flow.label(text, check_ptr);
    text.assign(
        format!("{tag}null"),
        LlvmInst::icmp(
            ICmpCond::Eq,
            LlvmType::Ptr,
            LlvmOperand::reg(format!("{tag}rec")),
            LlvmOperand::null(),
        ),
    );
    text.emit(LlvmInst::CondBr {
        cond: LlvmOperand::reg(format!("{tag}null")),
        true_dest: done.clone(),
        false_dest: walk.clone(),
    });
    state.control_flow.label(text, walk);
    emit_ownership(
        text,
        module,
        op,
        record,
        &LlvmOperand::reg(format!("{tag}rec")),
        state,
    );
    text.emit(LlvmInst::Br {
        dest: format!("{tag}free"),
    });
    state.control_flow.label(text, format!("{tag}free"));
    if op == Ownership::Release {
        text.assign(
            format!("{tag}free_null"),
            LlvmInst::icmp(
                ICmpCond::Eq,
                LlvmType::Ptr,
                LlvmOperand::reg(format!("{tag}rec")),
                LlvmOperand::null(),
            ),
        );
        text.emit(LlvmInst::CondBr {
            cond: LlvmOperand::reg(format!("{tag}free_null")),
            true_dest: done.clone(),
            false_dest: format!("{tag}do_free"),
        });
        state.control_flow.label(text, format!("{tag}do_free"));
        text.emit(LlvmInst::call(
            LlvmType::Void,
            "free",
            vec![(LlvmType::Ptr, LlvmOperand::reg(format!("{tag}rec")))],
        ));
    }
    text.emit(LlvmInst::Br { dest: done.clone() });
    state.control_flow.label(text, done);
}
