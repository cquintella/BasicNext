#![allow(clippy::wildcard_imports, clippy::too_many_lines)]
use super::*;

#[allow(clippy::too_many_arguments)]
pub(crate) fn lower_ownership_emission(
    text: &mut String,
    module: &Module,
    function: &Function,
    block_id: BlockId,
    instruction: &Instruction,
    analysis: &LoweringAnalysis<'_>,
    symbols: &HashMap<SymbolId, usize>,
    block_state: &mut BlockState,
    state: &mut EmissionState,
) -> bool {
    match instruction {
        Instruction::Release { value, .. } => {
            let ty = analysis
                .values
                .get(value)
                .expect("validated delete value type");
            let released_symbol = function
                .blocks
                .iter()
                .flat_map(|block| &block.instructions)
                .find_map(|instruction| match instruction {
                    Instruction::Load {
                        destination,
                        symbol,
                        ..
                    } if *destination == *value => Some(*symbol),
                    _ => None,
                });
            if released_symbol.is_some_and(|symbol| function.weak_symbols.contains(&symbol)) {
                let symbol = released_symbol.expect("checked weak release symbol");
                let _ = writeln!(
                    text,
                    "  call void @bn_arc_weak_unregister(ptr %s{})",
                    symbols[&symbol]
                );
                let _ = writeln!(text, "  store ptr null, ptr %s{}", symbols[&symbol]);
            } else if matches!(
                ty,
                Type::Integer(_)
                    | Type::IntegerLiteral(_)
                    | Type::Float(_)
                    | Type::FloatLiteral
                    | Type::Boolean
                    | Type::String
            ) {
                // RELEASE ends the binding lifetime; primary values need no
                // runtime destruction.
            } else if matches!(ty, Type::Named(_) if is_struct_type(module, ty)) {
                let Type::Named(owner) = ty else {
                    unreachable!("validated struct release type");
                };
                for (declaring, name, field_ty) in class_layout_fields(module, owner) {
                    if !is_class_type(module, &field_ty)
                        || module
                            .weak_fields
                            .contains(&(declaring.clone(), name.clone()))
                    {
                        continue;
                    }
                    let offset = field_byte_offset(module, &declaring, &name);
                    let field_ptr = format!("%structreleasefield{}_{}", value.0, offset);
                    let object = format!("%structreleaseobj{}_{}", value.0, offset);
                    let _ = writeln!(
                        text,
                        "  {field_ptr} = getelementptr i8, ptr %v{}, i32 {offset}",
                        value.0
                    );
                    let _ = writeln!(text, "  {object} = load ptr, ptr {field_ptr}");
                    emit_destroy_if_last(
                        text, module, function, &object, &field_ty, symbols, state,
                    );
                    let _ = writeln!(text, "  store ptr null, ptr {field_ptr}");
                }
            } else if let Type::Vector {
                element,
                dimensions,
            } = ty
                && dimensions.len() == 1
                && let Type::Named(class) = element.as_ref()
                && module
                    .functions
                    .iter()
                    .any(|candidate| candidate.name == format!("{class}.$fields"))
            {
                let data = format!("%vectorreleaseptr{}", value.0);
                let _ = writeln!(
                    text,
                    "  {data} = extractvalue {{ ptr, i32 }} %v{}, 0",
                    value.0
                );
                for index in 0..dimensions[0] {
                    let slot = format!("%vectorreleaseslot{}_{}", value.0, index);
                    let object = format!("%vectorreleaseobj{}_{}", value.0, index);
                    let _ = writeln!(
                        text,
                        "  {slot} = getelementptr ptr, ptr {data}, i64 {index}"
                    );
                    let _ = writeln!(text, "  {object} = load ptr, ptr {slot}");
                    emit_destroy_if_last(text, module, function, &object, element, symbols, state);
                }
            } else if matches!(ty, Type::Vector { .. }) {
                // Fixed aggregate vectors use function-local storage; RELEASE
                // closes their elements but never frees the fat-pointer base.
            } else if is_bndata_dataframe_type(module, ty) {
                let handle = format!("%dfdelhandle{}", value.0);
                let _ = writeln!(text, "  {handle} = ptrtoint ptr %v{} to i64", value.0);
                emit_checked_i32_eq_zero(
                    text,
                    block_id,
                    *value,
                    &format!("call i32 @bn_rt_dataframe_close(i64 {handle})"),
                    state,
                );
            } else if llvm_type(ty) == Some("{ i1, ptr, i64 }")
                && matches!(ty, Type::Alternative(alternatives) if alternatives.iter().any(|item| matches!(item, Type::ImportedNamed { name, .. } | Type::ImportedTypeName { name, .. } if name == "DataFrame")))
            {
                let tag = format!("dfaltdelete{}", value.0);
                let continuation = take_continuation(block_id, state);
                let _ = writeln!(
                    text,
                    "  %dfaltiserr{} = extractvalue {{ i1, ptr, i64 }} %v{}, 0",
                    value.0, value.0
                );
                let _ = writeln!(
                    text,
                    "  br i1 %dfaltiserr{}, label %{}, label %{}",
                    value.0, continuation, tag
                );
                state.control_flow.label(text, tag.clone());
                let _ = writeln!(
                    text,
                    "  %dfalthandle{} = extractvalue {{ i1, ptr, i64 }} %v{}, 2",
                    value.0, value.0
                );
                emit_checked_i32_eq_zero(
                    text,
                    block_id,
                    *value,
                    &format!(
                        "call i32 @bn_rt_dataframe_close(i64 %dfalthandle{})",
                        value.0
                    ),
                    state,
                );
                let _ = writeln!(text, "  br label %{continuation}");
                state.control_flow.label(text, continuation);
            } else if llvm_type(ty) == Some("{ i1, ptr, i64 }")
                && matches!(
                    ty,
                    Type::Alternative(alternatives)
                        if alternatives.iter().any(|item| matches!(
                            item,
                            Type::Named(name) if name == "HOST.Exec.Result"
                        ))
                )
            {
                let _ = writeln!(
                    text,
                    "  %execdelhandle{} = extractvalue {{ i1, ptr, i64 }} %v{}, 2",
                    value.0, value.0
                );
                let _ = writeln!(
                    text,
                    "  call i32 @bn_rt_exec_result_close(i64 %execdelhandle{})",
                    value.0
                );
            } else if llvm_type(ty) == Some("{ i1, ptr, i64 }") {
                let _ = writeln!(
                    text,
                    "  %filedelhandle{} = extractvalue {{ i1, ptr, i64 }} %v{}, 2",
                    value.0, value.0
                );
                let _ = writeln!(
                    text,
                    "  call i32 @bn_rt_file_close(i64 %filedelhandle{})",
                    value.0
                );
            } else if let Some(kind) = bnlog_resource_kind(module, ty) {
                let handle = format!("%logdelhandle{}", value.0);
                let symbol = if kind == "Fields" {
                    "bn_rt_log_fields_close"
                } else {
                    "bn_rt_log_logger_delete"
                };
                let _ = writeln!(text, "  {handle} = ptrtoint ptr %v{} to i64", value.0);
                let _ = writeln!(
                    text,
                    "  %logdelrc{} = call i32 @{symbol}(i64 {handle})",
                    value.0
                );
            } else {
                if is_class_type(module, ty) {
                    emit_destroy_if_last(
                        text,
                        module,
                        function,
                        &format!("%v{}", value.0),
                        ty,
                        symbols,
                        state,
                    );
                } else {
                    emit_delete(text, module, *value, ty);
                }
                if llvm_type(ty) == Some("ptr") {
                    for owned_value in analysis.owned_object_results.keys() {
                        let tag = format!("arc_clear{}_{}", owned_value.0, value.0);
                        let _ = writeln!(
                            text,
                            "  %{tag}old = load ptr, ptr %objectowned{}",
                            owned_value.0
                        );
                        let _ = writeln!(text, "  %{tag}eq = icmp eq ptr %{tag}old, %v{}", value.0);
                        let _ = writeln!(
                            text,
                            "  %{tag}next = select i1 %{tag}eq, ptr null, ptr %{tag}old",
                        );
                        let _ = writeln!(
                            text,
                            "  store ptr %{tag}next, ptr %objectowned{}",
                            owned_value.0
                        );
                    }
                }
                if let Some(symbol) = released_symbol
                    && analysis.symbols.get(&symbol).and_then(llvm_type) == Some("ptr")
                {
                    let _ = writeln!(text, "  store ptr null, ptr %s{}", symbols[&symbol]);
                }
            }
            if let Some(symbol) = released_symbol
                && analysis.released_symbols.contains(&symbol)
            {
                let _ = writeln!(text, "  store i1 false, ptr %slive{}", symbols[&symbol]);
            }
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
            state.control_flow.label(text, format!("initrun{tag}"));
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
            state.control_flow.label(text, format!("initdone{tag}"));
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
            let strong_object_field = is_class_type(module, ty)
                && !module.weak_fields.contains(&(owner.clone(), name.clone()));
            emit_set_member(
                text,
                module,
                function,
                analysis,
                symbols,
                *object,
                offset,
                *value,
                value_ty,
                ty,
                strong_object_field,
                state,
            );
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
            let strong_object_field = is_class_type(module, ty)
                && !module
                    .weak_fields
                    .contains(&(owner.to_string(), field.to_string()));
            if strong_object_field {
                let old = format!("%fieldsetold{}", value.0);
                let _ = writeln!(text, "  {old} = load ptr, ptr %fieldptr{}", value.0);
                emit_destroy_if_last(text, module, function, &old, ty, symbols, state);
                if analysis.owned_object_results.contains_key(value) {
                    let _ = writeln!(text, "  store ptr null, ptr %objectowned{}", value.0);
                } else {
                    let _ = writeln!(text, "  call void @bn_arc_retain(ptr {value_op})");
                }
            }
            let _ = writeln!(
                text,
                "  store {llvm_ty} {value_op}, ptr %fieldptr{}",
                value.0
            );
        }
        Instruction::SetFieldIndex {
            symbol,
            path,
            indices,
            value,
            ty,
            ..
        } => {
            let owner = match analysis.symbols.get(symbol) {
                Some(Type::Named(name) | Type::ImportedNamed { name, .. }) => name.as_str(),
                _ if function.parameters.first() == Some(symbol) => function
                    .name
                    .rsplit_once('.')
                    .map(|(class, _)| class)
                    .expect("validated method owner"),
                _ => unreachable!("validated indexed field owner"),
            };
            let field = path.first().expect("validated indexed field path");
            let offset = field_byte_offset(module, owner, field);
            let index = indices[0];
            let transfers_object =
                is_class_type(module, ty) && analysis.owned_object_results.contains_key(value);
            if transfers_object {
                let _ = writeln!(text, "  store ptr null, ptr %objectowned{}", value.0);
            }
            emit_field_set_index(
                text,
                module,
                function,
                symbols,
                block_id,
                symbols[symbol],
                offset,
                index,
                analysis.values.get(&index).expect("validated index type"),
                *value,
                analysis.values.get(value).expect("validated value type"),
                ty,
                transfers_object,
                state,
            );
        }
        _ => return false,
    }
    true
}
