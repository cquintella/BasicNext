#![allow(clippy::wildcard_imports)]
use super::*;

impl Executor<'_, '_> {
    /// `HOST` members that are language surface rather than a capability
    /// (`HOST.NumProcs`); every `HOST.<capability>.*` goes through the
    /// `hosts` registry.
    pub fn host_call(name: &str, arguments: &[Value], span: Span) -> Result<Value, Diagnostic> {
        match name {
            "HOST.NumProcs" => {
                require_arity(name, arguments, 0, span)?;
                match std::thread::available_parallelism() {
                    Ok(count) => match i32::try_from(count.get()) {
                        Ok(count) => Ok(Value::Integer(i128::from(count), IntegerType::Int32)),
                        Err(_) => Ok(Value::Error {
                            code: 1,
                            message: "available processor count exceeds INTEGER range".into(),
                        }),
                    },
                    Err(error) => Ok(Value::Error {
                        code: 1,
                        message: shared_string(format!(
                            "available processor count is unavailable: {error}"
                        )),
                    }),
                }
            }
            _ => Err(runtime_error(
                bn_diag::DiagId::HOST_CAPABILITY_UNAVAILABLE,
                format!("host function '{name}' is not available"),
                span,
            )),
        }
    }
}
