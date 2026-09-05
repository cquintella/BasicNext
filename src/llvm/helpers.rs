#![allow(
    clippy::wildcard_imports,
    clippy::match_same_arms,
    clippy::cast_possible_truncation
)]
use super::*;

pub(crate) fn fold_unary(
    operator: &str,
    operand: Option<&ConstantValue>,
    ty: &Type,
) -> Option<ConstantValue> {
    let operand = operand?;
    match (operator, operand) {
        ("Plus", ConstantValue::Integer(value, kind)) => {
            Some(ConstantValue::Integer(*value, *kind))
        }
        ("Minus", ConstantValue::Integer(value, _)) => Some(ConstantValue::Integer(
            checked_integer_value(value.checked_neg()?, ty)?,
            integer_kind(ty),
        )),
        ("Plus", ConstantValue::Float(value)) => Some(ConstantValue::Float(*value)),
        ("Minus", ConstantValue::Float(value)) => Some(ConstantValue::Float(-value)),
        ("NOT", ConstantValue::Boolean(value)) => Some(ConstantValue::Boolean(!value)),
        ("NOT", ConstantValue::Integer(value, _)) => Some(ConstantValue::Integer(
            checked_integer_value(!*value, ty)?,
            integer_kind(ty),
        )),
        _ => None,
    }
}

#[allow(clippy::cast_sign_loss, clippy::float_cmp, clippy::too_many_lines)]
pub(crate) fn fold_binary(
    operator: &str,
    left: Option<&ConstantValue>,
    right: Option<&ConstantValue>,
    ty: &Type,
) -> Option<ConstantValue> {
    match (left?, right?) {
        (ConstantValue::Integer(left, _), ConstantValue::Integer(right, _)) => match operator {
            "Plus" => Some(ConstantValue::Integer(
                checked_integer_value(left.checked_add(*right)?, ty)?,
                integer_kind(ty),
            )),
            "Minus" => Some(ConstantValue::Integer(
                checked_integer_value(left.checked_sub(*right)?, ty)?,
                integer_kind(ty),
            )),
            "Star" | "Multiply" => Some(ConstantValue::Integer(
                checked_integer_value(left.checked_mul(*right)?, ty)?,
                integer_kind(ty),
            )),
            "DIV" if *right != 0 => Some(ConstantValue::Integer(
                checked_integer_value(left.checked_div_euclid(*right)?, ty)?,
                integer_kind(ty),
            )),
            "Percent" if *right != 0 => Some(ConstantValue::Integer(
                checked_integer_value(left.checked_rem_euclid(*right)?, ty)?,
                integer_kind(ty),
            )),
            "Power" if *right >= 0 => Some(ConstantValue::Integer(
                checked_integer_value(left.checked_pow(u32::try_from(*right).ok()?)?, ty)?,
                integer_kind(ty),
            )),
            "SHL" if (0..i128::from(shift_width(ty))).contains(right) => {
                let count = u32::try_from(*right).ok()?;
                checked_integer_value(left.checked_shl(count)?, ty)
                    .map(|value| ConstantValue::Integer(value, integer_kind(ty)))
            }
            "SHR" if (0..i128::from(shift_width(ty))).contains(right) => {
                let count = u32::try_from(*right).ok()?;
                let mask = (1_u128 << shift_width(ty)) - 1;
                Some(ConstantValue::Integer(
                    checked_integer_value(
                        ((left.cast_unsigned() & mask) >> count).cast_signed(),
                        ty,
                    )?,
                    integer_kind(ty),
                ))
            }
            "AND" => Some(ConstantValue::Integer(
                checked_integer_value(*left & *right, ty)?,
                integer_kind(ty),
            )),
            "OR" => Some(ConstantValue::Integer(
                checked_integer_value(*left | *right, ty)?,
                integer_kind(ty),
            )),
            "XOR" => Some(ConstantValue::Integer(
                checked_integer_value(*left ^ *right, ty)?,
                integer_kind(ty),
            )),
            "Less" => Some(ConstantValue::Boolean(if is_unsigned(ty) {
                left.cast_unsigned() < right.cast_unsigned()
            } else {
                left < right
            })),
            "LessEqual" => Some(ConstantValue::Boolean(if is_unsigned(ty) {
                left.cast_unsigned() <= right.cast_unsigned()
            } else {
                left <= right
            })),
            "Greater" => Some(ConstantValue::Boolean(if is_unsigned(ty) {
                left.cast_unsigned() > right.cast_unsigned()
            } else {
                left > right
            })),
            "GreaterEqual" => Some(ConstantValue::Boolean(if is_unsigned(ty) {
                left.cast_unsigned() >= right.cast_unsigned()
            } else {
                left >= right
            })),
            "Equal" | "Assign" => Some(ConstantValue::Boolean(left == right)),
            "NotEqual" => Some(ConstantValue::Boolean(left != right)),
            _ => None,
        },
        (ConstantValue::Boolean(left), ConstantValue::Boolean(right)) => match operator {
            "AND" => Some(ConstantValue::Boolean(*left && *right)),
            "OR" => Some(ConstantValue::Boolean(*left || *right)),
            "XOR" => Some(ConstantValue::Boolean(*left ^ *right)),
            "Equal" | "Assign" => Some(ConstantValue::Boolean(left == right)),
            "NotEqual" => Some(ConstantValue::Boolean(left != right)),
            _ => None,
        },
        (ConstantValue::Float(left), ConstantValue::Float(right)) => match operator {
            "Plus" => Some(ConstantValue::Float(left + right)),
            "Minus" => Some(ConstantValue::Float(left - right)),
            "Star" | "Multiply" => Some(ConstantValue::Float(left * right)),
            "Slash" | "Divide" => Some(ConstantValue::Float(left / right)),
            "Power" => Some(ConstantValue::Float(left.powf(*right))),
            "Less" => Some(ConstantValue::Boolean(left < right)),
            "LessEqual" => Some(ConstantValue::Boolean(left <= right)),
            "Greater" => Some(ConstantValue::Boolean(left > right)),
            "GreaterEqual" => Some(ConstantValue::Boolean(left >= right)),
            "Equal" | "Assign" => Some(ConstantValue::Boolean(left == right)),
            "NotEqual" => Some(ConstantValue::Boolean(left != right)),
            _ => None,
        },
        _ => None,
    }
}

