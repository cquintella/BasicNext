// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

// BNLog lowering (bnlog.md, "API (0.6.5)"): which values are BNLog loggers,
// the support check of their members, and the calls into `bn_rt_log_*`. A
// `Log.Event` or `Log.Apache` argument is passed as its fields.

#![allow(clippy::wildcard_imports)]
use super::*;
use crate::ir::{CastOp, InstSink as _, LlvmInst as I, LlvmOperand as O, LlvmType as T};
use crate::layout::{class_layout_fields, field_byte_offset, handle_result_ty};

/// The `BNLog` resource `ty` names (`Logger`), when it is one of an
/// imported `BNLog`.
pub(crate) fn bnlog_resource_kind(module: &Module, ty: &Type) -> Option<&'static str> {
    match ty {
        Type::ImportedNamed {
            module: module_id,
            name,
        } if module
            .bnlog_providers
            .contains(&bn_ir::ModuleId::from(*module_id))
            && name == BNLOG_LOGGER =>
        {
            Some(BNLOG_LOGGER)
        }
        _ => None,
    }
}

/// The class of a `BNLog` logger.
const BNLOG_LOGGER: &str = "Logger";

/// The `BNLog` resource an argument of type `ty` holds: the resource itself
/// or, narrowed by `IS`, the `Logger OR Error` that `Logger.New` made.
fn bnlog_argument_kind(module: &Module, ty: &Type) -> Option<&'static str> {
    match ty {
        Type::Alternative(members) if members.len() == 2 && members.iter().any(is_error_type) => {
            members
                .iter()
                .find_map(|member| bnlog_resource_kind(module, member))
        }
        other => bnlog_resource_kind(module, other),
    }
}

/// The `bn_rt` function that releases a logger.
pub(crate) const fn bnlog_release_symbol() -> &'static str {
    "bn_rt_log_logger_delete"
}

/// The `bn_rt` function that retains a logger (a copy holds it).
pub(crate) const fn bnlog_retain_symbol() -> &'static str {
    "bn_rt_log_logger_retain"
}

/// The `BNLog` STRUCT `ty` names (`Event`, `Apache`), when it is one.
fn bnlog_record(module: &Module, ty: &Type) -> Option<&'static str> {
    match ty {
        Type::ImportedNamed {
            module: module_id,
            name,
        } if module
            .bnlog_providers
            .contains(&bn_ir::ModuleId::from(*module_id)) =>
        {
            [BNLOG_EVENT, BNLOG_APACHE]
                .into_iter()
                .find(|record| name == record)
        }
        _ => None,
    }
}

const BNLOG_EVENT: &str = "Event";
const BNLOG_APACHE: &str = "Apache";

/// Each level member with its lowering method, in level order: the position
/// is the level (`Logger.Info` → 2), as `Log.Level` declares it.
const LEVEL_MEMBERS: [(&str, &str); 7] = [
    ("Logger.Error", "logger_error"),
    ("Logger.Warn", "logger_warn"),
    ("Logger.Info", "logger_info"),
    ("Logger.Http", "logger_http"),
    ("Logger.Verbose", "logger_verbose"),
    ("Logger.Debug", "logger_debug"),
    ("Logger.Silly", "logger_silly"),
];

/// The lowering method of the level member `member` (`Logger.Info`).
pub(crate) fn level_method(member: &str) -> Option<&'static str> {
    LEVEL_MEMBERS
        .iter()
        .find(|(name, _)| *name == member)
        .map(|(_, method)| *method)
}

/// The level a level method writes at (`logger_info` → 2).
fn method_level(method: &str) -> Option<i64> {
    LEVEL_MEMBERS
        .iter()
        .position(|(_, level)| *level == method)
        .and_then(|level| i64::try_from(level).ok())
}

