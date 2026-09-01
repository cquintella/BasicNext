#![allow(clippy::wildcard_imports, dead_code)]
use super::*;

fn is_value_legacy(value: &Value, test: &str) -> bool {
    match test {
        "INTEGER" | "INT32" => matches!(value, Value::Integer(_, IntegerType::Int32)),
        "BYTE" => matches!(value, Value::Integer(_, IntegerType::Byte)),
        "INT8" => matches!(value, Value::Integer(_, IntegerType::Int8)),
        "INT16" => matches!(value, Value::Integer(_, IntegerType::Int16)),
        "INT64" | "TIMESTAMP" => matches!(value, Value::Integer(_, IntegerType::Int64)),
        "UINT16" => matches!(value, Value::Integer(_, IntegerType::UInt16)),
        "UINT32" => matches!(value, Value::Integer(_, IntegerType::UInt32)),
        "UINT64" => matches!(value, Value::Integer(_, IntegerType::UInt64)),
        "FLOAT" | "FLOAT64" => matches!(value, Value::Float(_, FloatType::Float64)),
        "FLOAT32" => matches!(value, Value::Float(_, FloatType::Float32)),
        "BOOLEAN" => matches!(value, Value::Boolean(_)),
        "STRING" => matches!(value, Value::String(_)),
        "NAN" => matches!(value, Value::Float(value, _) if value.is_nan()),
        "INF" => matches!(value, Value::Float(value, _) if *value == f64::INFINITY),
        "-INF" => matches!(value, Value::Float(value, _) if *value == f64::NEG_INFINITY),
        "NULL" => matches!(value, Value::Null),
        "NA" => matches!(value, Value::NotAvailable),
        "EOF" => matches!(value, Value::EndOfFile),
        "Error" => matches!(value, Value::Error { .. }),
        "DATE" => matches!(value, Value::Date(_)),
        "TIME" => matches!(value, Value::Time(_)),
        "TIMEZONE" => matches!(value, Value::TimeZone(_)),
        test if test.starts_with("POINTER TO ") => matches!(value, Value::Pointer { .. }),
        _ => match value {
            Value::File(_) => is_host_file_type(test),
            Value::DataFrame(_) => {
                test == "DataFrame" || (test.ends_with(".DataFrame") && !test.starts_with('#'))
            }
            Value::LogFields(_) => test == "BNLog.Fields" || test == "Fields",
            Value::LogEntry(_) => test == "BNLog.Entry" || test == "Entry",
            Value::LogLogger(_) => test == "BNLog.Logger" || test == "Logger",
            Value::Json(_) => test == "BNJson.Json" || test == "Json",
            Value::DispatchQueue(_) => test == "BNDispatch.Queue" || test == "Queue",
            Value::DispatchTicket(_) => test == "BNDispatch.Ticket" || test == "Ticket",
            Value::DispatchGroup(_) => test == "BNDispatch.Group" || test == "Group",
            Value::DispatchBarrier(_) => test == "BNDispatch.Barrier" || test == "Barrier",
            Value::DispatchSemaphore(_) => test == "BNDispatch.Semaphore" || test == "Semaphore",
            Value::DispatchMutex(_) => test == "BNDispatch.Mutex" || test == "Mutex",
            Value::Object { class, .. } => class == test || class.rsplit('.').next() == Some(test),
            Value::Record { type_name, .. } => type_name == test,
            _ => false,
        },
    }
}

pub(super) fn lifecycle_dispatch(name: &str, arguments: &[Value]) -> Option<(Handle, String)> {
    let class = name
        .strip_suffix(".CONSTRUCTOR")
        .or_else(|| name.strip_suffix(".DESTRUCTOR"))
        .or_else(|| name.strip_suffix(".$fields"))?;
    let Value::Object { handle, .. } = arguments.first()? else {
        return None;
    };
    Some((*handle, class.into()))
}

pub(super) fn require_console(value: &Value, span: Span) -> Result<(), Diagnostic> {
    if matches!(value, Value::HostConsole) {
        Ok(())
    } else {
        Err(runtime_error(
            "TYPE_MISMATCH",
            "CLS and BEEP require HOST.Console",
            span,
        ))
    }
}

pub(super) fn integer_from_count(count: usize, span: Span) -> Result<Value, Diagnostic> {
    let count = i128::try_from(count).map_err(|_| integer_overflow(span))?;
    integer_from_i128_count(count, span)
}

pub(super) fn integer_from_i128_count(count: i128, span: Span) -> Result<Value, Diagnostic> {
    if !(0..=i128::from(i32::MAX)).contains(&count) {
        return Err(integer_overflow(span));
    }
    Ok(Value::Integer(count, IntegerType::Int32))
}

pub(super) fn integer_from_u64(count: u64, span: Span) -> Result<Value, Diagnostic> {
    if count > 2_147_483_647 {
        return Err(integer_overflow(span));
    }
    Ok(Value::Integer(i128::from(count), IntegerType::Int32))
}

pub(super) fn integer_overflow(span: Span) -> Diagnostic {
    runtime_error("NUMERIC_OVERFLOW", "result does not fit INTEGER", span)
}

pub(super) fn runtime_error(code: &'static str, message: impl Into<String>, span: Span) -> Diagnostic {
    Diagnostic {
        code,
        message: message.into(),
        span,
    }
}