fn shift_width(ty: &Type) -> u8 {
    match integer_kind(ty) {
        IntegerType::Byte | IntegerType::Int8 => 8,
        IntegerType::Int16 | IntegerType::UInt16 => 16,
        IntegerType::Int32 | IntegerType::UInt32 => 32,
        IntegerType::Int64 | IntegerType::UInt64 => 64,
    }
}

#[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
pub(crate) fn fold_cast(value: Option<&ConstantValue>, ty: &Type) -> Option<ConstantValue> {
    match (value?, ty) {
        (ConstantValue::Integer(value, _), Type::Integer(_)) => Some(ConstantValue::Integer(
            checked_integer_value(*value, ty)?,
            integer_kind(ty),
        )),
        (ConstantValue::Integer(value, _), Type::Float(_)) => {
            Some(ConstantValue::Float(*value as f64))
        }
        (ConstantValue::Float(value), Type::Float(FloatType::Float32)) => {
            Some(ConstantValue::Float(f64::from(*value as f32)))
        }
        (ConstantValue::Float(value), Type::Float(FloatType::Float64)) => {
            Some(ConstantValue::Float(*value))
        }
        (ConstantValue::Float(value), Type::Integer(_)) if value.is_finite() => {
            Some(ConstantValue::Integer(
                checked_integer_value(value.trunc() as i128, ty)?,
                integer_kind(ty),
            ))
        }
        (ConstantValue::Boolean(value), Type::Boolean) => Some(ConstantValue::Boolean(*value)),
        (ConstantValue::Integer(value, _), Type::Boolean) => {
            Some(ConstantValue::Boolean(*value != 0))
        }
        (ConstantValue::Float(value), Type::Boolean) => Some(ConstantValue::Boolean(*value != 0.0)),
        (ConstantValue::String(value), Type::Boolean) => {
            Some(ConstantValue::Boolean(!value.is_empty()))
        }
        (ConstantValue::String(value), Type::String) => Some(ConstantValue::String(value.clone())),
        _ => None,
    }
}

pub(crate) fn checked_integer_value(value: i128, ty: &Type) -> Option<i128> {
    let kind = integer_kind(ty);
    let (minimum, maximum) = integer_range(kind);
    (minimum..=maximum).contains(&value).then_some(value)
}