/// Whether the support check admits a call of the `BNLog` member `method`.
pub(crate) fn bnlog_arguments_supported(
    module: &Module,
    method: Option<&str>,
    arguments: &[ValueId],
    values: &HashMap<ValueId, Type>,
) -> bool {
    let argument = |index: usize| arguments.get(index).and_then(|value| values.get(value));
    let logger =
        || argument(0).and_then(|ty| bnlog_argument_kind(module, ty)) == Some(BNLOG_LOGGER);
    let text = |index: usize| argument(index) == Some(&Type::String);
    let integer = |index: usize| {
        argument(index)
            .and_then(llvm_type)
            .is_some_and(|ty| ty.is_integer())
    };
    let record = |index: usize, wanted: &str| {
        argument(index).and_then(|ty| bnlog_record(module, ty)) == Some(wanted)
    };
    match method {
        Some("logger_new") => arguments.len() == 3 && text(0) && text(1) && text(2),
        Some(method) if method_level(method).is_some() => {
            arguments.len() == 3 && logger() && text(1) && record(2, BNLOG_EVENT)
        }
        Some("logger_access" | "logger_set_access_defaults") => {
            arguments.len() == 2 && logger() && record(1, BNLOG_APACHE)
        }
        Some("logger_set_defaults") => arguments.len() == 2 && logger() && record(1, BNLOG_EVENT),
        Some("logger_set_level" | "logger_flush" | "logger_close") => {
            arguments.len() == 2 && logger() && integer(1)
        }
        _ => false,
    }
}

pub(crate) fn lower_bnlog_call(
    text: &mut crate::ir::LlvmModule,
    module: &Module,
    destination: ValueId,
    method: &str,
    arguments: &[ValueId],
    analysis: &LoweringAnalysis<'_>,
) {
    let dest = destination.0;
    let r = |name: &str| O::reg(format!("log{name}{dest}"));
    let v = |index: usize| (T::Ptr, O::reg(format!("v{}", arguments[index].0)));
    let integer = |text: &mut crate::ir::LlvmModule, index: usize| {
        let value = arguments[index];
        let Some(ty) = analysis.values.get(&value) else {
            unreachable!()
        };
        (T::I64, extend_to_i64(text, value, ty))
    };
    if method == "logger_new" {
        text.assign(format!("logout{dest}"), I::alloca(T::I64));
        let args = vec![v(0), v(1), v(2), (T::Ptr, r("out"))];
        let call = I::call(T::I32, "bn_rt_log_logger_new", args);
        text.assign(format!("logrc{dest}"), call);
        text.assign(format!("loghandle{dest}"), I::load(T::I64, r("out")));
        let pointer = I::cast(CastOp::IntToPtr, T::I64, r("handle"), T::Ptr);
        text.assign(format!("logvalue{dest}"), pointer);
        let (rc, value) = (r("rc"), r("value"));
        emit_status_result(text, destination, &rc, None, &value, &O::int(0));
        return;
    }
    let receiver = logger_handle(text, dest, arguments[0], analysis);
    let (symbol, mut args) = match method {
        "logger_access" => ("bn_rt_log_logger_access", vec![receiver]),
        "logger_set_defaults" => ("bn_rt_log_logger_set_defaults", vec![receiver]),
        "logger_set_access_defaults" => ("bn_rt_log_logger_set_access_defaults", vec![receiver]),
        "logger_set_level" => (
            "bn_rt_log_logger_set_level",
            vec![receiver, integer(text, 1)],
        ),
        "logger_flush" => ("bn_rt_log_logger_flush", vec![receiver, integer(text, 1)]),
        "logger_close" => ("bn_rt_log_logger_close", vec![receiver, integer(text, 1)]),
        level => {
            let Some(level) = method_level(level) else {
                unreachable!()
            };
            let level = (T::I64, O::int(level));
            ("bn_rt_log_logger_write", vec![receiver, level, v(1)])
        }
    };
    if matches!(
        method,
        "logger_access" | "logger_set_defaults" | "logger_set_access_defaults"
    ) || method_level(method).is_some()
    {
        let record = *arguments.last().unwrap_or_else(|| unreachable!());
        args.extend(struct_fields(text, module, dest, record, analysis));
    }
    text.assign(format!("logrc{dest}"), I::call(T::I32, symbol, args));
    emit_status_result(
        text,
        destination,
        &O::reg(format!("logrc{dest}")),
        None,
        &O::null(),
        &O::int(0),
    );
}

