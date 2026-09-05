#![allow(
    clippy::wildcard_imports,
    clippy::match_same_arms,
    clippy::too_many_lines
)]
use super::*;
use std::collections::HashSet;

pub(crate) fn llvm_function_symbol(name: &str) -> String {
    if name == "Start" {
        return "main".into();
    }
    let mut symbol = String::from("bn_");
    for byte in name.bytes() {
        if byte.is_ascii_alphanumeric() || byte == b'_' {
            symbol.push(byte as char);
        } else {
            let _ = write!(symbol, "_{byte:02x}");
        }
    }
    symbol
}

pub(crate) fn dispatch_trampoline_symbol(name: &str) -> String {
    format!("bn_dispatch_trampoline_{}", llvm_function_symbol(name))
}

pub(crate) fn string_global(function_name: &str, value: u32) -> String {
    if function_name == "Start" {
        format!("@.bn_str{value}")
    } else {
        format!("@.bn_str_{}_{value}", llvm_function_symbol(function_name))
    }
}

pub(crate) fn is_void_type(ty: &Type) -> bool {
    matches!(ty, Type::Named(name) if name == "VOID")
}

pub(crate) fn function_return_llvm(ty: &Type) -> Option<&'static str> {
    if is_void_type(ty) {
        Some("void")
    } else {
        llvm_type(ty)
    }
}

pub(crate) fn analyze_reachable<'a>(
    module: &'a Module,
    start: &'a Function,
) -> Result<Vec<(&'a Function, LoweringAnalysis<'a>)>, String> {
    let module_functions = module
        .functions
        .iter()
        .map(|function| function.name.as_str())
        .collect::<HashSet<_>>();
    let mut analyzed = HashMap::<&str, LoweringAnalysis<'a>>::new();
    let mut stack = vec![start];
    while let Some(function) = stack.pop() {
        if analyzed.contains_key(function.name.as_str()) {
            continue;
        }
        if function.name != "Start" {
            validate_user_function(function)?;
        }
        let analysis = analyze_function(module, function, &module_functions)?;
        for block in &function.blocks {
            for instruction in &block.instructions {
                match instruction {
                    Instruction::Call { callee, .. } => {
                        if let Some(name) = analysis.functions.get(callee).copied() {
                            let resolved = name.strip_prefix("@super:").unwrap_or(name);
                            if let Some(callee_fn) = module
                                .functions
                                .iter()
                                .find(|candidate| candidate.name == resolved)
                            {
                                stack.push(callee_fn);
                            }
                            // Virtual call sites may need every override of the method.
                            if let Some(method) = resolved.rsplit('.').next() {
                                for candidate in &module.functions {
                                    if candidate.name.ends_with(&format!(".{method}"))
                                        && !candidate.name.contains('$')
                                    {
                                        stack.push(candidate);
                                    }
                                }
                            }
                        }
                    }
                    Instruction::DispatchSubmit { task, .. } => {
                        if let Some(name) = analysis.functions.get(task).copied()
                            && let Some(callee_fn) = module
                                .functions
                                .iter()
                                .find(|candidate| candidate.name == name)
                        {
                            stack.push(callee_fn);
                        }
                    }
                    Instruction::EnsureClass { class, .. } => {
                        let init_name = format!("{class}.$init");
                        if let Some(init_fn) = module
                            .functions
                            .iter()
                            .find(|candidate| candidate.name == init_name)
                        {
                            stack.push(init_fn);
                        }
                    }
                    _ => {}
                }
            }
        }
        analyzed.insert(function.name.as_str(), analysis);
    }
    let mut ordered = Vec::new();
    for function in &module.functions {
        if let Some(analysis) = analyzed.remove(function.name.as_str()) {
            ordered.push((function, analysis));
        }
    }
    Ok(ordered)
}

fn validate_user_function(function: &Function) -> Result<(), String> {
    if function_return_llvm(&function.return_type).is_none() {
        return Err(format!(
            "BUILD_LOWERING_UNAVAILABLE: function '{}' return type '{}' is unsupported",
            function.name,
            crate::semantic::display(&function.return_type)
        ));
    }
    Ok(())
}

