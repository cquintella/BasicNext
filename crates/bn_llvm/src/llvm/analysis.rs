#![allow(clippy::wildcard_imports, clippy::match_same_arms)]
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
    let mut owned_object_results = HashMap::new();
    let mut owned_log_results = HashMap::new();
    let loaded_symbols = function
        .blocks
        .iter()
        .flat_map(|block| &block.instructions)
        .filter_map(|instruction| match instruction {
            Instruction::Load {
                destination,
                symbol,
                ..
            } => Some((*destination, *symbol)),
            _ => None,
        })
        .collect::<HashMap<_, _>>();
    let released_symbols = function
        .blocks
        .iter()
        .flat_map(|block| &block.instructions)
        .filter_map(|instruction| match instruction {
            Instruction::Release { value, .. } => loaded_symbols.get(value).copied(),
            _ => None,
        })
        .collect::<HashSet<_>>();

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
                    if is_class_type(module, ty) {
                        owned_object_results.insert(
                            *destination,
                            destructor_symbol(module, ty).unwrap_or_default(),
                        );
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
                    if is_class_type(module, ty) || is_region_type(ty) {
                        owned_object_results.insert(
                            *destination,
                            destructor_symbol(module, ty).unwrap_or_default(),
                        );
                    }
                    values.insert(*destination, ty.clone());
                }
                Instruction::Release { .. } => {
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
    Ok(LoweringAnalysis {
        values,
        symbols,
        functions,
        strings,
        input_count,
        input_targets,
        input_symbols,
        released_symbols,
        owned_string_results,
        owned_struct_results,
        owned_object_results,
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
