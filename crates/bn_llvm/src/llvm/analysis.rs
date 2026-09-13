#![allow(clippy::wildcard_imports, clippy::match_same_arms)]
use super::runtime::is_bndata_function;
use super::runtime::{is_bool_vector, is_float_vector, is_string_vector};
use super::*;

pub(crate) fn analyze_function<'a>(
    module: &Module,
    function: &'a Function,
    module_functions: &std::collections::HashSet<&str>,
) -> Result<LoweringAnalysis<'a>, String> {
    let mut values = HashMap::<ValueId, Type>::new();
    let mut symbols = HashMap::new();
    let mut functions = HashMap::new();
    let mut strings = Vec::new();
    let mut input_count = 0;
    let mut uses_random = false;
    let mut uses_string_concat = false;
    let mut uses_bn_rt = false;
    let mut uses_bn_rt_math = false;
    let mut uses_float_print = false;
    let mut uses_string_ops = false;
    let mut uses_string_sizeof = false;
    let mut uses_temporal_print = false;
    let mut uses_heap = false;
    let mut seeds_random = false;
    let mut intrinsics = BTreeSet::new();
    let mut def_counts = HashMap::<ValueId, usize>::new();
    let mut input_values = HashSet::new();
    let mut owned_string_results = HashSet::new();
    let mut owned_struct_results = HashSet::new();
    let mut owned_log_results = HashMap::new();

    for block in &function.blocks {
        for instruction in &block.instructions {
            if let Some(destination) = instruction_destination(instruction) {
                *def_counts.entry(destination).or_insert(0) += 1;
            }
            match instruction {
                Instruction::Constant {
                    destination,
                    value,
                    ty,
                    ..
                } => match value {
                    Constant::Function(name) => {
                        if matches!(name.as_str(), "HOST.Random.Random" | "HOST.Random.Seed") {
                            uses_random = true;
                            uses_bn_rt = true;
                        }
                        if is_bn_rt_host_call(name) {
                            uses_bn_rt = true;
                        }
                        if matches!(name.as_str(), "ASC" | "CHAR" | "TOLOWER" | "TOUPPER") {
                            uses_bn_rt = true;
                        }
                        if is_bndata_dataframe_call(module, name) {
                            uses_bn_rt = true;
                        }
                        if bnlog_method(module, name).is_some() {
                            uses_bn_rt = true;
                        }
                        if bnmath_method(module, name).is_some() {
                            uses_bn_rt_math = true;
                        }
                        functions.insert(*destination, name.as_str());
                        values.insert(*destination, ty.clone());
                    }
                    Constant::String(value) => {
                        values.insert(*destination, ty.clone());
                        strings.push((*destination, value.clone()));
                    }
                    Constant::Type(_) => {
                        values.insert(*destination, ty.clone());
                    }
                    _ => {
                        values.insert(*destination, ty.clone());
                    }
                },
                Instruction::Default {
                    destination,
                    ty,
                    dimensions,
                    dynamic_dimensions,
                    ..
                } if (dimensions.is_empty() && dynamic_dimensions.is_empty())
                    || matches!(ty, Type::Vector { .. })
                        && dimensions.len() == 1
                        && dynamic_dimensions.is_empty() =>
                {
                    if function.name.ends_with(".$default") && llvm_type(ty) == Some("ptr") {
                        uses_heap = true;
                    }
                    values.insert(*destination, ty.clone());
                }
                Instruction::Phi {
                    destination, ty, ..
                } => {
                    values.insert(*destination, ty.clone());
                }
                Instruction::Load {
                    destination,
                    symbol,
                    ty,
                    ..
                } => {
                    let stored_ty = symbols
                        .get(symbol)
                        .cloned()
                        .filter(|stored| llvm_type(stored).is_some());
                    let load_ty = stored_ty
                        .filter(|stored| llvm_type(stored) != llvm_type(ty))
                        .unwrap_or_else(|| ty.clone());
                    values.insert(*destination, load_ty.clone());
                    symbols.entry(*symbol).or_insert(load_ty);
                }
                Instruction::Store {
                    symbol, value, ty, ..
                } => {
                    let stored = if llvm_type(ty).is_some() {
                        ty.clone()
                    } else {
                        values
                            .get(value)
                            .cloned()
                            .filter(|value_ty| llvm_type(value_ty).is_some())
                            .unwrap_or_else(|| ty.clone())
                    };
                    symbols.insert(*symbol, stored);
                    if is_struct_type(module, ty) {
                        uses_heap = true;
                    }
                }
                Instruction::Copy {
                    destination, ty, ..
                }
                | Instruction::Unary {
                    destination, ty, ..
                }
                | Instruction::Cast {
                    destination, ty, ..
                } => {
                    values.insert(*destination, ty.clone());
                }
                Instruction::Index {
                    destination,
                    object,
                    ty,
                    ..
                } => {
                    if values.get(object) == Some(&Type::String) {
                        uses_string_ops = true;
                    }
                    values.insert(*destination, ty.clone());
                }
                Instruction::Binary {
                    destination,
                    operator,
                    left,
                    ty,
                    ..
                } => {
                    if operator == "Plus" && *ty == Type::String {
                        uses_string_concat = true;
                    }
                    if matches!(operator.as_str(), "Equal" | "Assign" | "NotEqual")
                        && values.get(left) == Some(&Type::String)
                    {
                        uses_string_ops = true;
                    }
                    values.insert(*destination, ty.clone());
                }
                Instruction::Call {
                    destination,
                    callee,
                    ty,
                    ..
                } => {
                    if functions
                        .get(callee)
                        .is_some_and(|name| name.ends_with(".$default"))
                    {
                        if function.name != "Start" || block_is_cyclic(function, block.id) {
                            return Err(unsupported_instruction(
                                module,
                                function,
                                instruction,
                                "STRUCT default allocation requires an acyclic Start lifetime",
                            ));
                        }
                        uses_heap = true;
                        owned_struct_results.insert(*destination);
                    }
                    if functions.get(callee) == Some(&"HOST.Random.Seed") {
                        seeds_random = true;
                    }
                    if matches!(
                        functions
                            .get(callee)
                            .and_then(|name| bndata_dataframe_method(name)),
                        Some("column_name" | "get_string")
                    ) {
                        uses_heap = true;
                        owned_string_results.insert(*destination);
                    }
                    if !matches!(ty, Type::Named(name) if name == "VOID") {
                        let value_ty = if functions.get(callee).is_some_and(|name| {
                            matches!(
                                *name,
                                "HOST.Net.TCPListener.LocalEndpoint"
                                    | "HOST.Net.TCPStream.LocalEndpoint"
                                    | "HOST.Net.TCPStream.RemoteEndpoint"
                                    | "HOST.Net.UDPPacket.Source"
                            )
                        }) && !matches!(ty, Type::Alternative(_))
                        {
                            Type::Alternative(vec![ty.clone(), Type::Named("Error".into())])
                        } else {
                            ty.clone()
                        };
                        values.insert(*destination, value_ty);
                    }
                }
                Instruction::Input { destination, .. } => {
                    input_count += 1;
                    input_values.insert(*destination);
                    values.insert(*destination, Type::String);
                }
                Instruction::Length {
                    destination,
                    vector,
                    ..
                } if values.get(vector) == Some(&Type::HostArgs)
                    || values.get(vector) == Some(&Type::String) =>
                {
                    if values.get(vector) == Some(&Type::String) {
                        uses_string_ops = true;
                    }
                    values.insert(*destination, Type::Integer(IntegerType::Int32));
                }
                Instruction::Vector {
                    destination, ty, ..
                } => {
                    if let Type::Vector { element, .. } = ty
                        && is_struct_type(module, element)
                    {
                        uses_heap = true;
                    }
                    values.insert(*destination, ty.clone());
                }
                Instruction::SizeOf {
                    destination, value, ..
                } => {
                    if values.get(value) == Some(&Type::String) {
                        uses_string_sizeof = true;
                    }
                    values.insert(*destination, Type::Integer(IntegerType::Int32));
                }
                Instruction::DispatchSubmit {
                    destination, ty, ..
                }
                | Instruction::DispatchAwait {
                    destination, ty, ..
                } => {
                    uses_bn_rt = true;
                    values.insert(*destination, ty.clone());
                }
                Instruction::Print {
                    values: printed, ..
                } => {
                    if printed.iter().any(|value| {
                        matches!(
                            values.get(value),
                            Some(
                                Type::Float(_)
                                    | Type::FloatLiteral
                                    | Type::NotAvailable
                                    | Type::Alternative(_)
                            )
                        )
                    }) {
                        uses_float_print = true;
                    }
                    if printed.iter().any(|value| {
                        matches!(
                            values.get(value),
                            Some(Type::Named(name)) if name == "DATE" || name == "TIME"
                        )
                    }) {
                        uses_temporal_print = true;
                    }
                }
                Instruction::Length {
                    destination,
                    vector,
                    ..
                } if matches!(
                    values.get(vector),
                    Some(Type::Vector { .. } | Type::Pointer { .. })
                ) =>
                {
                    values.insert(*destination, Type::Integer(IntegerType::Int32));
                }
                Instruction::Allocate {
                    destination, ty, ..
                } => {
                    uses_heap = true;
                    if let Some(kind) = bnlog_resource_kind(module, ty) {
                        uses_bn_rt = true;
                        owned_log_results.insert(*destination, kind);
                    }
                    values.insert(*destination, ty.clone());
                }
                Instruction::Delete { .. } => {
                    uses_heap = true;
                }
                Instruction::EnsureClass { .. } => {}
                Instruction::SetIndex { symbol, ty, .. } => {
                    // A parameter used only as an indexed assignment has no
                    // Load from which to recover its container type. The
                    // SetIndex `ty` is the element type, so retain the pointer
                    // shape instead of incorrectly recording the parameter as
                    // a scalar element.
                    symbols.entry(*symbol).or_insert_with(|| Type::Pointer {
                        element: Box::new(ty.clone()),
                        length: bn_types::PointerLength::Dynamic,
                    });
                }
                Instruction::SetField { symbol, .. }
                | Instruction::SetFieldIndex { symbol, .. }
                    if function.parameters.first() == Some(symbol) =>
                {
                    if let Some((owner, _)) = function.name.rsplit_once('.') {
                        symbols
                            .entry(*symbol)
                            .or_insert_with(|| Type::Named(owner.to_string()));
                    }
                }
                Instruction::SetMember { .. }
                | Instruction::SetMemberIndex { .. }
                | Instruction::SetField { .. }
                | Instruction::SetFieldIndex { .. }
                | Instruction::SetStaticIndex { .. } => {}
                Instruction::Member {
                    destination, ty, ..
                } => {
                    values.insert(*destination, ty.clone());
                }
                Instruction::LoadStatic {
                    destination, ty, ..
                } => {
                    values.insert(*destination, ty.clone());
                }
                Instruction::StoreStatic { .. } => {}
                Instruction::Length { .. }
                | Instruction::ClearScreen { .. }
                | Instruction::Beep { .. }
                | Instruction::Default { .. } => {}
            }
        }
    }

    for block in &function.blocks {
        for instruction in &block.instructions {
            validate_instruction(
                module,
                function,
                instruction,
                &values,
                &symbols,
                &functions,
                &strings,
                module_functions,
                &mut intrinsics,
            )?;
        }
    }
    if uses_random && !seeds_random {
        let instruction = function
            .blocks
            .iter()
            .flat_map(|block| &block.instructions)
            .find(|instruction| matches!(
                instruction,
                Instruction::Call { callee, .. } if functions.get(callee) == Some(&"HOST.Random.Random")
            ))
            .expect("random use without seed must come from a call");
        return Err(unsupported_instruction(
            module,
            function,
            instruction,
            "HOST.Random.Random without HOST.Random.Seed",
        ));
    }

    let mut input_targets = HashMap::new();
    let mut input_symbols = HashSet::new();
    for block in &function.blocks {
        for instruction in &block.instructions {
            if let Instruction::Store { symbol, value, .. } = instruction
                && input_values.contains(value)
            {
                if input_targets.insert(*value, *symbol).is_some() {
                    return Err(unsupported_instruction(
                        module,
                        function,
                        instruction,
                        "INPUT result stored more than once",
                    ));
                }
                input_symbols.insert(*symbol);
            }
        }
    }
    if input_targets.len() != input_values.len() {
        let instruction = function
            .blocks
            .iter()
            .flat_map(|block| &block.instructions)
            .find(|instruction| matches!(instruction, Instruction::Input { destination, .. } if !input_targets.contains_key(destination)))
            .expect("unowned INPUT result must have an instruction");
        return Err(unsupported_instruction(
            module,
            function,
            instruction,
            "INPUT result without a variable owner",
        ));
    }
    for owned in &owned_string_results {
        let mut direct_print_uses = 0;
        let mut unsupported_use = false;
        for block in &function.blocks {
            for instruction in &block.instructions {
                let uses = bn_ir::instruction_uses(instruction)
                    .into_iter()
                    .filter(|used| used == owned)
                    .count();
                if uses == 0 {
                    continue;
                }
                if matches!(instruction, Instruction::Print { .. }) {
                    direct_print_uses += uses;
                } else if matches!(instruction, Instruction::Call { arguments, .. } if arguments.contains(owned))
                {
                    // Ownership is transferred to a STRING parameter; the
                    // caller releases the buffer after the call returns.
                    direct_print_uses += uses;
                } else {
                    unsupported_use = true;
                }
            }
            unsupported_use |= match block.terminator {
                Terminator::Branch { condition, .. } => condition == *owned,
                Terminator::Return { value } => value == Some(*owned),
                Terminator::Stop { code } => code == *owned,
                Terminator::Jump { .. } => false,
            };
        }
        let _ = (unsupported_use, direct_print_uses);
    }
    Ok(LoweringAnalysis {
        values,
        symbols,
        functions,
        strings,
        input_count,
        input_targets,
        input_symbols,
        owned_string_results,
        owned_struct_results,
        owned_log_results,
        uses_string_concat,
        uses_bn_rt,
        uses_bn_rt_math,
        uses_float_print,
        uses_string_ops,
        uses_string_sizeof,
        uses_temporal_print,
        uses_heap,
        multi_defs: def_counts
            .into_iter()
            .filter_map(|(value, count)| (count > 1).then_some(value))
            .collect(),
        intrinsics,
    })
}

