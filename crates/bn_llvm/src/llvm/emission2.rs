// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

// Block terminators and returns (including `T OR Error` return values),
// cleanup of owned memory at function exit, and `PRINT` of values.
#![allow(
    clippy::wildcard_imports,
    clippy::match_same_arms,
    clippy::too_many_lines
)]
use super::*;
use crate::ir::{CastOp, InstSink, LlvmInst, LlvmOperand, LlvmType};
use arc_runtime::{STOP_CODE_GLOBAL, STOPPING_GLOBAL};

#[allow(clippy::too_many_arguments)]
pub(crate) fn lower_terminator(
    text: &mut crate::ir::LlvmModule,
    module: &Module,
    function: &Function,
    terminator: &Terminator,
    analysis: &LoweringAnalysis<'_>,
    symbols: &HashMap<SymbolId, usize>,
    _block_state: &mut BlockState,
    state: &mut EmissionState,
) {
    match terminator {
        Terminator::Jump { target } => {
            text.emit(LlvmInst::Br {
                dest: format!("b{}", target.0),
            });
        }
        Terminator::Branch {
            condition,
            then_block,
            else_block,
        } => {
            let operand = i1_operand(text, analysis, state, *condition);
            text.emit(LlvmInst::CondBr {
                cond: operand,
                true_dest: format!("b{}", then_block.0),
                false_dest: format!("b{}", else_block.0),
            });
        }
        Terminator::Return { value: None } if state.is_start => {
            cleanup_owned_memory(text, module, analysis, symbols, state);
            text.emit(LlvmInst::Ret {
                val: Some((LlvmType::I32, LlvmOperand::int(0))),
            });
        }
        Terminator::Return { value: None } if state.return_llvm == LlvmType::Void => {
            cleanup_owned_memory(text, module, analysis, symbols, state);
            text.emit(LlvmInst::Ret { val: None });
        }
        Terminator::Return { value: None }
            if state.return_llvm == crate::layout::handle_result_ty() =>
        {
            cleanup_owned_memory(text, module, analysis, symbols, state);
            let union = union_ty();
            let insert = |text: &mut crate::ir::LlvmModule,
                          name: &str,
                          from: LlvmOperand,
                          ty,
                          value,
                          index| {
                text.assign(
                    name,
                    LlvmInst::insert(union.clone(), from, ty, value, index),
                );
            };
            insert(
                text,
                "implicitret0",
                LlvmOperand::undef(),
                LlvmType::I1,
                LlvmOperand::bool(false),
                0,
            );
            insert(
                text,
                "implicitret1",
                LlvmOperand::reg("implicitret0"),
                LlvmType::Ptr,
                LlvmOperand::null(),
                1,
            );
            insert(
                text,
                "implicitret2",
                LlvmOperand::reg("implicitret1"),
                LlvmType::I64,
                LlvmOperand::int(0),
                2,
            );
            text.emit(LlvmInst::Ret {
                val: Some((union_ty(), LlvmOperand::reg("implicitret2"))),
            });
        }
        Terminator::Return { value: None } => text.emit(LlvmInst::Unreachable),
        Terminator::Stop { code: value } if !state.is_start => {
            let operand = coerce_return_operand(
                text,
                *value,
                analysis.values.get(value).expect("validated stop type"),
            );
            cleanup_owned_memory(text, module, analysis, symbols, state);
            // The caller sees the STOP through `$stopping` and releases its
            // own locals in turn (0.6.md, "`STOP`"); the value returned is
            // never read.
            text.emit(LlvmInst::store(
                LlvmType::I32,
                operand,
                LlvmOperand::global(STOP_CODE_GLOBAL),
            ));
            text.emit(LlvmInst::store(
                LlvmType::I1,
                LlvmOperand::bool(true),
                LlvmOperand::global(STOPPING_GLOBAL),
            ));
            text.emit(LlvmInst::Ret {
                val: (state.return_llvm != LlvmType::Void)
                    .then(|| (state.return_llvm.clone(), LlvmOperand::zero_initializer())),
            });
        }
        Terminator::Return { value: Some(value) } if !state.is_start => {
            if state.return_llvm == LlvmType::Void {
                cleanup_owned_memory(text, module, analysis, symbols, state);
                text.emit(LlvmInst::Ret { val: None });
            } else {
                let value_ty = analysis.values.get(value).expect("validated return type");
                let union_return = state.return_llvm == crate::layout::handle_result_ty();
                let operand = if state.return_llvm == crate::general_alternative::layout()
                    || union_return && matches!(value_ty, Type::NotAvailable | Type::EndOfFile)
                    // An `Error` returned as `Endpoint OR Error` / `Addresses OR
                    // Error` (K36).
                    || (crate::layout::is_one_of(&state.return_llvm, &[crate::layout::status_endpoint_ty(), crate::layout::status_pointer_ty()])
                        && is_error_type(value_ty))
                {
                    coerce_to_type(text, *value, value_ty, &function.return_type)
                } else if union_return
                    && let Some(operand) = union_success_operand(text, *value, value_ty)
                {
                    operand
                } else if state.return_llvm.is_integer()
                    && llvm_type(value_ty)
                        .is_some_and(|ty| ty.is_integer() && ty != state.return_llvm)
                {
                    coerce_to_type(
                        text,
                        *value,
                        value_ty,
                        match state.return_llvm {
                            LlvmType::I8 => &Type::Integer(IntegerType::Int8),
                            LlvmType::I16 => &Type::Integer(IntegerType::Int16),
                            LlvmType::I32 => &Type::Integer(IntegerType::Int32),
                            _ => &Type::Integer(IntegerType::Int64),
                        },
                    )
                } else {
                    LlvmOperand::reg(format!("v{}", value.0))
                };
                // A returned STRUCT leaves the dying frame in a heap block the caller
                // owns (`owned_struct_results`): the frame's storage ends here.
                let operand = match struct_owner(module, &function.return_type) {
                    Some(owner) if function.kind != FunctionKind::Default => {
                        let bytes = class_instance_bytes(module, &owner);
                        let copy = format!("structret{}", value.0);
                        text.assign(
                            &copy,
                            LlvmInst::call(
                                LlvmType::Ptr,
                                "malloc",
                                vec![(LlvmType::I64, LlvmOperand::uint(bytes))],
                            ),
                        );
                        let copy_witness = arc_runtime::struct_copy_witness_name(&owner);
                        text.emit(LlvmInst::call(
                            LlvmType::Void,
                            &copy_witness,
                            vec![
                                (LlvmType::Ptr, LlvmOperand::reg(&copy)),
                                (LlvmType::Ptr, operand.clone()),
                            ],
                        ));
                        LlvmOperand::reg(&copy)
                    }
                    _ => operand,
                };
                // A returned vector leaves the dying frame through the function's
                // thread-local buffer; the caller copies it right away.
                let buffer =
                    LlvmOperand::global(format!("bn_vret_{}", sanitize_symbol(&function.name)));
                let out = format!("vrout{}", text.fresh_tag());
                let operand = vectors::emit_vector_relocate(
                    text,
                    module,
                    &operand,
                    &function.return_type,
                    &buffer,
                    &out,
                )
                .map_or(operand, |()| LlvmOperand::reg(out));
                cleanup_owned_memory(text, module, analysis, symbols, state);
                let layout = state.return_llvm.clone();
                text.emit(LlvmInst::Ret {
                    val: Some((layout, operand)),
                });
            }
        }
        Terminator::Return { value: Some(value) } | Terminator::Stop { code: value } => {
            let operand = coerce_return_operand(
                text,
                *value,
                analysis.values.get(value).expect("validated return type"),
            );
            cleanup_owned_memory(text, module, analysis, symbols, state);
            text.emit(LlvmInst::Ret {
                val: Some((LlvmType::I32, operand)),
            });
        }
    }
}

