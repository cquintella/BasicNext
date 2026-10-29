#![allow(clippy::wildcard_imports)]
use super::*;

impl Executor<'_, '_> {
    /// `HOST` members that are language surface rather than a capability
    /// (`HOST.NumProcs`); every `HOST.<capability>.*` goes through the
    /// `hosts` registry.
    pub(crate) fn host_call(
        name: &str,
        arguments: &[Value],
        span: Span,
    ) -> Result<Value, Diagnostic> {
        match name {
            name if name == bn_catalog::text("modules", "capability.num_procs") => {
                require_arity(name, arguments, 0, span)?;
                match std::thread::available_parallelism() {
                    Ok(count) => match i32::try_from(count.get()) {
                        Ok(count) => Ok(Value::Integer(i128::from(count), IntegerType::Int32)),
                        Err(_) => Ok(Value::error_report(
                            1,
                            bn_catalog::text("modules", "capability.num_procs"),
                            "available processor count exceeds INTEGER range".to_string(),
                            bn_catalog::text("frontend", "msg.integer_overflow_cause").to_string(),
                        )),
                    },
                    Err(error) => Ok(Value::error_report(
                        1,
                        bn_catalog::text("modules", "capability.num_procs"),
                        format!("available processor count is unavailable: {error}"),
                        error.to_string(),
                    )),
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
