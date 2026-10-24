// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Native `BNFfi` lowering onto the `bn_rt_ffi_*` C ABI.

#![allow(clippy::wildcard_imports)]
use super::host_results::{emit_status_result, emit_void_call_result};
use super::*;
use crate::ir::{CastOp, ICmpCond, InstSink as _, LlvmInst as I, LlvmOperand as O, LlvmType as T};

pub(crate) const TYPE_INT8: u32 = 1;
pub(crate) const TYPE_UINT8: u32 = 2;
pub(crate) const TYPE_INT16: u32 = 3;
pub(crate) const TYPE_UINT16: u32 = 4;
pub(crate) const TYPE_INT32: u32 = 5;
pub(crate) const TYPE_UINT32: u32 = 6;
pub(crate) const TYPE_INT64: u32 = 7;
pub(crate) const TYPE_UINT64: u32 = 8;
pub(crate) const TYPE_INT: u32 = 9;
pub(crate) const TYPE_UINT: u32 = 10;
pub(crate) const TYPE_LONG: u32 = 11;
pub(crate) const TYPE_ULONG: u32 = 12;
pub(crate) const TYPE_SIZE_T: u32 = 13;
pub(crate) const TYPE_FLOAT: u32 = 14;
pub(crate) const TYPE_DOUBLE: u32 = 15;
pub(crate) const TYPE_BOOL: u32 = 16;
pub(crate) const TYPE_STRING: u32 = 17;
pub(crate) const TYPE_VOID: u32 = 18;
pub(crate) const TYPE_HANDLE: u32 = 19;
pub(crate) const TYPE_HANDLE_RESULT: u32 = 20;
pub(crate) const TYPE_BYTE_ARRAY: u32 = 21;
pub(crate) const TYPE_INT8_ARRAY: u32 = 22;
pub(crate) const TYPE_INT16_ARRAY: u32 = 23;
pub(crate) const TYPE_INT32_ARRAY: u32 = 24;
pub(crate) const TYPE_INT64_ARRAY: u32 = 25;
pub(crate) const TYPE_UINT16_ARRAY: u32 = 26;
pub(crate) const TYPE_UINT32_ARRAY: u32 = 27;
pub(crate) const TYPE_UINT64_ARRAY: u32 = 28;
pub(crate) const TYPE_FLOAT_ARRAY: u32 = 29;
pub(crate) const TYPE_DOUBLE_ARRAY: u32 = 30;
pub(crate) const TYPE_ARGS: u32 = 32;

/// True when `ty` is a `BNFfi` class or `C.Handle OR NULL`.
pub(crate) fn is_bnffi_type(module: &Module, ty: &Type) -> bool {
    match ty {
        Type::ImportedNamed {
            module: module_id, ..
        } => module
            .bnffi_providers
            .contains(&bn_ir::ModuleId::from(*module_id)),
        Type::Alternative(members) if llvm_type(ty) == Some(T::Ptr) => {
            members.iter().any(|member| {
                matches!(
                    member,
                    Type::ImportedNamed {
                        module: module_id,
                        ..
                    } if module.bnffi_providers.contains(&bn_ir::ModuleId::from(*module_id))
                )
            })
        }
        _ => false,
    }
}

/// Type code for a `BNFfi` class name.
pub(crate) fn bnffi_type_code(type_name: &str) -> Option<u32> {
    let class = type_name.rsplit('.').next()?;
    Some(match class {
        "Int8" => TYPE_INT8,
        "UInt8" => TYPE_UINT8,
        "Int16" => TYPE_INT16,
        "UInt16" => TYPE_UINT16,
        "Int32" => TYPE_INT32,
        "UInt32" => TYPE_UINT32,
        "Int64" => TYPE_INT64,
        "UInt64" => TYPE_UINT64,
        "Int" => TYPE_INT,
        "UInt" => TYPE_UINT,
        "Long" => TYPE_LONG,
        "ULong" => TYPE_ULONG,
        "SizeT" => TYPE_SIZE_T,
        "Float" => TYPE_FLOAT,
        "Double" => TYPE_DOUBLE,
        "Bool" => TYPE_BOOL,
        "String" => TYPE_STRING,
        "Void" => TYPE_VOID,
        "Handle" => TYPE_HANDLE,
        "HandleResult" => TYPE_HANDLE_RESULT,
        "ByteArray" => TYPE_BYTE_ARRAY,
        "Int8Array" => TYPE_INT8_ARRAY,
        "Int16Array" => TYPE_INT16_ARRAY,
        "Int32Array" => TYPE_INT32_ARRAY,
        "Int64Array" => TYPE_INT64_ARRAY,
        "UInt16Array" => TYPE_UINT16_ARRAY,
        "UInt32Array" => TYPE_UINT32_ARRAY,
        "UInt64Array" => TYPE_UINT64_ARRAY,
        "FloatArray" => TYPE_FLOAT_ARRAY,
        "DoubleArray" => TYPE_DOUBLE_ARRAY,
        "Args" => TYPE_ARGS,
        _ => return None,
    })
}