pub(crate) fn integer_kind(ty: &Type) -> IntegerType {
    match ty {
        Type::Integer(kind) => *kind,
        Type::IntegerLiteral(_) => IntegerType::Int64,
        _ => IntegerType::Int32,
    }
}

/// Render an integer for LLVM IR. Never pass BN hex source (`0x…`) through:
/// LLVM treats `0x` as a floating-point constant encoding.
pub(crate) fn render_llvm_integer(value: i128, llvm_ty: &str) -> String {
    match llvm_ty {
        "i8" => render_signed_bits(value, 8),
        "i16" => render_signed_bits(value, 16),
        "i32" => render_signed_bits(value, 32),
        "i64" => render_signed_bits(value, 64),
        _ => value.to_string(),
    }
}

fn render_signed_bits(value: i128, bits: u32) -> String {
    let modulus = 1_i128 << bits;
    let normalized = value.rem_euclid(modulus);
    let sign = 1_i128 << (bits - 1);
    if normalized >= sign {
        (normalized - modulus).to_string()
    } else {
        normalized.to_string()
    }
}

pub(crate) fn coerce_integer(
    text: &mut String,
    value: ValueId,
    from_ty: &str,
    to_ty: &str,
    unsigned: bool,
) -> String {
    if from_ty == to_ty {
        return format!("%v{}", value.0);
    }
    let temp = format!("coer{}_{from_ty}_{to_ty}", value.0);
    let from_w = integer_llvm_width(from_ty);
    let to_w = integer_llvm_width(to_ty);
    if from_w < to_w {
        let opcode = if unsigned { "zext" } else { "sext" };
        let _ = writeln!(
            text,
            "  %{temp} = {opcode} {from_ty} %v{} to {to_ty}",
            value.0
        );
    } else {
        let _ = writeln!(text, "  %{temp} = trunc {from_ty} %v{} to {to_ty}", value.0);
    }
    format!("%{temp}")
}

pub(crate) fn coerce_to_type(text: &mut String, value: ValueId, from: &Type, to: &Type) -> String {
    let from_llvm = llvm_type(from).expect("validated coerce source");
    let to_llvm = llvm_type(to).expect("validated coerce target");
    if from_llvm == to_llvm {
        return format!("%v{}", value.0);
    }
    if matches!(from_llvm, "i8" | "i16" | "i32" | "i64")
        && matches!(to_llvm, "i8" | "i16" | "i32" | "i64")
    {
        return coerce_integer(
            text,
            value,
            from_llvm,
            to_llvm,
            is_unsigned(to) || is_unsigned(from),
        );
    }
    if matches!(
        (from_llvm, to_llvm),
        ("float", "double") | ("double", "float")
    ) {
        let temp = format!("fcoer{}_{from_llvm}_{to_llvm}", value.0);
        let opcode = if from_llvm == "float" {
            "fpext"
        } else {
            "fptrunc"
        };
        let _ = writeln!(
            text,
            "  %{temp} = {opcode} {from_llvm} %v{} to {to_llvm}",
            value.0
        );
        return format!("%{temp}");
    }
    format!("%v{}", value.0)
}

fn integer_llvm_width(llvm_ty: &str) -> u8 {
    match llvm_ty {
        "i1" => 1,
        "i8" => 8,
        "i16" => 16,
        "i32" => 32,
        "i64" => 64,
        _ => 64,
    }
}

pub(crate) fn integer_range(kind: IntegerType) -> (i128, i128) {
    match kind {
        IntegerType::Byte => (0, i128::from(u8::MAX)),
        IntegerType::Int8 => (i128::from(i8::MIN), i128::from(i8::MAX)),
        IntegerType::Int16 => (i128::from(i16::MIN), i128::from(i16::MAX)),
        IntegerType::Int32 => (i128::from(i32::MIN), i128::from(i32::MAX)),
        IntegerType::Int64 => (i128::from(i64::MIN), i128::from(i64::MAX)),
        IntegerType::UInt16 => (0, i128::from(u16::MAX)),
        IntegerType::UInt32 => (0, i128::from(u32::MAX)),
        IntegerType::UInt64 => (0, i128::from(u64::MAX)),
    }
}

pub(crate) fn is_unsigned(ty: &Type) -> bool {
    matches!(
        ty,
        Type::Integer(
            IntegerType::Byte | IntegerType::UInt16 | IntegerType::UInt32 | IntegerType::UInt64
        )
    )
}

