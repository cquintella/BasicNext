#![allow(clippy::wildcard_imports, clippy::too_many_lines)]
use super::*;

impl Executor<'_, '_> {
    pub(crate) fn function(&mut self, function: &Function, arguments: Vec<Value>) -> Result<Flow, Diagnostic> {
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
        let mut values = HashMap::new();
        let local_symbols = function
            .blocks
            .iter()
            .flat_map(|block| &block.instructions)
            .filter_map(|instruction| match instruction {
                Instruction::Store { symbol, .. } if !function.parameters.contains(symbol) => {
                    Some(*symbol)
                }
                _ => None,
            })
            .collect();
        self.ownership_frames.push(OwnershipFrame {
            local_symbols,
            weak_symbols: function.weak_symbols.clone(),
            release_values: function
                .blocks
                .iter()
                .flat_map(|block| &block.instructions)
                .filter_map(|instruction| match instruction {
                    Instruction::Release { value, .. } => Some(*value),
                    _ => None,
                })
                .collect(),
            ..OwnershipFrame::default()
        });
        let mut block = function.entry;
        let mut previous_block = None;
        loop {
            let current = find_block(function, block)?;
            for instruction in &current.instructions {
                if let Some(hook) = self.debug_hook.as_deref_mut() {
                    hook(&function.name, instruction.span());
                }
                if let Some(control) = self.debug_control.as_deref_mut()
                    && control(
                        &function.name,
                        self.call_depth,
                        instruction.span(),
                        &debug_variables(&symbols, &values),
                    ) == DebugDecision::Terminate
                {
                    return Err(runtime_error(crate::diagnostic::DiagId::DEBUG_TERMINATED,
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
                        runtime_error(crate::diagnostic::DiagId::INVALID_IR,
                            "Phi cannot execute in the function entry block",
                            *span,
                        )
                    })?;
                    let source = incoming
                        .iter()
                        .find(|(candidate, _)| *candidate == predecessor)
                        .map(|(_, source)| *source)
                        .ok_or_else(|| {
                            runtime_error(crate::diagnostic::DiagId::INVALID_IR,
                                "Phi has no incoming value for the predecessor block",
                                *span,
                            )
                        })?;
                    let selected = value(&values, source, *span)?.clone();
                    set(&mut values, *destination, selected);
                } else {
                    self.instruction(instruction, &mut symbols, &mut values)?;
                }
                if let Some(code) = self.stop_code.take() {
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
                    self.finish_ownership_frame(&mut symbols, *result, function.span)?;
                    return Ok(Flow::Return(returned));
                }
                Terminator::Stop { code } => {
                    let code = integer(value(&values, *code, function.span)?, function.span)?.0;
                    self.finish_ownership_frame(&mut symbols, None, function.span)?;
                    return Ok(Flow::Stop(code));
                }
            }
        }
    }

    pub(crate) fn ensure_class(&mut self, class: &str, span: Span) -> Result<(), Diagnostic> {
        match self.class_init.get(class).copied() {
            Some(ClassInit::Ready) => return Ok(()),
            Some(ClassInit::Running) => {
                return Err(runtime_error(crate::diagnostic::DiagId::STATIC_INITIALIZATION_CYCLE,
                    format!("STATIC initialization of {class} reentered"),
                    span,
                ));
            }
            None => {}
        }
        self.class_init
            .insert(class.to_string(), ClassInit::Running);
        let init_name = format!("{class}.$init");
        if let Some(index) = self
            .module
            .functions
            .iter()
            .position(|function| function.name == init_name)
        {
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
