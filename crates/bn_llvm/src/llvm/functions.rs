// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

// One emitted LLVM function per BN function: symbols, reachability,
// support validation, the function body, and its trap blocks.
#![allow(
    clippy::wildcard_imports,
    clippy::match_same_arms,
    clippy::too_many_lines
)]
use super::*;
use crate::ir::{InstSink as _, LlvmInst as I, LlvmOperand as O, LlvmType as T};
use crate::layout::{typed_llvm, vector_ty};
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
                    // The destruction of an allocated object runs its
                    // destructor and its field release (`arc_runtime`).
                    Instruction::Allocate { type_name, .. } => {
                        for kind in [FunctionKind::Destructor, FunctionKind::ReleaseFields] {
                            if let Some(function) = module.function_of_kind(kind, type_name) {
                                stack.push(function);
                            }
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
    debug: bool,
) -> Result<(), String> {
    let is_start = function.kind == FunctionKind::Entry;
    // Hash maps iterate in a random order; slots, allocas and flags follow
    // id order so the same program always lowers to the same LLVM text.
    let mut symbols = analysis.symbols.iter().collect::<Vec<_>>();
    symbols.sort_by_key(|(symbol, _)| symbol.0);
    let symbol_names = symbols
        .iter()
        .enumerate()
        .map(|(index, (symbol, _))| (**symbol, index))
        .collect::<HashMap<_, _>>();
    if debug {
        mark_function(text, function.span, &function.name);
    }
    if is_start {
        let params = vec![(T::I32, "argc".to_string()), (T::Ptr, "argv".to_string())];
        let main = crate::ir::LlvmFunction::new_definition("main", T::I32, params);
        let _ = writeln!(text, "\n{}", main.header());
    } else {
        emit_user_signature(text, function, analysis);
    }
    let mut state = EmissionState {
        print_count: 0,
        input_cleanup_count: 0,
        continuation_count: 0,
        control_flow: control_flow::EmittedControlFlow::default(),
        md_temp: 0,
        needs_numeric_overflow_trap: false,
        needs_bn_rt_trap: false,
        span: traps::unknown_span(),
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
                crate::platform_stdio::emit_windows_binary_stdio(text, synchronize_prints);
                // Every native program applies the environment policy, so a
                // malformed input stops it (CONFIG_INVALID, exit 2) as it
                // stops `bni`, imports or not (0.6.md).
                let ceiling = super::policy_ceiling(module);
                if ceiling != 0 || synchronize_prints {
                    let args = vec![(T::I32, O::int(1)), (T::I64, O::uint(ceiling))];
                    let init = I::call(T::I32, "bn_rt_policy_init", args);
                    text.assign("bn_policy_status", init);
                    let status = vec![(T::I32, O::reg("bn_policy_status"))];
                    text.emit(I::call(T::Void, "bn_rt_policy_check", status));
                    if policy.sandboxed {
                        let sandbox = I::call(T::I32, "bn_rt_policy_filesystem_sandboxed", vec![]);
                        text.emit(sandbox);
                        // Read roots (mode 0) come first, then write roots (mode 1).
                        let modes = policy
                            .read_roots
                            .iter()
                            .map(|_| 0)
                            .chain(policy.write_roots.iter().map(|_| 1));
                        for (index, mode) in modes.enumerate() {
                            let root = O::global(format!(".bn_policy_root{index}"));
                            let args = vec![(T::I32, O::int(mode)), (T::Ptr, root)];
                            text.emit(I::call(T::I32, "bn_rt_policy_filesystem_root", args));
                        }
                    }
                }
            }
            // Each call site receives a vector result into storage of its own.
            for instruction in function.blocks.iter().flat_map(|block| &block.instructions) {
                if let Instruction::Call {
                    destination, ty, ..
                } = instruction
                    && let Some((array, _)) =
                        vectors::returned_vector(ty).and_then(vectors::fixed_vector_array)
                {
                    text.assign(
                        format!("vret{}", destination.0),
                        I::alloca(typed_llvm(&array)),
                    );
                }
            }
            for &(symbol, ty) in &symbols {
                let llvm_ty = llvm_type(ty).expect("validated alloca type");
                let slot = O::reg(format!("s{}", symbol_names[symbol]));
                text.assign(slot.to_string(), I::alloca(typed_llvm(llvm_ty)));
                vectors::emit_vector_storage(
                    text,
                    symbol_names[symbol],
                    ty,
                    arc_ops::holds_references(module, ty),
                );
                if (matches!(ty, Type::Alternative(types) if types.iter().any(|item| matches!(item, Type::Null)))
                    || is_class_type(module, ty))
                    && llvm_ty == "ptr"
                {
                    text.emit(I::store(T::Ptr, O::null(), slot.clone()));
                }
                if general_alternative(ty).is_some()
                    && object_class(module, ty).is_some()
                    && !function.parameters.contains(symbol)
                {
                    // Tag 0 holds no object, so the first Store releases nothing.
                    let layout = typed_llvm(GENERAL_LAYOUT);
                    text.emit(I::store(layout, O::zero_initializer(), slot.clone()));
                }
                if is_region_type(ty) && !function.parameters.contains(symbol) {
                    // The first Store releases the "previous" region; a zeroed
                    // slot makes that a no-op instead of a garbage pointer.
                    text.emit(I::store(vector_ty(), O::zero_initializer(), slot.clone()));
                }
            }
            // Whether each binding a RELEASE ends is live, by symbol (a
            // parameter only released has no value slot).
            let mut released = analysis.released_symbols.iter().collect::<Vec<_>>();
            released.sort_by_key(|symbol| symbol.0);
            for symbol in released {
                let live = live_flag(*symbol);
                let initial = function.parameters.contains(symbol) && !is_start;
                emit_flag_slot(text, &live, T::I1, O::bool(initial));
            }
            let mut input_symbols = analysis.input_symbols.iter().collect::<Vec<_>>();
            input_symbols.sort_by_key(|symbol| symbol.0);
            for symbol in input_symbols {
                let slot = symbol_names[symbol];
                let owned = format!("inputowned{slot}");
                text.assign(&owned, I::alloca(T::I1));
                text.emit(I::store(T::Ptr, O::null(), O::reg(format!("s{slot}"))));
                text.emit(I::store(T::I1, O::bool(false), O::reg(owned)));
            }
            let mut owned_struct_results = analysis
                .owned_struct_results
                .iter()
                .copied()
                .collect::<Vec<_>>();
            owned_struct_results.sort_by_key(|value| value.0);
            for value in owned_struct_results {
                emit_flag_slot(text, &format!("structowned{}", value.0), T::Ptr, O::null());
            }
            let mut owned_log_results = analysis
                .owned_log_results
                .keys()
                .copied()
                .collect::<Vec<_>>();
            owned_log_results.sort_by_key(|value| value.0);
            for value in owned_log_results {
                emit_flag_slot(text, &format!("logowned{}", value.0), T::I64, O::int(0));
            }
            let mut multi_defs = analysis.multi_defs.iter().collect::<Vec<_>>();
            multi_defs.sort_by_key(|value| value.0);
            for value in multi_defs {
                text.assign(format!("sc{}", value.0), I::alloca(T::I1));
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
            state.span = instruction.span();
            if debug {
                mark_location(text, state.span, function.span);
            }
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
    if debug {
        mark_location(text, function.span, function.span);
    }
    emit_traps(text, analysis, &symbol_names, &mut state);
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
    analysis: &LoweringAnalysis<'_>,
    symbols: &HashMap<SymbolId, usize>,
    state: &mut EmissionState,
) {
    let exits = [
        (state.needs_numeric_overflow_trap, "trap_numeric_overflow"),
        (state.needs_bn_rt_trap, "trap_bn_rt"),
    ];
    for (needed, label) in exits {
        if !needed {
            continue;
        }
        text.label(label);
        if state.is_start {
            cleanup_owned_memory(text, analysis, symbols, state);
            text.emit(I::Ret {
                val: Some((T::I32, O::int(1))),
            });
        } else {
            text.emit(I::call(T::Void, "exit", vec![(T::I32, O::int(1))]));
            text.emit(I::Unreachable);
        }
    }
}

/// The flag that says the binding `symbol`, which a `RELEASE` ends, is live.
pub(crate) fn live_flag(symbol: SymbolId) -> String {
    format!("%slive.sym{}", symbol.0)
}

/// `%{name} = alloca ty` holding `initial`.
fn emit_flag_slot(text: &mut String, name: &str, ty: T, initial: O) {
    text.assign(name, I::alloca(ty.clone()));
    text.emit(I::store(ty, initial, O::reg(name)));
}
