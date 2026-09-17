#![allow(clippy::wildcard_imports, clippy::too_many_lines)]
use super::*;

pub(crate) fn emit_user_signature(
    text: &mut String,
    function: &Function,
    analysis: &LoweringAnalysis<'_>,
) -> Result<(), String> {
    let ret = function_return_llvm(&function.return_type).expect("validated return type");
    let mut params = Vec::new();
    for (index, symbol) in function.parameters.iter().enumerate() {
        let ty = parameter_type(function, *symbol, analysis).ok_or_else(|| {
            format!(
                "TARGET_UNSUPPORTED_TYPE: function '{}' parameter {index} has no LLVM type",
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

pub(crate) fn store_parameters(
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
        let operand = if param_llvm == "{ ptr, i32 }"
            && llvm_type(arg_ty) == Some("{ i1, ptr, i32 }")
        {
            let tag = state.continuation_count;
            state.continuation_count += 1;
            let _ = writeln!(
                text,
                "  %call_endpoint_ptr{tag} = extractvalue {{ i1, ptr, i32 }} %v{}, 1",
                argument.0
            );
            let _ = writeln!(
                text,
                "  %call_endpoint_port{tag} = extractvalue {{ i1, ptr, i32 }} %v{}, 2",
                argument.0
            );
            let _ = writeln!(
                text,
                "  %call_endpoint{tag}_0 = insertvalue {{ ptr, i32 }} undef, ptr %call_endpoint_ptr{tag}, 0"
            );
            let _ = writeln!(
                text,
                "  %call_endpoint{tag} = insertvalue {{ ptr, i32 }} %call_endpoint{tag}_0, i32 %call_endpoint_port{tag}, 1"
            );
            format!("%call_endpoint{tag}")
        } else if param_llvm == "i1" && llvm_type(arg_ty) == Some("{ i1, ptr, i64 }") {
            let tag = format!("callbool{}_{}", destination.0, argument.0);
            let _ = writeln!(
                text,
                "  %{tag}raw = extractvalue {{ i1, ptr, i64 }} %v{}, 2",
                argument.0
            );
            let _ = writeln!(text, "  %{tag} = trunc i64 %{tag}raw to i1");
            format!("%{tag}")
        } else if param_llvm == "i1" {
            i1_operand(text, analysis, state, *argument)
        } else if param_llvm == "ptr" && llvm_type(arg_ty) == Some("ptr") {
            format!("%v{}", argument.0)
        } else if param_llvm == "ptr" && llvm_type(arg_ty) == Some("{ i1, ptr, i64 }") {
            let tag = format!("callstr{}_{}", destination.0, argument.0);
            let _ = writeln!(
                text,
                "  %{tag}msg = extractvalue {{ i1, ptr, i64 }} %v{}, 1",
                argument.0
            );
            let _ = writeln!(
                text,
                "  %{tag}raw = extractvalue {{ i1, ptr, i64 }} %v{}, 2",
                argument.0
            );
            let _ = writeln!(text, "  %{tag}payload = inttoptr i64 %{tag}raw to ptr");
            let _ = writeln!(text, "  %{tag}hasmsg = icmp ne ptr %{tag}msg, null");
            let _ = writeln!(
                text,
                "  %{tag} = select i1 %{tag}hasmsg, ptr %{tag}msg, ptr %{tag}payload"
            );
            format!("%{tag}")
        } else if llvm_type(arg_ty) == Some("{ i1, ptr, i64 }")
            && matches!(param_llvm, "i32" | "i64" | "i1" | "double")
        {
            let tag = format!("callunion{}_{}", destination.0, argument.0);
            let _ = writeln!(
                text,
                "  %{tag}raw = extractvalue {{ i1, ptr, i64 }} %v{}, 2",
                argument.0
            );
            if param_llvm == "double" {
                let _ = writeln!(text, "  %{tag} = bitcast i64 %{tag}raw to double");
            } else if param_llvm == "i1" {
                let _ = writeln!(text, "  %{tag} = trunc i64 %{tag}raw to i1");
            } else if param_llvm == "i32" {
                let _ = writeln!(text, "  %{tag} = trunc i64 %{tag}raw to i32");
            } else {
                let _ = writeln!(text, "  %{tag} = add i64 %{tag}raw, 0");
            }
            format!("%{tag}")
        } else {
            coerce_to_type(text, *argument, arg_ty, param_ty)
        };
        args.push(format!("{param_llvm} {operand}"));
    }
    let args_joined = args.join(", ");
    let method = resolved.rsplit('.').next().unwrap_or(resolved);
    let virtualish = !is_super
        && !matches!(
            module.kind_of(resolved),
            Some(FunctionKind::Constructor | FunctionKind::FieldInit | FunctionKind::Init)
        )
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
        if analysis.owned_struct_results.contains(&destination) {
            let _ = writeln!(
                text,
                "  store ptr %v{}, ptr %structowned{}",
                destination.0, destination.0
            );
        }
    }
    for argument in arguments {
        if analysis.owned_string_results.contains(argument) {
            let _ = writeln!(text, "  call void @free(ptr %v{})", argument.0);
        }
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
        state.control_flow.label(text, label.clone());
        let symbol = llvm_function_symbol(candidate);
        if ret == "void" {
            let _ = writeln!(text, "  call void @{symbol}({args})");
        } else {
            let _ = writeln!(text, "  %vtmp{n}_{index} = call {ret} @{symbol}({args})");
            incoming.push(format!("[ %vtmp{n}_{index}, %{label} ]"));
        }
        let _ = writeln!(text, "  br label %{join}");
        if index + 1 != overrides.len() {
            state.control_flow.label(text, next.clone());
        }
    }
    state.control_flow.label(text, fallback_label.clone());
    let fallback_symbol = llvm_function_symbol(fallback);
    if ret == "void" {
        let _ = writeln!(text, "  call void @{fallback_symbol}({args})");
        let _ = writeln!(text, "  br label %{join}");
        state.control_flow.label(text, join.clone());
    } else {
        let _ = writeln!(text, "  %vfb{n} = call {ret} @{fallback_symbol}({args})");
        incoming.push(format!("[ %vfb{n}, %{fallback_label} ]"));
        let _ = writeln!(text, "  br label %{join}");
        state.control_flow.label(text, join.clone());
        let _ = writeln!(
            text,
            "  %v{} = phi {ret} {}",
            destination.0,
            incoming.join(", ")
        );
    }
}