fn union_ty() -> LlvmType {
    LlvmType::struct_of([LlvmType::I1, LlvmType::Ptr, LlvmType::I64])
}

/// A success value as a `T OR Error` result (`{ i1 false, ptr, i64 }`): a
/// STRING in the pointer, an integer, float bits, `BOOLEAN` or pointer bits
/// in the payload. `None` when `ty` is none of those.
fn union_success_operand(
    text: &mut crate::ir::LlvmModule,
    value: ValueId,
    ty: &Type,
) -> Option<crate::ir::LlvmOperand> {
    let v = value.0;
    let own = LlvmOperand::reg(format!("v{v}"));
    let (message, payload) = match ty {
        Type::Integer(_) | Type::IntegerLiteral(_) => (
            LlvmOperand::null(),
            coerce_to_type(text, value, ty, &Type::Integer(IntegerType::Int64)),
        ),
        Type::Float(_) | Type::FloatLiteral => {
            let wide = coerce_to_type(text, value, ty, &Type::Float(FloatType::Float64));
            let bits = LlvmInst::cast(CastOp::BitCast, LlvmType::Double, wide, LlvmType::I64);
            text.assign(format!("retunionbits{v}"), bits);
            (
                LlvmOperand::null(),
                LlvmOperand::reg(format!("retunionbits{v}")),
            )
        }
        Type::String => (own, LlvmOperand::int(0)),
        Type::Boolean => {
            text.assign(
                format!("retunionbool{v}"),
                LlvmInst::cast(CastOp::ZExt, LlvmType::I1, own, LlvmType::I64),
            );
            (
                LlvmOperand::null(),
                LlvmOperand::reg(format!("retunionbool{v}")),
            )
        }
        _ if llvm_type(ty) == Some(LlvmType::Ptr) => {
            text.assign(
                format!("retunionbits{v}"),
                LlvmInst::cast(CastOp::PtrToInt, LlvmType::Ptr, own, LlvmType::I64),
            );
            (
                LlvmOperand::null(),
                LlvmOperand::reg(format!("retunionbits{v}")),
            )
        }
        _ => return None,
    };
    let union = union_ty();
    let tag = LlvmOperand::reg(format!("retuniontag{v}"));
    let with_message = LlvmOperand::reg(format!("retunionmessage{v}"));
    text.assign(
        format!("retuniontag{v}"),
        LlvmInst::insert(
            union.clone(),
            LlvmOperand::undef(),
            LlvmType::I1,
            LlvmOperand::bool(false),
            0,
        ),
    );
    text.assign(
        format!("retunionmessage{v}"),
        LlvmInst::insert(union.clone(), tag, LlvmType::Ptr, message, 1),
    );
    text.assign(
        format!("retunion{v}"),
        LlvmInst::insert(union, with_message, LlvmType::I64, payload, 2),
    );
    Some(LlvmOperand::reg(format!("retunion{v}")))
}