fn block_is_cyclic(function: &Function, candidate: BlockId) -> bool {
    let blocks = function
        .blocks
        .iter()
        .map(|block| (block.id.0, block))
        .collect::<HashMap<_, _>>();
    let Some(block) = blocks.get(&candidate.0) else {
        return false;
    };
    let mut pending = match block.terminator {
        Terminator::Jump { target } => vec![target.0],
        Terminator::Branch {
            then_block,
            else_block,
            ..
        } => vec![then_block.0, else_block.0],
        Terminator::Return { .. } | Terminator::Stop { .. } => Vec::new(),
    };
    let mut visited = HashSet::new();
    while let Some(block_id) = pending.pop() {
        if block_id == candidate.0 {
            return true;
        }
        if !visited.insert(block_id) {
            continue;
        }
        let Some(block) = blocks.get(&block_id) else {
            continue;
        };
        match block.terminator {
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
    false
}

fn llvm_vector_dimension_supported(length: usize) -> bool {
    length <= usize::try_from(u32::MAX).unwrap_or(usize::MAX)
}

fn instruction_destination(instruction: &Instruction) -> Option<ValueId> {
    match instruction {
        Instruction::Constant { destination, .. }
        | Instruction::Default { destination, .. }
        | Instruction::Phi { destination, .. }
        | Instruction::Load { destination, .. }
        | Instruction::Copy { destination, .. }
        | Instruction::Unary { destination, .. }
        | Instruction::Binary { destination, .. }
        | Instruction::Cast { destination, .. }
        | Instruction::Call { destination, .. }
        | Instruction::DispatchSubmit { destination, .. }
        | Instruction::DispatchAwait { destination, .. }
        | Instruction::Input { destination, .. }
        | Instruction::Vector { destination, .. }
        | Instruction::Index { destination, .. }
        | Instruction::Member { destination, .. }
        | Instruction::Length { destination, .. }
        | Instruction::SizeOf { destination, .. }
        | Instruction::Allocate { destination, .. }
        | Instruction::LoadStatic { destination, .. } => Some(*destination),
        Instruction::Store { .. }
        | Instruction::SetIndex { .. }
        | Instruction::SetMemberIndex { .. }
        | Instruction::SetFieldIndex { .. }
        | Instruction::SetStaticIndex { .. }
        | Instruction::SetMember { .. }
        | Instruction::SetField { .. }
        | Instruction::Print { .. }
        | Instruction::ClearScreen { .. }
        | Instruction::Beep { .. }
        | Instruction::Delete { .. }
        | Instruction::EnsureClass { .. }
        | Instruction::StoreStatic { .. } => None,
    }
}

#[allow(clippy::too_many_lines, clippy::too_many_arguments)]
fn validate_instruction(
    module: &Module,
    function: &Function,
    instruction: &Instruction,
    values: &HashMap<ValueId, Type>,
    symbols: &HashMap<SymbolId, Type>,
    functions: &HashMap<ValueId, &str>,
    strings: &[(ValueId, String)],
    module_functions: &std::collections::HashSet<&str>,
    intrinsics: &mut BTreeSet<&'static str>,
) -> Result<(), String> {
    let supported = match instruction {
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
            Constant::EndOfFile => false,
        },
        Instruction::Phi { ty, .. } => llvm_type(ty).is_some(),
        Instruction::Default {
            ty,
            dimensions,
            dynamic_dimensions,
            ..
        } => {
            (dimensions.is_empty() && dynamic_dimensions.is_empty() && llvm_type(ty).is_some())
                || (matches!(ty, Type::Vector { element, dimensions: ty_dimensions, .. }
                if ty_dimensions.len() == 1
                    && dimensions.len() == 1
                    && dynamic_dimensions.is_empty()
                    && llvm_vector_dimension_supported(dimensions[0])
                    && llvm_type(ty).is_some()
                    && llvm_type(element).is_some_and(|element| matches!(
                    element,
                    "i1" | "i8" | "i16" | "i32" | "i64" | "float" | "double"
                ))))
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
            if let Some(intrinsic) = checked_intrinsic_declaration(left_ty, operator) {
                intrinsics.insert(intrinsic);
            }
            if operator == "Power"
                && let Some(intrinsic) = pow_intrinsic_declaration(ty)
            {
                intrinsics.insert(intrinsic);
            }
            if operator == "IS" {
                matches!(ty, Type::Boolean)
            } else {
                binary_supported(operator, left_ty, right_ty, ty)
            }
        }
        Instruction::Cast { value, ty, .. } => {
            cast_supported(values.get(value), ty)
                || values.get(value).is_some_and(|source| {
                    llvm_type(source) == Some("{ i1, double }")
                        && matches!(ty, Type::Float(_) | Type::FloatLiteral)
                })
        }
        Instruction::Call {
            callee, arguments, ..
        } => match functions.get(callee).copied() {
            Some("$for_condition") => for_condition_supported(arguments, values),
            Some("HOST.Random.Seed") => arguments.len() == 1,
            Some("HOST.Random.Random") => arguments.is_empty(),
            Some(name) if is_bn_rt_host_call(name) => bn_rt_call_supported(name, arguments, values),
            Some(name)
                if name.ends_with(".Queue.Concurrent")
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
                arguments.iter().all(|argument| {
                    values
                        .get(argument)
                        .is_some_and(|ty| llvm_type(ty).is_some())
                })
            }
            Some(name) if bnmath_method(module, name).is_some() => bnmath_call_supported(
                bnmath_method(module, name).unwrap_or(name),
                arguments,
                values,
            ),
            Some(name) if is_bndata_dataframe_call(module, name) => {
                matches!(
                    bndata_dataframe_method(name),
                    Some("constructor") if arguments.len() == 1
                ) || matches!(
                    bndata_dataframe_method(name),
                    Some("row_count" | "column_count") if arguments.len() == 1
                ) || matches!(
                    bndata_dataframe_method(name),
                    Some("add_integer_column") if arguments.len() == 3
                        && values.get(&arguments[1]) == Some(&Type::String)
                        && values.get(&arguments[2]).is_some_and(is_int_vector)
                ) || matches!(
                    bndata_dataframe_method(name),
                    Some("add_string_column") if arguments.len() == 3
                        && values.get(&arguments[1]) == Some(&Type::String)
                        && values.get(&arguments[2]).is_some_and(is_string_vector)
                ) || matches!(
                    bndata_dataframe_method(name),
                    Some("add_float_column") if arguments.len() == 3
                        && values.get(&arguments[1]) == Some(&Type::String)
                        && values.get(&arguments[2]).is_some_and(is_float_vector)
                ) || matches!(
                    bndata_dataframe_method(name),
                    Some("add_boolean_column") if arguments.len() == 3
                        && values.get(&arguments[1]) == Some(&Type::String)
                        && values.get(&arguments[2]).is_some_and(is_bool_vector)
                ) || matches!(
                    bndata_dataframe_method(name),
                    Some("column_name") if arguments.len() == 2
                        && values.get(&arguments[1]).and_then(llvm_type).is_some_and(integer_llvm)
                ) || matches!(
                    bndata_dataframe_method(name),
                    Some("set_label") if arguments.len() == 3
                        && values.get(&arguments[1]) == Some(&Type::String)
                        && values.get(&arguments[2]) == Some(&Type::String)
                ) || matches!(
                    bndata_dataframe_method(name),
                    Some("get_string" | "get_integer" | "get_float" | "get_boolean") if arguments.len() == 3
                        && values.get(&arguments[1]).and_then(llvm_type).is_some_and(integer_llvm)
                        && values.get(&arguments[2]).is_some_and(|ty| {
                            *ty == Type::String || llvm_type(ty) == Some("{ i1, ptr, i64 }")
                        })
                ) || matches!(
                    bndata_dataframe_method(name),
                        Some("mean" | "median" | "quartile1" | "quartile3" | "mode" | "stdev" | "variance" | "range" | "min" | "max") if arguments.len() == 2
                        && values.get(&arguments[1]).is_some_and(|ty| {
                            *ty == Type::String || llvm_type(ty) == Some("{ i1, ptr, i64 }")
                        })
                ) || matches!(
                    bndata_dataframe_method(name),
                    Some("zscore") if arguments.len() == 2
                        && values.get(&arguments[1]).is_some_and(|ty| {
                            *ty == Type::String || llvm_type(ty) == Some("{ i1, ptr, i64 }")
                        })
                ) || matches!(
                    bndata_dataframe_method(name),
                    Some("copy_integer" | "copy_float") if arguments.len() == 3
                        && values.get(&arguments[1]) == Some(&Type::String)
                        && matches!(values.get(&arguments[2]), Some(Type::Pointer { .. }))
                ) || matches!(
                    bndata_dataframe_method(name),
                    Some("select") if arguments.len() == 3
                        && matches!(values.get(&arguments[1]), Some(Type::Vector { .. }))
                        && matches!(values.get(&arguments[2]), Some(Type::Vector { .. }))
                ) || matches!(
                    bndata_dataframe_method(name),
                    Some("slice") if arguments.len() == 5
                        && arguments[1..].iter().all(|argument| values.get(argument).and_then(llvm_type).is_some_and(integer_llvm))
                ) || matches!(
                    bndata_dataframe_method(name),
                    Some("transpose") if arguments.len() == 1
                ) || matches!(
                    bndata_dataframe_method(name),
                    Some("append_rows" | "append_columns") if arguments.len() == 2
                ) || matches!(
                    bndata_dataframe_method(name),
                    Some("join" | "left_join" | "right_join" | "full_join") if arguments.len() == 4
                        && values.get(&arguments[2]) == Some(&Type::String)
                        && values.get(&arguments[3]) == Some(&Type::String)
                ) || matches!(
                    bndata_dataframe_method(name),
                    Some("convert_integer" | "convert_float") if arguments.len() == 2
                        && values.get(&arguments[1]) == Some(&Type::String)
                )
            }
            Some(name) if is_bndata_function(name) => {
                (name.ends_with("ReadCSV") && arguments.len() == 3)
                    || (name.ends_with("WriteCSV") && arguments.len() == 4)
            }
            Some(name) if bnlog_method(module, name).is_some() => {
                match bnlog_method(module, name) {
                    Some("fields_constructor" | "logger_constructor") => arguments.len() == 1,
                    Some("fields_set_string") => {
                        arguments.len() == 3
                            && bnlog_resource_kind(module, &values[&arguments[0]]) == Some("Fields")
                            && values.get(&arguments[1]) == Some(&Type::String)
                            && values.get(&arguments[2]) == Some(&Type::String)
                    }
                    Some("logger_add_file") => {
                        arguments.len() == 3
                            && bnlog_resource_kind(module, &values[&arguments[0]]) == Some("Logger")
                            && values.get(&arguments[1]) == Some(&Type::String)
                            && values
                                .get(&arguments[2])
                                .and_then(llvm_type)
                                .is_some_and(integer_llvm)
                    }
                    Some("logger_log") => {
                        arguments.len() == 4
                            && bnlog_resource_kind(module, &values[&arguments[0]]) == Some("Logger")
                            && values
                                .get(&arguments[1])
                                .and_then(llvm_type)
                                .is_some_and(integer_llvm)
                            && values.get(&arguments[2]) == Some(&Type::String)
                            && bnlog_resource_kind(module, &values[&arguments[3]]) == Some("Fields")
                    }
                    Some("logger_flush" | "logger_close") => {
                        arguments.len() == 2
                            && bnlog_resource_kind(module, &values[&arguments[0]]) == Some("Logger")
                            && values
                                .get(&arguments[1])
                                .and_then(llvm_type)
                                .is_some_and(integer_llvm)
                    }
                    _ => false,
                }
            }
            Some("TimeZone.Parse") => {
                arguments.len() == 1
                    && strings.iter().any(|(value, text)| {
                        Some(value) == arguments.first() && is_canonical_timezone(text)
                    })
            }
            Some("ASC") => arguments.len() == 1 && values.get(&arguments[0]) == Some(&Type::String),
            Some("TOLOWER" | "TOUPPER") => {
                arguments.len() == 1 && values.get(&arguments[0]) == Some(&Type::String)
            }
            Some("CHAR") => {
                arguments.len() == 1
                    && values
                        .get(&arguments[0])
                        .is_some_and(|ty| matches!(ty, Type::Integer(_) | Type::IntegerLiteral(_)))
            }
            Some(name)
                if module_functions.contains(name.strip_prefix("@super:").unwrap_or(name)) =>
            {
                arguments.iter().all(|argument| {
                    values
                        .get(argument)
                        .is_some_and(|ty| llvm_type(ty).is_some() || is_void_type(ty))
                })
            }
            Some(name) => {
                return Err(unsupported_instruction(
                    module,
                    function,
                    instruction,
                    &unsupported_call_detail(module, name),
                ));
            }
            None => { matches!(values.get(callee), Some(Type::Function { parameters, .. })
                if parameters.len() == arguments.len()
                    && arguments.iter().all(|argument| values.get(argument).and_then(llvm_type).is_some())) },
        },
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
                || values.get(object).is_some_and(is_native_pointer))
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
        Instruction::Delete { value, .. } => {
            values.get(value).is_some_and(|ty| {
                is_native_pointer(ty)
                    || matches!(ty, Type::Pointer { element, .. } if matches!(element.as_ref(), Type::Vector { .. }))
            })
                || values.get(value).is_some_and(|ty| {
                    llvm_type(ty) == Some("{ ptr, i32 }") || llvm_type(ty) == Some("ptr")
                        || llvm_type(ty) == Some("{ i1, ptr, i64 }")
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
                            && indices.len() == dimensions.len()
                            && dimensions.iter().all(|dimension| *dimension != u64::MAX)
                            && llvm_type(element).is_some()
                    }
                    _ => indices.len() == 1 && is_native_pointer(container),
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
            let receiver_supported = symbols.get(symbol).and_then(llvm_type) == Some("ptr")
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
        Instruction::SetMemberIndex { .. } | Instruction::SetStaticIndex { .. } => false,
        Instruction::EnsureClass { .. } => true,
        Instruction::Member {
            object,
            name,
            owner,
            ty,
            ..
        } => {
            (owner == "Error"
                && name == "Message"
                && matches!(
                    values.get(object).and_then(llvm_type),
                    Some("{ i1, ptr }" | "{ i1, ptr, i32 }" | "{ i1, ptr, i64 }")
                ))
                || (owner == "Error"
                    && name == "Code"
                    && values.get(object).and_then(llvm_type) == Some("{ i1, ptr, i64 }"))
                || (values.get(object).and_then(llvm_type) == Some("ptr")
                    && llvm_type(ty).is_some())
        }
        Instruction::SetMember {
            object, value, ty, ..
        } => {
            values.get(object).and_then(llvm_type) == Some("ptr")
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
            path.len() == 1
                && symbols.get(symbol).and_then(llvm_type) == Some("ptr")
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
            arguments.is_empty()
                && llvm_type(ty) == Some("{ i1, ptr, i64 }")
                && values.get(queue).and_then(llvm_type) == Some("{ i1, ptr, i64 }")
                && functions.get(task).is_some_and(|name| {
                    module
                        .functions
                        .iter()
                        .find(|candidate| candidate.name == **name)
                        .is_some_and(|task| {
                            task.parameters.is_empty() && is_void_type(&task.return_type)
                        })
                })
        }
        Instruction::DispatchAwait {
            ticket,
            timeout,
            ty,
            ..
        } => {
            values.get(ticket).and_then(llvm_type) == Some("{ i1, ptr, i64 }")
                && values
                    .get(timeout)
                    .and_then(llvm_type)
                    .is_some_and(integer_llvm)
                && matches!(ty, Type::Alternative(alternatives) if void_or_error(alternatives))
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

fn for_condition_supported(arguments: &[ValueId], values: &HashMap<ValueId, Type>) -> bool {
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
