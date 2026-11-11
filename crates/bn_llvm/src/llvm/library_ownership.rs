// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Counted library handles (`BNCrypto`, `BNJson`, `BNData`, `BNSqlite`,
//! `BNCompress`). The IR emits `Retain` and `Release`; this module calls the
//! runtime table, never `bn_rt_arc_retain`. Handle 0 is the empty binding and
//! the error arm of `T OR Error`.

use super::{Ownership, emit_ownership, fresh};
use crate::ir::{CastOp, ICmpCond, InstSink as _, LlvmInst, LlvmOperand, LlvmType};
use bn_ir::{Module, ModuleId};
use bn_types::Type;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ProviderKind {
    Crypto,
    Json,
    Data,
    Sqlite,
    Compress,
    Web,
}

/// Whether `ty` is a counted library handle, a `KeyPair`, or an alternative
/// that holds one.
pub(super) fn holds_counted_library(module: &Module, ty: &Type) -> bool {
    match ty {
        Type::Alternative(members) => members
            .iter()
            .any(|member| holds_counted_library(module, member)),
        other => handle_symbols(module, other).is_some() || is_keypair(module, other),
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

fn provider_of(module: &Module, module_id: ModuleId) -> Option<ProviderKind> {
    if module.bncrypto_providers.contains(&module_id) {
        Some(ProviderKind::Crypto)
    } else if module.bnjson_providers.contains(&module_id) {
        Some(ProviderKind::Json)
    } else if module.bndata_providers.contains(&module_id) {
        Some(ProviderKind::Data)
    } else if module.bnsqlite_providers.contains(&module_id) {
        Some(ProviderKind::Sqlite)
    } else if module.bncompress_providers.contains(&module_id) {
        Some(ProviderKind::Compress)
    } else if module.bnweb_providers.contains(&module_id) {
        Some(ProviderKind::Web)
    } else {
        None
    }
}

fn provider_and_name<'a>(module: &Module, ty: &'a Type) -> Option<(ProviderKind, &'a str)> {
    match ty {
        Type::ImportedNamed {
            module: id, name, ..
        } => {
            let provider = provider_of(module, ModuleId::from(*id))?;
            Some((provider, name.as_str()))
        }
        // A field of a library STRUCT is `Named("#<id>.PublicKey")`. The
        // layout qualifier rewrites the module's own types in place, so the
        // walk of a `KeyPair` never sees `ImportedNamed`.
        Type::Named(qualified) => {
            let (id_text, name) = qualified.strip_prefix('#')?.split_once('.')?;
            let id = id_text.parse::<u32>().ok()?;
            let provider = provider_of(module, ModuleId(id))?;
            Some((provider, name))
        }
        _ => None,
    }
}

fn symbols_of(provider: ProviderKind, name: &str) -> Option<(&'static str, &'static str)> {
    match (provider, name) {
        (ProviderKind::Crypto, "Bytes") => {
            Some(("bn_rt_crypto_bytes_retain", "bn_rt_crypto_bytes_release"))
        }
        (ProviderKind::Crypto, "Key") => {
            Some(("bn_rt_crypto_key_retain", "bn_rt_crypto_key_release"))
        }
        (ProviderKind::Crypto, "Secret") => {
            Some(("bn_rt_crypto_secret_retain", "bn_rt_crypto_secret_release"))
        }
        (ProviderKind::Crypto, "SecretKey") => Some((
            "bn_rt_crypto_secret_key_retain",
            "bn_rt_crypto_secret_key_release",
        )),
        (ProviderKind::Crypto, "PublicKey") => Some((
            "bn_rt_crypto_public_key_retain",
            "bn_rt_crypto_public_key_release",
        )),
        (ProviderKind::Crypto, "PrivateKey") => Some((
            "bn_rt_crypto_private_key_retain",
            "bn_rt_crypto_private_key_release",
        )),
        (ProviderKind::Crypto, "Hasher") => {
            Some(("bn_rt_crypto_hasher_retain", "bn_rt_crypto_hasher_release"))
        }
        (ProviderKind::Crypto, "MacStream") => Some((
            "bn_rt_crypto_mac_stream_retain",
            "bn_rt_crypto_mac_stream_release",
        )),
        (ProviderKind::Json, "Json") => Some(("bn_rt_json_retain", "bn_rt_json_release")),
        (ProviderKind::Data, "DataFrame") => {
            Some(("bn_rt_dataframe_retain", "bn_rt_dataframe_close"))
        }
        (ProviderKind::Sqlite, "Connection") => Some(("bn_rt_sqlite_retain", "bn_rt_sqlite_close")),
        (ProviderKind::Compress, "Bytes") => Some((
            "bn_rt_compress_bytes_retain",
            "bn_rt_compress_bytes_release",
        )),
        (ProviderKind::Compress, "ZipArchive") => Some(("bn_rt_zip_retain", "bn_rt_zip_release")),
        (ProviderKind::Web, "Client") => {
            Some(("bn_rt_web_client_retain", "bn_rt_web_client_release"))
        }
        (ProviderKind::Web, "ClientReply") => Some((
            "bn_rt_web_client_reply_retain",
            "bn_rt_web_client_reply_release",
        )),
        (ProviderKind::Web, "Headers") => {
            Some(("bn_rt_web_headers_retain", "bn_rt_web_headers_release"))
        }
        (ProviderKind::Web, "Query") => Some(("bn_rt_web_query_retain", "bn_rt_web_query_release")),
        (ProviderKind::Web, "Fields") => {
            Some(("bn_rt_web_fields_retain", "bn_rt_web_fields_release"))
        }
        (ProviderKind::Web, "Form") => Some(("bn_rt_web_form_retain", "bn_rt_web_form_release")),
        (ProviderKind::Web, "Response") => {
            Some(("bn_rt_web_response_retain", "bn_rt_web_response_release"))
        }
        (ProviderKind::Web, "Request") => {
            Some(("bn_rt_web_request_retain", "bn_rt_web_request_release"))
        }
        (ProviderKind::Web, "Server") => {
            Some(("bn_rt_web_server_retain", "bn_rt_web_server_release"))
        }
        (ProviderKind::Web, "ServerOptions") => Some((
            "bn_rt_web_server_options_retain",
            "bn_rt_web_server_options_release",
        )),
        (ProviderKind::Web, "CookieJar") => Some((
            "bn_rt_web_cookie_jar_retain",
            "bn_rt_web_cookie_jar_release",
        )),
        (ProviderKind::Web, "SessionStore") => Some((
            "bn_rt_web_session_store_retain",
            "bn_rt_web_session_store_release",
        )),
        (ProviderKind::Web, "Scraper") => {
            Some(("bn_rt_web_scraper_retain", "bn_rt_web_scraper_release"))
        }
        (ProviderKind::Web, "ACL") => Some(("bn_rt_web_acl_retain", "bn_rt_web_acl_release")),
        (ProviderKind::Web, "EgressPolicy") => Some((
            "bn_rt_web_egress_policy_retain",
            "bn_rt_web_egress_policy_release",
        )),
        (ProviderKind::Web, "TLSConfig") => Some((
            "bn_rt_web_tls_config_retain",
            "bn_rt_web_tls_config_release",
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
            .find_map(|member| handle_symbols(module, member)),
        other => {
            let (provider, name) = provider_and_name(module, other)?;
            symbols_of(provider, name)
        }
    }
}

fn is_keypair(module: &Module, ty: &Type) -> bool {
    matches!(
        provider_and_name(module, ty),
        Some((ProviderKind::Crypto, "KeyPair"))
    )
}

/// The `KeyPair` member of a `KeyPair OR Error` alternative.
fn keypair_member(module: &Module, ty: &Type) -> Option<Type> {
    let Type::Alternative(members) = ty else {
        return None;
    };
    members
        .iter()
        .find(|member| is_keypair(module, member))
        .cloned()
}

fn emit_handle(
    text: &mut crate::ir::LlvmModule,
    ty: &Type,
    operand: &LlvmOperand,
    symbol: &str,
    state: &mut crate::EmissionState,
) {
    let tag = fresh(state, "libown");
    let handle = if crate::layout::llvm_type(ty) == Some(crate::layout::handle_result_ty()) {
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