/// Frees what the function owns outside ARC (the IR releases objects):
/// `INPUT` lines, `STRUCT` copies, and log resources.
pub(crate) fn cleanup_owned_memory(
    text: &mut crate::ir::LlvmModule,
    module: &Module,
    analysis: &LoweringAnalysis<'_>,
    symbols: &HashMap<SymbolId, usize>,
    state: &mut EmissionState,
) {
    let cleanup = state.input_cleanup_count;
    state.input_cleanup_count += 1;
    let reg = LlvmOperand::reg;
    let free = |text: &mut crate::ir::LlvmModule, pointer: String| {
        text.emit(LlvmInst::call(
            LlvmType::Void,
            "free",
            vec![(LlvmType::Ptr, reg(pointer))],
        ));
    };
    for (symbol, ty) in &analysis.symbols {
        if let Some(owner) = struct_owner(module, ty) {
            let slot = symbols[symbol];
            let destroy_witness = arc_runtime::struct_destroy_witness_name(&owner);
            text.emit(LlvmInst::call(
                LlvmType::Void,
                &destroy_witness,
                vec![(LlvmType::Ptr, reg(format!("structslot{slot}")))],
            ));
        }
    }
    let mut input_symbols = analysis.input_symbols.iter().collect::<Vec<_>>();
    input_symbols.sort_by_key(|symbol| symbol.0);
    for symbol in input_symbols {
        let slot = symbols[symbol];
        let (line, owned) = (
            format!("inputfree{cleanup}_{slot}"),
            format!("inputfreeowned{cleanup}_{slot}"),
        );
        text.assign(
            line.clone(),
            LlvmInst::load(LlvmType::Ptr, reg(format!("s{slot}"))),
        );
        text.assign(
            owned.clone(),
            LlvmInst::load(LlvmType::I1, reg(format!("inputowned{slot}"))),
        );
        let select = LlvmInst::select(reg(owned), LlvmType::Ptr, reg(line), LlvmOperand::null());
        text.assign(format!("inputfreenull{cleanup}_{slot}"), select);
        free(text, format!("inputfreenull{cleanup}_{slot}"));
    }
    let mut struct_results = analysis
        .owned_struct_results
        .iter()
        .copied()
        .collect::<Vec<_>>();
    struct_results.sort_by_key(|value| value.0);
    for value in struct_results {
        let pointer = format!("structfree{cleanup}_{}", value.0);
        text.assign(
            pointer.clone(),
            LlvmInst::load(LlvmType::Ptr, reg(format!("structowned{}", value.0))),
        );
        if let Some(ty) = analysis.values.get(&value)
            && let Some(owner) = struct_owner(module, ty)
        {
            let destroy_witness = arc_runtime::struct_destroy_witness_name(&owner);
            text.emit(LlvmInst::call(
                LlvmType::Void,
                &destroy_witness,
                vec![(LlvmType::Ptr, reg(pointer.clone()))],
            ));
        }
        free(text, pointer);
    }
}

