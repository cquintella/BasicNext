#![allow(clippy::wildcard_imports, clippy::too_many_lines)]
use super::*;

impl Executor<'_, '_> {
    pub fn function(
        &mut self,
        function: &Function,
        arguments: Vec<Value>,
    ) -> Result<Flow, Diagnostic> {
        if arguments.len() != function.parameters.len() {
            return Err(super::super::type_mismatch(
                format!("{} argument(s)", function.parameters.len()),
                format!("{} argument(s)", arguments.len()),
                format!("FUNCTION {}", function.name),
                function.span,
            ));
        }
        let mut symbols = function
            .parameters
            .iter()
            .copied()
            .zip(arguments)
            .collect::<HashMap<_, _>>();
        // Parameters are borrowed: the lowering emits every ownership
        // operation, so a call only tracks which bindings ended.
        let mut values = HashMap::new();
        self.ownership_frames.push(OwnershipFrame {
            weak_symbols: function.weak_symbols.clone(),
            ..OwnershipFrame::default()
        });
        let mut block = function.entry;
        let mut previous_block = None;
        loop {
            let current = find_block(function, block)?;
            for (index, instruction) in current.instructions.iter().enumerate() {
                if let Some(hook) = self.debug_hook.as_deref_mut() {
                    hook(&function.name, instruction.span());
                }
                let view = self
                    .debug_control
                    .is_some()
                    .then(|| self.debug_view(&symbols, &values));
                if let (Some(control), Some(view)) = (self.debug_control.as_deref_mut(), view)
                    && control(&function.name, self.call_depth, instruction.span(), &view)
                        == DebugDecision::Terminate
                {
                    return Err(runtime_error(
                        bn_diag::DiagId::DEBUG_TERMINATED,
                        "execution terminated by debugger",
                        instruction.span(),
                    ));
                }
                if let Instruction::Phi {
                    destination,
                    incoming,
                    span,
                    ..
                } = instruction
                {
                    let predecessor = previous_block.ok_or_else(|| {
                        runtime_error(
                            bn_diag::DiagId::INVALID_IR,
                            "Phi cannot execute in the function entry block",
                            *span,
                        )
                    })?;
                    let source = incoming
                        .iter()
                        .find(|(candidate, _)| *candidate == predecessor)
                        .map(|(_, source)| *source)
                        .ok_or_else(|| {
                            runtime_error(
                                bn_diag::DiagId::INVALID_IR,
                                "Phi has no incoming value for the predecessor block",
                                *span,
                            )
                        })?;
                    let selected = value(&values, source, *span)?.clone();
                    set(&mut values, *destination, selected);
                } else {
                    self.instruction(instruction, &mut symbols, &mut values)?;
                }
                // A STOP in a call the IR checks (`$stopping` follows) is
                // handled by the IR, which releases this function's locals
                // first (0.6.md, "`STOP`"); any other one stops at once.
                if self.stop_code.is_some()
                    && !is_stop_check(instruction)
                    && !current
                        .instructions
                        .get(index + 1)
                        .is_some_and(is_stop_check)
                    && let Some(code) = self.stop_code.take()
                {
                    return Ok(Flow::Stop(code));
                }
            }
            match &current.terminator {
                Terminator::Jump { target } => {
                    previous_block = Some(block);
                    block = *target;
                }
                Terminator::Branch {
                    condition,
                    then_block,
                    else_block,
                } => {
                    previous_block = Some(block);
                    block = if boolean(value(&values, *condition, function.span)?, function.span)? {
                        *then_block
                    } else {
                        *else_block
                    };
                }
                Terminator::Return { value: result } => {
                    let returned = result
                        .map(|result| value(&values, result, function.span).cloned())
                        .transpose()?;
                    self.ownership_frames.pop();
                    return Ok(Flow::Return(returned));
                }
                Terminator::Stop { code } => {
                    let code = integer(value(&values, *code, function.span)?, function.span)?.0;
                    self.ownership_frames.pop();
                    return Ok(Flow::Stop(code));
                }
            }
        }
    }

    pub fn ensure_class(&mut self, class: &str, span: Span) -> Result<(), Diagnostic> {
        match self.class_init.get(class).copied() {
            Some(ClassInit::Ready) => return Ok(()),
            Some(ClassInit::Running) => {
                return Err(runtime_error(
                    bn_diag::DiagId::STATIC_INITIALIZATION_CYCLE,
                    format!("STATIC initialization of {class} reentered"),
                    span,
                ));
            }
            None => {}
        }
        self.class_init
            .insert(class.to_string(), ClassInit::Running);
        if let Some(index) = self.module.functions.iter().position(|function| {
            function.kind == bn_ir::FunctionKind::Init && function.owner.as_deref() == Some(class)
        }) {
            let function = &self.module.functions[index];
            match self.function(function, Vec::new())? {
                Flow::Stop(code) => self.stop_code = Some(code),
                Flow::Return(_) => {}
            }
        }
        self.class_init.insert(class.to_string(), ClassInit::Ready);
        Ok(())
    }
}

/// The function constant of a `$stopping` check, which the lowering emits
/// right after a call that may stop.
fn is_stop_check(instruction: &Instruction) -> bool {
    matches!(
        instruction,
        Instruction::Constant {
            value: bn_ir::Constant::Function(name),
            ..
        } if name == bn_ir::names::STOPPING
    )
}
