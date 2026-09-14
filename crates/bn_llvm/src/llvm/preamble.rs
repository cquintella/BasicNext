#![allow(clippy::wildcard_imports, clippy::too_many_lines)]
use super::*;
pub(crate) fn emit_preamble(
    text: &mut String,
    functions: &[(&Function, LoweringAnalysis<'_>)],
    synchronize_prints: bool,
    needs_na: bool,
) {
    let uses_arc = functions
        .iter()
        .any(|(_, analysis)| !analysis.owned_object_results.is_empty());
    if uses_arc {
        text.push_str(super::arc::runtime_ir());
    }
    let mut uses_concat = false;
    let mut uses_bn_rt = false;
    let mut uses_input = false;
    let mut uses_string_sizeof = false;
    let mut uses_exit = false;
    let mut intrinsics = BTreeSet::new();
    for (function, analysis) in functions {
        uses_concat |= analysis.uses_string_concat;
        uses_bn_rt |= analysis.uses_bn_rt;
        uses_input |= analysis.input_count > 0;
        uses_string_sizeof |= analysis.uses_string_sizeof;
        uses_exit |= function.name != "Start"
            && (analysis.uses_bn_rt
                || function.blocks.iter().any(|block| {
                    matches!(block.terminator, Terminator::Stop { .. })
                        || matches!(
                            block.terminator,
                            Terminator::Return { .. }
                                if function.name != "Start"
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
    if functions.iter().any(|(function, analysis)| {
        !analysis.released_symbols.is_empty()
            || function.blocks.iter().any(|block| {
                block.instructions.iter().any(|instruction| {
                    matches!(instruction, Instruction::Member { owner, .. } if owner != "Error")
                })
            })
    }) {
        text.push_str(
            "@.bn_use_after_release = private constant [41 x i8] c\"USE_AFTER_RELEASE: binding was released\\0A\\00\"\n@.bn_double_release = private constant [46 x i8] c\"DOUBLE_RELEASE: binding was already released\\0A\\00\"\n",
        );
    }
    if synchronize_prints {
        // Darwin/FreeBSD: __stdoutp; Linux/glibc: stdout.
        let stdout_sym = crate::helpers::stdout_file_symbol();
        let _ = write!(
            text,
            "@{stdout_sym} = external global ptr\ndeclare void @flockfile(ptr)\ndeclare void @funlockfile(ptr)\n"
        );
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
        text.push_str("declare void @bn_rt_print_float(double)\n");
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
                    || matches!(ty, Type::Alternative(types) if string_na_or_error(types) || scalar_na_or_error(types))
            })
        })
    {
        text.push_str("@.bn_na = private unnamed_addr constant [3 x i8] c\"NA\\00\"\n");
    }
    if functions.iter().any(|(_, analysis)| {
        analysis.values.values().any(|ty| {
            llvm_type(ty) == Some("{ i1, i32 }")
                || matches!(ty, Type::Alternative(types) if void_or_error(types))
        })
    }) {
        text.push_str("@.bn_null = private constant [5 x i8] c\"NULL\\00\"\n");
    }
    if functions
        .iter()
        .any(|(_, analysis)| analysis.uses_string_ops)
    {
        text.push_str(
            "declare i32 @bn_rt_str_len(ptr)\ndeclare i64 @bn_rt_str_index_utf8(ptr, i32)\ndeclare i32 @bn_rt_str_eq(ptr, ptr)\n",
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
                            static_globals.insert((class.clone(), field.clone(), llvm_ty));
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
    for (class, field, llvm_ty) in &static_globals {
        let gclass = sanitize_symbol(class);
        let gfield = sanitize_symbol(field);
        let _ = writeln!(
            text,
            "@bn_st_{gclass}_{gfield} = global {llvm_ty} {}",
            match *llvm_ty {
                "i1" => "false",
                "float" | "double" => "0.0",
                "ptr" => "null",
                _ => "0",
            }
        );
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
        let ret = match &task_fn.return_type {
            Type::Alternative(alternatives) if integer_or_error(alternatives) => "i32",
            Type::Alternative(alternatives) if float_or_error(alternatives) => "double",
            Type::Alternative(alternatives) if string_or_error(alternatives) => "ptr",
            Type::Alternative(alternatives) if boolean_or_error(alternatives) => "i1",
            _ => function_return_llvm(&task_fn.return_type).unwrap_or("void"),
        };
        let _ = writeln!(
            text,
            "\ndefine i32 @{wrapper}(ptr %context, ptr %arguments, i32 %argument_count, ptr %result, ptr %error) {{"
        );
        let aggregate_return = matches!(&task_fn.return_type, Type::Alternative(alternatives)
            if void_or_error(alternatives)
                || integer_or_error(alternatives)
                || float_or_error(alternatives)
                || string_or_error(alternatives)
                || boolean_or_error(alternatives));
        let mut dispatch_arguments = Vec::new();
        for (index, parameter) in task_fn.parameters.iter().enumerate() {
            let parameter_ty = &task_analysis.symbols[parameter];
            let llvm_ty = llvm_type(parameter_ty).expect("validated dispatch parameter type");
            let offset = index * 16 + 8;
            let _ = writeln!(
                text,
                "  %dispatch_arg_payload{index} = getelementptr i8, ptr %arguments, i64 {offset}"
            );
            match parameter_ty {
                Type::Boolean => {
                    let _ = writeln!(
                        text,
                        "  %dispatch_arg_raw{index} = load i64, ptr %dispatch_arg_payload{index}"
                    );
                    let _ = writeln!(
                        text,
                        "  %dispatch_arg{index} = icmp ne i64 %dispatch_arg_raw{index}, 0"
                    );
                }
                Type::Integer(_) if llvm_ty == "i64" => {
                    let _ = writeln!(
                        text,
                        "  %dispatch_arg{index} = load i64, ptr %dispatch_arg_payload{index}"
                    );
                }
                Type::Integer(_) => {
                    let _ = writeln!(
                        text,
                        "  %dispatch_arg_raw{index} = load i64, ptr %dispatch_arg_payload{index}"
                    );
                    let _ = writeln!(
                        text,
                        "  %dispatch_arg{index} = trunc i64 %dispatch_arg_raw{index} to {llvm_ty}"
                    );
                }
                Type::Float(_) => {
                    if llvm_ty == "float" {
                        let _ = writeln!(
                            text,
                            "  %dispatch_arg_raw{index} = load double, ptr %dispatch_arg_payload{index}"
                        );
                        let _ = writeln!(
                            text,
                            "  %dispatch_arg{index} = fptrunc double %dispatch_arg_raw{index} to float"
                        );
                    } else {
                        let _ = writeln!(
                            text,
                            "  %dispatch_arg{index} = load double, ptr %dispatch_arg_payload{index}"
                        );
                    }
                }
                Type::String => {
                    let _ = writeln!(
                        text,
                        "  %dispatch_arg{index} = load ptr, ptr %dispatch_arg_payload{index}"
                    );
                }
                _ => unreachable!("validated dispatch parameter type"),
            }
            dispatch_arguments.push(format!("{llvm_ty} %dispatch_arg{index}"));
        }
        let dispatch_arguments = dispatch_arguments.join(", ");
        if ret == "void" {
            let _ = writeln!(text, "  call void @{symbol}({dispatch_arguments})");
        } else if aggregate_return {
            let aggregate = function_return_llvm(&task_fn.return_type).unwrap_or(ret);
            let _ = writeln!(
                text,
                "  %dispatch_raw = call {aggregate} @{symbol}({dispatch_arguments})"
            );
            let _ = writeln!(
                text,
                "  %dispatch_is_error = extractvalue {aggregate} %dispatch_raw, 0"
            );
            let _ = writeln!(
                text,
                "  br i1 %dispatch_is_error, label %dispatch_error, label %dispatch_success"
            );
            text.push_str("dispatch_error:\n");
            let _ = writeln!(
                text,
                "  %dispatch_error_message = extractvalue {aggregate} %dispatch_raw, 1"
            );
            let _ = writeln!(
                text,
                "  %dispatch_error_code64 = extractvalue {aggregate} %dispatch_raw, 2"
            );
            text.push_str(
                "  %dispatch_error_code = trunc i64 %dispatch_error_code64 to i32\n  store i32 %dispatch_error_code, ptr %error\n  %dispatch_error_message_slot = getelementptr i8, ptr %error, i64 8\n  store ptr %dispatch_error_message, ptr %dispatch_error_message_slot\n  ret i32 1\ndispatch_success:\n",
            );
            let kind = if ret == "double" {
                3
            } else if ret == "ptr" {
                4
            } else if ret == "i1" {
                1
            } else if matches!(&task_fn.return_type, Type::Alternative(alternatives) if void_or_error(alternatives))
            {
                0
            } else {
                2
            };
            let _ = writeln!(text, "  store i32 {kind}, ptr %result");
            let _ = writeln!(
                text,
                "  %dispatch_value = extractvalue {aggregate} %dispatch_raw, 2"
            );
            let _ = writeln!(
                text,
                "  %dispatch_payload = getelementptr i8, ptr %result, i64 8"
            );
            if kind == 0 {
                let _ = writeln!(text, "  store i64 0, ptr %dispatch_payload");
            } else if ret == "ptr" {
                let _ = writeln!(
                    text,
                    "  %dispatch_string = extractvalue {aggregate} %dispatch_raw, 1"
                );
                let _ = writeln!(text, "  store ptr %dispatch_string, ptr %dispatch_payload");
                let _ = writeln!(
                    text,
                    "  %dispatch_string_length = getelementptr i8, ptr %dispatch_payload, i64 8"
                );
                let _ = writeln!(text, "  store i32 0, ptr %dispatch_string_length");
            } else if ret == "i1" {
                let _ = writeln!(
                    text,
                    "  %dispatch_bool64 = extractvalue {aggregate} %dispatch_raw, 2"
                );
                let _ = writeln!(text, "  store i64 %dispatch_bool64, ptr %dispatch_payload");
            } else if ret == "double" {
                let _ = writeln!(
                    text,
                    "  %dispatch_float = bitcast i64 %dispatch_value to double"
                );
                let _ = writeln!(
                    text,
                    "  store double %dispatch_float, ptr %dispatch_payload"
                );
            } else {
                let _ = writeln!(text, "  store i64 %dispatch_value, ptr %dispatch_payload");
            }
        } else {
            let _ = writeln!(
                text,
                "  %dispatch_value = call {ret} @{symbol}({dispatch_arguments})"
            );
            let kind = if ret == "double" { 3 } else { 2 };
            let _ = writeln!(text, "  store i32 {kind}, ptr %result");
            if ret == "double" {
                let _ = writeln!(
                    text,
                    "  store double %dispatch_value, ptr getelementptr (i8, ptr %result, i64 8)"
                );
            } else if ret == "i32" {
                let _ = writeln!(text, "  %dispatch_i64 = sext i32 %dispatch_value to i64");
                let _ = writeln!(
                    text,
                    "  store i64 %dispatch_i64, ptr getelementptr (i8, ptr %result, i64 8)"
                );
            } else {
                let _ = writeln!(
                    text,
                    "  store i64 %dispatch_value, ptr getelementptr (i8, ptr %result, i64 8)"
                );
            }
        }
        text.push_str("  ret i32 0\n}\n");
    }
    let _ = uses_exit;
}
