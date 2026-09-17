#![allow(
    clippy::wildcard_imports,
    clippy::match_same_arms,
    clippy::too_many_lines
)]
use super::*;
use std::collections::HashSet;

/// LLVM symbol for a BN function name. The entry name is fixed by the
/// language (`FUNCTION Start`), so mapping it to `main` is spec, not a
/// lowering convention.
pub(crate) fn llvm_function_symbol(name: &str) -> String {
    if name == bn_ir::names::ENTRY {
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
    if function_name == bn_ir::names::ENTRY {
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
        if function.kind != FunctionKind::Entry {
            validate_user_function(function)?;
        }
        let analysis = analyze_function(module, function, &module_functions)?;
        for block in &function.blocks {
            for instruction in &block.instructions {
                match instruction {
                    Instruction::Constant {
                        value: Constant::Function(name),
                        ..
                    } => {
                        if let Some(callee_fn) = module
                            .functions
                            .iter()
                            .find(|candidate| candidate.name == *name)
                        {
                            stack.push(callee_fn);
                        }
                    }
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
                    Instruction::Release {
                        destructor: Some(destructor),
                        ..
                    } => {
                        if let Some(destructor_fn) = module
                            .functions
                            .iter()
                            .find(|candidate| candidate.name == *destructor)
                        {
                            stack.push(destructor_fn);
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
                    Instruction::Allocate { type_name, .. } => {
                        if let Some(destructor_fn) =
                            module.function_of_kind(FunctionKind::Destructor, type_name)
                        {
                            stack.push(destructor_fn);
                        }
                    }
                    Instruction::EnsureClass { class, .. } => {
                        if let Some(init_fn) = module.function_of_kind(FunctionKind::Init, class) {
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
            "TARGET_UNSUPPORTED_TYPE: function '{}' return type '{}' is unsupported",
            function.name,
            crate::display_type(&function.return_type)
        ));
    }
    Ok(())
}

pub(crate) fn emit_function(
    text: &mut String,
    module: &Module,
    function: &Function,
    analysis: &LoweringAnalysis<'_>,
    synchronize_prints: bool,
    policy: &crate::CompiledPolicy,
) -> Result<(), String> {
    let is_start = function.kind == FunctionKind::Entry;
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
        input_cleanup_count: 0,
        continuation_count: 0,
        control_flow: control_flow::EmittedControlFlow::default(),
        md_temp: 0,
        needs_numeric_overflow_trap: false,
        needs_bn_rt_trap: false,
        is_start,
        synchronize_prints,
        return_llvm: if is_start {
            "i32"
        } else {
            function_return_llvm(&function.return_type).expect("validated return type")
        },
    };
    let reachable_blocks = reachable_block_ids(function);
    for block in &function.blocks {
        if !reachable_blocks.contains(&block.id.0) {
            continue;
        }
        state.control_flow.label(text, format!("b{}", block.id.0));
        if block.id == function.entry {
            if is_start {
                let ceiling = super::policy_ceiling(module);
                if ceiling != 0 {
                    let _ = writeln!(text, "  call i32 @bn_rt_policy_init(i32 1, i64 {ceiling})");
                    if policy.sandboxed {
                        let _ = writeln!(text, "  call i32 @bn_rt_policy_filesystem_sandboxed()");
                        for (index, _) in policy.read_roots.iter().enumerate() {
                            let _ = writeln!(
                                text,
                                "  call i32 @bn_rt_policy_filesystem_root(i32 0, ptr @.bn_policy_root{index})"
                            );
                        }
                        let offset = policy.read_roots.len();
                        for (index, _) in policy.write_roots.iter().enumerate() {
                            let _ = writeln!(
                                text,
                                "  call i32 @bn_rt_policy_filesystem_root(i32 1, ptr @.bn_policy_root{})",
                                offset + index
                            );
                        }
                    }
                }
            }
            for (symbol, ty) in &analysis.symbols {
                let llvm_ty = llvm_type(ty).expect("validated alloca type");
                let _ = writeln!(text, "  %s{} = alloca {llvm_ty}", symbol_names[symbol]);
                if (matches!(ty, Type::Alternative(types) if types.iter().any(|item| matches!(item, Type::Null)))
                    || is_class_type(module, ty))
                    && llvm_ty == "ptr"
                {
                    let _ = writeln!(text, "  store ptr null, ptr %s{}", symbol_names[symbol]);
                }
                if is_region_type(ty) && !function.parameters.contains(symbol) {
                    // The first Store releases the "previous" region; a zeroed
                    // slot makes that a no-op instead of a garbage pointer.
                    let _ = writeln!(
                        text,
                        "  store {{ ptr, i32 }} zeroinitializer, ptr %s{}",
                        symbol_names[symbol]
                    );
                }
                if analysis.released_symbols.contains(symbol) {
                    let slot = symbol_names[symbol];
                    let _ = writeln!(text, "  %slive{slot} = alloca i1");
                    let _ = writeln!(text, "  store i1 false, ptr %slive{slot}");
                }
            }
            for symbol in &analysis.input_symbols {
                let slot = symbol_names[symbol];
                let _ = writeln!(
                    text,
                    "  %inputowned{slot} = alloca i1\n  store ptr null, ptr %s{slot}\n  store i1 false, ptr %inputowned{slot}"
                );
            }
            let mut owned_struct_results = analysis
                .owned_struct_results
                .iter()
                .copied()
                .collect::<Vec<_>>();
            owned_struct_results.sort_by_key(|value| value.0);
            for value in owned_struct_results {
                let _ = writeln!(
                    text,
                    "  %structowned{} = alloca ptr\n  store ptr null, ptr %structowned{}",
                    value.0, value.0
                );
            }
            let mut owned_objects = analysis
                .owned_object_results
                .keys()
                .copied()
                .collect::<Vec<_>>();
            owned_objects.sort_by_key(|value| value.0);
            for value in owned_objects {
                let _ = writeln!(
                    text,
                    "  %objectowned{} = alloca ptr\n  store ptr null, ptr %objectowned{}",
                    value.0, value.0
                );
            }
            let mut owned_log_results = analysis
                .owned_log_results
                .keys()
                .copied()
                .collect::<Vec<_>>();
            owned_log_results.sort_by_key(|value| value.0);
            for value in owned_log_results {
                let _ = writeln!(
                    text,
                    "  %logowned{} = alloca i64\n  store i64 0, ptr %logowned{}",
                    value.0, value.0
                );
            }
            for value in &analysis.multi_defs {
                let _ = writeln!(text, "  %sc{} = alloca i1", value.0);
            }
            if !is_start {
                store_parameters(text, function, analysis, &symbol_names);
                for symbol in &function.parameters {
                    if analysis.released_symbols.contains(symbol) {
                        let _ =
                            writeln!(text, "  store i1 true, ptr %slive{}", symbol_names[symbol]);
                    }
                }
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
        state.control_flow.finish_block(block.id);
        lower_terminator(
            text,
            module,
            function,
            &block.terminator,
            analysis,
            &symbol_names,
            &mut block_state,
            &mut state,
        );
    }
    emit_traps(text, module, function, analysis, &symbol_names, &mut state);
    text.push_str("}\n");
    state.control_flow.resolve(text)
}

fn reachable_block_ids(function: &Function) -> HashSet<u32> {
    let blocks = function
        .blocks
        .iter()
        .map(|block| (block.id.0, block))
        .collect::<HashMap<_, _>>();
    let mut reachable = HashSet::new();
    let mut pending = vec![function.entry.0];
    while let Some(id) = pending.pop() {
        if !reachable.insert(id) {
            continue;
        }
        let Some(block) = blocks.get(&id) else {
            continue;
        };
        match &block.terminator {
            Terminator::Jump { target } => pending.push(target.0),
            Terminator::Branch {
                then_block,
                else_block,
                ..
            } => {
                pending.push(then_block.0);
                pending.push(else_block.0);
            }
            Terminator::Return { .. } | Terminator::Stop { .. } => {}
        }
    }
    reachable
}

fn emit_traps(
    text: &mut String,
    module: &Module,
    function: &Function,
    analysis: &LoweringAnalysis<'_>,
    symbols: &HashMap<SymbolId, usize>,
    state: &mut EmissionState,
) {
    if state.needs_numeric_overflow_trap {
        if state.is_start {
            text.push_str("trap_numeric_overflow:\n");
            cleanup_owned_memory(text, module, function, analysis, symbols, None, state);
            text.push_str("  ret i32 1\n");
        } else {
            text.push_str("trap_numeric_overflow:\n  call void @exit(i32 1)\n  unreachable\n");
        }
    }
    if state.needs_bn_rt_trap {
        if state.is_start {
            text.push_str("trap_bn_rt:\n");
            cleanup_owned_memory(text, module, function, analysis, symbols, None, state);
            text.push_str("  ret i32 1\n");
        } else {
            text.push_str("trap_bn_rt:\n  call void @exit(i32 1)\n  unreachable\n");
        }
    }
}
