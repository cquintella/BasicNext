use std::collections::{HashMap, HashSet};

use bn_source::{Position, Revision, SourceId, Span};
use bn_types::{IntegerType, Type};

use super::{Diagnostic, Module, Terminator, invalid_ir};

#[path = "validate/fields.rs"]
mod fields;
#[path = "validate/kinds.rs"]
mod kinds;
#[path = "validate/operands.rs"]
mod operands;
#[path = "validate/ownership.rs"]
mod ownership;
#[path = "validate/upcast.rs"]
mod upcast;
use fields::{
    receiver_matches_owner, validate_class_bases, validate_field_layouts, validate_field_reference,
    validate_resolved_field_path,
};
use operands::{instruction_defines, instruction_type};
pub use operands::{instruction_result, instruction_uses};
#[cfg(test)]
pub(crate) use upcast::call_types_compatible;
use upcast::{alternative_widens, call_types_compatible_with_module, types_compatible};

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
    ownership::validate_ownership(module)?;
    validate_field_layouts(module)?;
    kinds::validate_function_kinds(module)?;
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
                    validate_return_type(module, function, value.as_ref(), &value_types)?;
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
                                !vector_overload
                                    && !call_types_compatible_with_module(
                                        module,
                                        argument_type,
                                        parameter,
                                    )
                            })
                        })
                    || !call_types_compatible_with_module(module, return_type, ty)
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
        | super::Instruction::TakeMember { name, owner, .. }
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
        | super::Instruction::TakeMember { field: None, .. }
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
        }
        | super::Instruction::TakeMember {
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
                | Type::HostEnv
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
        || (numeric(source) || is_boolean_type(source)) && is_string_type(target)
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
    module: &Module,
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
            if value_types.get(value).is_none_or(|value_type| {
                !types_compatible(value_type, &function.return_type)
                    && !alternative_widens(value_type, &function.return_type)
                    && !upcast::is_upcast(module, value_type, &function.return_type)
            }) =>
        {
            Err(invalid_ir(
                "return value does not match the function return type",
                function.span,
            ))
        }
        _ => Ok(()),
    }
}

fn is_named_value_type(ty: &Type) -> bool {
    matches!(ty, Type::Named(_) | Type::ImportedNamed { .. })
}

/// Assignment destinations do not carry the class hierarchy table in the
/// language IR. Semantic analysis has already checked class inheritance, so
/// preserve that valid subtype assignment here while retaining strict checks
/// for primitive, pointer and vector values.
fn assignment_types_compatible(actual: &Type, expected: &Type) -> bool {
    matches!(actual, Type::Unknown) || matches!(expected, Type::Unknown)
        || types_compatible(actual, expected)
        || alternative_widens(actual, expected)
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

#[cfg(test)]
#[path = "validate/kind_tests.rs"]
mod kind_tests;
