// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

// The module preamble: string globals, runtime declarations, and the
// helper IR each emitted module needs.
#![allow(clippy::wildcard_imports, clippy::too_many_lines)]
use super::*;
use crate::ir::{CastOp, ICmpCond, InstSink as _, LlvmInst as I, LlvmOperand as O, LlvmType as T};
use crate::layout::typed_llvm;
pub(crate) fn emit_preamble(
    text: &mut String,
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
        text.push_str(&super::arc_runtime::stop_globals());
    }
    let mut uses_concat = false;
    // `Start` of a module that imports a capability calls the policy entry
    // points (`functions.rs`), which live in bn_rt.
    let mut uses_bn_rt = calls_policy;
    let mut uses_input = false;
    let mut uses_string_sizeof = false;
    let mut uses_exit = false;
    let mut intrinsics = BTreeSet::new();
    for (function, analysis) in functions {
        uses_concat |= analysis.uses_string_concat;
        uses_bn_rt |= analysis.uses_bn_rt;
        uses_input |= analysis.input_count > 0;
        uses_string_sizeof |= analysis.uses_string_sizeof;
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
            let _ = writeln!(
                text,
                "{global} = private constant [{} x i8] c\"{}\\00\"",
                string.len() + 1,
                escape_llvm(string)
            );
        }
        intrinsics.extend(analysis.intrinsics.iter().copied());
    }
    if uses_input {
        text.push_str(input_runtime_ir());
    }
    if uses_string_sizeof {
        text.push_str(string_byte_length_ir());
    }
    text.push_str("\ndeclare i32 @printf(ptr, ...)\ndeclare i32 @putchar(i32)\n");
    text.push_str(crate::platform_stdio::windows_binary_stdio_decl(
        synchronize_prints,
    ));
    if synchronize_prints {
        text.push_str(&crate::platform_stdio::stdout_lock_decls());
    }
    if uses_concat {
        text.push_str(STRING_CONCAT_DECLS);
    }
    if uses_bn_rt {
        text.push_str(BN_RT_DECLS);
    }
    if functions
        .iter()
        .any(|(_, analysis)| analysis.uses_bn_rt_math)
    {
        text.push_str(BN_RT_MATH_DECLS);
    }
    if functions
        .iter()
        .any(|(_, analysis)| analysis.uses_float_print)
    {
        text.push_str(
            "declare void @bn_rt_print_float(double)\ndeclare void @bn_rt_print_float32(double)\n",
        );
    }
    if functions
        .iter()
        .any(|(_, analysis)| analysis.uses_temporal_print)
    {
        text.push_str("declare void @bn_rt_print_date(i32)\ndeclare void @bn_rt_print_time(i32)\n");
    }
    if needs_na || functions.iter().any(|(_, analysis)| {
            analysis.values.values().any(|ty| {
                llvm_type(ty) == Some("{ i1, double }")
                    || matches!(ty, Type::Alternative(types) if string_na_or_error(types) || scalar_na_or_error(types) || error_or_na(types))
                    || general_alternative(ty).is_some_and(|members| members.contains(&Type::NotAvailable))
            })
        })
    {
        text.push_str("@.bn_na = private unnamed_addr constant [3 x i8] c\"NA\\00\"\n");
    }
    if functions.iter().any(|(function, analysis)| {
        analysis.values.values().any(|ty| {
            matches!(ty, Type::Alternative(types) if void_or_error(types))
                || general_alternative(ty).is_some_and(|members| members.contains(&Type::Null))
        }) || prints_null(function, analysis)
    }) {
        text.push_str("@.bn_null = private constant [5 x i8] c\"NULL\\00\"\n");
    }
    if functions
        .iter()
        .any(|(_, analysis)| analysis.uses_string_ops)
    {
        text.push_str(
            "declare i32 @bn_rt_str_len(ptr)\ndeclare i64 @bn_rt_str_index_utf8(ptr, i32, ptr)\ndeclare i32 @bn_rt_str_eq(ptr, ptr)\n",
        );
    }
    if functions.iter().any(|(_, analysis)| analysis.uses_heap) {
        if !uses_concat {
            text.push_str(
                "declare ptr @malloc(i64)\ndeclare void @llvm.memcpy.p0.p0.i64(ptr, ptr, i64, i1)\n",
            );
        }
        text.push_str("declare ptr @calloc(i64, i64)\n");
        if !uses_input {
            text.push_str("declare void @free(ptr)\n");
        }
    }
    let mut static_globals = BTreeSet::new();
    let mut class_inits = BTreeSet::new();
    for (function, analysis) in functions {
        for block in &function.blocks {
            for instruction in &block.instructions {
                match instruction {
                    Instruction::LoadStatic {
                        class, field, ty, ..
                    }
                    | Instruction::StoreStatic {
                        class, field, ty, ..
                    } => {
                        if let Some(llvm_ty) = llvm_type(ty) {
                            static_globals.insert((
                                class.clone(),
                                field.clone(),
                                llvm_ty,
                                vectors::fixed_vector_array(ty),
                            ));
                        }
                    }
                    Instruction::EnsureClass { class, .. } => {
                        class_inits.insert(class.clone());
                    }
                    _ => {}
                }
            }
        }
        let _ = analysis;
    }
    for (class, field, llvm_ty, vector) in &static_globals {
        let gclass = sanitize_symbol(class);
        let gfield = sanitize_symbol(field);
        // A fixed vector STATIC owns its elements (and a copy of the replaced ones).
        if let Some((array, count)) = vector {
            let _ = writeln!(
                text,
                "@bn_sv_{gclass}_{gfield} = global {array} zeroinitializer\n@bn_svp_{gclass}_{gfield} = global {array} zeroinitializer\n@bn_st_{gclass}_{gfield} = global {{ ptr, i32 }} {{ ptr @bn_sv_{gclass}_{gfield}, i32 {count} }}"
            );
            continue;
        }
        let _ = writeln!(
            text,
            "@bn_st_{gclass}_{gfield} = global {llvm_ty} {}",
            match *llvm_ty {
                "i1" => "false",
                "float" | "double" => "0.0",
                "ptr" => "null",
                aggregate if aggregate.starts_with('{') => "zeroinitializer",
                _ => "0",
            }
        );
    }
    for (function, _) in functions {
        if let Some((array, _)) =
            vectors::returned_vector(&function.return_type).and_then(vectors::fixed_vector_array)
        {
            let name = sanitize_symbol(&function.name);
            let _ = writeln!(
                text,
                "@bn_vret_{name} = internal thread_local global {array} zeroinitializer"
            );
        }
    }
    for class in &class_inits {
        let gclass = sanitize_symbol(class);
        let _ = writeln!(text, "@bn_init_{gclass} = global i1 false");
    }
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
        text.push_str("declare i32 @bn_rt_str_eq(ptr, ptr)\n");
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
        text.push_str(&super::arc_runtime::glue(module, &allocated));
    }
    for class in &class_names {
        let gclass = sanitize_symbol(class);
        let _ = writeln!(
            text,
            "@.bn_cls_{gclass} = private unnamed_addr constant [{} x i8] c\"{}\\00\"",
            class.len() + 1,
            escape_llvm(class)
        );
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
        text.push_str("declare void @exit(i32)\n");
    }
    for intrinsic in intrinsics {
        let _ = writeln!(text, "declare {intrinsic}");
    }
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
        emit_trampoline(text, task_fn, task_analysis, &symbol, &wrapper, may_stop);
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
                if values.iter().any(|value| analysis.values.get(value) == Some(&Type::Null)))
        })
}