/// `%dest = call i32 (ptr, ...) @printf(args)`.
fn printf(text: &mut crate::ir::LlvmModule, dest: String, args: Vec<(LlvmType, LlvmOperand)>) {
    let call = LlvmInst::call_variadic(LlvmType::I32, vec![LlvmType::Ptr], "printf", args);
    text.assign(dest, call);
}

pub(crate) fn lower_print_value(
    text: &mut crate::ir::LlvmModule,
    value: ValueId,
    ty: &Type,
    state: &mut EmissionState,
) {
    let reg = LlvmOperand::reg;
    let own = reg(format!("v{}", value.0));
    let global = LlvmOperand::global;
    if let Some(members) = general_alternative(ty) {
        general_alternative::emit_print(text, value, members, state);
        return;
    }
    if *ty == Type::Null {
        let args = vec![
            (LlvmType::Ptr, global(".bn_fmt_str")),
            (LlvmType::Ptr, global(".bn_null")),
        ];
        printf(text, format!("print{}", state.print_count), args);
        state.print_count += 1;
        return;
    }
    if let Type::Alternative(alternatives) = ty
        && (integer_or_error(alternatives)
            || float_or_error(alternatives)
            || boolean_or_error(alternatives)
            || string_or_error(alternatives)
            || void_or_error(alternatives)
            || string_na_or_error(alternatives)
            || string_eof_or_error(alternatives)
            || integer_eof_or_error(alternatives)
            || scalar_na_or_error(alternatives))
    {
        lower_print_language_error_union(
            text,
            value,
            integer_or_error(alternatives) || integer_eof_or_error(alternatives),
            void_or_error(alternatives),
            Sentinel::of(alternatives),
            if float_or_error(alternatives)
                || boolean_or_error(alternatives)
                || string_na_or_error(alternatives)
                || string_eof_or_error(alternatives)
                || scalar_na_or_error(alternatives)
            {
                alternatives.iter().find(|ty| {
                    matches!(
                        ty,
                        Type::Integer(_) | Type::Float(_) | Type::Boolean | Type::String
                    )
                })
            } else {
                None
            },
            state,
        );
        return;
    }
    if lower_print_handle_error_union(text, value, ty, state) {
        return;
    }
    let union = union_ty();
    if is_error_type(ty) && llvm_type(ty) == Some(crate::layout::handle_result_ty()) {
        // An `Error` (a narrowed union too): its pointer is the runtime
        // record, not text; print it as the unions do.
        let count = state.print_count;
        text.assign(
            format!("errprintptr{count}"),
            LlvmInst::extract(union.clone(), own.clone(), 1),
        );
        text.assign(
            format!("errprintcode{count}"),
            LlvmInst::extract(union, own, 2),
        );
        let args = vec![
            (LlvmType::I64, reg(format!("errprintcode{count}"))),
            (LlvmType::Ptr, reg(format!("errprintptr{count}"))),
        ];
        text.emit(LlvmInst::call(LlvmType::Void, "bn_rt_error_print", args));
        return;
    }
    if let Type::Vector {
        element,
        dimensions,
    } = ty
        && dimensions.len() == 1
        && matches!(element.as_ref(), Type::Integer(IntegerType::Int32))
    {
        lower_print_int32_vector(text, value, dimensions[0], state);
        return;
    }
    for (name, runtime) in [("DATE", "bn_rt_print_date"), ("TIME", "bn_rt_print_time")] {
        if matches!(ty, Type::Named(named) if named == name) {
            text.emit(LlvmInst::call(
                LlvmType::Void,
                runtime,
                vec![(LlvmType::I32, own)],
            ));
            return;
        }
    }
    if llvm_type(ty) == Some(crate::layout::handle_result_ty()) {
        let count = state.print_count;
        text.assign(format!("netprint{count}"), LlvmInst::extract(union, own, 1));
        let args = vec![
            (LlvmType::Ptr, global(".bn_fmt_str")),
            (LlvmType::Ptr, reg(format!("netprint{count}"))),
        ];
        printf(text, format!("print{count}"), args);
        state.print_count += 1;
        return;
    }
    if llvm_type(ty) == Some(crate::layout::optional_float_ty()) {
        let count = state.print_count;
        let optional = LlvmType::struct_of([LlvmType::I1, LlvmType::Double]);
        text.assign(
            format!("optisna{count}"),
            LlvmInst::extract(optional.clone(), own.clone(), 0),
        );
        text.assign(
            format!("optval{count}"),
            LlvmInst::extract(optional, own, 1),
        );
        text.emit(LlvmInst::CondBr {
            cond: reg(format!("optisna{count}")),
            true_dest: format!("optna{count}"),
            false_dest: format!("optnum{count}"),
        });
        state.control_flow.label(text, format!("optna{count}"));
        let args = vec![
            (LlvmType::Ptr, global(".bn_fmt_str")),
            (LlvmType::Ptr, global(".bn_na")),
        ];
        printf(text, format!("optnaprint{count}"), args);
        text.emit(LlvmInst::Br {
            dest: format!("optjoin{count}"),
        });
        state.control_flow.label(text, format!("optnum{count}"));
        let args = vec![(LlvmType::Double, reg(format!("optval{count}")))];
        text.emit(LlvmInst::call(LlvmType::Void, "bn_rt_print_float", args));
        text.emit(LlvmInst::Br {
            dest: format!("optjoin{count}"),
        });
        state.control_flow.label(text, format!("optjoin{count}"));
        return;
    }
    if let Type::Alternative(alternatives) = ty
        && alternatives.len() == 2
        && alternatives.contains(&Type::Null)
        && let Some(name) = alternatives.iter().find_map(|item| match item {
            Type::Named(name) => Some(name.as_str()),
            Type::ImportedNamed { name, .. } => Some(name.as_str()),
            _ => None,
        })
    {
        let short_name = name.rsplit('.').next().unwrap_or(name);
        let name_sym = print_emission::type_name_global(short_name);
        let count = state.print_count;
        let is_null = LlvmInst::icmp(
            crate::ir::ICmpCond::Eq,
            LlvmType::Ptr,
            own.clone(),
            LlvmOperand::null(),
        );
        text.assign(format!("printnullcond{count}"), is_null);
        let target_str = LlvmInst::select(
            LlvmOperand::reg(format!("printnullcond{count}")),
            LlvmType::Ptr,
            global(".bn_null"),
            global(&name_sym),
        );
        text.assign(format!("printstr{count}"), target_str);
        let args = vec![
            (LlvmType::Ptr, global(".bn_fmt_str")),
            (LlvmType::Ptr, LlvmOperand::reg(format!("printstr{count}"))),
        ];
        printf(text, format!("print{count}"), args);
        state.print_count += 1;
        return;
    }
    if let Type::Named(name) = ty
        && !matches!(name.as_str(), "DATE" | "TIME" | "Error" | "VOID")
        && !name.starts_with("HOST.Net.")
        && !name.starts_with("FS.")
        && llvm_type(ty) == Some(LlvmType::Ptr)
    {
        let short_name = name.rsplit('.').next().unwrap_or(name.as_str());
        let name_sym = print_emission::type_name_global(short_name);
        let count = state.print_count;
        let args = vec![
            (LlvmType::Ptr, global(".bn_fmt_str")),
            (LlvmType::Ptr, global(&name_sym)),
        ];
        printf(text, format!("print{count}"), args);
        state.print_count += 1;
        return;
    }
    if let Type::ImportedNamed { name, .. } = ty
        && !matches!(
            name.as_str(),
            "Address"
                | "PingReply"
                | "Error"
                | "Addresses"
                | "Endpoint"
                | "TCPStream"
                | "TCPListener"
                | "UDPSocket"
                | "UDPPacket"
        )
        && llvm_type(ty) == Some(LlvmType::Ptr)
    {
        let short_name = name.rsplit('.').next().unwrap_or(name.as_str());
        let name_sym = print_emission::type_name_global(short_name);
        let count = state.print_count;
        let args = vec![
            (LlvmType::Ptr, global(".bn_fmt_str")),
            (LlvmType::Ptr, global(&name_sym)),
        ];
        printf(text, format!("print{count}"), args);
        state.print_count += 1;
        return;
    }
    if matches!(ty, Type::Pointer { .. }) {
        let name_sym = print_emission::type_name_global("POINTER");
        let count = state.print_count;
        let args = vec![
            (LlvmType::Ptr, global(".bn_fmt_str")),
            (LlvmType::Ptr, global(&name_sym)),
        ];
        printf(text, format!("print{count}"), args);
        state.print_count += 1;
        return;
    }
    let count = state.print_count;
    let print = format!("print{count}");
    match llvm_type(ty).expect("validated print type") {
        LlvmType::I1 => {
            let text_of =
                LlvmInst::select(own, LlvmType::Ptr, global(".bn_true"), global(".bn_false"));
            text.assign(format!("bool{count}"), text_of);
            let args = vec![
                (LlvmType::Ptr, global(".bn_fmt_str")),
                (LlvmType::Ptr, reg(format!("bool{count}"))),
            ];
            printf(text, print, args);
        }
        width @ (LlvmType::I8 | LlvmType::I16 | LlvmType::I32) => {
            let opcode = if is_unsigned(ty) {
                CastOp::ZExt
            } else {
                CastOp::SExt
            };
            let wide = LlvmInst::cast(opcode, width, own, LlvmType::I64);
            text.assign(format!("printint{count}"), wide);
            let args = vec![
                (LlvmType::Ptr, global(".bn_fmt_int")),
                (LlvmType::I64, reg(format!("printint{count}"))),
            ];
            printf(text, print, args);
        }
        LlvmType::I64 => {
            let fmt = if is_unsigned(ty) {
                ".bn_fmt_uint"
            } else {
                ".bn_fmt_int"
            };
            printf(
                text,
                print,
                vec![(LlvmType::Ptr, global(fmt)), (LlvmType::I64, own)],
            );
        }
        LlvmType::Float => {
            let wide = LlvmInst::cast(CastOp::FPExt, LlvmType::Float, own, LlvmType::Double);
            text.assign(format!("printfloat{count}"), wide);
            let args = vec![(LlvmType::Double, reg(format!("printfloat{count}")))];
            text.emit(LlvmInst::call(LlvmType::Void, "bn_rt_print_float32", args));
        }
        LlvmType::Double => {
            text.emit(LlvmInst::call(
                LlvmType::Void,
                "bn_rt_print_float",
                vec![(LlvmType::Double, own)],
            ));
        }
        LlvmType::Ptr => {
            printf(
                text,
                print,
                vec![(LlvmType::Ptr, global(".bn_fmt_str")), (LlvmType::Ptr, own)],
            );
        }
        _ => unreachable!("validated printable LLVM type"),
    }
    state.print_count += 1;
}