/// The `BNFfi` member `name` selects, if this module imports `BNFfi`.
pub(crate) fn bnffi_method<'a>(module: &Module, name: &'a str) -> Option<&'a str> {
    if module.bnffi_providers.is_empty() {
        return None;
    }
    let rest = name.strip_prefix('#')?;
    let (mod_id_str, member) = rest.split_once('.')?;
    let mod_id: u32 = mod_id_str.parse().ok()?;
    if module.bnffi_providers.contains(&bn_ir::ModuleId(mod_id)) {
        Some(member)
    } else {
        None
    }
}

fn emit_handle_index(
    text: &mut crate::ir::LlvmModule,
    analysis: &LoweringAnalysis<'_>,
    slot: &str,
    operand: ValueId,
) {
    let own = O::reg(format!("v{}", operand.0));
    let inst = if matches!(analysis.values.get(&operand), Some(Type::Alternative(_))) {
        I::extract(T::struct_of([T::I1, T::Ptr, T::I64]), own, 2)
    } else {
        I::cast(CastOp::PtrToInt, T::Ptr, own, T::I64)
    };
    text.assign(slot, inst);
}

pub(super) fn lower_bnffi_call(
    text: &mut crate::ir::LlvmModule,
    analysis: &LoweringAnalysis<'_>,
    destination: ValueId,
    member: &str,
    arguments: &[ValueId],
) {
    let dest = destination.0;
    let v = |index: usize| O::reg(format!("v{}", arguments[index].0));
    let r = |name: &str| O::reg(format!("{name}{dest}"));
    let handle = |text: &mut crate::ir::LlvmModule, index: usize, prefix: &str| {
        emit_handle_index(
            text,
            analysis,
            &format!("{prefix}h{dest}"),
            arguments[index],
        );
        (T::I64, r(&format!("{prefix}h")))
    };

    if member == "Call" {
        let (ty_args, op_args) = handle(text, 2, "ffia");
        let (ty_res, op_res) = handle(text, 3, "ffir");
        let args = vec![
            (T::Ptr, v(0)),
            (T::Ptr, v(1)),
            (ty_args, op_args),
            (ty_res, op_res),
        ];
        let call = I::call(T::I32, "bn_rt_ffi_call", args);
        emit_void_call_result(text, destination, call);
        return;
    }

    if let Some((class, action)) = member.split_once('.') {
        match action {
            "CONSTRUCTOR" => {
                let (ty_h, op_h) = handle(text, 0, "ffio");
                lower_constructor(text, analysis, dest, class, (ty_h, op_h), arguments);
            }
            "Value" => {
                let (ty_h, op_h) = handle(text, 0, "ffiv");
                lower_value(text, dest, class, (ty_h, op_h));
            }
            "ReadString" => {
                let (ty_h, op_h) = handle(text, 0, "ffis");
                text.assign(format!("ffiout{dest}"), I::alloca(T::Ptr));
                let args = vec![(ty_h, op_h), (T::Ptr, r("ffiout"))];
                let rc = I::call(T::I32, "bn_rt_ffi_handle_read_string", args);
                text.assign(format!("ffirc{dest}"), rc);
                text.assign(format!("ffistr{dest}"), I::load(T::Ptr, r("ffiout")));
                emit_status_result(
                    text,
                    destination,
                    &O::reg(format!("ffirc{dest}")),
                    None,
                    &O::reg(format!("ffistr{dest}")),
                    &O::int(0),
                );
            }
            "ReadFrom" | "WriteTo" => {
                let (ty_arr, op_arr) = handle(text, 0, "ffiarr");
                let (ty_ptr, op_ptr) = handle(text, 1, "ffiptr");
                let sym = if action == "ReadFrom" {
                    "bn_rt_ffi_array_read_from"
                } else {
                    "bn_rt_ffi_array_write_to"
                };
                let call = I::call(T::I32, sym, vec![(ty_arr, op_arr), (ty_ptr, op_ptr)]);
                emit_void_call_result(text, destination, call);
            }
            "Add" => {
                let (ty_a, op_a) = handle(text, 0, "ffia");
                let (ty_v, op_v) = handle(text, 1, "ffiv");
                text.emit(I::call(
                    T::Void,
                    "bn_rt_ffi_args_add",
                    vec![(ty_a, op_a), (ty_v, op_v)],
                ));
            }
            "Count" => {
                let (ty_a, op_a) = handle(text, 0, "ffia");
                let call = I::call(T::I32, "bn_rt_ffi_args_count", vec![(ty_a, op_a)]);
                text.assign(format!("v{dest}"), call);
            }
            _ => {}
        }
    }
}