pub(crate) fn extend_to_i64(text: &mut String, value: ValueId, ty: &Type) -> String {
    match llvm_type(ty).expect("validated extension type") {
        "i64" => format!("%v{}", value.0),
        llvm_ty => {
            let temp = format!("seedext{}", value.0);
            let opcode = if is_unsigned(ty) { "zext" } else { "sext" };
            let _ = writeln!(text, "  %{temp} = {opcode} {llvm_ty} %v{} to i64", value.0);
            format!("%{temp}")
        }
    }
}

pub(crate) fn coerce_return_operand(text: &mut String, value: ValueId, ty: &Type) -> String {
    match llvm_type(ty).expect("validated return LLVM type") {
        "i32" => format!("%v{}", value.0),
        "i8" | "i16" => {
            let opcode = if is_unsigned(ty) { "zext" } else { "sext" };
            let temp = format!("ret{}", value.0);
            let _ = writeln!(
                text,
                "  %{temp} = {opcode} {} %v{} to i32",
                llvm_type(ty).expect("validated integer return type"),
                value.0
            );
            format!("%{temp}")
        }
        "i64" => {
            let temp = format!("ret{}", value.0);
            let _ = writeln!(text, "  %{temp} = trunc i64 %v{} to i32", value.0);
            format!("%{temp}")
        }
        _ => unreachable!("validated return type"),
    }
}

#[allow(clippy::cast_possible_truncation)]
pub(crate) fn sanitize_symbol(name: &str) -> String {
    name.chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '_' {
                ch
            } else {
                '_'
            }
        })
        .collect()
}

pub(crate) fn static_global_name(class: &str, field: &str) -> String {
    format!(
        "@bn_st_{}_{}",
        sanitize_symbol(class),
        sanitize_symbol(field)
    )
}

pub(crate) fn class_init_flag(class: &str) -> String {
    format!("@bn_init_{}", sanitize_symbol(class))
}

pub(crate) fn field_byte_offset(module: &Module, owner: &str, field: &str) -> u32 {
    let mut offset = OBJECT_HEADER_BYTES;
    for function in &module.functions {
        if !function.name.ends_with(".$fields") {
            continue;
        }
        let class = function
            .name
            .trim_end_matches(".$fields")
            .rsplit('.')
            .next()
            .unwrap_or(function.name.as_str());
        if class != owner && !function.name.ends_with(&format!("{owner}.$fields")) {
            continue;
        }
        for block in &function.blocks {
            for instruction in &block.instructions {
                if let Instruction::SetMember {
                    name,
                    ty,
                    owner: set_owner,
                    ..
                } = instruction
                {
                    if set_owner != owner && set_owner.rsplit('.').next() != Some(owner) {
                        continue;
                    }
                    if name == field {
                        return offset;
                    }
                    let width = match llvm_type(ty) {
                        Some("i1" | "i8") => 1,
                        Some("i16") => 2,
                        Some("i32" | "float") => 4,
                        Some("i64" | "double" | "ptr") => 8,
                        _ => 4,
                    };
                    offset = offset.saturating_add(width);
                }
            }
        }
    }
    offset
}

pub(crate) const OBJECT_HEADER_BYTES: u32 = 8;

pub(crate) fn class_instance_bytes(module: &Module, type_name: &str) -> u64 {
    let owner = type_name.rsplit('.').next().unwrap_or(type_name);
    let mut total = OBJECT_HEADER_BYTES;
    for function in &module.functions {
        if !function.name.ends_with(&format!("{owner}.$fields"))
            && !function.name.ends_with(".$fields")
        {
            continue;
        }
        let class = function
            .name
            .trim_end_matches(".$fields")
            .rsplit('.')
            .next()
            .unwrap_or("");
        if class != owner {
            continue;
        }
        for block in &function.blocks {
            for instruction in &block.instructions {
                if let Instruction::SetMember { ty, .. } = instruction {
                    total = total.saturating_add(match llvm_type(ty) {
                        Some("i1" | "i8") => 1,
                        Some("i16") => 2,
                        Some("i32" | "float") => 4,
                        Some("i64" | "double" | "ptr") => 8,
                        _ => 4,
                    });
                }
            }
        }
        break;
    }
    // Inheritance may add parent fields via other `$fields` helpers; keep slack.
    u64::from(total.max(64))
}

