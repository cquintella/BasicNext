use std::collections::{HashMap, HashSet};

use bn_source::{Position, Revision, SourceId, Span};
use bn_types::{IntegerType, Type};

use super::{Diagnostic, Module, Terminator, invalid_ir};

#[path = "validate/fields.rs"]
mod fields;
use fields::{
    receiver_matches_owner, validate_field_layouts, validate_field_reference,
    validate_resolved_field_path,
};

fn instruction_defines(instruction: &super::Instruction) -> Option<super::ValueId> {
    match instruction {
        super::Instruction::Constant { destination, .. }
        | super::Instruction::Default { destination, .. }
        | super::Instruction::Phi { destination, .. }
        | super::Instruction::Load { destination, .. }
        | super::Instruction::Copy { destination, .. }
        | super::Instruction::Unary { destination, .. }
        | super::Instruction::Binary { destination, .. }
        | super::Instruction::Cast { destination, .. }
        | super::Instruction::Call { destination, .. }
        | super::Instruction::DispatchSubmit { destination, .. }
        | super::Instruction::DispatchAwait { destination, .. }
        | super::Instruction::Input { destination, .. }
        | super::Instruction::Vector { destination, .. }
        | super::Instruction::Index { destination, .. }
        | super::Instruction::Member { destination, .. }
        | super::Instruction::Length { destination, .. }
        | super::Instruction::SizeOf { destination, .. }
        | super::Instruction::Allocate { destination, .. }
        | super::Instruction::LoadStatic { destination, .. } => Some(*destination),
        super::Instruction::Store { .. }
        | super::Instruction::SetIndex { .. }
        | super::Instruction::SetMemberIndex { .. }
        | super::Instruction::SetFieldIndex { .. }
        | super::Instruction::SetStaticIndex { .. }
        | super::Instruction::SetMember { .. }
        | super::Instruction::SetField { .. }
        | super::Instruction::Print { .. }
        | super::Instruction::ClearScreen { .. }
        | super::Instruction::Beep { .. }
        | super::Instruction::Release { .. }
        | super::Instruction::EnsureClass { .. }
        | super::Instruction::StoreStatic { .. } => None,
    }
}

/// Enumerates every SSA operand read by an instruction.
#[must_use]
pub fn instruction_uses(instruction: &super::Instruction) -> Vec<super::ValueId> {
    match instruction {
        super::Instruction::Copy { source, .. }
        | super::Instruction::Unary {
            operand: source, ..
        }
        | super::Instruction::Cast { value: source, .. }
        | super::Instruction::Length { vector: source, .. }
        | super::Instruction::SizeOf { value: source, .. }
        | super::Instruction::Store { value: source, .. }
        | super::Instruction::Release { value: source, .. } => vec![*source],
        super::Instruction::Binary { left, right, .. } => vec![*left, *right],
        super::Instruction::Call {
            callee, arguments, ..
        } => {
            let mut used = vec![*callee];
            used.extend(arguments.iter().copied());
            used
        }
        super::Instruction::DispatchSubmit {
            callee,
            queue,
            task,
            arguments,
            ..
        } => {
            let mut used = vec![*callee, *queue, *task];
            used.extend(arguments.iter().copied());
            used
        }
        super::Instruction::DispatchAwait {
            callee,
            ticket,
            timeout,
            ..
        } => vec![*callee, *ticket, *timeout],
        super::Instruction::Vector { values, .. } | super::Instruction::Print { values, .. } => {
            values.clone()
        }
        super::Instruction::Index { object, index, .. } => vec![*object, *index],
        super::Instruction::Member { object, .. }
        | super::Instruction::ClearScreen {
            console: object, ..
        }
        | super::Instruction::Beep {
            console: object, ..
        } => vec![*object],
        super::Instruction::SetIndex { indices, value, .. }
        | super::Instruction::SetFieldIndex { indices, value, .. }
        | super::Instruction::SetStaticIndex { indices, value, .. } => {
            let mut used = indices.clone();
            used.push(*value);
            used
        }
        super::Instruction::SetMemberIndex {
            object,
            indices,
            value,
            ..
        } => {
            let mut used = vec![*object];
            used.extend(indices.iter().copied());
            used.push(*value);
            used
        }
        super::Instruction::SetMember { object, value, .. } => vec![*object, *value],
        super::Instruction::SetField { value, .. }
        | super::Instruction::StoreStatic { value, .. } => vec![*value],
        super::Instruction::Allocate { arguments, .. } => arguments.clone(),
        super::Instruction::Default {
            dynamic_dimensions, ..
        } => dynamic_dimensions.clone(),
        super::Instruction::Phi { incoming, .. } => {
            incoming.iter().map(|(_, value)| *value).collect()
        }
        super::Instruction::Input { prompt, .. } => prompt.iter().copied().collect(),
        super::Instruction::EnsureClass { .. }
        | super::Instruction::LoadStatic { .. }
        | super::Instruction::Constant { .. }
        | super::Instruction::Load { .. } => Vec::new(),
    }
}

