#![allow(clippy::wildcard_imports)]
use super::*;

#[allow(clippy::too_many_arguments)] // Callable lowering keeps signature, body, and SELF explicit.
pub(crate) fn lower_callable(
    model: &SemanticModel,
    name: &str,
    asynchronous: bool,
    signature: Option<&FunctionSignature>,
    parameters: &[crate::ast::Parameter],
    statements: &[Statement],
    span: Span,
    self_span: Option<Span>,
    implicit_super: Option<String>,
    parent_destructor: Option<String>,
    methods: HashSet<String>,
    prefix: &str,
) -> Result<Function, Diagnostic> {
    let mut builder = Builder::new(model, methods, prefix);
    if name.ends_with(".CONSTRUCTOR") {
        builder.derived_fields = Some(format!(
            "{}{}",
            name.trim_end_matches("CONSTRUCTOR"),
            "$fields"
        ));
    }
    if let Some(self_span) = self_span {
        builder.receiver = Some((builder.symbol(self_span)?, type_at(model, self_span)?));
    }
    let explicit_super = statements.first().is_some_and(|statement| {
        matches!(statement, Statement::Call { expression, .. } if matches!(expression.kind, ExpressionKind::Call { ref callee, .. } if matches!(callee.kind, ExpressionKind::Super)))
    });
    if let Some(base) = implicit_super {
        let (receiver, receiver_type) = builder
            .receiver
            .clone()
            .ok_or_else(|| ir_error("implicit SUPER call has no receiver", span))?;
        let receiver = builder.load(receiver, receiver_type, span);
        builder.emit_super_construction(&base, receiver, Vec::new(), span);
        builder.emit_derived_fields(receiver, span);
    } else if name.ends_with(".CONSTRUCTOR")
        && !explicit_super
        && let Some((receiver, receiver_type)) = builder.receiver.clone()
    {
        let receiver = builder.load(receiver, receiver_type, span);
        builder.emit_derived_fields(receiver, span);
    }
    builder.statements(statements)?;
    if let Some(base) = parent_destructor
        && !builder.terminated()
    {
        let (receiver, receiver_type) = builder
            .receiver
            .clone()
            .ok_or_else(|| ir_error("destructor has no receiver", span))?;
        let receiver = builder.load(receiver, receiver_type, span);
        let callee = builder.function_constant(
            &format!("@super:{}", class_method_name(prefix, &base, "DESTRUCTOR")),
            span,
        );
        let unused = builder.value();
        builder.emit(Instruction::Call {
            destination: unused,
            callee,
            arguments: vec![receiver],
            ty: Type::Named("VOID".into()),
            span,
        });
    }
    if !builder.terminated() {
        builder.terminate(Terminator::Return { value: None });
    }
    let mut parameter_ids = Vec::new();
    if let Some((receiver, _)) = &builder.receiver {
        parameter_ids.push(*receiver);
    }
    for parameter in parameters {
        parameter_ids.push(builder.symbol(parameter.span)?);
    }
    let return_type = if let Some(signature) = signature {
        type_at(model, signature.return_type.span)
            .unwrap_or_else(|_| named_or_void(&signature.return_type))
    } else {
        Type::Named("VOID".into())
    };
    Ok(Function {
        name: name.into(),
        asynchronous,
        parameters: parameter_ids,
        return_type,
        entry: BlockId(0),
        blocks: builder.finish()?,
        span,
    })
}