/// The `i64` handle of a logger argument; a narrowed `Logger OR Error` holds
/// the logger in field 1.
fn logger_handle(
    text: &mut crate::ir::LlvmModule,
    dest: u32,
    value: ValueId,
    analysis: &LoweringAnalysis<'_>,
) -> (T, O) {
    let mut pointer = O::reg(format!("v{}", value.0));
    if matches!(analysis.values.get(&value), Some(Type::Alternative(_))) {
        let field = format!("logreceiver{dest}");
        text.assign(&field, I::extract(handle_result_ty(), pointer, 1));
        pointer = O::reg(field);
    }
    let handle = format!("loghandlearg{dest}");
    text.assign(&handle, I::cast(CastOp::PtrToInt, T::Ptr, pointer, T::I64));
    (T::I64, O::reg(handle))
}

/// The fields of the `BNLog` STRUCT `value`, in declaration order, as the C
/// ABI takes them: a STRING as `ptr`, an INTEGER widened to `i64`.
fn struct_fields(
    text: &mut crate::ir::LlvmModule,
    module: &Module,
    dest: u32,
    value: ValueId,
    analysis: &LoweringAnalysis<'_>,
) -> Vec<(T, O)> {
    let Some(Type::ImportedNamed {
        module: module_id,
        name,
    }) = analysis.values.get(&value)
    else {
        unreachable!()
    };
    let owner = format!("#{}.{name}", module_id.0);
    let mut fields = Vec::new();
    for (index, field) in class_layout_fields(module, &owner).into_iter().enumerate() {
        let Some(offset) = field_byte_offset(module, &field.reference) else {
            unreachable!()
        };
        let at = vec![(T::I32, O::uint(u64::from(offset)))];
        let pointer = format!("logfieldptr{dest}_{index}");
        let loaded = format!("logfield{dest}_{index}");
        text.assign(&pointer, I::gep(T::I8, O::reg(format!("v{}", value.0)), at));
        if field.ty == Type::String {
            text.assign(&loaded, I::load(T::Ptr, O::reg(pointer)));
            fields.push((T::Ptr, O::reg(loaded)));
        } else {
            text.assign(&loaded, I::load(T::I32, O::reg(pointer)));
            let wide = format!("logfieldwide{dest}_{index}");
            text.assign(&wide, I::cast(CastOp::SExt, T::I32, O::reg(loaded), T::I64));
            fields.push((T::I64, O::reg(wide)));
        }
    }
    fields
}

/// Whether `ty` holds a counted `BNLog.Logger`: the logger itself, or the
/// `Logger OR Error` that `Logger.New` makes (known issue K61).
pub(crate) fn holds_counted_logger(module: &Module, ty: &Type) -> bool {
    bnlog_argument_kind(module, ty) == Some(BNLOG_LOGGER)
}

/// Calls `symbol` (retain or release) on the logger `operand` holds; the
/// error case of `Logger OR Error` passes handle 0, which holds no logger.
pub(crate) fn emit_logger_ownership(
    text: &mut crate::ir::LlvmModule,
    ty: &Type,
    operand: &crate::ir::LlvmOperand,
    tag: &str,
    symbol: &str,
) {
    let mut pointer = operand.clone();
    let mut failed = None;
    if matches!(ty, Type::Alternative(_)) {
        let union = handle_result_ty();
        text.assign(
            format!("{tag}err"),
            I::extract(union.clone(), operand.clone(), 0),
        );
        text.assign(format!("{tag}ptr"), I::extract(union, operand.clone(), 1));
        pointer = O::reg(format!("{tag}ptr"));
        failed = Some(O::reg(format!("{tag}err")));
    }
    text.assign(
        format!("{tag}raw"),
        I::cast(CastOp::PtrToInt, T::Ptr, pointer.clone(), T::I64),
    );
    let handle = match failed {
        Some(failed) => {
            let select = I::select(failed, T::I64, O::int(0), O::reg(format!("{tag}raw")));
            text.assign(format!("{tag}handle"), select);
            O::reg(format!("{tag}handle"))
        }
        None => O::reg(format!("{tag}raw")),
    };
    text.assign(
        format!("{tag}rc"),
        I::call(T::I32, symbol, vec![(T::I64, handle)]),
    );
}