/// The `bn_rt` dispatch trampoline of an async task: unpacks the 16-byte
/// argument cells, calls the task, and writes its result (`kind` at 0, the
/// payload at 8) or its `Error` (code at 0, record at 8).
fn emit_trampoline(
    text: &mut String,
    task_fn: &Function,
    task_analysis: &LoweringAnalysis<'_>,
    symbol: &str,
    wrapper: &str,
    may_stop: bool,
) {
    let ret = match &task_fn.return_type {
        Type::Alternative(alternatives) if integer_or_error(alternatives) => "i32",
        Type::Alternative(alternatives) if float_or_error(alternatives) => "double",
        Type::Alternative(alternatives) if string_or_error(alternatives) => "ptr",
        Type::Alternative(alternatives) if boolean_or_error(alternatives) => "i1",
        _ => function_return_llvm(&task_fn.return_type).unwrap_or("void"),
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
    let header = crate::ir::LlvmFunction::new_definition(wrapper, T::I32, params).header();
    let _ = writeln!(text, "\n{header}");
    let aggregate_return = matches!(&task_fn.return_type, Type::Alternative(alternatives)
        if void_or_error(alternatives)
            || integer_or_error(alternatives)
            || float_or_error(alternatives)
            || string_or_error(alternatives)
            || boolean_or_error(alternatives));
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
        let load_raw =
            |text: &mut String, ty: T| text.assign(raw.to_string(), I::load(ty, payload.clone()));
        match parameter_ty {
            Type::Boolean => {
                load_raw(text, T::I64);
                text.assign(&arg, I::icmp(ICmpCond::Ne, T::I64, raw.clone(), O::int(0)));
            }
            Type::Integer(_) if llvm_ty == "i64" => text.assign(&arg, I::load(T::I64, payload)),
            Type::Integer(_) => {
                load_raw(text, T::I64);
                let narrow = I::cast(CastOp::Trunc, T::I64, raw.clone(), typed_llvm(llvm_ty));
                text.assign(&arg, narrow);
            }
            Type::Float(_) if llvm_ty == "float" => {
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
        dispatch_arguments.push((typed_llvm(llvm_ty), O::reg(arg)));
    }
    let stop_check = |text: &mut String| {
        if may_stop {
            super::arc_runtime::stop_exit_check(text);
        }
    };
    let payload_at = |base: O| I::gep(T::I8, base, vec![(T::I64, O::int(8))]);
    let store = |text: &mut String, ty: T, value: O, slot: O| text.emit(I::store(ty, value, slot));
    if ret == "void" {
        text.emit(I::call(T::Void, symbol, dispatch_arguments));
        stop_check(text);
    } else {
        // ASYNC_RETURN_TYPE: a typed task returns `T OR Error`.
        assert!(aggregate_return, "validated ASYNC return type");
        let aggregate = typed_llvm(function_return_llvm(&task_fn.return_type).unwrap_or(ret));
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
            "double" => 3,
            "ptr" => 4,
            "i1" => 1,
            _ if matches!(&task_fn.return_type, Type::Alternative(alternatives) if void_or_error(alternatives)) => {
                0
            }
            _ => 2,
        };
        store(text, T::I32, O::int(kind), r("result"));
        text.assign("dispatch_value", field(2));
        text.assign("dispatch_payload", payload_at(r("result")));
        let payload = r("dispatch_payload");
        if kind == 0 {
            store(text, T::I64, O::int(0), payload);
        } else if ret == "ptr" {
            text.assign("dispatch_string", field(1));
            store(text, T::Ptr, r("dispatch_string"), payload.clone());
            text.assign("dispatch_string_length", payload_at(payload));
            store(text, T::I32, O::int(0), r("dispatch_string_length"));
        } else if ret == "i1" {
            text.assign("dispatch_bool64", field(2));
            store(text, T::I64, r("dispatch_bool64"), payload);
        } else if ret == "double" {
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
    text.push_str("}\n");
}