pub(crate) fn emit_preamble(
    text: &mut String,
    functions: &[(&Function, LoweringAnalysis<'_>)],
    synchronize_prints: bool,
) -> bool {
    let mut uses_concat = false;
    let mut uses_bn_rt = false;
    let mut uses_input = false;
    let mut uses_random = false;
    let mut uses_exit = false;
    let mut intrinsics = BTreeSet::new();
    for (function, analysis) in functions {
        uses_concat |= analysis.uses_string_concat;
        uses_bn_rt |= analysis.uses_bn_rt;
        uses_input |= analysis.input_count > 0;
        uses_random |= analysis.uses_random;
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
                "{global} = private unnamed_addr constant [{} x i8] c\"{}\\00\"",
                string.len() + 1,
                escape_llvm(string)
            );
        }
        intrinsics.extend(analysis.intrinsics.iter().copied());
    }
    let random_global = uses_random
        && functions
            .iter()
            .any(|(function, analysis)| function.name != "Start" && analysis.uses_random);
    if random_global {
        text.push_str("@bn_rng = global i64 1\n");
    }
    if uses_input {
        text.push_str(input_runtime_ir());
    }
    text.push_str("\ndeclare i32 @printf(ptr, ...)\ndeclare i32 @putchar(i32)\n");
    if synchronize_prints {
        text.push_str("@__stdoutp = external global ptr\ndeclare void @flockfile(ptr)\ndeclare void @funlockfile(ptr)\n");
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
    if functions.iter().any(|(_, analysis)| {
        analysis
            .values
            .values()
            .any(|ty| llvm_type(ty) == Some("{ i1, double }"))
    }) {
        text.push_str("@.bn_na = private unnamed_addr constant [3 x i8] c\"NA\\00\"\n");
    }
    if functions
        .iter()
        .any(|(_, analysis)| analysis.uses_string_ops)
    {
        text.push_str(
            "declare i32 @bn_rt_str_len(ptr)\ndeclare ptr @bn_rt_str_index(ptr, i32)\ndeclare i32 @bn_rt_str_eq(ptr, ptr)\n",
        );
    }
    if functions.iter().any(|(_, analysis)| analysis.uses_heap) {
        if !uses_concat {
            text.push_str("declare ptr @malloc(i64)\n");
        }
        text.push_str("declare void @free(ptr)\n");
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
    if functions
        .iter()
        .any(|(function, _)| function.name != "Start")
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
        let Some((task_fn, _)) = functions.iter().find(|(function, _)| function.name == task)
        else {
            continue;
        };
        let symbol = llvm_function_symbol(&task);
        let wrapper = dispatch_trampoline_symbol(&task);
        let ret = function_return_llvm(&task_fn.return_type).unwrap_or("void");
        let _ = writeln!(
            text,
            "\ndefine i32 @{wrapper}(ptr %context, ptr %arguments, i32 %argument_count, ptr %result, ptr %error) {{"
        );
        if ret == "void" {
            let _ = writeln!(text, "  call void @{symbol}()");
        } else {
            let _ = writeln!(text, "  %dispatch_value = call {ret} @{symbol}()");
        }
        text.push_str("  ret i32 0\n}\n");
    }
    let _ = uses_exit;
    random_global
}

pub(crate) fn emit_function(
    text: &mut String,
    module: &Module,
    function: &Function,
    analysis: &LoweringAnalysis<'_>,
    rng_global: bool,
    synchronize_prints: bool,
) -> Result<(), String> {
    let is_start = function.name == "Start";
    let symbol_names = analysis
        .symbols
        .keys()
        .enumerate()
        .map(|(index, symbol)| (*symbol, index))
        .collect::<HashMap<_, _>>();
    if is_start {
        text.push_str("\ndefine i32 @main(i32 %argc, ptr %argv) {\n");
    } else {
        emit_user_signature(text, function, analysis)?;
    }
    let mut state = EmissionState {
        print_count: 0,
        continuation_count: 0,
        md_temp: 0,
        needs_numeric_overflow_trap: false,
        needs_bn_rt_trap: false,
        is_start,
        rng_global,
        synchronize_prints,
        return_llvm: if is_start {
            "i32"
        } else {
            function_return_llvm(&function.return_type).expect("validated return type")
        },
    };
    for block in &function.blocks {
        let _ = writeln!(text, "b{}:", block.id.0);
        if block.id == function.entry {
            for (symbol, ty) in &analysis.symbols {
                let llvm_ty = llvm_type(ty).expect("validated alloca type");
                let _ = writeln!(text, "  %s{} = alloca {llvm_ty}", symbol_names[symbol]);
            }
            if analysis.uses_random && is_start && !rng_global {
                text.push_str("  %rng = alloca i64\n  store i64 1, ptr %rng\n");
            }
            for value in &analysis.multi_defs {
                let _ = writeln!(text, "  %sc{} = alloca i1", value.0);
            }
            if !is_start {
                store_parameters(text, function, analysis, &symbol_names);
            }
        }
        let mut block_state = BlockState {
            constants: HashMap::new(),
            bindings: HashMap::new(),
        };
        for instruction in &block.instructions {
            lower_scalar_instruction(
                text,
                module,
                function,
                block.id,
                instruction,
                analysis,
                &symbol_names,
                &mut block_state,
                &mut state,
            )?;
        }
        lower_terminator(
            text,
            &block.terminator,
            analysis,
            &mut block_state,
            &mut state,
        );
    }
    emit_traps(text, &state);
    text.push_str("}\n");
    Ok(())
}

fn emit_user_signature(
    text: &mut String,
    function: &Function,
    analysis: &LoweringAnalysis<'_>,
) -> Result<(), String> {
    let ret = function_return_llvm(&function.return_type).expect("validated return type");
    let mut params = Vec::new();
    for (index, symbol) in function.parameters.iter().enumerate() {
        let ty = parameter_type(function, *symbol, analysis).ok_or_else(|| {
            format!(
                "BUILD_LOWERING_UNAVAILABLE: function '{}' parameter {index} has no LLVM type",
                function.name
            )
        })?;
        params.push(format!("{ty} %p{index}"));
    }
    let _ = writeln!(
        text,
        "\ndefine {ret} @{}({}) {{",
        llvm_function_symbol(&function.name),
        params.join(", ")
    );
    Ok(())
}

fn store_parameters(
    text: &mut String,
    function: &Function,
    analysis: &LoweringAnalysis<'_>,
    symbol_names: &HashMap<SymbolId, usize>,
) {
    for (index, symbol) in function.parameters.iter().enumerate() {
        let Some(&slot) = symbol_names.get(symbol) else {
            continue;
        };
        let Some(ty) = parameter_type(function, *symbol, analysis) else {
            continue;
        };
        let _ = writeln!(text, "  store {ty} %p{index}, ptr %s{slot}");
    }
}

fn parameter_type<'a>(
    function: &'a Function,
    symbol: SymbolId,
    analysis: &'a LoweringAnalysis<'a>,
) -> Option<&'static str> {
    analysis
        .symbols
        .get(&symbol)
        .and_then(llvm_type)
        .or_else(|| {
            function
                .blocks
                .iter()
                .flat_map(|block| &block.instructions)
                .find_map(|instruction| match instruction {
                    Instruction::Load {
                        symbol: loaded, ty, ..
                    } if *loaded == symbol => llvm_type(ty),
                    Instruction::Store {
                        symbol: stored, ty, ..
                    } if *stored == symbol => llvm_type(ty),
                    _ => None,
                })
        })
        // Object/`SELF` parameters may be unused in the body (e.g. Animal.Speak).
        .or(Some("ptr"))
}

