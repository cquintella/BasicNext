#![allow(clippy::wildcard_imports)]
use super::*;

pub(crate) fn binary_type(
    operator: &str,
    left: &Type,
    right: &Type,
    expression: &Expression,
) -> Result<Type, Diagnostic> {
    let invalid = || {
        error(
            "TYPE_MISMATCH",
            format!(
                "operator {operator} cannot combine {} and {}",
                display(left),
                display(right)
            ),
            expression.span,
        )
    };

    match operator {
        "Assign" | "NotEqual" => {
            if comparable(left, right) {
                Ok(Type::Boolean)
            } else {
                Err(invalid())
            }
        }
        "Less" | "LessEqual" | "Greater" | "GreaterEqual" => {
            numeric_result(left, right).map_or_else(|| Err(invalid()), |_| Ok(Type::Boolean))
        }
        "AND" | "OR" | "XOR" if left == &Type::Boolean && right == &Type::Boolean => {
            Ok(Type::Boolean)
        }
        "AND" | "OR" | "XOR" | "DIV" | "Percent" => integer_result(left, right).ok_or_else(invalid),
        "SHL" | "SHR" => {
            if !is_integer(left) || !is_integer(right) {
                return Err(invalid());
            }
            let result = default_literal_type(left.clone());
            if let Some(count) = constant_integer_from_type(right)
                && (count < 0 || count >= i128::from(integer_width(&result)))
            {
                return Err(error(
                    "INVALID_SHIFT_COUNT",
                    "shift count must be non-negative and smaller than the left operand width",
                    expression.span,
                ));
            }
            Ok(result)
        }
        "Plus" if left == &Type::String && right == &Type::String => Ok(Type::String),
        "Slash" => numeric_result(left, right)
            .map(|_| Type::Float(FloatType::Float64))
            .ok_or_else(invalid),
        "Plus" | "Minus" | "Star" | "Power" => {
            if let Some(value) = constant_integer(expression) {
                return Ok(Type::IntegerLiteral(value.to_string()));
            }
            numeric_result(left, right).ok_or_else(invalid)
        }
        _ => Err(invalid()),
    }
}

pub(crate) fn conversion_allowed(source: &Type, target: &Type) -> bool {
    let numeric = |ty: &Type| {
        matches!(
            ty,
            Type::Integer(_) | Type::IntegerLiteral(_) | Type::Float(_) | Type::FloatLiteral
        )
    };
    numeric(source) && (numeric(target) || *target == Type::Boolean)
        || *source == Type::String && *target == Type::Boolean
        || matches!(source, Type::Null | Type::NotAvailable | Type::EndOfFile)
            && *target == Type::Boolean
}

pub(crate) fn comparable(left: &Type, right: &Type) -> bool {
    compatible(left, right) || compatible(right, left) || numeric_result(left, right).is_some()
}

pub(crate) fn compound_operator(operator: &str) -> Option<&'static str> {
    match operator {
        "PlusAssign" => Some("Plus"),
        "MinusAssign" => Some("Minus"),
        "StarAssign" => Some("Star"),
        "SlashAssign" => Some("Slash"),
        "PercentAssign" => Some("Percent"),
        "PowerAssign" => Some("Power"),
        _ => None,
    }
}

pub(crate) fn numeric_result(left: &Type, right: &Type) -> Option<Type> {
    match (left, right) {
        (Type::IntegerLiteral(_), Type::Float(kind))
        | (Type::Float(kind), Type::IntegerLiteral(_)) => Some(Type::Float(*kind)),
        (Type::IntegerLiteral(_) | Type::FloatLiteral, Type::FloatLiteral)
        | (Type::FloatLiteral, Type::IntegerLiteral(_)) => Some(Type::Float(FloatType::Float64)),
        (Type::Float(left), Type::Float(right)) => Some(Type::Float(
            if left == &FloatType::Float64 || right == &FloatType::Float64 {
                FloatType::Float64
            } else {
                FloatType::Float32
            },
        )),
        (Type::Float(kind), Type::FloatLiteral) | (Type::FloatLiteral, Type::Float(kind)) => {
            Some(Type::Float(*kind))
        }
        _ => integer_result(left, right),
    }
}

pub(crate) fn integer_result(left: &Type, right: &Type) -> Option<Type> {
    match (left, right) {
        (Type::IntegerLiteral(left), Type::IntegerLiteral(right)) => {
            let left = parse_integer(left)?;
            let right = parse_integer(right)?;
            let minimum = left.min(right);
            let maximum = left.max(right);
            integer_kind_for_range(minimum, maximum).map(Type::Integer)
        }
        (Type::Integer(kind), Type::IntegerLiteral(value))
        | (Type::IntegerLiteral(value), Type::Integer(kind)) => {
            integer_literal_fits(value, *kind).then_some(Type::Integer(*kind))
        }
        (Type::Integer(left), Type::Integer(right)) => {
            promote_integers(*left, *right).map(Type::Integer)
        }
        _ => None,
    }
}

pub(crate) fn promote_integers(left: IntegerType, right: IntegerType) -> Option<IntegerType> {
    if left == right {
        return Some(left);
    }
    if left == IntegerType::Byte {
        return Some(if is_unsigned(right) {
            right
        } else {
            widest_signed(IntegerType::Int16, right)
        });
    }
    if right == IntegerType::Byte {
        return Some(if is_unsigned(left) {
            left
        } else {
            widest_signed(left, IntegerType::Int16)
        });
    }
    match (is_unsigned(left), is_unsigned(right)) {
        (true, true) => Some(
            if integer_width(&Type::Integer(left)) >= integer_width(&Type::Integer(right)) {
                left
            } else {
                right
            },
        ),
        (false, false) => Some(widest_signed(left, right)),
        _ => None,
    }
}

pub(crate) fn is_unsigned(kind: IntegerType) -> bool {
    matches!(
        kind,
        IntegerType::Byte | IntegerType::UInt16 | IntegerType::UInt32 | IntegerType::UInt64
    )
}

pub(crate) fn widest_signed(left: IntegerType, right: IntegerType) -> IntegerType {
    if integer_width(&Type::Integer(left)) >= integer_width(&Type::Integer(right)) {
        left
    } else {
        right
    }
}

pub(crate) fn integer_width(ty: &Type) -> u8 {
    match ty {
        Type::Integer(IntegerType::Byte | IntegerType::Int8) => 8,
        Type::Integer(IntegerType::Int16 | IntegerType::UInt16) => 16,
        Type::Integer(IntegerType::Int32 | IntegerType::UInt32) | Type::IntegerLiteral(_) => 32,
        Type::Integer(IntegerType::Int64 | IntegerType::UInt64) => 64,
        _ => 0,
    }
}

pub(crate) fn integer_kind_for_range(minimum: i128, maximum: i128) -> Option<IntegerType> {
    [IntegerType::Int32, IntegerType::Int64, IntegerType::UInt64]
        .into_iter()
        .find(|kind| {
            let (low, high) = integer_range(*kind);
            minimum >= low && maximum <= high
        })
}
