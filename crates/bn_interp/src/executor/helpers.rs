#![allow(clippy::wildcard_imports, dead_code)]
use super::*;

/// While a constructor, destructor or field initialiser of `function` runs on
/// its receiver, dispatch on that object is pinned to the declaring class.
pub(super) fn lifecycle_dispatch(
    function: &bn_ir::Function,
    arguments: &[Value],
) -> Option<(Handle, String)> {
    use bn_ir::FunctionKind::{Constructor, Destructor, FieldInit};
    if !matches!(function.kind, Constructor | Destructor | FieldInit) {
        return None;
    }
    let Value::Object { handle, .. } = arguments.first()? else {
        return None;
    };
    Some((*handle, function.owner.clone()?))
}

pub(super) fn require_console(value: &Value, span: Span) -> Result<(), Diagnostic> {
    if matches!(value, Value::HostConsole) {
        Ok(())
    } else {
        Err(super::super::type_mismatch(
            "HOST.Console",
            "non-console value",
            "console operation",
            span,
        ))
    }
}

/// # Errors
///
/// Returns `NUMERIC_OVERFLOW` when the count does not fit the integer type.
pub fn integer_from_count(count: usize, span: Span) -> Result<Value, Diagnostic> {
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
    numeric_overflow("converting a value to INTEGER", span)
}

/// # Panics
///
/// Only if the diagnostic registry schema for this identity is inconsistent (a build error, never a runtime state).
pub fn numeric_overflow(operation: impl Into<String>, span: Span) -> Diagnostic {
    Diagnostic::structured(
        bn_diag::DiagId::NUMERIC_OVERFLOW,
        vec![("operation".into(), operation.into().into())],
        vec![bn_diag::Label {
            span,
            style: bn_diag::LabelStyle::Primary,
            text: None,
        }],
    )
    .expect("numeric-overflow diagnostic schema")
}

pub(super) fn runtime_error(
    id: bn_diag::DiagId,
    message: impl Into<String>,
    span: Span,
) -> Diagnostic {
    let message: String = message.into();
    let argument = match id.argument_schema() {
        [only] => (only.name, message.into()),
        schema => unreachable!(
            "{} needs an explicit argument mapping ({} arguments)",
            id.code(),
            schema.len()
        ),
    };
    Diagnostic::structured(
        id,
        vec![(argument.0.into(), argument.1)],
        vec![bn_diag::Label {
            span,
            style: bn_diag::LabelStyle::Primary,
            text: None,
        }],
    )
    .expect("runtime compatibility diagnostic schema")
}
