#![allow(clippy::wildcard_imports, clippy::match_same_arms)]
use super::*;
use crate::ir::LlvmType;

#[allow(clippy::too_many_lines, clippy::too_many_arguments)]
pub(crate) fn validate_instruction(
    module: &Module,
    function: &Function,
    instruction: &Instruction,
    values: &HashMap<ValueId, Type>,
    symbols: &HashMap<SymbolId, Type>,
    functions: &HashMap<ValueId, &str>,
    strings: &[(ValueId, String)],
    module_functions: &std::collections::HashSet<&str>,
    intrinsics: &mut BTreeSet<crate::ir::Intrinsic>,
) -> Result<(), String> {
    let supported = match instruction {
        // Explicit ownership (proposal arc-shared-core-0.6.5) is lowered only
        // once the native backend executes it.
        // Explicit ownership (proposal arc-shared-core-0.6.5).
        Instruction::Retain {
            destination, ty, ..
        }
        | Instruction::Take {
            destination, ty, ..
        }
        | Instruction::TakeMember {
            destination, ty, ..
        } => llvm_type(values.get(destination).unwrap_or(ty)).is_some(),
        Instruction::EndBinding { .. } => true,
        Instruction::Constant { value, ty, .. } => match value {
            Constant::Integer(value) => llvm_type(ty).is_some() && parse_integer(value).is_some(),
            Constant::Float(value) => {
                llvm_type(ty).is_some() && parse_float_constant(value).is_some()
            }
            Constant::Boolean(_) | Constant::String(_) => llvm_type(ty).is_some(),
            Constant::Function(_)
            | Constant::Type(_)
            | Constant::HostArgs
            | Constant::HostConsole => true,
            Constant::NotAvailable => llvm_type(ty).is_some(),
            Constant::Null => true,
            Constant::EndOfFile => llvm_type(ty).is_some(),
        },
        Instruction::Phi { ty, .. } => llvm_type(ty).is_some(),
        Instruction::Default {
            ty,
            dimensions,
            dynamic_dimensions,
            ..
        } => {
            (dimensions.is_empty() && dynamic_dimensions.is_empty() && llvm_type(ty).is_some())
                || vectors::nested_default(ty, dimensions, dynamic_dimensions)
                || (matches!(ty, Type::Vector { element, dimensions: ty_dimensions, .. }
                if ty_dimensions.len() == 1
                    && dimensions.len() == 1
                    && dynamic_dimensions.is_empty()
                    && llvm_vector_dimension_supported(dimensions[0])
                    && llvm_type(ty).is_some()
                    && llvm_type(element).is_some_and(|element| matches!(
                    element,
                    LlvmType::I1 | LlvmType::I8 | LlvmType::I16 | LlvmType::I32 | LlvmType::I64 | LlvmType::Float | LlvmType::Double | LlvmType::Ptr
                ) || crate::layout::is_one_of(&element, &[crate::layout::handle_result_ty(), crate::layout::status_pointer_ty()])))
                || (matches!(ty, Type::Vector { element, dimensions: ty_dimensions, .. }
                if ty_dimensions.len() == 1
                    && dimensions.is_empty()
                    && dynamic_dimensions.len() == 1
                    && llvm_type(ty).is_some()
                    && llvm_type(element).is_some_and(|element| matches!(
                    element,
                    LlvmType::I1 | LlvmType::I8 | LlvmType::I16 | LlvmType::I32 | LlvmType::I64 | LlvmType::Float | LlvmType::Double | LlvmType::Ptr
                ) || crate::layout::is_one_of(&element, &[crate::layout::handle_result_ty(), crate::layout::status_pointer_ty()])))))
        }
        Instruction::Load {
            destination,
            symbol,
            ..
        } => {
            values.get(destination).and_then(llvm_type).is_some()
                && symbols.get(symbol).and_then(llvm_type).is_some()
        }
        Instruction::Store { value, symbol, .. } => {
            values.get(value).and_then(llvm_type).is_some()
                && symbols.get(symbol).and_then(llvm_type).is_some()
                && symbols
                    .get(symbol)
                    .is_none_or(|ty| struct_copy_supported(module, ty))
        }
        Instruction::Copy { source, ty, .. } => {
            values.get(source).and_then(llvm_type).is_some() && llvm_type(ty).is_some()
        }
        Instruction::Unary {
            operator,
            operand,
            ty,
            ..
        } => unary_supported(operator, values.get(operand), ty),
        Instruction::Binary {
            destination,
            operator,
            left,
            right,
            ty,
            ..
        } => {
            let Some(left_ty) = values.get(left) else {
                return Err(unsupported_instruction(
                    module,
                    function,
                    instruction,
                    "unknown value type",
                ));
            };
            let Some(right_ty) = values.get(right) else {
                return Err(unsupported_instruction(
                    module,
                    function,
                    instruction,
                    "unknown value type",
                ));
            };
            if let Some(intrinsic) = checked_intrinsic(left_ty, operator) {
                intrinsics.insert(intrinsic);
            }
            if operator == "Power"
                && let Some(intrinsic) = pow_intrinsic(ty)
            {
                intrinsics.insert(intrinsic);
            }
            if operator == "IS" {
                // The emitter decides which `IS` forms it lowers; asking it
                // on a scratch buffer keeps one rule and makes an unlowered
                // form a support diagnostic here (W4), not at emission.
                if matches!(ty, Type::Boolean)
                    && !emit_is(
                        &mut crate::ir::LlvmModule::new(),
                        *destination,
                        *left,
                        left_ty,
                        right_ty,
                    )
                {
                    let detail = bn_catalog::fill(
                        "llvm",
                        "is.unsupported",
                        &[
                            ("test", &crate::display_type(right_ty)),
                            ("value", &crate::display_type(left_ty)),
                        ],
                    );
                    return Err(unsupported_instruction(
                        module,
                        function,
                        instruction,
                        &detail,
                    ));
                }
                matches!(ty, Type::Boolean)
            } else {
                binary_supported(operator, left_ty, right_ty, ty)
            }
        }
        Instruction::Cast { value, ty, .. } => {
            cast_supported(values.get(value), ty)
                || values.get(value).is_some_and(|source| {
                    llvm_type(source) == Some(crate::layout::optional_float_ty())
                        && matches!(ty, Type::Float(_) | Type::FloatLiteral)
                })
        }
        Instruction::Call {
            callee, arguments, ..
        } => call_instruction_supported(
            module,
            function,
            instruction,
            callee,
            arguments,
            values,
            functions,
            strings,
            module_functions,
        )?,
        Instruction::Input { .. } => true,
        Instruction::Length { vector, .. } => {
            matches!(
                values.get(vector),
                Some(Type::HostArgs | Type::String | Type::Vector { .. } | Type::Pointer { .. })
            )
        }
        Instruction::Index {
            object, index, ty, ..
        } => {
            (matches!(values.get(object), Some(Type::HostArgs | Type::String))
                || values.get(object).is_some_and(is_native_vector)
                || values.get(object).is_some_and(is_native_pointer)
                || values.get(object).is_some_and(is_region_alternative))
                && values.get(index).and_then(llvm_type).is_some()
                && llvm_type(ty).is_some()
        }
        Instruction::Vector {
            values: elements,
            ty,
            ..
        } => {
            llvm_type(ty).is_some()
                && matches!(
                    ty,
                    Type::Vector { dimensions, .. }
                        if dimensions.first().is_some_and(|length| u32::try_from(*length).is_ok())
                )
                && elements
                    .iter()
                    .all(|element| values.get(element).and_then(llvm_type).is_some())
                && match ty {
                    Type::Vector { element, .. } => struct_copy_supported(module, element),
                    _ => true,
                }
        }
        Instruction::Print {
            values: printed, ..
        } => printed
            .iter()
            .all(|value| values.get(value).is_some_and(printable_type)),
        Instruction::Allocate { ty, arguments, .. } => {
            llvm_type(ty).is_some()
                && (matches!(ty, Type::Pointer { .. })
                    && arguments
                        .iter()
                        .all(|argument| values.get(argument).and_then(llvm_type).is_some())
                    || !matches!(ty, Type::Pointer { .. }))
        }
        Instruction::Release { value, .. } => {
            // A STRING or HOST.Args index is a fresh buffer. Releasing it
            // ends that buffer. An index into a vector or pointer is a view.
            let indexed_view = function
                .blocks
                .iter()
                .flat_map(|block| &block.instructions)
                .any(|instruction| {
                    matches!(
                        instruction,
                        Instruction::Index { destination, object, .. }
                            if destination == value
                                && !matches!(
                                    values.get(object),
                                    Some(Type::String | Type::HostArgs)
                                )
                    )
                });
            if indexed_view {
                return Err(unsupported_instruction(
                    module,
                    function,
                    instruction,
                    "RELEASE of an indexed element is unsupported",
                ));
            }
            values
                .get(value)
                .is_some_and(|ty| arc_ops::holds_references(module, ty))
                || values.get(value).is_some_and(|ty| {
                is_native_pointer(ty)
                    || matches!(ty, Type::Pointer { element, .. } if matches!(element.as_ref(), Type::Vector { .. }))
            })
                || values.get(value).is_some_and(|ty| {
                    llvm_type(ty) == Some(crate::layout::vector_ty()) || llvm_type(ty) == Some(LlvmType::Ptr)
                        || llvm_type(ty) == Some(crate::layout::handle_result_ty())
                        || general_alternative(ty).is_some()
                })
                || values.get(value).is_some_and(|ty| {
                    matches!(ty, Type::Integer(_) | Type::IntegerLiteral(_) | Type::Float(_) | Type::FloatLiteral | Type::Boolean | Type::String)
                })
        }
        Instruction::SetIndex {
            symbol,
            indices,
            value,
            ty,
            ..
        } => {
            symbols
                .get(symbol)
                .is_some_and(|container| match container {
                    Type::Vector {
                        element,
                        dimensions,
                    } => {
                        !indices.is_empty()
                            && (dimensions.iter().all(|dimension| *dimension != u64::MAX)
                                || (dimensions.len() == 1 && indices.len() == 1))
                            && if indices.len() == dimensions.len() {
                                llvm_type(element).is_some()
                            } else if indices.len() + 1 == dimensions.len() {
                                ty == &Type::Vector {
                                    element: element.clone(),
                                    dimensions: dimensions[indices.len()..].to_vec(),
                                }
                            } else {
                                false
                            }
                    }
                    _ => {
                        indices.len() == 1
                            && (is_native_pointer(container) || is_region_alternative(container))
                    }
                })
                && indices
                    .iter()
                    .all(|index| values.get(index).and_then(llvm_type).is_some())
                && values.get(value).and_then(llvm_type).is_some()
                && llvm_type(ty).is_some()
        }
        Instruction::SetFieldIndex {
            symbol,
            path,
            indices,
            value,
            ty,
            ..
        } => {
            let owner = match symbols.get(symbol) {
                Some(Type::Named(name) | Type::ImportedNamed { name, .. }) => Some(name.as_str()),
                _ if function.parameters.first() == Some(symbol) => {
                    function.name.rsplit_once('.').map(|(class, _)| class)
                }
                _ => None,
            };
            let field_ty = owner
                .zip(path.first().map(String::as_str))
                .and_then(|(owner, field)| field_type(module, owner, field));
            let receiver_supported = symbols.get(symbol).and_then(llvm_type) == Some(LlvmType::Ptr)
                || function.parameters.first() == Some(symbol);
            path.len() == 1
                && indices.len() == 1
                && receiver_supported
                && field_ty.as_ref().is_some_and(is_native_vector)
                && indices
                    .iter()
                    .all(|index| values.get(index).and_then(llvm_type).is_some())
                && values.get(value).and_then(llvm_type).is_some()
                && llvm_type(ty).is_some()
        }
        Instruction::SetMemberIndex {
            object,
            indices,
            value,
            ty,
            ..
        } => {
            indices.len() == 1
                && values.get(object).and_then(llvm_type) == Some(LlvmType::Ptr)
                && indices
                    .iter()
                    .all(|index| values.get(index).and_then(llvm_type).is_some())
                && values.get(value).and_then(llvm_type).is_some()
                && llvm_type(ty).is_some()
        }
        Instruction::SetStaticIndex {
            indices, value, ty, ..
        } => {
            indices.len() == 1
                && indices
                    .iter()
                    .all(|index| values.get(index).and_then(llvm_type).is_some())
                && values.get(value).and_then(llvm_type).is_some()
                && llvm_type(ty).is_some()
        }
        Instruction::EnsureClass { .. } => true,
        Instruction::Member {
            object,
            name,
            owner,
            ty,
            ..
        } => {
            (owner == "Error"
                && matches!(name.as_str(), "Message" | "Operation" | "Cause")
                && values.get(object).and_then(llvm_type).is_some_and(|ty| {
                    crate::layout::is_one_of(
                        &ty,
                        &[
                            crate::layout::status_pointer_ty(),
                            crate::layout::status_endpoint_ty(),
                            crate::layout::handle_result_ty(),
                        ],
                    )
                }))
                || (owner == "Error"
                    && name == "Code"
                    && values.get(object).and_then(llvm_type).is_some_and(|ty| {
                        crate::layout::is_one_of(
                            &ty,
                            &[
                                crate::layout::status_pointer_ty(),
                                crate::layout::status_endpoint_ty(),
                                crate::layout::handle_result_ty(),
                            ],
                        )
                    }))
                || (owner == "HOST.Exec.Result"
                    && matches!(name.as_str(), "ReturnCode" | "Stdout" | "Stderr")
                    && values.get(object).and_then(llvm_type)
                        == Some(crate::layout::handle_result_ty())
                    && llvm_type(ty).is_some())
                || (values.get(object).and_then(llvm_type) == Some(LlvmType::Ptr)
                    && llvm_type(ty).is_some())
        }
        Instruction::SetMember {
            object, value, ty, ..
        } => {
            values.get(object).and_then(llvm_type) == Some(LlvmType::Ptr)
                && values.get(value).and_then(llvm_type).is_some()
                && llvm_type(ty).is_some()
        }
        Instruction::SetField {
            symbol,
            path,
            value,
            ty,
            ..
        } => {
            !path.is_empty()
                && symbols.get(symbol).and_then(llvm_type) == Some(LlvmType::Ptr)
                && values.get(value).and_then(llvm_type).is_some()
                && llvm_type(ty).is_some()
        }
        Instruction::LoadStatic { ty, .. } => llvm_type(ty).is_some(),
        Instruction::StoreStatic { value, ty, .. } => {
            llvm_type(ty).is_some() && values.get(value).and_then(llvm_type).is_some()
        }
        Instruction::DispatchSubmit {
            queue,
            task,
            arguments,
            ty,
            ..
        } => {
            let signature = values.get(task).and_then(|ty| match ty {
                Type::Function { parameters, .. } => Some(parameters),
                _ => None,
            });
            llvm_type(ty) == Some(crate::layout::handle_result_ty())
                && values.get(queue).and_then(llvm_type) == Some(crate::layout::handle_result_ty())
                && signature.is_some_and(|parameters| {
                    parameters.len() == arguments.len()
                        && arguments
                            .iter()
                            .zip(parameters)
                            .all(|(argument, parameter)| {
                                values
                                    .get(argument)
                                    .is_some_and(|argument| match parameter {
                                        Type::Integer(_) => matches!(
                                            argument,
                                            Type::Integer(_) | Type::IntegerLiteral(_)
                                        ),
                                        Type::Float(_) => {
                                            matches!(argument, Type::Float(_) | Type::FloatLiteral)
                                        }
                                        Type::Boolean => matches!(argument, Type::Boolean),
                                        Type::String => matches!(argument, Type::String),
                                        Type::Named(name) | Type::TypeName(name)
                                            if matches!(name.as_str(), "DATE" | "TIME" | "TIMESTAMP") =>
                                        {
                                            matches!(argument, Type::Named(a) | Type::TypeName(a) if a == name)
                                        }
                                        Type::ImportedNamed { name, .. }
                                        | Type::ImportedTypeName { name, .. }
                                            if dispatch_handle_name(name) =>
                                        {
                                            match argument {
                                                Type::ImportedNamed { name: aname, .. }
                                                | Type::ImportedTypeName { name: aname, .. } => {
                                                    aname == name
                                                }
                                                Type::Named(aname) | Type::TypeName(aname) => {
                                                    aname == name
                                                        || aname.ends_with(&format!(".{name}"))
                                                }
                                                _ => false,
                                            }
                                        }
                                        Type::Named(name) | Type::TypeName(name)
                                            if dispatch_handle_name(name)
                                                || dispatch_handle_name(
                                                    name.rsplit('.').next().unwrap_or(name),
                                                ) =>
                                        {
                                            let base = name.rsplit('.').next().unwrap_or(name);
                                            match argument {
                                                Type::ImportedNamed { name: aname, .. }
                                                | Type::ImportedTypeName { name: aname, .. } => {
                                                    aname == base
                                                }
                                                Type::Named(aname) | Type::TypeName(aname) => {
                                                    aname == base
                                                        || aname.ends_with(&format!(".{base}"))
                                                }
                                                _ => false,
                                            }
                                        }
                                        _ => false,
                                    })
                            })
                })
                && functions.contains_key(task)
        }
        Instruction::DispatchAwait {
            ticket,
            timeout,
            ty,
            ..
        } => {
            values.get(ticket).and_then(llvm_type) == Some(crate::layout::handle_result_ty())
                && values
                    .get(timeout)
                    .and_then(llvm_type)
                    .is_some_and(|ty| ty.is_integer())
                && (matches!(ty, Type::Unknown)
                    || is_void_type(ty)
                    || matches!(ty, Type::Alternative(alternatives) if void_or_error(alternatives)
                        || integer_or_error(alternatives)
                        || float_or_error(alternatives)
                        || string_or_error(alternatives)
                        || boolean_or_error(alternatives)
                        || alternatives.iter().any(|alt| match alt {
                            Type::ImportedNamed { name, .. } | Type::ImportedTypeName { name, .. } => dispatch_handle_name(name),
                            Type::Named(name) | Type::TypeName(name) => dispatch_handle_name(name) || dispatch_handle_name(name.rsplit('.').next().unwrap_or(name)),
                            _ => false,
                        })
                        || alternatives.iter().any(|alt| matches!(alt, Type::Named(name) | Type::TypeName(name) if matches!(name.as_str(), "DATE" | "TIME" | "TIMESTAMP")))))
        }
        Instruction::SizeOf { value, .. } => values.get(value) == Some(&Type::String),
        Instruction::ClearScreen { .. } | Instruction::Beep { .. } => false,
    };
    if supported {
        Ok(())
    } else {
        Err(unsupported_instruction(
            module,
            function,
            instruction,
            &unsupported_instruction_detail(instruction),
        ))
    }
}

pub(crate) fn for_condition_supported(
    arguments: &[ValueId],
    values: &HashMap<ValueId, Type>,
) -> bool {
    if arguments.len() != 3 {
        return false;
    }
    let Some(types) = arguments
        .iter()
        .map(|argument| values.get(argument))
        .collect::<Option<Vec<_>>>()
    else {
        return false;
    };
    types.iter().all(|ty| {
        matches!(ty, Type::Integer(_) | Type::IntegerLiteral(_)) && llvm_type(ty).is_some()
    })
}

pub(crate) fn for_fits_supported(arguments: &[ValueId], values: &HashMap<ValueId, Type>) -> bool {
    if arguments.len() != 2 {
        return false;
    }
    arguments.iter().all(|argument| {
        values.get(argument).is_some_and(|ty| {
            matches!(ty, Type::Integer(_) | Type::IntegerLiteral(_)) && llvm_type(ty).is_some()
        })
    })
}