fn lower_constructor(
    text: &mut crate::ir::LlvmModule,
    analysis: &LoweringAnalysis<'_>,
    dest: u32,
    class: &str,
    handle: (T, O),
    arguments: &[ValueId],
) {
    if arguments.len() < 2 {
        return;
    }
    let arg_val = |text: &mut crate::ir::LlvmModule, target_ty: &Type| {
        let arg_id = arguments[1];
        let arg_ty = analysis
            .values
            .get(&arg_id)
            .expect("validated argument type");
        coerce_to_type(text, arg_id, arg_ty, target_ty)
    };
    let r = |name: &str| O::reg(format!("{name}{dest}"));
    match class {
        "Int8" => {
            let op = arg_val(text, &Type::Integer(IntegerType::Int8));
            text.emit(I::call(
                T::Void,
                "bn_rt_ffi_set_int8",
                vec![handle, (T::I8, op)],
            ));
        }
        "UInt8" => {
            let op = arg_val(text, &Type::Integer(IntegerType::Byte));
            text.emit(I::call(
                T::Void,
                "bn_rt_ffi_set_uint8",
                vec![handle, (T::I8, op)],
            ));
        }
        "Int16" => {
            let op = arg_val(text, &Type::Integer(IntegerType::Int16));
            text.emit(I::call(
                T::Void,
                "bn_rt_ffi_set_int16",
                vec![handle, (T::I16, op)],
            ));
        }
        "UInt16" => {
            let op = arg_val(text, &Type::Integer(IntegerType::UInt16));
            text.emit(I::call(
                T::Void,
                "bn_rt_ffi_set_uint16",
                vec![handle, (T::I16, op)],
            ));
        }
        "Int32" | "Int" => {
            let op = arg_val(text, &Type::Integer(IntegerType::Int32));
            text.emit(I::call(
                T::Void,
                "bn_rt_ffi_set_int32",
                vec![handle, (T::I32, op)],
            ));
        }
        "UInt32" | "UInt" => {
            let op = arg_val(text, &Type::Integer(IntegerType::UInt32));
            text.emit(I::call(
                T::Void,
                "bn_rt_ffi_set_uint32",
                vec![handle, (T::I32, op)],
            ));
        }
        "Int64" | "Long" => {
            let op = arg_val(text, &Type::Integer(IntegerType::Int64));
            text.emit(I::call(
                T::Void,
                "bn_rt_ffi_set_int64",
                vec![handle, (T::I64, op)],
            ));
        }
        "UInt64" | "ULong" | "SizeT" => {
            let op = arg_val(text, &Type::Integer(IntegerType::UInt64));
            text.emit(I::call(
                T::Void,
                "bn_rt_ffi_set_uint64",
                vec![handle, (T::I64, op)],
            ));
        }
        "Float" => {
            let op = arg_val(text, &Type::Float(FloatType::Float32));
            text.emit(I::call(
                T::Void,
                "bn_rt_ffi_set_float",
                vec![handle, (T::Float, op)],
            ));
        }
        "Double" => {
            let op = arg_val(text, &Type::Float(FloatType::Float64));
            text.emit(I::call(
                T::Void,
                "bn_rt_ffi_set_double",
                vec![handle, (T::Double, op)],
            ));
        }
        "Bool" => {
            let op = arg_val(text, &Type::Boolean);
            text.assign(
                format!("ffibool{dest}"),
                I::cast(CastOp::ZExt, T::I1, op, T::I32),
            );
            text.emit(I::call(
                T::Void,
                "bn_rt_ffi_set_bool",
                vec![handle, (T::I32, r("ffibool"))],
            ));
        }
        "String" => {
            let op = arg_val(text, &Type::String);
            text.emit(I::call(
                T::Void,
                "bn_rt_ffi_set_string",
                vec![handle, (T::Ptr, op)],
            ));
        }
        "ByteArray" | "Int8Array" | "Int16Array" | "Int32Array" | "Int64Array" | "UInt16Array"
        | "UInt32Array" | "UInt64Array" | "FloatArray" | "DoubleArray" => {
            let elem_size = match class {
                "ByteArray" | "Int8Array" => 1,
                "Int16Array" | "UInt16Array" => 2,
                "Int32Array" | "UInt32Array" | "FloatArray" => 4,
                "Int64Array" | "UInt64Array" | "DoubleArray" => 8,
                _ => 1,
            };
            let v1 = O::reg(format!("v{}", arguments[1].0));
            text.assign(
                format!("ffiarrptr{dest}"),
                I::extract(crate::layout::vector_ty(), v1.clone(), 0),
            );
            text.assign(
                format!("ffiarrcap{dest}"),
                I::extract(crate::layout::vector_ty(), v1, 1),
            );
            text.assign(
                format!("ffiarrcap64{dest}"),
                I::cast(CastOp::ZExt, T::I32, r("ffiarrcap"), T::I64),
            );
            text.emit(I::call(
                T::Void,
                "bn_rt_ffi_set_array",
                vec![
                    handle,
                    (T::Ptr, r("ffiarrptr")),
                    (T::I64, O::int(elem_size)),
                    (T::I64, r("ffiarrcap64")),
                ],
            ));
        }
        _ => {}
    }
}

