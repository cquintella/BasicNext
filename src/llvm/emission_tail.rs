#![allow(clippy::wildcard_imports)]
use super::*;
use crate::llvm::functions::dispatch_trampoline_symbol;

pub(crate) fn lower_scalar_instruction_tail(
    text: &mut String,
    module: &Module,
    function: &Function,
    block_id: BlockId,
    instruction: &Instruction,
    analysis: &LoweringAnalysis<'_>,
    symbols: &HashMap<SymbolId, usize>,
    block_state: &mut BlockState,
    state: &mut EmissionState,
) -> Result<(), String> {
    match instruction {
        Instruction::Call {
            destination,
            callee,
            arguments,
            ..
        } => match analysis
            .functions
            .get(callee)
            .copied()
            .expect("validated callee")
        {
            name if bnmath_method(module, name).is_some() => {
                lower_bnmath_call(
                    text,
                    *destination,
                    bnmath_method(module, name).expect("validated BNMath"),
                    arguments,
                    analysis,
                );
            }
            name if is_bn_rt_host_call(name) => {
                lower_bn_rt_call(
                    text,
                    block_id,
                    *destination,
                    name,
                    arguments,
                    analysis,
                    state,
                );
            }
            name if name.ends_with(".Queue.Concurrent")
                || name.ends_with(".Queue.Serial")
                || name.ends_with(".Queue.Auto")
                || name.ends_with(".Queue.Join")
                || name.ends_with(".Queue.Close")
                || name.ends_with(".Ticket.Close")
                || name.ends_with(".Group.New")
                || name.ends_with(".Group.Enter")
                || name.ends_with(".Group.Leave")
                || name.ends_with(".Group.Wait")
                || name.ends_with(".Barrier.New")
                || name.ends_with(".Barrier.Wait")
                || name.ends_with(".Semaphore.New")
                || name.ends_with(".Semaphore.Acquire")
                || name.ends_with(".Semaphore.Release")
                || name.ends_with(".Mutex.New")
                || name.ends_with(".Mutex.Lock")
                || name.ends_with(".Mutex.Unlock") =>
            {
                lower_bn_dispatch_call(text, *destination, name, arguments, analysis);
            }
            name if module
                .functions
                .iter()
                .any(|function| function.name == name.strip_prefix("@super:").unwrap_or(name)) =>
            {
                lower_user_call(text, module, *destination, name, arguments, analysis, state);
            }
            "HOST.Random.Seed" => {
                let seed = extend_to_i64(
                    text,
                    *arguments.first().expect("validated seed argument"),
                    analysis
                        .values
                        .get(arguments.first().expect("validated seed argument"))
                        .expect("validated seed type"),
                );
                let _ = writeln!(text, "  %rngzero{} = icmp eq i64 {seed}, 0", destination.0);
                let _ = writeln!(
                    text,
                    "  %rngseed{} = select i1 %rngzero{}, i64 1, i64 {seed}",
                    destination.0, destination.0
                );
                let rng = rng_ptr(state);
                let _ = writeln!(text, "  store i64 %rngseed{}, ptr {rng}", destination.0);
            }
            "HOST.Random.Random" => {
                let rng = rng_ptr(state);
                let _ = writeln!(text, "  %rng0{} = load i64, ptr {rng}", destination.0);
                let _ = writeln!(
                    text,
                    "  %rng1{} = lshr i64 %rng0{}, 12",
                    destination.0, destination.0
                );
                let _ = writeln!(
                    text,
                    "  %rng2{} = xor i64 %rng0{}, %rng1{}",
                    destination.0, destination.0, destination.0
                );
                let _ = writeln!(
                    text,
                    "  %rng3{} = shl i64 %rng2{}, 25",
                    destination.0, destination.0
                );
                let _ = writeln!(
                    text,
                    "  %rng4{} = xor i64 %rng2{}, %rng3{}",
                    destination.0, destination.0, destination.0
                );
                let _ = writeln!(
                    text,
                    "  %rng5{} = lshr i64 %rng4{}, 27",
                    destination.0, destination.0
                );
                let _ = writeln!(
                    text,
                    "  %rng6{} = xor i64 %rng4{}, %rng5{}",
                    destination.0, destination.0, destination.0
                );
                let _ = writeln!(
                    text,
                    "  %rngnext{} = mul i64 %rng6{}, 2685821657736338717",
                    destination.0, destination.0
                );
                let rng = rng_ptr(state);
                let _ = writeln!(text, "  store i64 %rngnext{}, ptr {rng}", destination.0);
                let _ = writeln!(
                    text,
                    "  %rngscale{} = lshr i64 %rngnext{}, 11",
                    destination.0, destination.0
                );
                let _ = writeln!(
                    text,
                    "  %rngfloat{} = uitofp i64 %rngscale{} to double",
                    destination.0, destination.0
                );
                let _ = writeln!(
                    text,
                    "  %v{} = fdiv double %rngfloat{}, 9007199254740992.0",
                    destination.0, destination.0
                );
            }
            _ => unreachable!("validated call target"),
        },
        Instruction::DispatchSubmit {
            destination,
            queue,
            task,
            arguments,
            ..
        } => {
            if !arguments.is_empty() {
                return Err(unsupported_instruction(
                    module,
                    function,
                    instruction,
                    "dispatch arguments are not yet representable in the compiled task ABI",
                ));
            }
            let task_name = analysis.functions.get(task).copied().ok_or_else(|| {
                unsupported_instruction(module, function, instruction, "missing async task target")
            })?;
            let _ = writeln!(
                text,
                "  %dispatchqueue{} = extractvalue {{ i1, ptr, i64 }} %v{}, 2",
                destination.0, queue.0
            );
            let _ = writeln!(text, "  %dispatchticket{} = alloca i64", destination.0);
            let _ = writeln!(
                text,
                "  %dispatchrc{} = call i32 @bn_rt_dispatch_submit(i64 %dispatchqueue{}, ptr @{}, ptr null, ptr null, i32 0, ptr %dispatchticket{})",
                destination.0,
                destination.0,
                dispatch_trampoline_symbol(task_name),
                destination.0
            );
            let _ = writeln!(
                text,
                "  %dispatchhandle{} = load i64, ptr %dispatchticket{}",
                destination.0, destination.0
            );
            emit_handle_result(
                text,
                *destination,
                format!("%dispatchrc{}", destination.0),
                format!("%dispatchhandle{}", destination.0),
            );
        }
        Instruction::DispatchAwait {
            destination,
            ticket,
            timeout,
            ..
        } => {
            let _ = writeln!(
                text,
                "  %dispatchticket{} = extractvalue {{ i1, ptr, i64 }} %v{}, 2",
                destination.0, ticket.0
            );
            let timeout_ty = analysis
                .values
                .get(timeout)
                .expect("validated timeout type");
            let timeout = extend_to_i64(text, *timeout, timeout_ty);
            let _ = writeln!(
                text,
                "  %dispatchresult{} = alloca [32 x i8]",
                destination.0
            );
            let _ = writeln!(text, "  %dispatcherror{} = alloca [24 x i8]", destination.0);
            emit_void_result(
                text,
                *destination,
                format!(
                    "call i32 @bn_rt_dispatch_await(i64 %dispatchticket{}, i64 {}, ptr %dispatchresult{}, ptr %dispatcherror{})",
                    destination.0, timeout, destination.0, destination.0
                ),
            );
        }
        Instruction::Input {
            destination,
            prompt,
            ..
        } => {
            block_state.constants.remove(destination);
            if let Some(prompt) = prompt {
                let _ = writeln!(
                    text,
                    "  call i32 (ptr, ...) @printf(ptr @.bn_fmt_str, ptr %v{})",
                    prompt.0
                );
            }
            let _ = writeln!(text, "  %v{} = call ptr @bn_input()", destination.0);
        }
        Instruction::Length {
            destination,
            vector,
            ..
        } if analysis.values.get(vector) == Some(&Type::HostArgs) => {
            block_state.constants.remove(destination);
            let _ = writeln!(text, "  %v{} = add i32 0, %argc", destination.0);
        }
        Instruction::Length {
            destination,
            vector,
            ..
        } if analysis.values.get(vector) == Some(&Type::String) => {
            block_state.constants.remove(destination);
            let _ = writeln!(
                text,
                "  %v{} = call i32 @bn_rt_str_len(ptr %v{})",
                destination.0, vector.0
            );
        }
        Instruction::Length {
            destination,
            vector,
            ..
        } if matches!(
            analysis.values.get(vector),
            Some(Type::Vector { .. } | Type::Pointer { .. })
        ) =>
        {
            block_state.constants.remove(destination);
            emit_vector_length(text, *destination, *vector);
        }
        Instruction::Vector {
            destination,
            values: elements,
            ty,
            ..
        } => {
            block_state.constants.remove(destination);
            emit_vector(text, *destination, elements, ty, analysis);
        }
        Instruction::Allocate {
            destination,
            type_name,
            arguments,
            ty,
            ..
        } => {
            block_state.constants.remove(destination);
            let object_bytes = class_instance_bytes(module, type_name);
            emit_allocate(text, *destination, arguments, ty, analysis, object_bytes);
            if !matches!(ty, Type::Pointer { .. }) {
                let class_global = format!("@.bn_cls_{}", sanitize_symbol(type_name));
                emit_store_object_class(text, *destination, &class_global);
            }
        }
        Instruction::Delete { value, .. } => {
            let ty = analysis
                .values
                .get(value)
                .expect("validated delete value type");
            emit_delete(text, *value, ty);
        }
        Instruction::EnsureClass { class, .. } => {
            let flag = class_init_flag(class);
            let init_name = format!("{class}.$init");
            let n = state.continuation_count;
            state.continuation_count += 1;
            let tag = format!("{}{n}", sanitize_symbol(class));
            let _ = writeln!(text, "  %initflag{tag} = load i1, ptr {flag}");
            let _ = writeln!(
                text,
                "  br i1 %initflag{tag}, label %initdone{tag}, label %initrun{tag}"
            );
            let _ = writeln!(text, "initrun{tag}:");
            let _ = writeln!(text, "  store i1 true, ptr {flag}");
            if module
                .functions
                .iter()
                .any(|function| function.name == init_name)
            {
                let init = llvm_function_symbol(&init_name);
                let _ = writeln!(text, "  call void @{init}()");
            }
            let _ = writeln!(text, "  br label %initdone{tag}");
            let _ = writeln!(text, "initdone{tag}:");
        }
        Instruction::LoadStatic {
            destination,
            class,
            field,
            ty,
            ..
        } => {
            block_state.constants.remove(destination);
            let llvm_ty = llvm_type(ty).expect("validated static type");
            let global = static_global_name(class, field);
            let _ = writeln!(text, "  %v{} = load {llvm_ty}, ptr {global}", destination.0);
        }
        Instruction::StoreStatic {
            class,
            field,
            value,
            ty,
            ..
        } => {
            let llvm_ty = llvm_type(ty).expect("validated static type");
            let value_ty = analysis
                .values
                .get(value)
                .expect("validated static value type");
            let operand = coerce_to_type(text, *value, value_ty, ty);
            let global = static_global_name(class, field);
            let _ = writeln!(text, "  store {llvm_ty} {operand}, ptr {global}");
        }
        Instruction::SetMember {
            object,
            name,
            owner,
            value,
            ty,
            ..
        } => {
            let offset = field_byte_offset(module, owner, name);
            let value_ty = analysis
                .values
                .get(value)
                .expect("validated member value type");
            emit_set_member(text, *object, offset, *value, value_ty, ty);
        }
        Instruction::SetField {
            symbol,
            path,
            value,
            ty,
            ..
        } => {
            let owner = match analysis.symbols.get(symbol) {
                Some(Type::Named(name) | Type::ImportedNamed { name, .. }) => name.as_str(),
                _ => "Box",
            };
            let field = path.first().map_or("value", String::as_str);
            let offset = field_byte_offset(module, owner, field);
            let value_ty = analysis
                .values
                .get(value)
                .expect("validated field value type");
            let _ = writeln!(
                text,
                "  %fieldobj{} = load ptr, ptr %s{}",
                value.0, symbols[symbol]
            );
            // Reuse SetMember emitter with a synthetic object value id name via temp.
            let llvm_ty = llvm_type(ty).expect("validated field type");
            let value_op = coerce_to_type(text, *value, value_ty, ty);
            let _ = writeln!(
                text,
                "  %fieldptr{} = getelementptr i8, ptr %fieldobj{}, i32 {offset}",
                value.0, value.0
            );
            let _ = writeln!(
                text,
                "  store {llvm_ty} {value_op}, ptr %fieldptr{}",
                value.0
            );
        }
        Instruction::Member {
            destination,
            object,
            name,
            owner,
            ty,
            ..
        } => {
            block_state.constants.remove(destination);
            if owner == "Error" && name == "Message" {
                let _ = writeln!(
                    text,
                    "  %v{} = extractvalue {{ i1, ptr, i64 }} %v{}, 1",
                    destination.0, object.0
                );
            } else {
                let offset = field_byte_offset(module, owner, name);
                emit_member(text, *destination, *object, offset, ty);
            }
        }
        Instruction::SetIndex {
            symbol,
            indices,
            value,
            ty,
            ..
        } => {
            let index = indices[0];
            let index_ty = analysis
                .values
                .get(&index)
                .expect("validated setindex index type");
            let value_ty = analysis
                .values
                .get(value)
                .expect("validated setindex value type");
            emit_pointer_set_index(
                text,
                block_id,
                symbols[symbol],
                index,
                index_ty,
                *value,
                value_ty,
                ty,
                state,
            );
        }
        Instruction::Index {
            destination,
            object,
            index,
            ty,
            ..
        } if analysis
            .values
            .get(object)
            .is_some_and(|ty| is_int_vector(ty) || is_int_pointer(ty)) =>
        {
            block_state.constants.remove(destination);
            let index_ty = analysis
                .values
                .get(index)
                .expect("validated vector index type");
            emit_vector_index(
                text,
                block_id,
                *destination,
                *object,
                *index,
                index_ty,
                ty,
                state,
            );
        }
        Instruction::Index {
            destination,
            object,
            index,
            ..
        } if analysis.values.get(object) == Some(&Type::String) => {
            block_state.constants.remove(destination);
            let idx = extend_to_i32_index(
                text,
                *index,
                analysis.values.get(index).expect("validated index type"),
            );
            let _ = writeln!(
                text,
                "  %v{} = call ptr @bn_rt_str_index(ptr %v{}, i32 {idx})",
                destination.0, object.0
            );
        }
        Instruction::Index {
            destination,
            object,
            index,
            ..
        } if analysis.values.get(object) == Some(&Type::HostArgs) => {
            block_state.constants.remove(destination);
            match llvm_type(analysis.values.get(index).expect("validated index type")) {
                Some("i32") => {
                    let _ = writeln!(
                        text,
                        "  %argptr{} = getelementptr ptr, ptr %argv, i32 %v{}",
                        destination.0, index.0
                    );
                }
                Some(other) => {
                    let cast =
                        if is_unsigned(analysis.values.get(index).expect("validated index type")) {
                            "zext"
                        } else {
                            "sext"
                        };
                    let _ = writeln!(
                        text,
                        "  %argindex{} = {cast} {other} %v{} to i32",
                        destination.0, index.0
                    );
                    let _ = writeln!(
                        text,
                        "  %argptr{} = getelementptr ptr, ptr %argv, i32 %argindex{}",
                        destination.0, destination.0
                    );
                }
                None => unreachable!("validated index type"),
            }
            let _ = writeln!(
                text,
                "  %v{} = load ptr, ptr %argptr{}",
                destination.0, destination.0
            );
        }
        Instruction::Print {
            values: printed, ..
        } => {
            let stdout = format!("%stdout{}", state.print_count);
            if state.synchronize_prints {
                let _ = writeln!(text, "  {stdout} = load ptr, ptr @__stdoutp");
                let _ = writeln!(text, "  call void @flockfile(ptr {stdout})");
            }
            for (index, value) in printed.iter().enumerate() {
                if index > 0 {
                    let _ = writeln!(
                        text,
                        "  %separator{} = call i32 @putchar(i32 32)",
                        state.print_count
                    );
                    state.print_count += 1;
                }
                lower_print_value(
                    text,
                    *value,
                    analysis
                        .values
                        .get(value)
                        .expect("validated printable type"),
                    state,
                );
            }
            let _ = writeln!(
                text,
                "  %newline{} = call i32 @putchar(i32 10)",
                state.print_count
            );
            state.print_count += 1;
            if state.synchronize_prints {
                let _ = writeln!(text, "  call void @funlockfile(ptr {stdout})");
            }
        }
        _ => {
            return Err(unsupported_instruction(
                module,
                function,
                instruction,
                instruction_name(instruction),
            ));
        }
    }
    Ok(())
}

fn extend_to_i32_index(text: &mut String, value: ValueId, ty: &Type) -> String {
    match llvm_type(ty).expect("validated index type") {
        "i32" => format!("%v{}", value.0),
        "i64" => {
            let temp = format!("stridx{}", value.0);
            let _ = writeln!(text, "  %{temp} = trunc i64 %v{} to i32", value.0);
            format!("%{temp}")
        }
        llvm_ty => {
            let opcode = if is_unsigned(ty) { "zext" } else { "sext" };
            let temp = format!("stridx{}", value.0);
            let _ = writeln!(text, "  %{temp} = {opcode} {llvm_ty} %v{} to i32", value.0);
            format!("%{temp}")
        }
    }
}

fn rng_ptr(state: &EmissionState) -> &'static str {
    if state.rng_global { "@bn_rng" } else { "%rng" }
}
