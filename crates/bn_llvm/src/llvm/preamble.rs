// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

// The module preamble: string globals, runtime declarations, and the
// helper IR each emitted module needs.
#![allow(clippy::wildcard_imports, clippy::too_many_lines)]
use super::*;
use crate::ir::{
    CastOp, ICmpCond, InstSink as _, Linkage, LlvmGlobal, LlvmInst as I, LlvmOperand as O,
    LlvmType as T, write_declarations,
};
pub(crate) fn emit_preamble(
    text: &mut crate::ir::LlvmModule,
    module: &Module,
    functions: &[(&Function, LoweringAnalysis<'_>)],
    synchronize_prints: bool,
    needs_na: bool,
    calls_policy: bool,
) {
    let uses_arc = functions.iter().any(|(_, analysis)| analysis.uses_arc);
    // A STOP outside Start, and the `$stopping` checks after calls, share
    // two globals (0.6.md, "`STOP`").
    let may_stop = functions.iter().any(|(function, analysis)| {
        analysis
            .functions
            .values()
            .any(|name| *name == bn_ir::names::STOPPING)
            || function.kind != FunctionKind::Entry
                && function
                    .blocks
                    .iter()
                    .any(|block| matches!(block.terminator, Terminator::Stop { .. }))
    });
    if may_stop {
        super::arc_runtime::emit_stop_globals(text);
    }
    let mut uses_concat = false;
    // `Start` of a module that imports a capability calls the policy entry
    // points (`functions.rs`), which live in bn_rt.
    let mut uses_bn_rt = calls_policy;
    let mut uses_input = false;
    let mut uses_exit = false;
    let mut intrinsics = BTreeSet::new();
    for (function, analysis) in functions {
        uses_concat |= analysis.uses_string_concat;
        uses_bn_rt |= analysis.uses_bn_rt;
        uses_input |= analysis.input_count > 0;
        uses_exit |= function.kind != FunctionKind::Entry
            && (analysis.uses_bn_rt
                || function.blocks.iter().any(|block| {
                    matches!(
                        block.terminator,
                        Terminator::Stop { .. } | Terminator::Return { .. }
                    )
                }));
        for (value, string) in &analysis.strings {
            let global = string_global(&function.name, value.0);
            text.global(LlvmGlobal::bn_string(global, string, false));
        }
        intrinsics.extend(analysis.intrinsics.iter().cloned());
    }
    if uses_input {
        emit_input_runtime(text);
    }
    text.blank();
    write_declarations(text, runtime_abi::PRINT_DECLARATIONS);
    crate::platform_stdio::emit_windows_binary_stdio_decl(text, synchronize_prints);
    if synchronize_prints {
        crate::platform_stdio::emit_stdout_lock_decls(text);
    }
    if uses_concat {
        write_declarations(text, runtime_abi::STRING_CONCAT_DECLARATIONS);
    }
    if uses_bn_rt {
        write_declarations(text, runtime_abi::BN_RT_DECLARATIONS);
    }
    if !module.bnweb_providers.is_empty() {
        crate::web_ops::declare_web_abi(text);
    }
    if functions
        .iter()
        .any(|(_, analysis)| analysis.uses_bn_rt_math)
    {
        write_declarations(text, runtime_abi::BN_RT_MATH_DECLARATIONS);
    }
    if functions
        .iter()
        .any(|(_, analysis)| analysis.uses_float_print)
    {
        write_declarations(text, runtime_abi::PRINT_FLOAT_DECLARATIONS);
    }
    if functions
        .iter()
        .any(|(_, analysis)| analysis.uses_temporal_print)
    {
        write_declarations(text, runtime_abi::PRINT_TEMPORAL_DECLARATIONS);
    }
    if needs_na || functions.iter().any(|(_, analysis)| {
            analysis.values.values().any(|ty| {
                llvm_type(ty) == Some(crate::layout::optional_float_ty())
                    || matches!(ty, Type::Alternative(types) if string_na_or_error(types) || scalar_na_or_error(types) || error_or_na(types))
                    || general_alternative(ty).is_some_and(|members| members.contains(&Type::NotAvailable))
            })
        })
    {
        text.global(LlvmGlobal::bn_string(".bn_na", "NA", false));
    }
    if functions.iter().any(|(function, analysis)| {
        analysis.values.values().any(|ty| {
            matches!(ty, Type::Alternative(types) if void_or_error(types))
                || general_alternative(ty).is_some_and(|members| members.contains(&Type::Null))
        }) || prints_null(function, analysis)
    }) {
        text.global(LlvmGlobal::text(".bn_null", "NULL", false));
    }
    if functions
        .iter()
        .any(|(_, analysis)| analysis.uses_string_ops)
    {
        write_declarations(text, runtime_abi::STRING_OPS_DECLARATIONS);
    }
    if functions.iter().any(|(_, analysis)| analysis.uses_heap) {
        if !uses_concat {
            write_declarations(text, runtime_abi::HEAP_COPY_DECLARATIONS);
        }
        write_declarations(text, &[&runtime_abi::CALLOC]);
        // INPUT no longer declares `free` itself. The object ARC glue,
        // emitted for a STRING buffer release, still calls it.
        if !uses_input || uses_arc {
            write_declarations(text, &[&runtime_abi::FREE]);
        }
    }
    for (function, _) in functions {
        if let Some((array, _)) =
            vectors::returned_vector(&function.return_type).and_then(vectors::fixed_vector_array)
        {
            let name = format!("bn_vret_{}", sanitize_symbol(&function.name));
            let buffer = LlvmGlobal::variable(name, array, O::zero_initializer());
            text.global(buffer.with_linkage(Linkage::Internal).thread_local());
        }
    }
    let static_state = super::statics::StaticState::collect(module);
    static_state.emit(text);
    let resets_statics = !static_state.is_empty();
    let mut class_names = BTreeSet::new();
    for (function, _) in functions {
        for block in &function.blocks {
            for instruction in &block.instructions {
                if let Instruction::Allocate { type_name, ty, .. } = instruction
                    && !matches!(ty, Type::Pointer { .. })
                {
                    class_names.insert(type_name.clone());
                }
            }
        }
        if let Some((class, method)) = function.name.rsplit_once('.')
            && !method.starts_with('$')
            && method != "CONSTRUCTOR"
            && method != "DESTRUCTOR"
        {
            class_names.insert(class.rsplit('.').next().unwrap_or(class).to_string());
            class_names.insert(class.to_string());
        }
    }
    if !class_names.is_empty()
        && !functions
            .iter()
            .any(|(_, analysis)| analysis.uses_string_ops)
    {
        write_declarations(text, &[&runtime_abi::STR_EQ]);
    }
    if uses_arc {
        // The classes `NEW` creates: the destruction dispatches on them, and
        // their destructors and field releases are emitted.
        let allocated = functions
            .iter()
            .flat_map(|(function, _)| &function.blocks)
            .flat_map(|block| &block.instructions)
            .filter_map(|instruction| match instruction {
                Instruction::Allocate { type_name, ty, .. } if is_class_type(module, ty) => {
                    Some(type_name.clone())
                }
                _ => None,
            })
            .collect::<BTreeSet<_>>();
        super::arc_runtime::emit_glue(text, module, &allocated);
    } else {
        super::arc_runtime::emit_witness_glue(text, module);
    }
    for (function, _) in functions {
        for block in &function.blocks {
            for instruction in &block.instructions {
                if let Instruction::Allocate {
                    ty: Type::Pointer { element, .. },
                    ..
                } = instruction
                {
                    class_names.insert(super::helpers::region_element_label(element));
                }
            }
        }
    }
    for class in &class_names {
        let gclass = sanitize_symbol(class);
        text.global(LlvmGlobal::text(format!(".bn_cls_{gclass}"), class, true));
    }
    if uses_arc
        || uses_exit
        || functions.iter().any(|(function, _)| {
            function.blocks.iter().any(|block| {
                block.instructions.iter().any(|instruction| {
                    matches!(instruction, Instruction::Member { owner, .. } if owner != "Error")
                        || matches!(instruction, Instruction::Release { .. })
                })
            })
        })
    {
        write_declarations(text, &[&runtime_abi::EXIT]);
    }
    let intrinsics: Vec<&dyn crate::ir::Declaration> =
        intrinsics.iter().map(|intrinsic| intrinsic as _).collect();
    write_declarations(text, &intrinsics);
    let mut trampolines = BTreeSet::new();
    for (_, analysis) in functions {
        for name in analysis.functions.values().copied() {
            if name.starts_with("@super:") {
                continue;
            }
            if functions.iter().any(|(function, _)| {
                function.blocks.iter().any(|block| {
                    block.instructions.iter().any(|instruction| {
                        matches!(instruction, Instruction::DispatchSubmit { task, .. } if analysis.functions.get(task).copied() == Some(name))
                    })
                })
            }) {
                trampolines.insert(name.to_string());
            }
        }
    }
    for task in trampolines {
        let Some((task_fn, task_analysis)) =
            functions.iter().find(|(function, _)| function.name == task)
        else {
            continue;
        };
        let symbol = llvm_function_symbol(&task);
        let wrapper = dispatch_trampoline_symbol(&task);
        emit_trampoline(
            text,
            task_fn,
            task_analysis,
            &symbol,
            &wrapper,
            may_stop,
            resets_statics,
        );
    }
    let _ = uses_exit;
}

/// Whether `function` prints a `NULL` value (a narrowed `NULL`), whose text
/// is `@.bn_null`.
fn prints_null(function: &Function, analysis: &LoweringAnalysis<'_>) -> bool {
    function
        .blocks
        .iter()
        .flat_map(|block| &block.instructions)
        .any(|instruction| {
            matches!(instruction, Instruction::Print { values, .. }
            if values.iter().any(|value| {
                analysis.values.get(value).is_some_and(|ty| {
                    *ty == Type::Null
                        || matches!(ty, Type::Alternative(types) if types.contains(&Type::Null))
                })
            }))
        })
}

/// The `bn_rt` dispatch trampoline of an async task: unpacks the 16-byte
/// argument cells, calls the task, and writes its result (`kind` at 0, the
/// payload at 8) or its `Error` (code at 0, record at 8). A task starts on
/// its own fresh copy of the `STATIC` fields (`statics.rs`, K44).
fn emit_trampoline(
    text: &mut crate::ir::LlvmModule,
    task_fn: &Function,
    task_analysis: &LoweringAnalysis<'_>,
    symbol: &str,
    wrapper: &str,
    may_stop: bool,
    resets_statics: bool,
) {
    let ret = match &task_fn.return_type {
        Type::Alternative(alternatives) if integer_or_error(alternatives) => T::I32,
        Type::Alternative(alternatives) if float_or_error(alternatives) => T::Double,
        Type::Alternative(alternatives) if string_or_error(alternatives) => T::Ptr,
        Type::Alternative(alternatives) if boolean_or_error(alternatives) => T::I1,
        _ => function_return_llvm(&task_fn.return_type).unwrap_or(T::Void),
    };
    let params = ["context", "arguments", "argument_count", "result", "error"]
        .into_iter()
        .map(|name| {
            let ty = if name == "argument_count" {
                T::I32
            } else {
                T::Ptr
            };
            (ty, name.to_string())
        })
        .collect();
    text.blank();
    text.begin_function(crate::ir::LlvmFunction::new_definition(
        wrapper,
        T::I32,
        params,
    ));
    if resets_statics {
        text.emit(super::statics::reset_call());
    }
    let is_task_handle = matches!(&task_fn.return_type, Type::Alternative(alternatives)
    if alternatives.iter().any(|ty| match ty {
        Type::ImportedNamed { name, .. } | Type::ImportedTypeName { name, .. } => dispatch_handle_name(name),
        Type::Named(name) | Type::TypeName(name) => dispatch_handle_name(name) || dispatch_handle_name(name.rsplit('.').next().unwrap_or(name)),
        _ => false,
    }));
    let is_task_datetime = matches!(&task_fn.return_type, Type::Alternative(alternatives)
        if alternatives.iter().any(|ty| matches!(ty, Type::Named(name) | Type::TypeName(name) if matches!(name.as_str(), "DATE" | "TIME" | "TIMESTAMP"))));
    let aggregate_return = matches!(&task_fn.return_type, Type::Alternative(alternatives)
        if void_or_error(alternatives)
            || integer_or_error(alternatives)
            || float_or_error(alternatives)
            || string_or_error(alternatives)
            || boolean_or_error(alternatives)
            || is_task_handle
            || is_task_datetime);
    let r = |name: &str| O::reg(name.to_string());
    let mut dispatch_arguments = Vec::new();
    for (index, parameter) in task_fn.parameters.iter().enumerate() {
        let parameter_ty = &task_analysis.symbols[parameter];
        let llvm_ty = llvm_type(parameter_ty).expect("validated dispatch parameter type");
        let payload = O::reg(format!("dispatch_arg_payload{index}"));
        let raw = O::reg(format!("dispatch_arg_raw{index}"));
        let arg = format!("dispatch_arg{index}");
        let offset = vec![(T::I64, O::uint(index as u64 * 16 + 8))];
        text.assign(payload.to_string(), I::gep(T::I8, r("arguments"), offset));
        let load_raw = |text: &mut crate::ir::LlvmModule, ty: T| {
            text.assign(raw.to_string(), I::load(ty, payload.clone()));
        };
        let is_handle = match parameter_ty {
            Type::ImportedNamed { name, .. } | Type::ImportedTypeName { name, .. } => {
                dispatch_handle_name(name)
            }
            Type::Named(name) | Type::TypeName(name) => {
                dispatch_handle_name(name)
                    || dispatch_handle_name(name.rsplit('.').next().unwrap_or(name))
            }
            _ => false,
        };
        let is_date_time = matches!(
            parameter_ty,
            Type::Named(name) | Type::TypeName(name) if matches!(name.as_str(), "DATE" | "TIME" | "TIMESTAMP")
        );
        if is_handle {
            load_raw(text, T::I64);
            let head = I::insert(
                crate::layout::handle_result_ty(),
                O::undef(),
                T::I1,
                O::bool(false),
                0,
            );
            text.assign(format!("dispatch_handle_agg0_{index}"), head);
            let null = I::insert(
                crate::layout::handle_result_ty(),
                O::reg(format!("dispatch_handle_agg0_{index}")),
                T::Ptr,
                O::null(),
                1,
            );
            text.assign(format!("dispatch_handle_agg1_{index}"), null);
            let handle = I::insert(
                crate::layout::handle_result_ty(),
                O::reg(format!("dispatch_handle_agg1_{index}")),
                T::I64,
                raw.clone(),
                2,
            );
            text.assign(&arg, handle);
        } else if is_date_time {
            if llvm_ty == T::I32 {
                load_raw(text, T::I64);
                let narrow = I::cast(CastOp::Trunc, T::I64, raw.clone(), T::I32);
                text.assign(&arg, narrow);
            } else {
                text.assign(&arg, I::load(T::I64, payload));
            }
        } else {
            match parameter_ty {
                Type::Boolean => {
                    load_raw(text, T::I64);
                    text.assign(&arg, I::icmp(ICmpCond::Ne, T::I64, raw.clone(), O::int(0)));
                }
                Type::Integer(_) if llvm_ty == T::I64 => {
                    text.assign(&arg, I::load(T::I64, payload));
                }
                Type::Integer(_) => {
                    load_raw(text, T::I64);
                    let narrow = I::cast(CastOp::Trunc, T::I64, raw.clone(), llvm_ty.clone());
                    text.assign(&arg, narrow);
                }
                Type::Float(_) if llvm_ty == T::Float => {
                    load_raw(text, T::Double);
                    text.assign(
                        &arg,
                        I::cast(CastOp::FPTrunc, T::Double, raw.clone(), T::Float),
                    );
                }
                Type::Float(_) => text.assign(&arg, I::load(T::Double, payload)),
                Type::String => text.assign(&arg, I::load(T::Ptr, payload)),
                _ => unreachable!("validated dispatch parameter type"),
            }
        }
        dispatch_arguments.push((llvm_ty, O::reg(arg)));
    }
    let stop_check = |text: &mut crate::ir::LlvmModule| {
        if may_stop {
            super::arc_runtime::stop_exit_check(text);
        }
    };
    let payload_at = |base: O| I::gep(T::I8, base, vec![(T::I64, O::int(8))]);
    let store = |text: &mut crate::ir::LlvmModule, ty: T, value: O, slot: O| {
        text.emit(I::store(ty, value, slot));
    };
    if ret == T::Void {
        text.emit(I::call(T::Void, symbol, dispatch_arguments));
        stop_check(text);
    } else {
        // ASYNC_RETURN_TYPE: a typed task returns `T OR Error`.
        assert!(aggregate_return, "validated ASYNC return type");
        let aggregate = function_return_llvm(&task_fn.return_type).unwrap_or_else(|| ret.clone());
        let field = |index| I::extract(aggregate.clone(), r("dispatch_raw"), index);
        text.assign(
            "dispatch_raw",
            I::call(aggregate.clone(), symbol, dispatch_arguments),
        );
        stop_check(text);
        text.assign("dispatch_is_error", field(0));
        text.emit(I::CondBr {
            cond: r("dispatch_is_error"),
            true_dest: "dispatch_error".into(),
            false_dest: "dispatch_success".into(),
        });
        text.label("dispatch_error");
        text.assign("dispatch_error_message", field(1));
        text.assign("dispatch_error_code64", field(2));
        let code = I::cast(CastOp::Trunc, T::I64, r("dispatch_error_code64"), T::I32);
        text.assign("dispatch_error_code", code);
        store(text, T::I32, r("dispatch_error_code"), r("error"));
        text.assign("dispatch_error_message_slot", payload_at(r("error")));
        store(
            text,
            T::Ptr,
            r("dispatch_error_message"),
            r("dispatch_error_message_slot"),
        );
        text.emit(I::Ret {
            val: Some((T::I32, O::int(1))),
        });
        text.label("dispatch_success");
        let kind = match ret {
            T::Double => 3,
            T::Ptr => 4,
            T::I1 => 1,
            _ if matches!(&task_fn.return_type, Type::Alternative(alternatives) if void_or_error(alternatives)) => {
                0
            }
            _ if is_task_handle => 6,
            _ => 2,
        };
        store(text, T::I32, O::int(kind), r("result"));
        text.assign("dispatch_value", field(2));
        text.assign("dispatch_payload", payload_at(r("result")));
        let payload = r("dispatch_payload");
        if kind == 0 {
            store(text, T::I64, O::int(0), payload);
        } else if ret == T::Ptr {
            text.assign("dispatch_string", field(1));
            store(text, T::Ptr, r("dispatch_string"), payload.clone());
            text.assign("dispatch_string_length", payload_at(payload));
            store(text, T::I32, O::int(0), r("dispatch_string_length"));
        } else if ret == T::I1 {
            text.assign("dispatch_bool64", field(2));
            store(text, T::I64, r("dispatch_bool64"), payload);
        } else if ret == T::Double {
            let float = I::cast(CastOp::BitCast, T::I64, r("dispatch_value"), T::Double);
            text.assign("dispatch_float", float);
            store(text, T::Double, r("dispatch_float"), payload);
        } else {
            store(text, T::I64, r("dispatch_value"), payload);
        }
    }
    text.emit(I::Ret {
        val: Some((T::I32, O::int(0))),
    });
    text.end_function();
}

pub(crate) const HOST_ARGC_GLOBAL: &str = ".bn_argc";
pub(crate) const HOST_ARGV_GLOBAL: &str = ".bn_argv";

/// The first lines of every module: the source banner and the string
/// constants `PRINT`, `ASC` and `CHAR` read (texts from `bn_diag::trap_texts`).
pub(crate) fn emit_module_header(text: &mut crate::ir::LlvmModule) {
    use crate::ir::{Linkage, LlvmGlobal, LlvmOperand, LlvmType};
    text.comment(super::debug_info::PRODUCER);
    let shared = [
        (".bn_fmt_int", "%lld"),
        (".bn_fmt_uint", "%llu"),
        (".bn_fmt_float", "%.17g"),
        (".bn_fmt_str", "%s"),
        (".bn_fmt_error", "Error(%lld, %s)"),
    ];
    for (name, value) in shared {
        text.global(LlvmGlobal::text(name, value, true));
    }
    for (name, value) in [
        (".bn_true", "TRUE"),
        (".bn_false", "FALSE"),
        (".bn_empty", ""),
        (".bn_asc_error", bn_diag::trap_texts::ASC_EMPTY_STRING),
        (".bn_char_error", bn_diag::trap_texts::CHAR_NOT_UNICODE),
    ] {
        text.global(LlvmGlobal::bn_string(name, value, true));
    }
    text.global(LlvmGlobal::bn_string(".bn_eof", "EOF", false));
    let private =
        |name: &str, ty, init| LlvmGlobal::variable(name, ty, init).with_linkage(Linkage::Private);
    text.global(private(
        HOST_ARGC_GLOBAL,
        LlvmType::I32,
        LlvmOperand::int(0),
    ));
    text.global(private(
        HOST_ARGV_GLOBAL,
        LlvmType::Ptr,
        LlvmOperand::null(),
    ));
}