#[allow(clippy::too_many_lines)]
/// Enforces language-level IR well-formedness. Backend capability gaps are
/// target-support concerns and must be checked separately by `validate_for`;
/// they are not validation failures.
///
/// # Errors
///
/// Returns `INVALID_IR` when the module violates an IR structural or
/// definite-assignment invariant.
pub fn validate(module: &Module) -> Result<(), Diagnostic> {
    validate_class_bases(module)?;
    validate_field_layouts(module)?;
    validate_function_kinds(module)?;
    for function in &module.functions {
        let block_count = u32::try_from(function.blocks.len())
            .map_err(|_| invalid_ir("function has too many basic blocks", function.span))?;
        if function.entry.0 >= block_count {
            return Err(invalid_ir(
                "function entry block does not exist",
                function.span,
            ));
        }
        for (index, block) in function.blocks.iter().enumerate() {
            if block.id.0
                != u32::try_from(index)
                    .map_err(|_| invalid_ir("function has too many basic blocks", function.span))?
            {
                return Err(invalid_ir(
                    "basic block IDs must be dense and ordered",
                    function.span,
                ));
            }
            validate_successor_bounds(block, block_count, function.span)?;
        }

        let entry = usize::try_from(function.entry.0)
            .map_err(|_| invalid_ir("function entry block does not fit", function.span))?;
        let mut successors = vec![Vec::new(); function.blocks.len()];
        let mut predecessors = vec![Vec::new(); function.blocks.len()];
        for (index, block) in function.blocks.iter().enumerate() {
            successors[index] = block_successors(&block.terminator);
            for &target in &successors[index] {
                let target = usize::try_from(target)
                    .map_err(|_| invalid_ir("block target does not fit", function.span))?;
                predecessors[target].push(index);
            }
        }

        let reachable = reachable_blocks(entry, &successors);
        let mut all_values = HashSet::new();
        let mut value_types = HashMap::new();
        for block in &function.blocks {
            for instruction in &block.instructions {
                if let Some(destination) = instruction_defines(instruction)
                    && !all_values.insert(destination)
                {
                    return Err(invalid_ir(
                        format!(
                            "value %{} is defined by more than one instruction in function {}",
                            destination.0, function.name
                        ),
                        instruction.span(),
                    ));
                }
                if let Some(destination) = instruction_defines(instruction) {
                    if let Some(ty) = instruction_type(instruction) {
                        value_types.insert(destination, ty.clone());
                    } else if matches!(
                        instruction,
                        super::Instruction::Length { .. } | super::Instruction::SizeOf { .. }
                    ) {
                        value_types.insert(destination, Type::Integer(IntegerType::Int32));
                    }
                }
            }
        }
        let mut incoming = vec![HashSet::new(); function.blocks.len()];
        let mut outgoing = vec![HashSet::new(); function.blocks.len()];
        for index in 0..function.blocks.len() {
            if reachable[index] && index != entry {
                incoming[index].clone_from(&all_values);
                outgoing[index].clone_from(&all_values);
            }
        }
        loop {
            let mut changed = false;
            for index in 0..function.blocks.len() {
                if !reachable[index] {
                    continue;
                }
                let mut next_incoming = if index == entry {
                    HashSet::new()
                } else {
                    let mut paths = predecessors[index]
                        .iter()
                        .copied()
                        .filter(|predecessor| reachable[*predecessor]);
                    match paths.next() {
                        Some(first) => {
                            let mut intersection = outgoing[first].clone();
                            for predecessor in paths {
                                intersection.retain(|value| outgoing[predecessor].contains(value));
                            }
                            intersection
                        }
                        None => HashSet::new(),
                    }
                };
                if next_incoming != incoming[index] {
                    incoming[index].clone_from(&next_incoming);
                    changed = true;
                }
                for instruction in &function.blocks[index].instructions {
                    if let Some(destination) = instruction_defines(instruction) {
                        next_incoming.insert(destination);
                    }
                }
                if next_incoming != outgoing[index] {
                    outgoing[index] = next_incoming;
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }
        let phi_context = PhiContext {
            predecessors: &predecessors,
            reachable: &reachable,
            outgoing: &outgoing,
            value_types: &value_types,
        };

        for (index, block) in function.blocks.iter().enumerate() {
            if !reachable[index] {
                continue;
            }
            let mut defined = incoming[index].clone();
            let mut saw_non_phi = false;
            for instruction in &block.instructions {
                if let super::Instruction::Phi {
                    incoming: phi_incoming,
                    ..
                } = instruction
                {
                    if saw_non_phi {
                        return Err(invalid_ir(
                            "Phi instructions must precede other instructions in a block",
                            instruction.span(),
                        ));
                    }
                    validate_phi(
                        index,
                        phi_incoming,
                        &phi_context,
                        instruction,
                        function.span,
                    )?;
                    if let Some(destination) = instruction_defines(instruction) {
                        defined.insert(destination);
                    }
                    continue;
                }
                saw_non_phi = true;
                for used in instruction_uses(instruction) {
                    if !defined.contains(&used)
                        && !instruction_defines(instruction)
                            .is_some_and(|defined_id| defined_id == used)
                    {
                        return Err(invalid_ir(
                            "instruction uses a value that is not defined on every executable path",
                            function.span,
                        ));
                    }
                }
                validate_instruction_types(module, instruction, &value_types)?;
                if let Some(destination) = instruction_defines(instruction) {
                    defined.insert(destination);
                }
            }
            match &block.terminator {
                Terminator::Branch { condition, .. } => {
                    if !defined.contains(condition) {
                        return Err(invalid_ir("branch condition is not defined", function.span));
                    }
                    if !is_boolean(value_types.get(condition)) {
                        return Err(invalid_ir(
                            "branch condition must have BOOLEAN type",
                            function.span,
                        ));
                    }
                }
                Terminator::Return { value: Some(value) } | Terminator::Stop { code: value }
                    if !defined.contains(value) =>
                {
                    return Err(invalid_ir("terminator value is not defined", function.span));
                }
                Terminator::Return { value } => {
                    validate_return_type(function, value.as_ref(), &value_types)?;
                }
                Terminator::Stop { code } if !is_integer(value_types.get(code)) => {
                    return Err(invalid_ir(
                        "stop code must have an integer type",
                        function.span,
                    ));
                }
                Terminator::Jump { .. } | Terminator::Stop { .. } => {}
            }
        }
    }
    Ok(())
}

fn default_module_span() -> Span {
    let position = Position {
        source_id: SourceId::UNKNOWN,
        revision: Revision::UNKNOWN,
        offset: 0,
        line: 1,
        column: 1,
    };
    Span {
        start: position,
        end: position,
    }
}

fn validate_class_bases(module: &Module) -> Result<(), Diagnostic> {
    let span = module
        .functions
        .first()
        .map_or_else(default_module_span, |function| function.span);
    for (class, base) in &module.class_bases {
        if class.is_empty() || base.is_empty() {
            return Err(invalid_ir(
                "class and base identities cannot be empty",
                span,
            ));
        }
        let mut seen = HashSet::new();
        let mut current = class.as_str();
        while let Some(parent) = module.class_bases.get(current) {
            if !seen.insert(current) {
                return Err(invalid_ir(
                    "class inheritance metadata must be acyclic",
                    span,
                ));
            }
            current = parent;
        }
        for identity in [class, base] {
            let fields = format!("{identity}.$fields");
            if !module
                .functions
                .iter()
                .any(|function| function.name == fields)
            {
                return Err(invalid_ir(
                    format!("class layout metadata references missing class '{identity}'"),
                    span,
                ));
            }
        }
    }
    Ok(())
}

struct PhiContext<'a> {
    predecessors: &'a [Vec<usize>],
    reachable: &'a [bool],
    outgoing: &'a [HashSet<super::ValueId>],
    value_types: &'a HashMap<super::ValueId, Type>,
}

fn validate_phi(
    block: usize,
    incoming: &[(super::BlockId, super::ValueId)],
    context: &PhiContext<'_>,
    instruction: &super::Instruction,
    span: super::Span,
) -> Result<(), Diagnostic> {
    let expected = context.predecessors[block]
        .iter()
        .copied()
        .filter(|predecessor| context.reachable[*predecessor])
        .collect::<HashSet<_>>();
    let mut actual = HashSet::new();
    for (predecessor_id, value) in incoming {
        let predecessor = usize::try_from(predecessor_id.0)
            .map_err(|_| invalid_ir("Phi predecessor does not fit", span))?;
        if predecessor >= context.predecessors.len()
            || !context.reachable[predecessor]
            || !actual.insert(predecessor)
            || !context.outgoing[predecessor].contains(value)
        {
            return Err(invalid_ir(
                format!(
                    "Phi in block {} has invalid incoming predecessor {} and value %{}",
                    block, predecessor, value.0
                ),
                span,
            ));
        }
    }
    if actual != expected {
        return Err(invalid_ir(
            "Phi incoming edges must match the block predecessors exactly",
            span,
        ));
    }
    let super::Instruction::Phi { ty, .. } = instruction else {
        unreachable!("validate_phi called for a non-Phi instruction");
    };
    if incoming.iter().any(|(_, value)| {
        context
            .value_types
            .get(value)
            .is_none_or(|incoming_type| !types_compatible(incoming_type, ty))
    }) {
        return Err(invalid_ir(
            "Phi incoming values must match the Phi result type",
            instruction.span(),
        ));
    }
    Ok(())
}

fn instruction_type(instruction: &super::Instruction) -> Option<&Type> {
    match instruction {
        super::Instruction::Constant { ty, .. }
        | super::Instruction::Default { ty, .. }
        | super::Instruction::Phi { ty, .. }
        | super::Instruction::Load { ty, .. }
        | super::Instruction::Copy { ty, .. }
        | super::Instruction::Unary { ty, .. }
        | super::Instruction::Binary { ty, .. }
        | super::Instruction::Cast { ty, .. }
        | super::Instruction::Call { ty, .. }
        | super::Instruction::DispatchSubmit { ty, .. }
        | super::Instruction::DispatchAwait { ty, .. }
        | super::Instruction::Input { ty, .. }
        | super::Instruction::Vector { ty, .. }
        | super::Instruction::Index { ty, .. }
        | super::Instruction::Member { ty, .. }
        | super::Instruction::Allocate { ty, .. }
        | super::Instruction::LoadStatic { ty, .. } => Some(ty),
        _ => None,
    }
}

#[allow(clippy::too_many_lines)] // The instruction contract remains exhaustive in one match.
fn validate_instruction_types(
    module: &Module,
    instruction: &super::Instruction,
    value_types: &HashMap<super::ValueId, Type>,
) -> Result<(), Diagnostic> {
    let span = instruction.span();
    match instruction {
        super::Instruction::Constant { value, ty, .. } if !constant_matches_type(value, ty) => {
            return Err(invalid_ir(
                "constant value does not match its declared type",
                span,
            ));
        }
        super::Instruction::Length { destination, .. }
        | super::Instruction::SizeOf { destination, .. } => {
            if !is_integer(value_types.get(destination)) {
                return Err(invalid_ir(
                    "length and sizeof results must have an integer type",
                    span,
                ));
            }
        }
        super::Instruction::Copy { source, ty, .. }
            if value_types
                .get(source)
                .is_none_or(|source_type| !types_compatible(source_type, ty)) =>
        {
            return Err(invalid_ir("copied value does not match its type", span));
        }
        super::Instruction::Store { value, ty, .. }
        | super::Instruction::SetMember { value, ty, .. }
        | super::Instruction::SetField { value, ty, .. }
        | super::Instruction::StoreStatic { value, ty, .. }
        | super::Instruction::SetIndex { value, ty, .. }
        | super::Instruction::SetMemberIndex { value, ty, .. }
        | super::Instruction::SetFieldIndex { value, ty, .. }
        | super::Instruction::SetStaticIndex { value, ty, .. }
            if value_types
                .get(value)
                .is_none_or(|value_type| !assignment_types_compatible(value_type, ty)) =>
        {
            return Err(invalid_ir(
                "stored value does not match its declared type",
                span,
            ));
        }
        super::Instruction::Default {
            dynamic_dimensions, ..
        } if dynamic_dimensions
            .iter()
            .any(|value| !is_integer(value_types.get(value))) =>
        {
            return Err(invalid_ir(
                "dynamic vector dimensions must have integer type",
                span,
            ));
        }
        super::Instruction::Vector { values, ty, .. } => {
            let Type::Vector { dimensions, .. } = ty else {
                return Err(invalid_ir(
                    "vector instruction must produce a vector type",
                    span,
                ));
            };
            if dimensions.first().is_some_and(|dimension| {
                *dimension != u64::MAX && *dimension != values.len() as u64
            }) {
                return Err(invalid_ir(
                    "vector value count does not match its first dimension",
                    span,
                ));
            }
            let expected_element = vector_element_type(ty);
            if values.iter().any(|value| {
                value_types
                    .get(value)
                    .is_none_or(|value_type| !types_compatible(value_type, &expected_element))
            }) {
                return Err(invalid_ir(
                    "vector element does not match the vector element type",
                    span,
                ));
            }
        }
        super::Instruction::Unary {
            operator,
            operand,
            ty,
            ..
        } => {
            let Some(operand_type) = value_types.get(operand) else {
                return Err(invalid_ir("unary operand has no type", span));
            };
            let valid = match operator.as_str() {
                "Minus" => is_numeric_type(operand_type) && is_numeric_type(ty),
                "NOT" => {
                    (is_boolean_type(operand_type) && is_boolean_type(ty))
                        || (is_integer_type(operand_type) && is_integer_type(ty))
                }
                _ => false,
            };
            if !valid {
                return Err(invalid_ir(
                    format!("unary operator {operator} has incompatible operand/result types"),
                    span,
                ));
            }
        }
        super::Instruction::Binary {
            operator,
            left,
            right,
            ty,
            ..
        } => {
            let (Some(left_type), Some(right_type)) =
                (value_types.get(left), value_types.get(right))
            else {
                return Err(invalid_ir("binary operand has no type", span));
            };
            if !valid_binary_types(operator, left_type, right_type, ty) {
                return Err(invalid_ir(
                    format!("binary operator {operator} has incompatible operand/result types"),
                    span,
                ));
            }
        }
        super::Instruction::Cast { value, ty, .. } => {
            let Some(source_type) = value_types.get(value) else {
                return Err(invalid_ir("cast source has no type", span));
            };
            if !valid_cast_types(source_type, ty) {
                return Err(invalid_ir(
                    "cast source and target types are not convertible",
                    span,
                ));
            }
        }
        super::Instruction::Call {
            callee,
            arguments,
            ty,
            ..
        } => {
            let Some(callee_type) = value_types.get(callee) else {
                return Err(invalid_ir("call callee has no type", span));
            };
            if let Type::Function {
                parameters,
                return_type,
            } = callee_type
            {
                let vector_overload = parameters.len() == 2
                    && arguments.len() == 1
                    && value_types.get(&arguments[0]).is_some_and(|argument_type| {
                        matches!(
                            argument_type,
                            Type::Vector { element, dimensions }
                                if dimensions.len() == 1 && is_numeric_type(element)
                        ) || matches!(
                            argument_type,
                            Type::Pointer { element, .. } if is_numeric_type(element)
                        )
                    });
                if (!vector_overload && parameters.len() != arguments.len())
                    || arguments
                        .iter()
                        .zip(parameters)
                        .any(|(argument, parameter)| {
                            value_types.get(argument).is_none_or(|argument_type| {
                                !vector_overload && !call_types_compatible(argument_type, parameter)
                            })
                        })
                    || !call_types_compatible(return_type, ty)
                {
                    return Err(invalid_ir(
                        "call arguments or result do not match the callee signature",
                        span,
                    ));
                }
            } else if !matches!(callee_type, Type::Unknown) {
                return Err(invalid_ir("call callee must have a function type", span));
            }
        }
        super::Instruction::DispatchSubmit { queue, task, .. } => {
            if let Some(queue_type) = value_types.get(queue)
                && !contains_named_type(queue_type, "Queue")
            {
                return Err(invalid_ir(
                    "dispatch submission requires a BNDispatch.Queue",
                    span,
                ));
            }
            if let Some(task_type) = value_types.get(task)
                && !matches!(task_type, Type::Function { .. } | Type::Unknown)
            {
                return Err(invalid_ir(
                    "dispatch submission task must be a function",
                    span,
                ));
            }
        }
        super::Instruction::DispatchAwait {
            ticket, timeout, ..
        } => {
            if let Some(ticket_type) = value_types.get(ticket)
                && !is_dispatch_wait_handle(ticket_type)
            {
                return Err(invalid_ir(
                    "dispatch wait requires a BNDispatch synchronization handle",
                    span,
                ));
            }
            if !is_integer(value_types.get(timeout)) {
                return Err(invalid_ir(
                    "dispatch await timeout must have an integer type",
                    span,
                ));
            }
        }
        super::Instruction::ClearScreen { console, .. }
        | super::Instruction::Beep { console, .. }
            if !matches!(value_types.get(console), Some(Type::HostConsole)) =>
        {
            return Err(invalid_ir(
                "console control instruction requires HOST.Console",
                span,
            ));
        }
        super::Instruction::Member { name, owner, .. }
        | super::Instruction::SetMember { name, owner, .. }
        | super::Instruction::SetMemberIndex { name, owner, .. }
            if name.is_empty() || owner.is_empty() =>
        {
            return Err(invalid_ir(
                "member name and owner class cannot be empty",
                span,
            ));
        }
        super::Instruction::Member {
            field: None, ty, ..
        } if !matches!(ty, Type::Function { .. }) => {
            return Err(invalid_ir(
                "record member access must carry a resolved field",
                span,
            ));
        }
        super::Instruction::SetMember { field: None, .. }
        | super::Instruction::SetMemberIndex { field: None, .. } => {
            return Err(invalid_ir(
                "record member store must carry a resolved field",
                span,
            ));
        }
        super::Instruction::Member {
            field: Some(field),
            name,
            owner,
            object,
            ty,
            ..
        } if !matches!(ty, Type::Function { .. }) => {
            let field_ty = validate_field_reference(module, owner, name, field, span)?;
            if value_types
                .get(object)
                .is_none_or(|receiver| !receiver_matches_owner(module, receiver, owner))
            {
                return Err(invalid_ir(
                    "member receiver does not match its owner layout",
                    span,
                ));
            }
            if !types_compatible(field_ty, ty) {
                return Err(invalid_ir(
                    "member result type does not match its field layout",
                    span,
                ));
            }
        }
        super::Instruction::SetMember {
            field: Some(field),
            name,
            owner,
            object,
            ty,
            ..
        } => {
            let field_ty = validate_field_reference(module, owner, name, field, span)?;
            if value_types
                .get(object)
                .is_none_or(|receiver| !receiver_matches_owner(module, receiver, owner))
            {
                return Err(invalid_ir(
                    "member receiver does not match its owner layout",
                    span,
                ));
            }
            if !types_compatible(ty, field_ty) {
                return Err(invalid_ir(
                    "member store type does not match its field layout",
                    span,
                ));
            }
        }
        super::Instruction::SetMemberIndex {
            field: Some(field),
            name,
            owner,
            object,
            indices,
            ty,
            ..
        } => {
            let field_ty = validate_field_reference(module, owner, name, field, span)?;
            if value_types
                .get(object)
                .is_none_or(|receiver| !receiver_matches_owner(module, receiver, owner))
            {
                return Err(invalid_ir(
                    "indexed member store receiver does not match its owner layout",
                    span,
                ));
            }
            if !index_result_matches(field_ty, ty) {
                return Err(invalid_ir(
                    "indexed member store type does not match its field layout",
                    span,
                ));
            }
            if indices.is_empty()
                || indices
                    .iter()
                    .any(|index| !is_integer(value_types.get(index)))
            {
                return Err(invalid_ir(
                    "indexed store requires at least one integer index",
                    span,
                ));
            }
        }
        super::Instruction::LoadStatic { class, field, .. }
        | super::Instruction::StoreStatic { class, field, .. }
        | super::Instruction::SetStaticIndex { class, field, .. }
            if class.is_empty() || field.is_empty() =>
        {
            return Err(invalid_ir(
                "static member class and field names cannot be empty",
                span,
            ));
        }
        super::Instruction::EnsureClass { class, .. } if class.is_empty() => {
            return Err(invalid_ir("class name cannot be empty", span));
        }
        super::Instruction::Allocate { type_name, .. } if type_name.is_empty() => {
            return Err(invalid_ir("allocated type name cannot be empty", span));
        }
        super::Instruction::Release {
            destructor: Some(destructor),
            ..
        } if destructor.is_empty() => {
            return Err(invalid_ir("destructor name cannot be empty", span));
        }
        super::Instruction::SetField {
            root_owner,
            path,
            fields,
            ty,
            ..
        } => {
            let field_ty =
                validate_resolved_field_path(module, root_owner, path, fields.as_deref(), span)?;
            if !types_compatible(ty, &field_ty) {
                return Err(invalid_ir(
                    "field store type does not match its field layout",
                    span,
                ));
            }
        }
        super::Instruction::SetFieldIndex {
            root_owner,
            path,
            fields,
            indices,
            ty,
            ..
        } => {
            let field_ty =
                validate_resolved_field_path(module, root_owner, path, fields.as_deref(), span)?;
            if !index_result_matches(&field_ty, ty) {
                return Err(invalid_ir(
                    "indexed field store type does not match its field layout",
                    span,
                ));
            }
            if indices.is_empty()
                || indices
                    .iter()
                    .any(|index| !is_integer(value_types.get(index)))
            {
                return Err(invalid_ir("indices must be non-empty integers", span));
            }
        }
        super::Instruction::Index {
            object, index, ty, ..
        } => {
            if !is_integer(value_types.get(index)) {
                return Err(invalid_ir("index must have an integer type", span));
            }
            if let Some(object_type) = value_types.get(object)
                && !index_result_matches(object_type, ty)
            {
                return Err(invalid_ir(
                    "indexed object and result types are incompatible",
                    span,
                ));
            }
        }
        super::Instruction::SetIndex { indices, .. }
        | super::Instruction::SetStaticIndex { indices, .. }
            if indices.is_empty()
                || indices
                    .iter()
                    .any(|index| !is_integer(value_types.get(index))) =>
        {
            return Err(invalid_ir(
                "indexed store requires at least one integer index",
                span,
            ));
        }
        _ => {}
    }
    Ok(())
}

fn constant_matches_type(value: &super::Constant, ty: &Type) -> bool {
    match value {
        super::Constant::Integer(_) => {
            matches!(ty, Type::Integer(_) | Type::IntegerLiteral(_))
        }
        super::Constant::Float(_) => matches!(ty, Type::Float(_) | Type::FloatLiteral),
        super::Constant::String(_) => matches!(ty, Type::String),
        super::Constant::Boolean(_) => matches!(ty, Type::Boolean),
        super::Constant::Null => {
            matches!(ty, Type::Null | Type::Pointer { .. })
                || matches!(ty, Type::Named(name) if name == "VOID")
        }
        super::Constant::NotAvailable => matches!(ty, Type::NotAvailable),
        super::Constant::EndOfFile => matches!(ty, Type::EndOfFile),
        // The fallback function-value helper intentionally emits an unknown
        // signature when lowering a synthesized constructor/default call.
        super::Constant::Function(_) => matches!(ty, Type::Function { .. } | Type::Unknown),
        super::Constant::Type(_) => matches!(
            ty,
            Type::TypeName(_)
                | Type::ImportedTypeName { .. }
                | Type::Named(_)
                | Type::Module(_)
                | Type::HostClock
                | Type::HostRandom
                | Type::HostFileSystem
                | Type::HostNet
                | Type::HostExec
        ),
        super::Constant::HostConsole => matches!(ty, Type::HostConsole),
        super::Constant::HostArgs => matches!(ty, Type::HostArgs),
    }
}

fn valid_binary_types(operator: &str, left: &Type, right: &Type, result: &Type) -> bool {
    match operator {
        "Assign" | "NotEqual" => is_comparable(left, right) && is_boolean_type(result),
        "Less" | "LessEqual" | "Greater" | "GreaterEqual" => {
            is_numeric_pair(left, right) && is_boolean_type(result)
        }
        "AND" | "OR" | "XOR" => {
            (is_boolean_type(left) && is_boolean_type(right) && is_boolean_type(result))
                || (is_integer_pair(left, right) && is_integer_type(result))
        }
        "DIV" | "Percent" | "SHL" | "SHR" => {
            is_integer_pair(left, right) && is_integer_type(result)
        }
        "Plus" if is_string_type(left) || is_string_type(right) => {
            is_string_type(left) && is_string_type(right) && is_string_type(result)
        }
        "Plus" | "Minus" | "Star" | "Power" => {
            is_numeric_pair(left, right) && is_numeric_type(result)
        }
        "Slash" => is_numeric_pair(left, right) && is_float_type(result),
        // `IS` receives a type constant as its right operand after lowering.
        "IS" => is_type_test_type(right) && is_boolean_type(result),
        _ => false,
    }
}

fn valid_cast_types(source: &Type, target: &Type) -> bool {
    let numeric = |ty: &Type| is_numeric_type(ty);
    numeric(source) && (numeric(target) || is_boolean_type(target))
        || is_string_type(source) && is_boolean_type(target)
        || matches!(source, Type::Null | Type::NotAvailable | Type::EndOfFile)
            && is_boolean_type(target)
}

fn is_comparable(left: &Type, right: &Type) -> bool {
    types_compatible(left, right) || types_compatible(right, left) || is_numeric_pair(left, right)
}

fn is_numeric_pair(left: &Type, right: &Type) -> bool {
    is_numeric_type(left) && is_numeric_type(right)
}

fn is_integer_pair(left: &Type, right: &Type) -> bool {
    is_integer_type(left) && is_integer_type(right)
}

fn is_numeric_type(ty: &Type) -> bool {
    is_integer_type(ty) || is_float_type(ty)
}

fn is_integer_type(ty: &Type) -> bool {
    matches!(ty, Type::Integer(_) | Type::IntegerLiteral(_))
}

fn is_float_type(ty: &Type) -> bool {
    matches!(ty, Type::Float(_) | Type::FloatLiteral)
}

fn is_boolean_type(ty: &Type) -> bool {
    matches!(ty, Type::Boolean)
}

fn is_string_type(ty: &Type) -> bool {
    matches!(ty, Type::String)
}

fn is_type_test_type(ty: &Type) -> bool {
    matches!(ty, Type::TypeName(_) | Type::ImportedTypeName { .. })
}

fn contains_named_type(ty: &Type, name: &str) -> bool {
    match ty {
        Type::Named(actual) | Type::TypeName(actual) => actual == name,
        Type::ImportedNamed { name: actual, .. } | Type::ImportedTypeName { name: actual, .. } => {
            actual == name
        }
        Type::Alternative(options) => options
            .iter()
            .any(|option| contains_named_type(option, name)),
        _ => false,
    }
}

fn is_dispatch_wait_handle(ty: &Type) -> bool {
    ["Ticket", "Group", "Barrier", "Semaphore", "Mutex"]
        .iter()
        .any(|name| contains_named_type(ty, name))
}

fn index_result_matches(object: &Type, result: &Type) -> bool {
    match object {
        Type::Vector {
            element,
            dimensions,
        } => {
            let expected = if dimensions.len() > 1 {
                Type::Vector {
                    element: element.clone(),
                    dimensions: dimensions[1..].to_vec(),
                }
            } else {
                element.as_ref().clone()
            };
            types_compatible(&expected, result)
        }
        Type::Pointer { element, .. } => types_compatible(element, result),
        Type::String | Type::HostArgs => is_string_type(result),
        // Lowering may preserve an unresolved synthesized value. Its concrete
        // index contract is checked once the producer's type is known.
        Type::Unknown => true,
        _ => false,
    }
}

fn validate_return_type(
    function: &super::Function,
    value: Option<&super::ValueId>,
    value_types: &HashMap<super::ValueId, Type>,
) -> Result<(), Diagnostic> {
    let void_only = matches!(&function.return_type, Type::Named(name) if name == "VOID");
    let allows_void = void_only
        || matches!(&function.return_type, Type::Alternative(values) if values.iter().any(|value| matches!(value, Type::Named(name) if name == "VOID")));
    match (void_only, allows_void, value) {
        (true, _, Some(_)) => Err(invalid_ir(
            "VOID function cannot return a value",
            function.span,
        )),
        (false, false, None) => Err(invalid_ir(
            "non-VOID function must return a value",
            function.span,
        )),
        (false, _, Some(value))
            if value_types
                .get(value)
                .is_none_or(|value_type| !types_compatible(value_type, &function.return_type)) =>
        {
            Err(invalid_ir(
                "return value does not match the function return type",
                function.span,
            ))
        }
        _ => Ok(()),
    }
}

fn alternative_sets_compatible(actual: &[Type], expected: &[Type]) -> bool {
    actual.len() == expected.len()
        && actual.iter().all(|actual_option| {
            expected
                .iter()
                .any(|expected_option| types_compatible(actual_option, expected_option))
        })
        && expected.iter().all(|expected_option| {
            actual
                .iter()
                .any(|actual_option| types_compatible(actual_option, expected_option))
        })
}

fn types_compatible(actual: &Type, expected: &Type) -> bool {
    actual == expected
        || matches!(
            (actual, expected),
            (Type::ImportedNamed { name: actual, .. }, Type::Named(expected))
                | (Type::Named(expected), Type::ImportedNamed { name: actual, .. })
                if expected.rsplit('.').next() == Some(actual.as_str())
        )
        // The same exported class imported through two module graphs carries
        // distinct ModuleIds. Compatibility is by the class name: a companion
        // returning `Json.Json OR Error` must type-check against the caller's
        // `Json.Json OR Error` even when each file imported BNJson separately.
        || matches!(
            (actual, expected),
            (
                Type::ImportedNamed { name: actual_name, .. },
                Type::ImportedNamed {
                    name: expected_name,
                    ..
                }
            ) if actual_name == expected_name
        )
        || matches!(
            (actual, expected),
            (Type::Alternative(actual_options), Type::Alternative(expected_options))
                if alternative_sets_compatible(actual_options, expected_options)
        )
        || matches!(
            (actual, expected),
            (Type::IntegerLiteral(_), Type::Integer(_))
        )
        || matches!((actual, expected), (Type::FloatLiteral, Type::Float(_)))
        || matches!((actual, expected), (Type::Pointer { .. }, Type::Named(name)) if name == "POINTER")
        || matches!(
            (actual, expected),
            (
                Type::Pointer {
                    element: actual_element,
                    length: actual_length,
                },
                Type::Pointer {
                    element: expected_element,
                    length: expected_length,
                }
            ) if (is_void_type(actual_element)
                || is_void_type(expected_element)
                || types_compatible(actual_element, expected_element))
                && (actual_length == expected_length
                    || matches!(expected_length, bn_types::PointerLength::Dynamic))
        )
        || matches!(expected, Type::Alternative(options) if options.iter().any(|option| types_compatible(actual, option)))
        || matches!(
            (actual, expected),
            (
                Type::Vector {
                    element: actual_element,
                    dimensions: actual_dimensions,
                },
                Type::Vector {
                    element: expected_element,
                    dimensions: expected_dimensions,
                }
            ) if types_compatible(actual_element, expected_element)
                && actual_dimensions.len() == expected_dimensions.len()
                && actual_dimensions.iter().zip(expected_dimensions).all(
                    |(actual_dimension, expected_dimension)| {
                        actual_dimension == expected_dimension
                            || *actual_dimension == u64::MAX
                            || *expected_dimension == u64::MAX
                    }
                )
        )
        || matches!(
            (actual, expected),
            (
                Type::Pointer { element: actual_element, .. },
                Type::Vector {
                    element: expected_element,
                    dimensions,
                }
            ) if dimensions.len() == 1
                && is_numeric_type(actual_element)
                && is_numeric_type(expected_element)
        )
}

/// Assignment destinations do not carry the class hierarchy table in the
/// language IR. Semantic analysis has already checked class inheritance, so
/// preserve that valid subtype assignment here while retaining strict checks
/// for primitive, pointer and vector values.
fn assignment_types_compatible(actual: &Type, expected: &Type) -> bool {
    matches!(actual, Type::Unknown) || matches!(expected, Type::Unknown)
        || types_compatible(actual, expected)
        || (is_named_value_type(actual) && is_named_value_type(expected))
        // Pointer length is a runtime invariant: allocation can produce a
        // dynamic length that a fixed destination checks when stored. The
        // validator still requires compatible element types.
        || matches!(
            (actual, expected),
            (
                Type::Pointer {
                    element: actual_element,
                    ..
                },
                Type::Pointer {
                    element: expected_element,
                    ..
                }
            ) if types_compatible(actual_element, expected_element)
        )
}

fn is_named_value_type(ty: &Type) -> bool {
    matches!(ty, Type::Named(_) | Type::ImportedNamed { .. })
}

fn call_types_compatible(actual: &Type, expected: &Type) -> bool {
    types_compatible(actual, expected)
        || (is_numeric_type(actual) && is_numeric_type(expected))
        // Standard-library reduction calls use a scalar declaration type for
        // the overloaded one-vector form. Semantic analysis has already
        // restricted that form to the catalogued BNMath functions; the IR
        // handoff preserves the numeric element/result contract here.
        || matches!(
            actual,
            Type::Vector { element, dimensions }
                if dimensions.len() == 1
                    && is_numeric_type(element)
                    && is_numeric_type(expected)
        )
        || matches!(
            (actual, expected),
            (
                Type::Vector {
                    element: actual_element,
                    dimensions: actual_dimensions,
                },
                Type::Vector {
                    element: expected_element,
                    dimensions: expected_dimensions,
                }
            ) if call_types_compatible(actual_element, expected_element)
                && actual_dimensions.len() == expected_dimensions.len()
                && actual_dimensions.iter().zip(expected_dimensions).all(
                    |(actual_dimension, expected_dimension)| {
                        actual_dimension == expected_dimension
                            || *actual_dimension == u64::MAX
                            || *expected_dimension == u64::MAX
                    }
                )
        )
}

fn vector_element_type(vector: &Type) -> Type {
    let Type::Vector {
        element,
        dimensions,
    } = vector
    else {
        unreachable!("vector_element_type called for a non-vector type");
    };
    if dimensions.len() <= 1 {
        return (**element).clone();
    }
    Type::Vector {
        element: element.clone(),
        dimensions: dimensions[1..].to_vec(),
    }
}

fn is_void_type(ty: &Type) -> bool {
    matches!(ty, Type::Named(name) if name == "VOID")
}

fn is_integer(ty: Option<&Type>) -> bool {
    ty.is_some_and(|ty| matches!(ty, Type::Integer(_) | Type::IntegerLiteral(_)))
}

fn is_boolean(ty: Option<&Type>) -> bool {
    ty.is_some_and(|ty| matches!(ty, Type::Boolean))
}

fn validate_successor_bounds(
    block: &super::BasicBlock,
    block_count: u32,
    span: super::Span,
) -> Result<(), Diagnostic> {
    let targets = block_successors(&block.terminator);
    if targets.iter().any(|target| *target >= block_count) {
        return Err(invalid_ir(
            "terminator references a basic block that does not exist",
            span,
        ));
    }
    Ok(())
}

fn block_successors(terminator: &Terminator) -> Vec<u32> {
    match terminator {
        Terminator::Jump { target } => vec![target.0],
        Terminator::Branch {
            then_block,
            else_block,
            ..
        } => vec![then_block.0, else_block.0],
        Terminator::Return { .. } | Terminator::Stop { .. } => Vec::new(),
    }
}

fn reachable_blocks(entry: usize, successors: &[Vec<u32>]) -> Vec<bool> {
    let mut reachable = vec![false; successors.len()];
    let mut pending = vec![entry];
    while let Some(index) = pending.pop() {
        if reachable[index] {
            continue;
        }
        reachable[index] = true;
        pending.extend(
            successors[index]
                .iter()
                .filter_map(|target| usize::try_from(*target).ok()),
        );
    }
    reachable
}

/// Structural rules for `FunctionKind` (bucket 0.5.1c §3.2). Backends select
/// entry points, constructors and destructors by kind, so a mislabelled
/// function is invalid IR, not a backend surprise.
fn validate_function_kinds(module: &Module) -> Result<(), Diagnostic> {
    use super::FunctionKind::{Constructor, Default, Destructor, Entry, FieldInit, Init, User};
    let mut entries = 0usize;
    for function in &module.functions {
        match function.kind {
            Entry => {
                entries += 1;
                if entries > 1 {
                    return Err(invalid_ir(
                        "module declares more than one entry function",
                        function.span,
                    ));
                }
                if !function.parameters.is_empty() {
                    return Err(invalid_ir(
                        "entry function must not take parameters",
                        function.span,
                    ));
                }
            }
            Constructor | Destructor | FieldInit | Init => {
                if function.owner.is_none() {
                    return Err(invalid_ir(
                        "constructor, destructor, field-init and init functions need an owner class",
                        function.span,
                    ));
                }
                // `Init` allocates and returns the object, so it has no SELF.
                if function.kind != Init && function.parameters.is_empty() {
                    return Err(invalid_ir(
                        "constructor, destructor and field-init functions take SELF first",
                        function.span,
                    ));
                }
                if function.kind == Destructor
                    && !matches!(&function.return_type, Type::Named(name) if name == "VOID")
                {
                    return Err(invalid_ir("destructor must return VOID", function.span));
                }
            }
            Default => {
                if function.owner.is_none() {
                    return Err(invalid_ir(
                        "default constructor needs an owner struct",
                        function.span,
                    ));
                }
                if !function.parameters.is_empty() {
                    return Err(invalid_ir(
                        "default constructor must not take parameters",
                        function.span,
                    ));
                }
            }
            User => {}
        }
    }
    Ok(())
}

#[cfg(test)]
mod kind_tests {
    use std::collections::{BTreeMap, HashMap, HashSet};

    use bn_source::{Position, Revision, SourceId, Span};
    use bn_types::Type;

    use super::super::{
        BasicBlock, BlockId, Constant, FieldId, FieldLayout, FieldLayoutEntry, FieldRef, FieldSlot,
        Function, FunctionKind, Instruction, Module, SymbolId, Terminator, ValueId,
    };

    fn span() -> Span {
        let position = Position {
            source_id: SourceId(1),
            revision: Revision(1),
            offset: 0,
            line: 1,
            column: 1,
        };
        Span {
            start: position,
            end: position,
        }
    }

    fn function(
        name: &str,
        kind: FunctionKind,
        owner: Option<&str>,
        params: usize,
        ret: &str,
    ) -> Function {
        Function {
            name: name.into(),
            kind,
            owner: owner.map(str::to_string),
            asynchronous: false,
            parameters: (0..params)
                .map(|i| SymbolId(u32::try_from(i).expect("small")))
                .collect(),
            weak_symbols: HashSet::new(),
            return_type: Type::Named(ret.into()),
            entry: BlockId(0),
            blocks: vec![BasicBlock {
                id: BlockId(0),
                instructions: Vec::new(),
                terminator: Terminator::Return { value: None },
            }],
            span: span(),
        }
    }

    fn module(functions: Vec<Function>) -> Module {
        Module {
            functions,
            ..Module::default()
        }
    }

    fn detail(result: Result<(), bn_diag::Diagnostic>) -> String {
        result.expect_err("must be invalid IR").message.to_string()
    }

    #[test]
    fn field_layout_slots_must_be_dense_and_ordered() {
        let layout = FieldLayout {
            owner: "Point".into(),
            fields: vec![FieldLayoutEntry {
                id: FieldId::from_raw(0),
                slot: FieldSlot::from_raw(1),
                ty: Type::Named("INTEGER".into()),
                declaring_owner: "Point".into(),
                weak: false,
                span: span(),
            }],
            span: span(),
        };
        let mut module = module(Vec::new());
        module.field_names = vec!["x".into()];
        module.field_layouts = BTreeMap::from([("Point".into(), layout)]);

        assert!(detail(super::validate(&module)).contains("slots must be dense"));
    }

    #[test]
    fn field_layout_ids_must_exist_in_the_interned_name_table() {
        let layout = FieldLayout {
            owner: "Point".into(),
            fields: vec![FieldLayoutEntry {
                id: FieldId::from_raw(1),
                slot: FieldSlot::from_raw(0),
                ty: Type::Named("INTEGER".into()),
                declaring_owner: "Point".into(),
                weak: false,
                span: span(),
            }],
            span: span(),
        };
        let mut module = module(Vec::new());
        module.field_names = vec!["x".into()];
        module.field_layouts = BTreeMap::from([("Point".into(), layout)]);

        assert!(detail(super::validate(&module)).contains("absent from the name table"));
    }

    #[test]
    fn member_receiver_and_result_must_match_the_field_layout() {
        let layout = FieldLayout {
            owner: "Point".into(),
            fields: vec![FieldLayoutEntry {
                id: FieldId::from_raw(0),
                slot: FieldSlot::from_raw(0),
                ty: Type::Integer(bn_types::IntegerType::Int32),
                declaring_owner: "Point".into(),
                weak: false,
                span: span(),
            }],
            span: span(),
        };
        let mut module = module(vec![function(
            "Start",
            FunctionKind::Entry,
            None,
            0,
            "VOID",
        )]);
        module.field_names = vec!["x".into()];
        module.field_layouts = BTreeMap::from([("Point".into(), layout)]);
        module.functions[0].blocks[0].instructions = vec![
            Instruction::Default {
                destination: ValueId(0),
                ty: Type::Named("Point".into()),
                dimensions: Vec::new(),
                dynamic_dimensions: Vec::new(),
                span: span(),
            },
            Instruction::Member {
                destination: ValueId(1),
                object: ValueId(0),
                field: Some(FieldRef {
                    owner: "Point".into(),
                    id: FieldId::from_raw(0),
                    slot: FieldSlot::from_raw(0),
                }),
                name: "x".into(),
                owner: "Point".into(),
                ty: Type::String,
                span: span(),
            },
        ];

        assert!(detail(super::validate(&module)).contains("result type"));
        let Instruction::Default { ty, .. } = &mut module.functions[0].blocks[0].instructions[0]
        else {
            unreachable!("default");
        };
        *ty = Type::Boolean;
        let Instruction::Member { ty, .. } = &mut module.functions[0].blocks[0].instructions[1]
        else {
            unreachable!("member");
        };
        *ty = Type::Integer(bn_types::IntegerType::Int32);
        assert!(detail(super::validate(&module)).contains("receiver"));
    }

    #[test]
    fn derived_receiver_may_access_a_base_layout_field() {
        let base_layout = FieldLayout {
            owner: "Animal".into(),
            fields: vec![FieldLayoutEntry {
                id: FieldId::from_raw(0),
                slot: FieldSlot::from_raw(0),
                ty: Type::String,
                declaring_owner: "Animal".into(),
                weak: false,
                span: span(),
            }],
            span: span(),
        };
        let derived_layout = FieldLayout {
            owner: "Dog".into(),
            fields: base_layout.fields.clone(),
            span: span(),
        };
        let mut module = module(vec![
            function(
                "Animal.$fields",
                FunctionKind::FieldInit,
                Some("Animal"),
                1,
                "VOID",
            ),
            function(
                "Dog.$fields",
                FunctionKind::FieldInit,
                Some("Dog"),
                1,
                "VOID",
            ),
            function("Start", FunctionKind::Entry, None, 0, "VOID"),
        ]);
        module.field_names = vec!["name".into()];
        module.class_bases = HashMap::from([("Dog".into(), "Animal".into())]);
        module.field_layouts = BTreeMap::from([
            ("Animal".into(), base_layout),
            ("Dog".into(), derived_layout),
        ]);
        module.functions[2].blocks[0].instructions = vec![
            Instruction::Default {
                destination: ValueId(0),
                ty: Type::Named("Dog".into()),
                dimensions: Vec::new(),
                dynamic_dimensions: Vec::new(),
                span: span(),
            },
            Instruction::Member {
                destination: ValueId(1),
                object: ValueId(0),
                field: Some(FieldRef {
                    owner: "Animal".into(),
                    id: FieldId::from_raw(0),
                    slot: FieldSlot::from_raw(0),
                }),
                name: "name".into(),
                owner: "Animal".into(),
                ty: Type::String,
                span: span(),
            },
        ];

        super::validate(&module).expect("derived receiver may access inherited base field");
    }

    #[test]
    fn field_name_table_must_not_duplicate_spellings() {
        let mut module = module(Vec::new());
        module.field_names = vec!["x".into(), "x".into()];

        assert!(detail(super::validate(&module)).contains("field names must be unique"));
    }

    #[test]
    fn derived_layout_must_preserve_the_base_prefix() {
        let base = FieldLayout {
            owner: "Parent".into(),
            fields: vec![FieldLayoutEntry {
                id: FieldId::from_raw(0),
                slot: FieldSlot::from_raw(0),
                ty: Type::Named("INTEGER".into()),
                declaring_owner: "Parent".into(),
                weak: false,
                span: span(),
            }],
            span: span(),
        };
        let child = FieldLayout {
            owner: "Child".into(),
            fields: vec![FieldLayoutEntry {
                id: FieldId::from_raw(1),
                slot: FieldSlot::from_raw(0),
                ty: Type::Named("INTEGER".into()),
                declaring_owner: "Child".into(),
                weak: false,
                span: span(),
            }],
            span: span(),
        };
        let mut module = module(vec![function(
            "Parent.$fields",
            FunctionKind::FieldInit,
            Some("Parent"),
            1,
            "VOID",
        )]);
        module.functions.push(function(
            "Child.$fields",
            FunctionKind::FieldInit,
            Some("Child"),
            1,
            "VOID",
        ));
        module.field_names = vec!["first".into(), "second".into()];
        module.class_bases = HashMap::from([("Child".into(), "Parent".into())]);
        module.field_layouts = BTreeMap::from([("Parent".into(), base), ("Child".into(), child)]);

        assert!(detail(super::validate(&module)).contains("base layout prefix"));
    }

    #[test]
    fn field_path_stores_require_matching_resolved_fields() {
        let layout = FieldLayout {
            owner: "Point".into(),
            fields: vec![FieldLayoutEntry {
                id: FieldId::from_raw(0),
                slot: FieldSlot::from_raw(0),
                ty: Type::Integer(bn_types::IntegerType::Int32),
                declaring_owner: "Point".into(),
                weak: false,
                span: span(),
            }],
            span: span(),
        };
        let mut module = module(vec![function(
            "Start",
            FunctionKind::Entry,
            None,
            0,
            "VOID",
        )]);
        module.field_names = vec!["x".into()];
        module.field_layouts = BTreeMap::from([("Point".into(), layout)]);
        module.functions[0].blocks[0].instructions = vec![
            Instruction::Constant {
                destination: ValueId(0),
                value: Constant::Integer("1".into()),
                ty: Type::Integer(bn_types::IntegerType::Int32),
                span: span(),
            },
            Instruction::SetField {
                symbol: SymbolId(0),
                root_owner: "Point".into(),
                path: vec!["x".into()],
                fields: None,
                value: ValueId(0),
                ty: Type::Integer(bn_types::IntegerType::Int32),
                span: span(),
            },
        ];

        assert!(detail(super::validate(&module)).contains("must carry resolved fields"));

        let Instruction::SetField { fields, .. } =
            &mut module.functions[0].blocks[0].instructions[1]
        else {
            unreachable!("field store");
        };
        *fields = Some(vec![FieldRef {
            owner: "Point".into(),
            id: FieldId::from_raw(0),
            slot: FieldSlot::from_raw(1),
        }]);
        assert!(detail(super::validate(&module)).contains("does not match"));
    }

    #[test]
    fn well_formed_kinds_validate() {
        let m = module(vec![
            function("Start", FunctionKind::Entry, None, 0, "VOID"),
            function(
                "C.CONSTRUCTOR",
                FunctionKind::Constructor,
                Some("C"),
                1,
                "VOID",
            ),
            function(
                "C.DESTRUCTOR",
                FunctionKind::Destructor,
                Some("C"),
                1,
                "VOID",
            ),
            function("C.$fields", FunctionKind::FieldInit, Some("C"), 1, "VOID"),
            function("C.$init", FunctionKind::Init, Some("C"), 0, "VOID"),
            function("P.$default", FunctionKind::Default, Some("P"), 0, "VOID"),
            function("C.Method", FunctionKind::User, Some("C"), 1, "VOID"),
            function("Free", FunctionKind::User, None, 0, "VOID"),
        ]);
        super::validate(&m).expect("well-formed kinds");
    }

    #[test]
    fn two_entries_are_invalid() {
        let m = module(vec![
            function("Start", FunctionKind::Entry, None, 0, "VOID"),
            function("Start2", FunctionKind::Entry, None, 0, "VOID"),
        ]);
        assert!(detail(super::validate(&m)).contains("more than one entry"));
    }

    #[test]
    fn entry_with_parameters_is_invalid() {
        let m = module(vec![function(
            "Start",
            FunctionKind::Entry,
            None,
            1,
            "VOID",
        )]);
        assert!(detail(super::validate(&m)).contains("entry function must not take parameters"));
    }

    #[test]
    fn synthesised_kinds_need_an_owner() {
        for kind in [
            FunctionKind::Constructor,
            FunctionKind::Destructor,
            FunctionKind::FieldInit,
            FunctionKind::Init,
        ] {
            let m = module(vec![function("X.f", kind, None, 1, "VOID")]);
            assert!(
                detail(super::validate(&m)).contains("need an owner class"),
                "{kind:?}"
            );
        }
        let m = module(vec![function(
            "P.$default",
            FunctionKind::Default,
            None,
            0,
            "VOID",
        )]);
        assert!(detail(super::validate(&m)).contains("needs an owner struct"));
    }

    #[test]
    fn self_taking_kinds_need_a_parameter() {
        for kind in [
            FunctionKind::Constructor,
            FunctionKind::Destructor,
            FunctionKind::FieldInit,
        ] {
            let m = module(vec![function("C.f", kind, Some("C"), 0, "VOID")]);
            assert!(
                detail(super::validate(&m)).contains("take SELF first"),
                "{kind:?}"
            );
        }
    }

    #[test]
    fn destructor_must_return_void() {
        let m = module(vec![function(
            "C.DESTRUCTOR",
            FunctionKind::Destructor,
            Some("C"),
            1,
            "INTEGER",
        )]);
        assert!(detail(super::validate(&m)).contains("destructor must return VOID"));
    }

    #[test]
    fn default_constructor_takes_no_parameters() {
        let m = module(vec![function(
            "P.$default",
            FunctionKind::Default,
            Some("P"),
            1,
            "VOID",
        )]);
        assert!(detail(super::validate(&m)).contains("must not take parameters"));
    }
}