pub(crate) fn parse_float_constant(value: &str) -> Option<f64> {
    match value {
        "NAN" | "nan" | "NaN" => Some(f64::NAN),
        "INF" | "inf" | "+INF" | "+inf" => Some(f64::INFINITY),
        "-INF" | "-inf" => Some(f64::NEG_INFINITY),
        _ => value.parse().ok(),
    }
}

pub(crate) fn render_float(value: f64, ty: &Type) -> String {
    let is_float32 = matches!(ty, Type::Float(FloatType::Float32));
    if value.is_nan() {
        return if is_float32 {
            "0x7FC00000".into()
        } else {
            "0x7FF8000000000000".into()
        };
    }
    if value == f64::INFINITY {
        return if is_float32 {
            "0x7F800000".into()
        } else {
            "0x7FF0000000000000".into()
        };
    }
    if value == f64::NEG_INFINITY {
        return if is_float32 {
            "0xFF800000".into()
        } else {
            "0xFFF0000000000000".into()
        };
    }
    let rendered = match ty {
        Type::Float(FloatType::Float32) => f64::from(value as f32).to_string(),
        _ => value.to_string(),
    };
    if rendered.contains('.') || rendered.contains('e') || rendered.contains('E') {
        rendered
    } else {
        format!("{rendered}.0")
    }
}

pub(crate) fn unsupported_call_detail(module: &Module, name: &str) -> String {
    if let Some(provider) = provider_name(module, name) {
        return format!("{provider} calls");
    }
    if name.starts_with("HOST.") {
        return format!("unsupported HOST call {name}");
    }
    format!(
        "calls to user-defined function '{name}' are unavailable in the LLVM backend; \
         function calls are not supported by this build target yet (use 'bn run' \
         or inline the call)"
    )
}

pub(crate) fn provider_name(module: &Module, name: &str) -> Option<&'static str> {
    let module_id = name
        .strip_prefix('#')
        .and_then(|rest| rest.split('.').next())
        .and_then(|digits| digits.parse::<u32>().ok())?;
    if module
        .bnweb_providers
        .iter()
        .any(|provider| provider.0 == module_id)
    {
        Some("BNWeb")
    } else if module
        .bndispatch_providers
        .iter()
        .any(|provider| provider.0 == module_id)
    {
        Some("BNDispatch")
    } else if module
        .bnmath_providers
        .iter()
        .any(|provider| provider.0 == module_id)
    {
        Some("BNMath")
    } else {
        None
    }
}

pub(crate) fn unsupported_instruction(
    module: &Module,
    function: &Function,
    instruction: &Instruction,
    detail: &str,
) -> String {
    let span = instruction.span();
    let source_name = module.source_name.as_deref().unwrap_or("<unknown source>");
    format!(
        "BUILD_LOWERING_UNAVAILABLE: {source_name}:{}:{}: {detail} in FUNCTION {}",
        span.start.line, span.start.column, function.name
    )
}

pub(crate) fn parse_integer(value: &str) -> Option<i128> {
    if let Some(value) = value.strip_prefix("0b") {
        i128::from_str_radix(value, 2).ok()
    } else if let Some(value) = value.strip_prefix("0x") {
        i128::from_str_radix(value, 16).ok()
    } else {
        value.parse().ok()
    }
}

pub(crate) fn input_runtime_ir() -> &'static str {
    r"
declare i32 @getchar()
declare ptr @realloc(ptr, i64)