fn lower_value(text: &mut crate::ir::LlvmModule, dest: u32, class: &str, handle: (T, O)) {
    match class {
        "Int8" => {
            text.assign(
                format!("v{dest}"),
                I::call(T::I8, "bn_rt_ffi_get_int8", vec![handle]),
            );
        }
        "UInt8" => {
            text.assign(
                format!("v{dest}"),
                I::call(T::I8, "bn_rt_ffi_get_uint8", vec![handle]),
            );
        }
        "Int16" => {
            text.assign(
                format!("v{dest}"),
                I::call(T::I16, "bn_rt_ffi_get_int16", vec![handle]),
            );
        }
        "UInt16" => {
            text.assign(
                format!("v{dest}"),
                I::call(T::I16, "bn_rt_ffi_get_uint16", vec![handle]),
            );
        }
        "Int32" | "Int" => {
            text.assign(
                format!("v{dest}"),
                I::call(T::I32, "bn_rt_ffi_get_int32", vec![handle]),
            );
        }
        "UInt32" | "UInt" => {
            text.assign(
                format!("v{dest}"),
                I::call(T::I32, "bn_rt_ffi_get_uint32", vec![handle]),
            );
        }
        "Int64" | "Long" => {
            text.assign(
                format!("v{dest}"),
                I::call(T::I64, "bn_rt_ffi_get_int64", vec![handle]),
            );
        }
        "UInt64" | "ULong" | "SizeT" => {
            text.assign(
                format!("v{dest}"),
                I::call(T::I64, "bn_rt_ffi_get_uint64", vec![handle]),
            );
        }
        "Float" => {
            text.assign(
                format!("v{dest}"),
                I::call(T::Float, "bn_rt_ffi_get_float", vec![handle]),
            );
        }
        "Double" => {
            text.assign(
                format!("v{dest}"),
                I::call(T::Double, "bn_rt_ffi_get_double", vec![handle]),
            );
        }
        "Bool" => {
            text.assign(
                format!("ffibool{dest}"),
                I::call(T::I32, "bn_rt_ffi_get_bool", vec![handle]),
            );
            text.assign(
                format!("v{dest}"),
                I::icmp(
                    ICmpCond::Ne,
                    T::I32,
                    O::reg(format!("ffibool{dest}")),
                    O::int(0),
                ),
            );
        }
        "HandleResult" => {
            text.assign(
                format!("v{dest}"),
                I::call(T::Ptr, "bn_rt_ffi_handle_result_value", vec![handle]),
            );
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ffi_type_codes_match_runtime_abi_contract() {
        let table = [
            (TYPE_INT8, 1),
            (TYPE_UINT8, 2),
            (TYPE_INT16, 3),
            (TYPE_UINT16, 4),
            (TYPE_INT32, 5),
            (TYPE_UINT32, 6),
            (TYPE_INT64, 7),
            (TYPE_UINT64, 8),
            (TYPE_INT, 9),
            (TYPE_UINT, 10),
            (TYPE_LONG, 11),
            (TYPE_ULONG, 12),
            (TYPE_SIZE_T, 13),
            (TYPE_FLOAT, 14),
            (TYPE_DOUBLE, 15),
            (TYPE_BOOL, 16),
            (TYPE_STRING, 17),
            (TYPE_VOID, 18),
            (TYPE_HANDLE, 19),
            (TYPE_HANDLE_RESULT, 20),
            (TYPE_BYTE_ARRAY, 21),
            (TYPE_INT8_ARRAY, 22),
            (TYPE_INT16_ARRAY, 23),
            (TYPE_INT32_ARRAY, 24),
            (TYPE_INT64_ARRAY, 25),
            (TYPE_UINT16_ARRAY, 26),
            (TYPE_UINT32_ARRAY, 27),
            (TYPE_UINT64_ARRAY, 28),
            (TYPE_FLOAT_ARRAY, 29),
            (TYPE_DOUBLE_ARRAY, 30),
            (TYPE_ARGS, 32),
        ];
        for (actual, expected) in table {
            assert_eq!(actual, expected);
        }
    }
}
