#![allow(clippy::wildcard_imports)]
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
        _ => None,
    }
}

#[allow(clippy::cast_sign_loss, clippy::float_cmp)]
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
        _ => IntegerType::Int32,
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

pub(crate) fn integer_width_from_llvm(llvm_ty: &str) -> u8 {
    match llvm_ty {
        "i8" => 8,
        "i16" => 16,
        "i32" => 32,
        "i64" => 64,
        _ => unreachable!("integer LLVM type"),
    }
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
pub(crate) fn render_float(value: f64, ty: &Type) -> String {
    match ty {
        Type::Float(FloatType::Float32) => f64::from(value as f32).to_string(),
        _ => value.to_string(),
    }
}

pub(crate) fn unsupported_call_detail(module: &Module, name: &str) -> String {
    if let Some(provider) = provider_name(module, name) {
        return format!("{provider} calls");
    }
    if name.starts_with("HOST.") {
        return format!("unsupported HOST call {name}");
    }
    "calls".into()
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
    r#"
@.bn_eof = private unnamed_addr constant [4 x i8] c"EOF\00"
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
"#
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