define ptr @bn_input() {
entry:
  %input.initial = call ptr @realloc(ptr null, i64 64)
  br label %input.read
input.read:
  %input.buffer = phi ptr [ %input.initial, %entry ], [ %input.active.buffer, %input.store ], [ %input.buffer, %input.carriage ]
  %input.capacity = phi i64 [ 64, %entry ], [ %input.active.capacity, %input.store ], [ %input.capacity, %input.carriage ]
  %input.index = phi i64 [ 0, %entry ], [ %input.next, %input.store ], [ %input.index, %input.carriage ]
  %input.char = call i32 @getchar()
  %input.eof = icmp eq i32 %input.char, -1
  br i1 %input.eof, label %input.eof.check, label %input.line.check
input.eof.check:
  %input.empty = icmp eq i64 %input.index, 0
  br i1 %input.empty, label %input.eof.out, label %input.done
input.line.check:
  %input.newline = icmp eq i32 %input.char, 10
  %input.cr = icmp eq i32 %input.char, 13
  br i1 %input.newline, label %input.done, label %input.cr.check
input.cr.check:
  br i1 %input.cr, label %input.carriage, label %input.store.check
input.carriage:
  br label %input.read
input.store.check:
  %input.required = add i64 %input.index, 2
  %input.must.grow = icmp ugt i64 %input.required, %input.capacity
  br i1 %input.must.grow, label %input.grow, label %input.keep
input.grow:
  %input.new.capacity = shl i64 %input.capacity, 1
  %input.grown = call ptr @realloc(ptr %input.buffer, i64 %input.new.capacity)
  br label %input.store
input.keep:
  br label %input.store
input.store:
  %input.active.buffer = phi ptr [ %input.grown, %input.grow ], [ %input.buffer, %input.keep ]
  %input.active.capacity = phi i64 [ %input.new.capacity, %input.grow ], [ %input.capacity, %input.keep ]
  %input.byte = trunc i32 %input.char to i8
  %input.slot = getelementptr i8, ptr %input.active.buffer, i64 %input.index
  store i8 %input.byte, ptr %input.slot
  %input.next = add i64 %input.index, 1
  br label %input.read
input.done:
  %input.end = getelementptr i8, ptr %input.buffer, i64 %input.index
  store i8 0, ptr %input.end
  ret ptr %input.buffer
input.eof.out:
  ret ptr @.bn_eof
}
"
}

pub(crate) fn escape_llvm(value: &str) -> String {
    value
        .bytes()
        .map(|byte| match byte {
            b' '..=b'!' | b'#'..=b'[' | b']'..=b'~' => (byte as char).to_string(),
            _ => format!("\\{byte:02X}"),
        })
        .collect()
}

pub(crate) fn instruction_name(instruction: &Instruction) -> &'static str {
    match instruction {
        Instruction::Constant { .. } => "constants",
        Instruction::Default { .. } => "defaults",
        Instruction::Load { .. } => "loads",
        Instruction::Store { .. } => "stores",
        Instruction::Copy { .. } => "copies",
        Instruction::Unary { .. } => "unary operations",
        Instruction::Binary { .. } => "binary operations",
        Instruction::Cast { .. } => "casts",
        Instruction::Call { .. } => "calls",
        Instruction::DispatchSubmit { .. } | Instruction::DispatchAwait { .. } => {
            "asynchronous dispatch"
        }
        Instruction::Input { .. } => "INPUT",
        Instruction::Vector { .. } => "vectors",
        Instruction::Index { .. } => "indexing",
        Instruction::Member { .. } => "member access",
        Instruction::SetIndex { .. } => "indexed stores",
        Instruction::Length { .. } => "LEN",
        Instruction::SizeOf { .. } => "SIZEOF",
        Instruction::Print { .. } => "PRINT",
        Instruction::ClearScreen { .. } | Instruction::Beep { .. } => "console operations",
        Instruction::Allocate { .. } => "allocation",
        Instruction::Delete { .. } => "deletion",
        Instruction::SetMember { .. } => "member stores",
        Instruction::SetField { .. } => "field stores",
        Instruction::EnsureClass { .. } => "class initialization",
        Instruction::LoadStatic { .. } => "static loads",
        Instruction::StoreStatic { .. } => "static stores",
    }
}

pub(crate) fn unsupported_instruction_detail(instruction: &Instruction) -> String {
    match instruction {
        Instruction::Vector { ty, .. } => format!(
            "LLVM lowering for vector type '{}' is unavailable",
            crate::semantic::display(ty)
        ),
        Instruction::Allocate { type_name, ty, .. } => format!(
            "LLVM lowering for allocation of '{type_name}' as '{}' is unavailable",
            crate::semantic::display(ty)
        ),
        Instruction::Index { ty, .. } | Instruction::SetIndex { ty, .. } => format!(
            "LLVM lowering for indexed access producing '{}' is unavailable",
            crate::semantic::display(ty)
        ),
        Instruction::Default {
            ty,
            dimensions,
            dynamic_dimensions,
            ..
        } if !dimensions.is_empty() || !dynamic_dimensions.is_empty() => format!(
            "LLVM lowering for default value of '{}' with dimensions is unavailable",
            crate::semantic::display(ty)
        ),
        Instruction::Delete { .. } => "LLVM lowering for pointer deletion is unavailable".into(),
        _ => instruction_name(instruction).into(),
    }
}