fn lower_print_int32_vector(
    text: &mut crate::ir::LlvmModule,
    value: ValueId,
    length: u64,
    state: &mut EmissionState,
) {
    let reg = LlvmOperand::reg;
    let putchar = |text: &mut crate::ir::LlvmModule, dest: String, code: i64| {
        text.assign(
            dest,
            LlvmInst::call(
                LlvmType::I32,
                "putchar",
                vec![(LlvmType::I32, LlvmOperand::int(code))],
            ),
        );
    };
    let vector = state.print_count;
    putchar(text, format!("vecopen{vector}"), 91);
    let fat = LlvmType::struct_of([LlvmType::Ptr, LlvmType::I32]);
    text.assign(
        format!("vecprintdata{vector}"),
        LlvmInst::extract(fat, reg(format!("v{}", value.0)), 0),
    );
    state.print_count += 1;
    for index in 0..length {
        let item = state.print_count;
        if index > 0 {
            putchar(text, format!("veccomma{item}"), 44);
            putchar(text, format!("vecspace{item}"), 32);
        }
        let at = vec![(LlvmType::I64, LlvmOperand::uint(index))];
        text.assign(
            format!("vecitemptr{item}"),
            LlvmInst::gep(LlvmType::I32, reg(format!("vecprintdata{vector}")), at),
        );
        text.assign(
            format!("vecitem{item}"),
            LlvmInst::load(LlvmType::I32, reg(format!("vecitemptr{item}"))),
        );
        let wide = LlvmInst::cast(
            CastOp::SExt,
            LlvmType::I32,
            reg(format!("vecitem{item}")),
            LlvmType::I64,
        );
        text.assign(format!("vecitem64_{item}"), wide);
        let args = vec![
            (LlvmType::Ptr, LlvmOperand::global(".bn_fmt_int")),
            (LlvmType::I64, reg(format!("vecitem64_{item}"))),
        ];
        printf(text, format!("vecprint{item}"), args);
        state.print_count += 1;
    }
    let close = state.print_count;
    putchar(text, format!("vecclose{close}"), 93);
    state.print_count += 1;
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn emit_checked_integer_op(
    text: &mut crate::ir::LlvmModule,
    block_id: BlockId,
    destination: ValueId,
    operator: &str,
    left: ValueId,
    right: Option<ValueId>,
    left_ty: &Type,
    right_ty: &Type,
    ty: &Type,
    state: &mut EmissionState,
) {
    let intrinsic = checked_intrinsic_name(ty, operator).expect("validated checked intrinsic");
    let llvm_ty = llvm_type(ty).expect("validated integer type");
    let (left_operand, right_operand) = if operator == "Minus" && right.is_none() {
        (LlvmOperand::int(0), coerce_to_type(text, left, left_ty, ty))
    } else {
        (
            coerce_to_type(text, left, left_ty, ty),
            coerce_to_type(text, right.expect("binary op right"), right_ty, ty),
        )
    };
    let dest = destination.0;
    let reg = LlvmOperand::reg;
    let width = llvm_ty;
    let result = LlvmType::struct_of([width.clone(), LlvmType::I1]);
    let (lhs, rhs) = (left_operand, right_operand);
    let args = vec![(width.clone(), lhs.clone()), (width.clone(), rhs.clone())];
    text.assign(
        format!("ov{dest}"),
        LlvmInst::call(result.clone(), intrinsic, args),
    );
    text.assign(
        format!("v{dest}"),
        LlvmInst::extract(result.clone(), reg(format!("ov{dest}")), 0),
    );
    text.assign(
        format!("ovf{dest}"),
        LlvmInst::extract(result, reg(format!("ov{dest}")), 1),
    );
    // The exact result, as the interpreter reports it: BN integers are at
    // most 64 bits, so +, - and * of two of them fit i128.
    let extend = if is_unsigned(ty) {
        CastOp::ZExt
    } else {
        CastOp::SExt
    };
    let exact_op = match operator {
        "Plus" => crate::ir::BinaryOp::Add,
        "Minus" => crate::ir::BinaryOp::Sub,
        _ => crate::ir::BinaryOp::Mul,
    };
    text.assign(
        format!("ovl{dest}"),
        LlvmInst::cast(extend, width.clone(), lhs, LlvmType::I128),
    );
    text.assign(
        format!("ovr{dest}"),
        LlvmInst::cast(extend, width, rhs, LlvmType::I128),
    );
    let exact = LlvmInst::binary(
        exact_op,
        LlvmType::I128,
        reg(format!("ovl{dest}")),
        reg(format!("ovr{dest}")),
    );
    text.assign(format!("ovexact{dest}"), exact);
    let ok = take_continuation(block_id, state);
    emit_overflow_trap(
        text,
        block_id,
        state,
        &LlvmOperand::reg(format!("ovf{dest}")),
        ok,
        &LlvmOperand::reg(format!("ovexact{dest}")),
        ty,
    );
}

pub(crate) fn checked_intrinsic_name(ty: &Type, operator: &str) -> Option<&'static str> {
    let width = match llvm_type(ty)? {
        LlvmType::I8 => "i8",
        LlvmType::I16 => "i16",
        LlvmType::I32 => "i32",
        LlvmType::I64 => "i64",
        _ => return None,
    };
    let signed = !is_unsigned(ty);
    match operator {
        "Plus" => Some(match (signed, width) {
            (true, "i8") => "llvm.sadd.with.overflow.i8",
            (true, "i16") => "llvm.sadd.with.overflow.i16",
            (true, "i32") => "llvm.sadd.with.overflow.i32",
            (true, "i64") => "llvm.sadd.with.overflow.i64",
            (false, "i8") => "llvm.uadd.with.overflow.i8",
            (false, "i16") => "llvm.uadd.with.overflow.i16",
            (false, "i32") => "llvm.uadd.with.overflow.i32",
            (false, "i64") => "llvm.uadd.with.overflow.i64",
            _ => unreachable!(),
        }),
        "Minus" => Some(match (signed, width) {
            (true, "i8") => "llvm.ssub.with.overflow.i8",
            (true, "i16") => "llvm.ssub.with.overflow.i16",
            (true, "i32") => "llvm.ssub.with.overflow.i32",
            (true, "i64") => "llvm.ssub.with.overflow.i64",
            (false, "i8") => "llvm.usub.with.overflow.i8",
            (false, "i16") => "llvm.usub.with.overflow.i16",
            (false, "i32") => "llvm.usub.with.overflow.i32",
            (false, "i64") => "llvm.usub.with.overflow.i64",
            _ => unreachable!(),
        }),
        "Star" | "Multiply" => Some(match (signed, width) {
            (true, "i8") => "llvm.smul.with.overflow.i8",
            (true, "i16") => "llvm.smul.with.overflow.i16",
            (true, "i32") => "llvm.smul.with.overflow.i32",
            (true, "i64") => "llvm.smul.with.overflow.i64",
            (false, "i8") => "llvm.umul.with.overflow.i8",
            (false, "i16") => "llvm.umul.with.overflow.i16",
            (false, "i32") => "llvm.umul.with.overflow.i32",
            (false, "i64") => "llvm.umul.with.overflow.i64",
            _ => unreachable!(),
        }),
        _ => None,
    }
}