fn parameter_semantic_type(function: &Function, symbol: SymbolId) -> Option<&Type> {
    function
        .blocks
        .iter()
        .flat_map(|block| &block.instructions)
        .find_map(|instruction| match instruction {
            Instruction::Load {
                symbol: loaded, ty, ..
            } if *loaded == symbol => Some(ty),
            Instruction::Store {
                symbol: stored, ty, ..
            } if *stored == symbol => Some(ty),
            _ => None,
        })
}

pub(crate) fn lower_user_call(
    text: &mut String,
    module: &Module,
    destination: ValueId,
    name: &str,
    arguments: &[ValueId],
    analysis: &LoweringAnalysis<'_>,
    state: &mut EmissionState,
) {
    let is_super = name.starts_with("@super:");
    let resolved = name.strip_prefix("@super:").unwrap_or(name);
    let callee = module
        .functions
        .iter()
        .find(|function| function.name == resolved)
        .unwrap_or_else(|| panic!("validated user function {resolved}"));
    let ret = function_return_llvm(&callee.return_type).expect("validated user return type");
    let mut args = Vec::with_capacity(arguments.len());
    for (argument, param_symbol) in arguments.iter().zip(callee.parameters.iter()) {
        let arg_ty = analysis
            .values
            .get(argument)
            .expect("validated call argument type");
        let param_ty = parameter_semantic_type(callee, *param_symbol).unwrap_or(arg_ty);
        if llvm_type(param_ty) == Some("{ i1, double }")
            || llvm_type(arg_ty) == Some("{ i1, double }")
        {
            let temp = format!("callopt{}", argument.0);
            let _ = writeln!(
                text,
                "  %{temp} = extractvalue {{ i1, double }} %v{}, 1",
                argument.0
            );
            args.push(format!("double %{temp}"));
            continue;
        }
        let param_llvm = llvm_type(param_ty).unwrap_or("ptr");
        let operand = if param_llvm == "i1" {
            i1_operand(text, analysis, state, *argument)
        } else if param_llvm == "ptr" && llvm_type(arg_ty) == Some("ptr") {
            format!("%v{}", argument.0)
        } else {
            coerce_to_type(text, *argument, arg_ty, param_ty)
        };
        args.push(format!("{param_llvm} {operand}"));
    }
    let args_joined = args.join(", ");
    let method = resolved.rsplit('.').next().unwrap_or(resolved);
    let virtualish = !is_super
        && !resolved.ends_with(".CONSTRUCTOR")
        && !resolved.ends_with(".$fields")
        && !resolved.ends_with(".$init")
        && !arguments.is_empty()
        && analysis
            .values
            .get(&arguments[0])
            .is_some_and(|ty| llvm_type(ty) == Some("ptr"));
    if virtualish {
        let overrides: Vec<&str> = module
            .functions
            .iter()
            .map(|function| function.name.as_str())
            .filter(|candidate| {
                candidate.ends_with(&format!(".{method}")) && !candidate.contains('$')
            })
            .collect();
        if overrides.len() > 1 {
            emit_virtual_method_call(
                text,
                state,
                destination,
                resolved,
                &overrides,
                &args_joined,
                ret,
                arguments[0],
            );
            return;
        }
    }
    let symbol = llvm_function_symbol(resolved);
    if ret == "void" {
        let _ = writeln!(text, "  call void @{symbol}({args_joined})");
    } else {
        let _ = writeln!(
            text,
            "  %v{} = call {ret} @{symbol}({args_joined})",
            destination.0
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn emit_virtual_method_call(
    text: &mut String,
    state: &mut EmissionState,
    destination: ValueId,
    fallback: &str,
    overrides: &[&str],
    args: &str,
    ret: &str,
    receiver: ValueId,
) {
    let n = state.continuation_count;
    state.continuation_count += 1;
    let _ = writeln!(text, "  %vcls{n} = load ptr, ptr %v{}", receiver.0);
    let join = format!("vjoin{n}");
    let fallback_label = format!("vfallback{n}");
    let mut incoming = Vec::new();
    for (index, candidate) in overrides.iter().enumerate() {
        let class = candidate
            .rsplit_once('.')
            .map_or(*candidate, |(class, _)| class);
        let class_leaf = class.rsplit('.').next().unwrap_or(class);
        let label = format!("vcase{n}_{index}");
        let next = if index + 1 == overrides.len() {
            fallback_label.clone()
        } else {
            format!("vnext{n}_{index}")
        };
        let class_global = format!("@.bn_cls_{}", sanitize_symbol(class_leaf));
        let _ = writeln!(
            text,
            "  %veq{n}_{index} = call i32 @bn_rt_str_eq(ptr %vcls{n}, ptr {class_global})"
        );
        let _ = writeln!(text, "  %vhit{n}_{index} = icmp ne i32 %veq{n}_{index}, 0");
        let _ = writeln!(
            text,
            "  br i1 %vhit{n}_{index}, label %{label}, label %{next}"
        );
        let _ = writeln!(text, "{label}:");
        let symbol = llvm_function_symbol(candidate);
        if ret == "void" {
            let _ = writeln!(text, "  call void @{symbol}({args})");
        } else {
            let _ = writeln!(text, "  %vtmp{n}_{index} = call {ret} @{symbol}({args})");
            incoming.push(format!("[ %vtmp{n}_{index}, %{label} ]"));
        }
        let _ = writeln!(text, "  br label %{join}");
        if index + 1 != overrides.len() {
            let _ = writeln!(text, "{next}:");
        }
    }
    let _ = writeln!(text, "{fallback_label}:");
    let fallback_symbol = llvm_function_symbol(fallback);
    if ret == "void" {
        let _ = writeln!(text, "  call void @{fallback_symbol}({args})");
        let _ = writeln!(text, "  br label %{join}");
        let _ = writeln!(text, "{join}:");
    } else {
        let _ = writeln!(text, "  %vfb{n} = call {ret} @{fallback_symbol}({args})");
        incoming.push(format!("[ %vfb{n}, %{fallback_label} ]"));
        let _ = writeln!(text, "  br label %{join}");
        let _ = writeln!(text, "{join}:");
        let _ = writeln!(
            text,
            "  %v{} = phi {ret} {}",
            destination.0,
            incoming.join(", ")
        );
    }
}

fn emit_traps(text: &mut String, state: &EmissionState) {
    if state.needs_numeric_overflow_trap {
        if state.is_start {
            text.push_str("trap_numeric_overflow:\n  ret i32 1\n");
        } else {
            text.push_str("trap_numeric_overflow:\n  call void @exit(i32 1)\n  unreachable\n");
        }
    }
    if state.needs_bn_rt_trap {
        if state.is_start {
            text.push_str("trap_bn_rt:\n  ret i32 1\n");
        } else {
            text.push_str("trap_bn_rt:\n  call void @exit(i32 1)\n  unreachable\n");
        }
    }
}
