use std::{
    collections::HashMap,
    io::{BufRead, Write},
};

use crate::{
    diagnostic::Diagnostic,
    ir::{BasicBlock, BlockId, Constant, Function, Instruction, Module, Terminator, ValueId},
    semantic::{FloatType, IntegerType, SymbolId, Type},
    source::Span,
};

#[derive(Clone, Debug)]
enum Value {
    Integer(i128, IntegerType),
    Float(f64, FloatType),
    Boolean(bool),
    String(String),
    Vector(Vec<Value>),
    Function(String),
    Type(String),
    Null,
    NotAvailable,
    EndOfFile,
    Error { code: i32, message: String },
}

struct Executor<'a> {
    module: &'a Module,
    input: &'a mut dyn BufRead,
    output: &'a mut dyn Write,
    stop_code: Option<i128>,
}

/// Executes the `Start` function of a validated BN IR module.
///
/// # Errors
///
/// Returns a source-spanned runtime diagnostic for invalid operations, missing
/// entry points, overflow, invalid indices, or I/O failures.
pub fn execute(
    module: &Module,
    input: &mut dyn BufRead,
    output: &mut dyn Write,
) -> Result<u8, Diagnostic> {
    let start = module
        .functions
        .iter()
        .find(|function| function.name == "Start")
        .ok_or_else(|| {
            runtime_error(
                "START_NOT_FOUND",
                "executable module requires FUNCTION Start",
                default_span(),
            )
        })?;
    if !start.parameters.is_empty() {
        return Err(runtime_error(
            "INVALID_START",
            "FUNCTION Start cannot declare parameters",
            start.span,
        ));
    }
    let mut executor = Executor {
        module,
        input,
        output,
        stop_code: None,
    };
    match executor.function(start, Vec::new())? {
        Flow::Return(None) => Ok(0),
        Flow::Return(Some(Value::Integer(code, _))) | Flow::Stop(code) => {
            exit_code(code, start.span)
        }
        Flow::Return(Some(_)) => Err(runtime_error(
            "INVALID_START",
            "FUNCTION Start must return VOID or INTEGER",
            start.span,
        )),
    }
}

enum Flow {
    Return(Option<Value>),
    Stop(i128),
}