/// The overflow-checking intrinsic of an integer `operator` on `ty`.
pub(crate) fn checked_intrinsic(ty: &Type, operator: &str) -> Option<crate::ir::Intrinsic> {
    let width = llvm_type(ty)?;
    Some(crate::runtime_abi::overflow(
        checked_intrinsic_name(ty, operator)?,
        width,
    ))
}

/// The `icmp` predicate of a BN comparison; `ty` decides signedness.
pub(crate) fn integer_compare_cond(operator: &str, ty: &Type) -> crate::ir::ICmpCond {
    use crate::ir::ICmpCond::{Eq, Ne, Sge, Sgt, Sle, Slt, Uge, Ugt, Ule, Ult};
    match (operator, is_unsigned(ty)) {
        ("Less", false) => Slt,
        ("Less", true) => Ult,
        ("LessEqual", false) => Sle,
        ("LessEqual", true) => Ule,
        ("Greater", false) => Sgt,
        ("Greater", true) => Ugt,
        ("GreaterEqual", false) => Sge,
        ("GreaterEqual", true) => Uge,
        ("Equal" | "Assign", _) => Eq,
        ("NotEqual", _) => Ne,
        _ => unreachable!("validated integer comparison"),
    }
}

/// The `fcmp` predicate of a BN comparison. IEEE 754: every ordered
/// comparison with `NAN` is false and `<>` is true, as in `bni`; so `<>` is
/// `une` (unordered or not equal), not `one`.
pub(crate) fn float_compare_cond(operator: &str) -> crate::ir::FCmpCond {
    use crate::ir::FCmpCond::{Oeq, Oge, Ogt, Ole, Olt, Une};
    match operator {
        "Less" => Olt,
        "LessEqual" => Ole,
        "Greater" => Ogt,
        "GreaterEqual" => Oge,
        "Equal" | "Assign" => Oeq,
        "NotEqual" => Une,
        _ => unreachable!("validated float comparison"),
    }
}