impl Executor<'_> {
    fn function(&mut self, function: &Function, arguments: Vec<Value>) -> Result<Flow, Diagnostic> {
        if arguments.len() != function.parameters.len() {
            return Err(runtime_error(
                "TYPE_MISMATCH",
                format!(
                    "FUNCTION {} expects {} argument(s), found {}",
                    function.name,
                    function.parameters.len(),
                    arguments.len()
                ),
                function.span,
            ));
        }
        let mut symbols = function
            .parameters
            .iter()
            .copied()
            .zip(arguments)
            .collect::<HashMap<_, _>>();
        let mut values = HashMap::new();
        let mut block = function.entry;
        loop {
            let current = find_block(function, block)?;
            for instruction in &current.instructions {
                self.instruction(instruction, &mut symbols, &mut values)?;
                if let Some(code) = self.stop_code.take() {
                    return Ok(Flow::Stop(code));
                }
            }
            match &current.terminator {
                Terminator::Jump { target } => block = *target,
                Terminator::Branch {
                    condition,
                    then_block,
                    else_block,
                } => {
                    block = if boolean(value(&values, *condition, function.span)?, function.span)? {
                        *then_block
                    } else {
                        *else_block
                    };
                }
                Terminator::Return { value: result } => {
                    return Ok(Flow::Return(
                        result
                            .map(|result| value(&values, result, function.span).cloned())
                            .transpose()?,
                    ));
                }
                Terminator::Stop { code } => {
                    let code = integer(value(&values, *code, function.span)?, function.span)?.0;
                    return Ok(Flow::Stop(code));
                }
            }
        }
    }

    #[allow(clippy::too_many_lines)] // Runtime operations mirror the compact IR instruction set.
    fn instruction(
        &mut self,
        instruction: &Instruction,
        symbols: &mut HashMap<SymbolId, Value>,
        values: &mut HashMap<ValueId, Value>,
    ) -> Result<(), Diagnostic> {
        match instruction {
            Instruction::Constant {
                destination,
                value: constant,
                ty,
                span,
            } => set(values, *destination, constant_value(constant, ty, *span)?),
            Instruction::Default {
                destination,
                ty,
                dimensions,
                span,
            } => set(values, *destination, default_value(ty, dimensions, *span)?),
            Instruction::Load {
                destination,
                symbol,
                span,
                ..
            } => {
                let loaded = symbols.get(symbol).cloned().ok_or_else(|| {
                    runtime_error("UNINITIALIZED_VALUE", "binding has no value", *span)
                })?;
                set(values, *destination, loaded);
            }
            Instruction::Store {
                symbol,
                value: source,
                ty,
                span,
            } => {
                let stored = coerce(value(values, *source, *span)?.clone(), ty, *span)?;
                symbols.insert(*symbol, stored);
            }
            Instruction::Copy {
                destination,
                source,
                ty,
                span,
            } => {
                let copied = coerce(value(values, *source, *span)?.clone(), ty, *span)?;
                set(values, *destination, copied);
            }
            Instruction::Unary {
                destination,
                operator,
                operand,
                ty,
                span,
            } => {
                let result = unary(operator, value(values, *operand, *span)?, ty, *span)?;
                set(values, *destination, result);
            }
            Instruction::Binary {
                destination,
                operator,
                left,
                right,
                ty,
                span,
            } => {
                let result = binary(
                    operator,
                    value(values, *left, *span)?,
                    value(values, *right, *span)?,
                    ty,
                    *span,
                )?;
                set(values, *destination, result);
            }
            Instruction::Cast {
                destination,
                value: source,
                ty,
                span,
            } => {
                let result = cast(value(values, *source, *span)?.clone(), ty, *span)?;
                set(values, *destination, result);
            }
            Instruction::Call {
                destination,
                callee,
                arguments,
                ty,
                span,
            } => {
                let Value::Function(name) = value(values, *callee, *span)? else {
                    return Err(runtime_error(
                        "TYPE_MISMATCH",
                        "value is not callable",
                        *span,
                    ));
                };
                let arguments = arguments
                    .iter()
                    .map(|argument| value(values, *argument, *span).cloned())
                    .collect::<Result<Vec<_>, _>>()?;
                let result = if name.starts_with("Math.")
                    || name == "Float.TryParse"
                    || name == "$for_condition"
                {
                    builtin(name, &arguments, *span)?
                } else {
                    let function = self
                        .module
                        .functions
                        .iter()
                        .find(|function| function.name == *name)
                        .ok_or_else(|| {
                            runtime_error(
                                "NAME_NOT_FOUND",
                                format!("function '{name}' is not available"),
                                *span,
                            )
                        })?;
                    match self.function(function, arguments)? {
                        Flow::Return(value) => value.unwrap_or(Value::Null),
                        Flow::Stop(code) => {
                            self.stop_code = Some(code);
                            Value::Null
                        }
                    }
                };
                set(values, *destination, coerce(result, ty, *span)?);
            }
            Instruction::Input {
                destination, span, ..
            } => {
                let mut line = String::new();
                let count = self.input.read_line(&mut line).map_err(|error| {
                    runtime_error("INPUT_ERROR", format!("cannot read input: {error}"), *span)
                })?;
                let result = if count == 0 {
                    Value::EndOfFile
                } else {
                    while matches!(line.as_bytes().last(), Some(b'\n' | b'\r')) {
                        line.pop();
                    }
                    Value::String(line)
                };
                set(values, *destination, result);
            }
            Instruction::Vector {
                destination,
                values: elements,
                span,
                ..
            } => {
                let vector = elements
                    .iter()
                    .map(|element| value(values, *element, *span).cloned())
                    .collect::<Result<Vec<_>, _>>()?;
                set(values, *destination, Value::Vector(vector));
            }
            Instruction::Index {
                destination,
                object,
                index,
                span,
                ..
            } => {
                let index = usize::try_from(integer(value(values, *index, *span)?, *span)?.0)
                    .map_err(|_| {
                        runtime_error("INDEX_OUT_OF_BOUNDS", "index cannot be negative", *span)
                    })?;
                let Value::Vector(vector) = value(values, *object, *span)? else {
                    return Err(runtime_error(
                        "TYPE_MISMATCH",
                        "value is not indexable",
                        *span,
                    ));
                };
                let element = vector.get(index).cloned().ok_or_else(|| {
                    runtime_error(
                        "INDEX_OUT_OF_BOUNDS",
                        format!("index {index} is outside vector length {}", vector.len()),
                        *span,
                    )
                })?;
                set(values, *destination, element);
            }
            Instruction::Member {
                destination,
                object,
                name,
                span,
                ..
            } => {
                let member = match (value(values, *object, *span)?, name.as_str()) {
                    (Value::Error { code, .. }, "Code") => {
                        Value::Integer(i128::from(*code), IntegerType::Int32)
                    }
                    (Value::Error { message, .. }, "Message") => Value::String(message.clone()),
                    _ => {
                        return Err(runtime_error(
                            "NAME_NOT_FOUND",
                            format!("runtime value has no member '{name}'"),
                            *span,
                        ));
                    }
                };
                set(values, *destination, member);
            }
            Instruction::SetIndex {
                symbol,
                indices,
                value: source,
                ty,
                span,
            } => {
                let indices = indices
                    .iter()
                    .map(|index| {
                        usize::try_from(integer(value(values, *index, *span)?, *span)?.0).map_err(
                            |_| {
                                runtime_error(
                                    "INDEX_OUT_OF_BOUNDS",
                                    "index cannot be negative",
                                    *span,
                                )
                            },
                        )
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                let source = coerce(value(values, *source, *span)?.clone(), ty, *span)?;
                let target = symbols.get_mut(symbol).ok_or_else(|| {
                    runtime_error("UNINITIALIZED_VALUE", "binding has no value", *span)
                })?;
                set_index(target, &indices, source, *span)?;
            }
            Instruction::Length {
                destination,
                vector,
                span,
            } => {
                let Value::Vector(vector) = value(values, *vector, *span)? else {
                    return Err(runtime_error(
                        "TYPE_MISMATCH",
                        "value has no vector length",
                        *span,
                    ));
                };
                set(
                    values,
                    *destination,
                    Value::Integer(vector.len() as i128, IntegerType::Int32),
                );
            }
            Instruction::Print {
                values: printed,
                span,
            } => {
                for printed in printed {
                    write!(self.output, "{}", render(value(values, *printed, *span)?)).map_err(
                        |error| {
                            runtime_error(
                                "OUTPUT_ERROR",
                                format!("cannot write output: {error}"),
                                *span,
                            )
                        },
                    )?;
                }
                writeln!(self.output).map_err(|error| {
                    runtime_error(
                        "OUTPUT_ERROR",
                        format!("cannot write output: {error}"),
                        *span,
                    )
                })?;
            }
        }
        Ok(())
    }
}

fn find_block(function: &Function, id: BlockId) -> Result<&BasicBlock, Diagnostic> {
    function
        .blocks
        .get(id.0 as usize)
        .filter(|block| block.id == id)
        .ok_or_else(|| runtime_error("INVALID_IR", "basic block does not exist", function.span))
}

fn set(values: &mut HashMap<ValueId, Value>, destination: ValueId, value: Value) {
    values.insert(destination, value);
}

fn set_index(
    target: &mut Value,
    indices: &[usize],
    value: Value,
    span: Span,
) -> Result<(), Diagnostic> {
    let Some((&index, remaining)) = indices.split_first() else {
        *target = value;
        return Ok(());
    };
    let Value::Vector(vector) = target else {
        return Err(runtime_error(
            "TYPE_MISMATCH",
            "value is not indexable",
            span,
        ));
    };
    let length = vector.len();
    let element = vector.get_mut(index).ok_or_else(|| {
        runtime_error(
            "INDEX_OUT_OF_BOUNDS",
            format!("index {index} is outside vector length {length}"),
            span,
        )
    })?;
    set_index(element, remaining, value, span)
}

fn value(values: &HashMap<ValueId, Value>, id: ValueId, span: Span) -> Result<&Value, Diagnostic> {
    values
        .get(&id)
        .ok_or_else(|| runtime_error("INVALID_IR", format!("value %{} is undefined", id.0), span))
}

fn constant_value(constant: &Constant, ty: &Type, span: Span) -> Result<Value, Diagnostic> {
    match constant {
        Constant::Integer(value) => Ok(Value::Integer(
            parse_integer(value)
                .ok_or_else(|| runtime_error("INVALID_IR", "invalid integer constant", span))?,
            integer_kind(ty).unwrap_or(IntegerType::Int32),
        )),
        Constant::Float(value) => Ok(float_value(parse_float(value), float_kind(ty))),
        Constant::String(value) => Ok(Value::String(value.clone())),
        Constant::Boolean(value) => Ok(Value::Boolean(*value)),
        Constant::Null => Ok(Value::Null),
        Constant::NotAvailable => Ok(Value::NotAvailable),
        Constant::EndOfFile => Ok(Value::EndOfFile),
        Constant::Function(value) => Ok(Value::Function(value.clone())),
        Constant::Type(value) => Ok(Value::Type(value.clone())),
    }
}

fn default_value(ty: &Type, dimensions: &[usize], span: Span) -> Result<Value, Diagnostic> {
    match ty {
        Type::Boolean => Ok(Value::Boolean(false)),
        Type::Integer(kind) => Ok(Value::Integer(0, *kind)),
        Type::Float(kind) => Ok(Value::Float(0.0, *kind)),
        Type::String => Ok(Value::String(String::new())),
        Type::Vector {
            element,
            dimensions: declared_dimensions,
        } => {
            let owned_dimensions;
            let dimensions = if dimensions.is_empty() {
                owned_dimensions = declared_dimensions
                    .iter()
                    .map(|dimension| usize::try_from(*dimension))
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(|_| {
                        runtime_error("INVALID_IR", "vector dimension is too large", span)
                    })?;
                &owned_dimensions
            } else {
                dimensions
            };
            let Some(_) = dimensions.first() else {
                return Err(runtime_error(
                    "INVALID_IR",
                    "vector default is missing its dimension",
                    span,
                ));
            };
            let mut value = default_value(element, &[], span)?;
            for length in dimensions.iter().rev() {
                value = Value::Vector(vec![value; *length]);
            }
            Ok(value)
        }
        Type::Alternative(types) => default_value(
            types
                .first()
                .ok_or_else(|| runtime_error("INVALID_IR", "empty alternative type", span))?,
            dimensions,
            span,
        ),
        _ => Err(runtime_error(
            "UNINITIALIZED_VALUE",
            "type has no default value",
            span,
        )),
    }
}

fn unary(operator: &str, operand: &Value, ty: &Type, span: Span) -> Result<Value, Diagnostic> {
    match (operator, operand) {
        ("Minus", Value::Integer(value, _)) => checked_integer(value.checked_neg(), ty, span),
        ("Minus", Value::Float(value, _)) => Ok(float_value(-value, float_kind(ty))),
        ("NOT", Value::Boolean(value)) => Ok(Value::Boolean(!value)),
        ("NOT", Value::Integer(value, _)) => checked_integer(Some(!value), ty, span),
        _ => Err(runtime_error(
            "TYPE_MISMATCH",
            "invalid unary operation",
            span,
        )),
    }
}

#[allow(clippy::too_many_lines)] // Operator behavior is intentionally explicit and centralized.
fn binary(
    operator: &str,
    left: &Value,
    right: &Value,
    ty: &Type,
    span: Span,
) -> Result<Value, Diagnostic> {
    if operator == "IS" {
        let Value::Type(test) = right else {
            return Err(runtime_error(
                "INVALID_IR",
                "IS requires a type operand",
                span,
            ));
        };
        return Ok(Value::Boolean(is_value(left, test)));
    }
    if matches!(operator, "Assign" | "NotEqual") {
        let equal = equals(left, right);
        return Ok(Value::Boolean(if operator == "Assign" {
            equal
        } else {
            !equal
        }));
    }
    if let (Value::Boolean(left), Value::Boolean(right)) = (left, right) {
        return match operator {
            "AND" => Ok(Value::Boolean(*left && *right)),
            "OR" => Ok(Value::Boolean(*left || *right)),
            "XOR" => Ok(Value::Boolean(*left ^ *right)),
            _ => Err(runtime_error(
                "TYPE_MISMATCH",
                "invalid BOOLEAN operation",
                span,
            )),
        };
    }
    if let (Value::String(left), Value::String(right)) = (left, right) {
        return match operator {
            "Plus" => Ok(Value::String(format!("{left}{right}"))),
            _ => Err(runtime_error(
                "TYPE_MISMATCH",
                "invalid STRING operation",
                span,
            )),
        };
    }
    if is_float_value(left) || is_float_value(right) || operator == "Slash" {
        let left = number_as_float(left, span)?;
        let right = number_as_float(right, span)?;
        return match operator {
            "Plus" => Ok(float_value(left + right, float_kind(ty))),
            "Minus" => Ok(float_value(left - right, float_kind(ty))),
            "Star" => Ok(float_value(left * right, float_kind(ty))),
            "Slash" => Ok(float_value(left / right, float_kind(ty))),
            "Power" => Ok(float_value(left.powf(right), float_kind(ty))),
            "Less" => Ok(Value::Boolean(left < right)),
            "LessEqual" => Ok(Value::Boolean(left <= right)),
            "Greater" => Ok(Value::Boolean(left > right)),
            "GreaterEqual" => Ok(Value::Boolean(left >= right)),
            _ => Err(runtime_error(
                "TYPE_MISMATCH",
                "invalid floating operation",
                span,
            )),
        };
    }
    let (left, _) = integer(left, span)?;
    let (right, _) = integer(right, span)?;
    match operator {
        "Plus" => checked_integer(left.checked_add(right), ty, span),
        "Minus" => checked_integer(left.checked_sub(right), ty, span),
        "Star" => checked_integer(left.checked_mul(right), ty, span),
        "DIV" if right != 0 => checked_integer(left.checked_div_euclid(right), ty, span),
        "Percent" if right != 0 => checked_integer(left.checked_rem_euclid(right), ty, span),
        "DIV" | "Percent" => Err(runtime_error(
            "DIVISION_BY_ZERO",
            "integer divisor cannot be zero",
            span,
        )),
        "Power" if right >= 0 => checked_integer(
            left.checked_pow(u32::try_from(right).map_err(|_| {
                runtime_error("INVALID_EXPONENT", "integer exponent is too large", span)
            })?),
            ty,
            span,
        ),
        "Power" => Err(runtime_error(
            "INVALID_EXPONENT",
            "integer exponent cannot be negative",
            span,
        )),
        "AND" => checked_integer(Some(left & right), ty, span),
        "OR" => checked_integer(Some(left | right), ty, span),
        "XOR" => checked_integer(Some(left ^ right), ty, span),
        "SHL" => shift(left, right, ty, true, span),
        "SHR" => shift(left, right, ty, false, span),
        "Less" => Ok(Value::Boolean(left < right)),
        "LessEqual" => Ok(Value::Boolean(left <= right)),
        "Greater" => Ok(Value::Boolean(left > right)),
        "GreaterEqual" => Ok(Value::Boolean(left >= right)),
        _ => Err(runtime_error(
            "TYPE_MISMATCH",
            "invalid integer operation",
            span,
        )),
    }
}

fn shift(value: i128, count: i128, ty: &Type, left: bool, span: Span) -> Result<Value, Diagnostic> {
    let width = integer_width(integer_kind(ty).unwrap_or(IntegerType::Int32));
    if count < 0 || count >= i128::from(width) {
        return Err(runtime_error(
            "INVALID_SHIFT_COUNT",
            format!("shift count must be in 0..{width}"),
            span,
        ));
    }
    let count = u32::try_from(count).expect("validated shift count");
    if left {
        checked_integer(value.checked_shl(count), ty, span)
    } else {
        let mask = (1_u128 << width) - 1;
        checked_integer(
            Some(((value.cast_unsigned() & mask) >> count).cast_signed()),
            ty,
            span,
        )
    }
}

fn cast(value: Value, ty: &Type, span: Span) -> Result<Value, Diagnostic> {
    match ty {
        Type::Boolean => Ok(Value::Boolean(match value {
            Value::Boolean(value) => value,
            Value::Integer(value, _) => value != 0,
            Value::Float(value, _) => value != 0.0,
            Value::String(value) => !value.is_empty(),
            Value::Null | Value::NotAvailable | Value::EndOfFile => false,
            _ => true,
        })),
        Type::Integer(_) => match value {
            Value::Integer(value, _) => checked_integer(Some(value), ty, span),
            #[allow(clippy::cast_possible_truncation)]
            // BN specifies truncation followed by range checking.
            Value::Float(value, _) if value.is_finite() => {
                checked_integer(Some(value.trunc() as i128), ty, span)
            }
            Value::Float(_, _) => Err(runtime_error(
                "INVALID_NUMERIC_CONVERSION",
                "NAN and infinity cannot convert to an integer",
                span,
            )),
            _ => Err(runtime_error(
                "TYPE_MISMATCH",
                "value cannot convert to an integer",
                span,
            )),
        },
        Type::Float(_) => Ok(float_value(number_as_float(&value, span)?, float_kind(ty))),
        _ => Err(runtime_error(
            "TYPE_MISMATCH",
            "unsupported conversion",
            span,
        )),
    }
}

fn coerce(value: Value, ty: &Type, span: Span) -> Result<Value, Diagnostic> {
    match (&value, ty) {
        (Value::Integer(number, _), Type::Integer(_)) => checked_integer(Some(*number), ty, span),
        (Value::Float(number, _), Type::Float(_)) => Ok(float_value(*number, float_kind(ty))),
        (_, Type::Alternative(types)) if types.iter().any(|ty| value_matches_type(&value, ty)) => {
            Ok(value)
        }
        (Value::Boolean(_), Type::Boolean)
        | (Value::String(_), Type::String)
        | (Value::Vector(_), Type::Vector { .. })
        | (Value::Function(_), Type::Function { .. })
        | (Value::Null, Type::Null)
        | (Value::NotAvailable, Type::NotAvailable)
        | (Value::EndOfFile, Type::EndOfFile) => Ok(value),
        (Value::Null, Type::Named(name)) if name == "VOID" => Ok(value),
        _ => Err(runtime_error(
            "TYPE_MISMATCH",
            "runtime value does not match its IR destination type",
            span,
        )),
    }
}

fn checked_integer(value: Option<i128>, ty: &Type, span: Span) -> Result<Value, Diagnostic> {
    let value = value
        .ok_or_else(|| runtime_error("NUMERIC_OVERFLOW", "integer operation overflowed", span))?;
    let kind = integer_kind(ty).unwrap_or(IntegerType::Int32);
    let (minimum, maximum) = integer_range(kind);
    if !(minimum..=maximum).contains(&value) {
        return Err(runtime_error(
            "NUMERIC_OVERFLOW",
            format!("{value} does not fit {kind:?}"),
            span,
        ));
    }
    Ok(Value::Integer(value, kind))
}

#[allow(clippy::too_many_lines)] // Standard numeric functions share argument decoding and errors.
fn builtin(name: &str, arguments: &[Value], span: Span) -> Result<Value, Diagnostic> {
    if name == "$for_condition" {
        let current = integer(&arguments[0], span)?.0;
        let end = integer(&arguments[1], span)?.0;
        let step = integer(&arguments[2], span)?.0;
        if step == 0 {
            return Err(runtime_error(
                "INVALID_FOR_STEP",
                "FOR STEP cannot be zero",
                span,
            ));
        }
        return Ok(Value::Boolean(if step > 0 {
            current <= end
        } else {
            current >= end
        }));
    }
    if name == "Float.TryParse" {
        let Value::String(text) = &arguments[0] else {
            return Err(runtime_error(
                "TYPE_MISMATCH",
                "Float.TryParse expects STRING",
                span,
            ));
        };
        return Ok(text.parse::<f64>().map_or_else(
            |_| Value::Error {
                code: 1,
                message: format!("'{text}' is not a FLOAT"),
            },
            |value| Value::Float(value, FloatType::Float64),
        ));
    }
    let math_name = name
        .strip_prefix("Math.")
        .ok_or_else(|| runtime_error("NAME_NOT_FOUND", "unknown builtin", span))?;
    if matches!(math_name, "TOHOUR" | "TOWEEKDAY") {
        let milliseconds = integer(&arguments[0], span)?.0;
        let days = milliseconds.div_euclid(86_400_000);
        let result = if math_name == "TOHOUR" {
            milliseconds.div_euclid(3_600_000).rem_euclid(24)
        } else {
            // 1970-01-01 was Thursday (ISO weekday 4).
            (days + 3).rem_euclid(7) + 1
        };
        return Ok(Value::Integer(result, IntegerType::Int32));
    }
    if matches!(math_name, "ABS" | "MIN" | "MAX" | "SIGN")
        && arguments
            .iter()
            .all(|argument| matches!(argument, Value::Integer(_, _)))
    {
        let integers = arguments
            .iter()
            .map(|argument| integer(argument, span).map(|(value, _)| value))
            .collect::<Result<Vec<_>, _>>()?;
        let kind = integer(&arguments[0], span)?.1;
        let result = match math_name {
            "ABS" => integers[0]
                .checked_abs()
                .ok_or_else(|| runtime_error("NUMERIC_OVERFLOW", "Math.ABS overflowed", span))?,
            "MIN" => integers[0].min(integers[1]),
            "MAX" => integers[0].max(integers[1]),
            "SIGN" => integers[0].signum(),
            _ => unreachable!(),
        };
        return Ok(Value::Integer(result, kind));
    }
    let numbers = arguments
        .iter()
        .map(|value| number_as_float(value, span))
        .collect::<Result<Vec<_>, _>>()?;
    let result = match math_name {
        "ABS" => numbers[0].abs(),
        "MIN" => {
            if numbers.iter().any(|value| value.is_nan()) {
                f64::NAN
            } else {
                numbers[0].min(numbers[1])
            }
        }
        "MAX" => {
            if numbers.iter().any(|value| value.is_nan()) {
                f64::NAN
            } else {
                numbers[0].max(numbers[1])
            }
        }
        "SIGN" => {
            if numbers[0] == 0.0 || numbers[0].is_nan() {
                numbers[0]
            } else {
                numbers[0].signum()
            }
        }
        "FLOOR" => numbers[0].floor(),
        "CEIL" => numbers[0].ceil(),
        "TRUNC" => numbers[0].trunc(),
        "ROUND" => {
            let scale = 10_f64.powf(numbers[1]);
            (numbers[0] * scale).round_ties_even() / scale
        }
        "EXP" => numbers[0].exp(),
        "LOG" => numbers[0].ln(),
        "LOG10" => numbers[0].log10(),
        "LOG2" => numbers[0].log2(),
        "POW" => numbers[0].powf(numbers[1]),
        "SIN" => numbers[0].sin(),
        "COS" => numbers[0].cos(),
        "TAN" => numbers[0].tan(),
        "ASIN" => numbers[0].asin(),
        "ACOS" => numbers[0].acos(),
        "ATAN" => numbers[0].atan(),
        "ATAN2" => numbers[0].atan2(numbers[1]),
        "SQRT" => numbers[0].sqrt(),
        "HYPOT" => numbers[0].hypot(numbers[1]),
        "FMA" => numbers[0].mul_add(numbers[1], numbers[2]),
        _ => {
            return Err(runtime_error(
                "NAME_NOT_FOUND",
                format!("unknown Math function '{math_name}'"),
                span,
            ));
        }
    };
    Ok(Value::Float(result, FloatType::Float64))
}

#[allow(clippy::cast_precision_loss, clippy::float_cmp)] // BN equality is exact IEEE equality.
fn equals(left: &Value, right: &Value) -> bool {
    match (left, right) {
        (Value::Integer(left, _), Value::Integer(right, _)) => left == right,
        (Value::Float(left, _), Value::Float(right, _)) => left == right,
        (Value::Integer(left, _), Value::Float(right, _)) => *left as f64 == *right,
        (Value::Float(left, _), Value::Integer(right, _)) => *left == *right as f64,
        (Value::Boolean(left), Value::Boolean(right)) => left == right,
        (Value::String(left), Value::String(right)) => left == right,
        (Value::Null, Value::Null)
        | (Value::NotAvailable, Value::NotAvailable)
        | (Value::EndOfFile, Value::EndOfFile) => true,
        (Value::Vector(left), Value::Vector(right)) => {
            left.len() == right.len()
                && left
                    .iter()
                    .zip(right)
                    .all(|(left, right)| equals(left, right))
        }
        _ => false,
    }
}

fn is_value(value: &Value, test: &str) -> bool {
    match test {
        "NAN" => matches!(value, Value::Float(value, _) if value.is_nan()),
        "INF" => matches!(value, Value::Float(value, _) if *value == f64::INFINITY),
        "-INF" => matches!(value, Value::Float(value, _) if *value == f64::NEG_INFINITY),
        "NULL" => matches!(value, Value::Null),
        "NA" => matches!(value, Value::NotAvailable),
        "EOF" => matches!(value, Value::EndOfFile),
        "Error" => matches!(value, Value::Error { .. }),
        _ => false,
    }
}

fn value_matches_type(value: &Value, ty: &Type) -> bool {
    match (value, ty) {
        (Value::Integer(_, _), Type::Integer(_))
        | (Value::Float(_, _), Type::Float(_))
        | (Value::Boolean(_), Type::Boolean)
        | (Value::String(_), Type::String)
        | (Value::Vector(_), Type::Vector { .. })
        | (Value::Null, Type::Null)
        | (Value::NotAvailable, Type::NotAvailable)
        | (Value::EndOfFile, Type::EndOfFile) => true,
        (Value::Error { .. }, Type::Named(name)) => name == "Error",
        _ => false,
    }
}

fn integer(value: &Value, span: Span) -> Result<(i128, IntegerType), Diagnostic> {
    let Value::Integer(value, kind) = value else {
        return Err(runtime_error(
            "TYPE_MISMATCH",
            "expected integral value",
            span,
        ));
    };
    Ok((*value, *kind))
}

fn boolean(value: &Value, span: Span) -> Result<bool, Diagnostic> {
    let Value::Boolean(value) = value else {
        return Err(runtime_error(
            "TYPE_MISMATCH",
            "expected BOOLEAN value",
            span,
        ));
    };
    Ok(*value)
}

#[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)] // IEEE conversion is part of BN numeric semantics.
fn number_as_float(value: &Value, span: Span) -> Result<f64, Diagnostic> {
    match value {
        Value::Integer(value, _) => Ok(*value as f64),
        Value::Float(value, kind) => Ok(match kind {
            FloatType::Float32 => f64::from(*value as f32),
            FloatType::Float64 => *value,
        }),
        _ => Err(runtime_error(
            "TYPE_MISMATCH",
            "expected numeric value",
            span,
        )),
    }
}

fn is_float_value(value: &Value) -> bool {
    matches!(value, Value::Float(_, _))
}

fn integer_kind(ty: &Type) -> Option<IntegerType> {
    match ty {
        Type::Integer(kind) => Some(*kind),
        _ => None,
    }
}

fn float_kind(ty: &Type) -> FloatType {
    match ty {
        Type::Float(kind) => *kind,
        _ => FloatType::Float64,
    }
}

#[allow(clippy::cast_possible_truncation)] // FLOAT32 storage requires IEEE binary32 rounding.
fn float_value(value: f64, kind: FloatType) -> Value {
    Value::Float(
        match kind {
            FloatType::Float32 => f64::from(value as f32),
            FloatType::Float64 => value,
        },
        kind,
    )
}

fn integer_width(kind: IntegerType) -> u8 {
    match kind {
        IntegerType::Byte | IntegerType::Int8 => 8,
        IntegerType::Int16 | IntegerType::UInt16 => 16,
        IntegerType::Int32 | IntegerType::UInt32 => 32,
        IntegerType::Int64 | IntegerType::UInt64 => 64,
    }
}

fn integer_range(kind: IntegerType) -> (i128, i128) {
    match kind {
        IntegerType::Byte => (0, u8::MAX.into()),
        IntegerType::Int8 => (i8::MIN.into(), i8::MAX.into()),
        IntegerType::Int16 => (i16::MIN.into(), i16::MAX.into()),
        IntegerType::Int32 => (i32::MIN.into(), i32::MAX.into()),
        IntegerType::Int64 => (i64::MIN.into(), i64::MAX.into()),
        IntegerType::UInt16 => (0, u16::MAX.into()),
        IntegerType::UInt32 => (0, u32::MAX.into()),
        IntegerType::UInt64 => (0, u64::MAX.into()),
    }
}

fn parse_integer(value: &str) -> Option<i128> {
    if let Some(value) = value.strip_prefix("0b") {
        i128::from_str_radix(value, 2).ok()
    } else if let Some(value) = value.strip_prefix("0x") {
        i128::from_str_radix(value, 16).ok()
    } else {
        value.parse().ok()
    }
}

fn parse_float(value: &str) -> f64 {
    match value {
        "NAN" => f64::NAN,
        "INF" => f64::INFINITY,
        "-INF" => f64::NEG_INFINITY,
        _ => value.parse().expect("validated FLOAT literal"),
    }
}

fn render(value: &Value) -> String {
    match value {
        Value::Integer(value, _) => value.to_string(),
        Value::Float(value, _) if value.is_nan() => "NAN".into(),
        Value::Float(value, _) if *value == f64::INFINITY => "INF".into(),
        Value::Float(value, _) if *value == f64::NEG_INFINITY => "-INF".into(),
        Value::Float(value, _) => {
            let mut text = value.to_string();
            if !text.contains(['.', 'e', 'E']) {
                text.push_str(".0");
            }
            text
        }
        Value::Boolean(value) => if *value { "TRUE" } else { "FALSE" }.into(),
        Value::String(value) => value.clone(),
        Value::Null => "NULL".into(),
        Value::NotAvailable => "NA".into(),
        Value::EndOfFile => "EOF".into(),
        Value::Vector(values) => format!(
            "[{}]",
            values.iter().map(render).collect::<Vec<_>>().join(", ")
        ),
        Value::Function(name) | Value::Type(name) => name.clone(),
        Value::Error { code, message } => format!("Error({code}, {message})"),
    }
}

fn exit_code(code: i128, span: Span) -> Result<u8, Diagnostic> {
    u8::try_from(code)
        .map_err(|_| runtime_error("INVALID_EXIT_CODE", "exit code must be in 0..255", span))
}

fn runtime_error(code: &'static str, message: impl Into<String>, span: Span) -> Diagnostic {
    Diagnostic {
        code,
        message: message.into(),
        span,
    }
}

fn default_span() -> Span {
    Span {
        start: crate::source::Position {
            offset: 0,
            line: 1,
            column: 1,
        },
        end: crate::source::Position {
            offset: 0,
            line: 1,
            column: 1,
        },
    }
}
